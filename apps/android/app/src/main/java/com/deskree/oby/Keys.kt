package com.deskree.oby

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.interfaces.ECPublicKey
import java.security.spec.ECGenParameterSpec
import java.time.Instant
import java.time.temporal.ChronoUnit
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * This phone's approval key (apps/shared/DeviceKey.swift): a P-256 key in Android Keystore, made
 * in StrongBox when the phone has one, that never leaves it. The owner's tap is the approval (the
 * owner's decision of 2026-10-24): signing asks for no fingerprint, face or PIN. What keeps other
 * apps from signing is the Keystore itself: only Oby's own process can use its keys.
 */
object DeviceKey {
    private const val ALIAS = "approval-key"
    /** The biometric key it replaces (until 2026-11-01): nothing signs with it any more. */
    private const val REPLACED = "publish-key"
    private val keystore get() = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }

    /** The public half as the runner takes it: X9.63 (65 bytes, uncompressed), base64. */
    fun publicKey(): String {
        val key = keystore.getCertificate(ALIAS)?.publicKey as? ECPublicKey ?: make()
        fun bytes(n: java.math.BigInteger) = n.toByteArray().let { if (it.size > 32) it.copyOfRange(it.size - 32, it.size) else ByteArray(32 - it.size) + it }
        return Base64.encodeToString(byteArrayOf(4) + bytes(key.w.affineX) + bytes(key.w.affineY), Base64.NO_WRAP)
    }

    private fun make(): ECPublicKey {
        fun spec(strongBox: Boolean) = KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_SIGN)
            .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
            .setDigests(KeyProperties.DIGEST_SHA256)
            .setIsStrongBoxBacked(strongBox)
            .build()
        val generator = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore")
        val pair = runCatching { generator.initialize(spec(true)); generator.generateKeyPair() }
            .getOrElse { generator.initialize(spec(false)); generator.generateKeyPair() }
        runCatching { keystore.deleteEntry(REPLACED) }
        return pair.public as ECPublicKey
    }

    private fun now() = Instant.now().truncatedTo(ChronoUnit.SECONDS).toString()

    /** Signs one publish of `job` by `action`, the runner's own sentence. */
    fun approve(job: String, action: String, device: String): Approval {
        val signedAt = now()
        return Approval(device, action, signedAt, sign(Approval.message(job, action, signedAt)))
    }

    /** Signs the owner's Confirm of inbox item `item`: `action` is the item's title, word for word (#776). */
    fun approveConfirm(item: String, title: String, device: String): Approval {
        val signedAt = now()
        return Approval(device, title, signedAt, sign("oby-confirm\n$item\n$title\n$signedAt".toByteArray()))
    }

    /**
     * Signs a request that carries words the owner typed, as device `device` (docs/runner-api.md,
     * "Words the owner typed"): the value of its `X-Oby-Words` header, `"<device> <signedAt> <signature>"`,
     * over its method, its path from `/v1/` on and its body's SHA-256. To the millisecond: two of
     * the same request in one second are two signatures.
     */
    fun signWords(method: String, path: String, body: ByteArray, device: String): String {
        val signedAt = Instant.now().truncatedTo(ChronoUnit.MILLIS).toString()
        val digest = java.security.MessageDigest.getInstance("SHA-256").digest(body).joinToString("") { "%02x".format(it) }
        return "$device $signedAt ${sign("oby-words\n$method\n$path\n$digest\n$signedAt".toByteArray())}"
    }

    /** ECDSA P-256 with SHA-256, DER, base64: what the runner checks. */
    private fun sign(message: ByteArray): String {
        if (keystore.getKey(ALIAS, null) == null) make()
        val signature = Signature.getInstance("SHA256withECDSA").apply { initSign(keystore.getKey(ALIAS, null) as PrivateKey); update(message) }
        return Base64.encodeToString(signature.sign(), Base64.NO_WRAP)
    }
}

/**
 * This phone's device token for the linked Mac's gateway: encrypted with an AES key that lives in
 * Android Keystore and never leaves it (the iPhone's keychain item), the ciphertext in the app's
 * private preferences, out of backups. Nothing else ever sees it.
 */
object GatewayTokenStore : SealedPreference("gateway.token") {
    override fun saveFailed() = RunnerError("The phone couldn't keep the Mac's device token.")
}

/**
 * The Mac's Obysphere relay this phone paired through (RelayLink), sealed beside the device token,
 * so its relay token never reaches the preferences. (The link there names the relay's id and the
 * Mac's public key, `MacLink.relayed`, neither of them secret: which relay and key this must be.)
 */
object RelayStore {
    private val sealed = object : SealedPreference("relay.link") {
        override fun saveFailed() = RunnerError("The phone couldn't keep its way to your Mac.")
    }

    fun read(context: Context): RelayLink? = sealed.read(context)?.let { runCatching { wire.decodeFromString(RelayLink.serializer(), it) }.getOrNull() }
    fun save(context: Context, link: RelayLink) = sealed.save(context, wire.encodeToString(RelayLink.serializer(), link))
    fun remove(context: Context) = sealed.remove(context)
}

/** A string sealed with the app's Keystore AES key, in its private preferences under `pref`. */
abstract class SealedPreference(private val pref: String) {
    /** Read once, then kept in memory: every request and route reads it. */
    @Volatile private var cached: String? = null

    abstract fun saveFailed(): RunnerError

    private fun key(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM).setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE).build())
        return generator.generateKey()
    }

    fun read(context: Context): String? {
        cached?.let { return it }
        val saved = Store.prefs(context).getString(pref, null) ?: return null
        return runCatching {
            val bytes = Base64.decode(saved, Base64.NO_WRAP)
            val cipher = Cipher.getInstance("AES/GCM/NoPadding")
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, bytes, 0, 12))
            String(cipher.doFinal(bytes, 12, bytes.size - 12))
        }.getOrNull()?.also { cached = it }
    }

    fun save(context: Context, value: String) {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding").apply { init(Cipher.ENCRYPT_MODE, key()) }
        val sealed = cipher.iv + cipher.doFinal(value.toByteArray())
        if (!Store.prefs(context).edit().putString(pref, Base64.encodeToString(sealed, Base64.NO_WRAP)).commit()) throw saveFailed()
        cached = value
    }

    fun remove(context: Context) { cached = null; Store.prefs(context).edit().remove(pref).apply() }

    private companion object { const val ALIAS = "gateway-token-key" }
}
