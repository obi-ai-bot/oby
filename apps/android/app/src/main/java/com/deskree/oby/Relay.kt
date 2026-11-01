package com.deskree.oby

import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.Headers
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.asResponseBody
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.Buffer
import okio.ByteString
import okio.ByteString.Companion.toByteString
import okio.Source
import okio.Timeout
import okio.buffer
import org.bouncycastle.crypto.digests.SHA256Digest
import org.bouncycastle.crypto.generators.HKDFBytesGenerator
import org.bouncycastle.crypto.modes.ChaCha20Poly1305
import org.bouncycastle.crypto.params.AEADParameters
import org.bouncycastle.crypto.params.HKDFParameters
import org.bouncycastle.crypto.params.KeyParameter
import org.bouncycastle.math.ec.rfc7748.X25519
import java.io.IOException
import java.nio.ByteBuffer
import java.security.MessageDigest
import java.security.SecureRandom
import java.util.Base64
import java.util.concurrent.CompletableFuture
import java.util.concurrent.ExecutionException
import java.util.concurrent.Executors
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.TimeoutException

/**
 * A Mac reached through its owner's Obysphere (apps/shared/RemoteRoute.swift's RelayLink;
 * docs/runner-api.md, "obid only: the phone relay"): what its pairing QR code adds as `link.relay`.
 * The token and the Mac's key are this phone's alone: kept sealed in Keystore (RelayStore), never
 * in the preferences with the link.
 */
@Serializable
data class RelayLink(val sphere: String, val id: String, val token: String, val key: String) {
    /** What a link paired through it keeps as `MacLink.relayed`: the relay and the pinned key. */
    val pinned get() = "$id.$key"

    /** The Mac's key as its 32 bytes, or null when it isn't one. */
    val macKey: ByteArray? get() = if (!isBase64URL(key)) null else runCatching { Base64.getUrlDecoder().decode(key) }.getOrNull()?.takeIf { it.size == 32 }

    private val sphereURI get() = runCatching { java.net.URI(sphere) }.getOrNull()

    /** Every field as obid makes it, and an Obysphere over HTTPS: plain HTTP only on this device's loopback (a Sphere run locally for tests). */
    val isValid: Boolean get() {
        val uri = sphereURI ?: return false
        val host = uri.host?.lowercase() ?: return false
        val loopback = host in setOf("127.0.0.1", "localhost", "[::1]", "::1")
        return (uri.scheme == "https" || uri.scheme == "http" && loopback) && host.isNotEmpty() && uri.userInfo == null &&
            uri.rawPath.orEmpty().trimEnd('/').isEmpty() && uri.rawQuery == null && uri.rawFragment == null &&
            id.length in 22..43 && isBase64URL(id) && token.length == 43 && isBase64URL(token) && macKey != null
    }

    /** Where the phone's socket opens: `GET <sphere>/v0.1/relay/{id}` as a WebSocket (OkHttp takes it as http(s)). */
    val socketURL get() = sphere.trimEnd('/') + "/v0.1/relay/$id"

    companion object {
        private fun isBase64URL(text: String) = text.isNotEmpty() && text.all { it in 'a'..'z' || it in 'A'..'Z' || it in '0'..'9' || it == '-' || it == '_' }
    }
}

/**
 * The phone's side of the envelope (apps/shared/PhoneRelay.swift's RelayEnvelope; docs/runner-api.md,
 * "The envelope (version 1)"): a hello sealed to the Mac's key from the QR code, the Mac's welcome,
 * then one key per direction and an exact counter each way. X25519, HKDF-SHA256 and
 * ChaCha20-Poly1305 are BouncyCastle's: Android 11 has no X25519 of its own.
 */
object RelayEnvelope {
    val versions = byteArrayOf(1)
    const val LARGEST = 262_144

    /** Why a welcome starts nothing: not from the Mac whose key the QR code pinned, or not a welcome this phone asked for. */
    class Refused(val notTheMac: Boolean) : Exception(if (notTheMac) "not the Mac" else "unreadable")

    class Hello(mac: ByteArray, private val ephemeral: ByteArray = newKey(), private val offered: ByteArray = versions) {
        val bytes: ByteArray
        private val es: ByteArray = agree(ephemeral, mac)

        init {
            val mine = publicOf(ephemeral)
            val head = byteArrayOf(1, 1) + mine
            bytes = head + seal(derive(mac + mine, es, "oby relay v1 hello"), 0, offered, head)
        }

        /** The session, from the Mac's welcome: only the Mac with the pinned key could have made it. */
        fun welcomed(welcome: ByteArray): Session {
            if (welcome.size != 34 + 1 + 16 || welcome[1] != 2.toByte() || welcome[0] !in offered) throw Refused(false)
            val version = welcome[0]
            val theirs = welcome.copyOfRange(2, 34)
            val ee = try { agree(ephemeral, theirs) } catch (_: Refused) { throw Refused(true) }
            val salt = MessageDigest.getInstance("SHA-256").digest(bytes + theirs)
            fun key(info: String) = derive(salt, es + ee, info)
            val said = open(key("oby relay v1 welcome"), 0, welcome.copyOfRange(34, welcome.size), welcome.copyOfRange(0, 34)) ?: throw Refused(true)
            if (!said.contentEquals(byteArrayOf(version))) throw Refused(false)
            return Session(version, key("oby relay v1 phone>mac"), key("oby relay v1 mac>phone"))
        }
    }

    sealed interface Opened {
        class Frame(val plain: ByteArray) : Opened
        /** Changed, replayed, older, sent back the other way, another session's or no envelope at all: nothing spent. */
        data object Dropped : Opened
        /** It opens, but one before it is missing: the session is over, say hello again. */
        data object Gap : Opened
    }

    /** A session's two directions, each with its key and its exact counter. */
    class Session(val version: Byte, private val sealing: ByteArray, private val opening: ByteArray) {
        var sent = 0L; private set
        var next = 0L; private set

        /** `v 03 ‖ n ‖ seal(k, nonce(n), frame)`: the 10-byte header is the associated data. */
        fun seal(frame: ByteArray): ByteArray {
            val head = byteArrayOf(version, 3) + counter(sent)
            return (head + seal(sealing, sent, frame, head)).also { sent += 1 }
        }

        /** Exactly the next envelope opens; one past it is a gap. */
        fun open(envelope: ByteArray): Opened {
            if (envelope.size < 26 || envelope.size > LARGEST || envelope[1] != 3.toByte() || envelope[0] != version) return Opened.Dropped
            val n = ByteBuffer.wrap(envelope, 2, 8).long
            if (java.lang.Long.compareUnsigned(n, next) < 0) return Opened.Dropped
            val plain = open(opening, n, envelope.copyOfRange(10, envelope.size), envelope.copyOfRange(0, 10)) ?: return Opened.Dropped
            if (n != next) return Opened.Gap
            next += 1
            return Opened.Frame(plain)
        }
    }

    fun newKey(): ByteArray = ByteArray(X25519.SCALAR_SIZE).also { X25519.generatePrivateKey(SecureRandom(), it) }

    fun publicOf(private: ByteArray): ByteArray = ByteArray(X25519.POINT_SIZE).also { X25519.generatePublicKey(private, 0, it, 0) }

    /** X25519 (RFC 7748), refusing a key that agrees to all zeros (6.1). */
    fun agree(mine: ByteArray, theirs: ByteArray): ByteArray {
        if (theirs.size != X25519.POINT_SIZE) throw Refused(true)
        val shared = ByteArray(X25519.POINT_SIZE)
        if (!X25519.calculateAgreement(mine, 0, theirs, 0, shared, 0)) throw Refused(true)
        return shared
    }

    /** HKDF-SHA256, 32 bytes. */
    fun derive(salt: ByteArray, ikm: ByteArray, info: String): ByteArray =
        ByteArray(32).also { HKDFBytesGenerator(SHA256Digest()).apply { init(HKDFParameters(ikm, salt, info.toByteArray())) }.generateBytes(it, 0, 32) }

    fun counter(n: Long): ByteArray = ByteBuffer.allocate(8).putLong(n).array()

    private fun nonce(n: Long) = ByteArray(4) + counter(n)

    /** ChaCha20-Poly1305 with 4 zero bytes then the counter as its nonce, the tag after the ciphertext. */
    fun seal(key: ByteArray, n: Long, plain: ByteArray, aad: ByteArray): ByteArray {
        val cipher = ChaCha20Poly1305().apply { init(true, AEADParameters(KeyParameter(key), 128, nonce(n), aad)) }
        val out = ByteArray(cipher.getOutputSize(plain.size))
        val written = cipher.processBytes(plain, 0, plain.size, out, 0)
        cipher.doFinal(out, written)
        return out
    }

    fun open(key: ByteArray, n: Long, sealed: ByteArray, aad: ByteArray): ByteArray? = try {
        val cipher = ChaCha20Poly1305().apply { init(false, AEADParameters(KeyParameter(key), 128, nonce(n), aad)) }
        val out = ByteArray(cipher.getOutputSize(sealed.size))
        val written = cipher.processBytes(sealed, 0, sealed.size, out, 0)
        cipher.doFinal(out, written)
        out
    } catch (_: Exception) { null }
}

/** What a sealed envelope holds (obi-sphere `protocol.md` section 13): a stream (4 bytes, big-endian), a kind and its payload. */
class RelayFrame(val stream: Int, val kind: Kind, val payload: ByteArray = ByteArray(0)) {
    enum class Kind { Open, Data, End, Reset }

    val encoded: ByteArray get() = ByteBuffer.allocate(5 + payload.size).putInt(stream).put(kind.ordinal.toByte()).put(payload).array()

    companion object {
        fun of(plain: ByteArray): RelayFrame? {
            if (plain.size < 5) return null
            val kind = Kind.entries.getOrNull(plain[4].toInt()) ?: return null
            return RelayFrame(ByteBuffer.wrap(plain).int, kind, plain.copyOfRange(5, plain.size))
        }
    }
}

/**
 * The relay route (apps/shared/PhoneRelay.swift): one WebSocket to the Mac's Obysphere with this
 * phone's relay token, carrying sealed requests, their answers and the event stream. It's an
 * OkHttp interceptor that never proceeds: a request to the Mac goes sealed through it or not at
 * all, never over plain HTTP to the Sphere. Each socket starts a session with a hello and sends
 * nothing before the Mac's welcome; nothing of a session outlives its socket.
 */
class PhoneRelay(
    val link: RelayLink,
    private val connect: (Request, WebSocketListener) -> WebSocket = { request, listener -> sockets.newWebSocket(request, listener) },
    private val patience: Long = 5_000,
) : Interceptor {
    companion object {
        val unreachable = RunnerError("Your Mac isn't reachable.")
        val forgotten = RunnerError("This phone was removed from your Mac.", 401)
        val replaced = RunnerError("Another connection took this phone's place at your Obysphere. Open Oby again to reconnect.")
        /** No welcome to a hello: a Mac that's busy or just restarted, or one that can't open it. */
        val unanswered = RunnerError("Your Mac isn't answering.")
        /** Several hellos in a row unanswered: a Mac whose key changed looks like this, and only pairing again pins its new key. */
        val stillUnanswered = RunnerError("Your Mac still isn't answering. If this keeps happening, pair again with the code in Oby on your Mac.")
        /** The most chunks of one answer held for a reader slower than the Mac (64 KiB each at most). */
        const val HELD = 256
        private const val CHUNK = 65_536

        // The socket's client: Runner's, with its cleartext rule and its timeouts for the upgrade (OkHttp
        // drops the read timeout once a WebSocket is up; the ping below finds a link gone quiet).
        private val sockets get() = Runner.client
        /** How long the Sphere may be silent (a `pong` every 30 seconds) before the socket is taken for gone. */
        private const val SILENT = 75_000L
        /** The longest a socket may take to open before its hello's wait starts anyway. */
        private const val UPGRADE = 30_000L
        private val timer = Executors.newSingleThreadScheduledExecutor { Thread(it, "oby-relay").apply { isDaemon = true } }
    }

    private sealed interface Chunk {
        class Bytes(val bytes: ByteArray) : Chunk
        data object End : Chunk
        class Failed(val error: IOException) : Chunk
    }

    private class Head(val status: Int, val headers: Headers)

    private class Answer {
        val head = CompletableFuture<Head>()
        val body = LinkedBlockingDeque<Chunk>()
        @Volatile var heard = System.currentTimeMillis()
    }

    private val lock = Any()
    private var socket: WebSocket? = null
    /** Counts sockets: what an earlier one says or how it ends changes nothing any more. */
    private var generation = 0
    private var opened = false
    private var hello: RelayEnvelope.Hello? = null
    private var session: RelayEnvelope.Session? = null
    private var waiting = mutableListOf<CompletableFuture<Unit>>()
    private val answers = mutableMapOf<Int, Answer>()
    private var lastStream = 0
    /** Why it opens nothing by itself any more (replaced, forgotten, another key), until `resume`. */
    private var stopped: RunnerError? = null
    /** After a 4008 (the Mac isn't connected) no socket opens before this, the wait doubling from 30 seconds to 5 minutes. */
    private var notBefore = 0L
    var backoff = 0L; private set
    private var unansweredInARow = 0
    private var pinging: ScheduledFuture<*>? = null
    /** When the Sphere last said anything on this socket (a `pong` included). */
    @Volatile private var heardAt = 0L

    // MARK: Requests

    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        val call = chain.call()
        val path = request.url.encodedPath + (request.url.encodedQuery?.let { "?$it" } ?: "")
        val body = request.body?.let { Buffer().also(it::writeTo).readByteArray() } ?: ByteArray(0)
        val headers = buildJsonObject {
            for ((name, value) in request.headers) put(name.lowercase(), value)
            request.body?.contentType()?.let { put("content-type", it.toString()) }
        }
        val head = wire.encodeToString(JsonObject.serializer(), buildJsonObject {
            put("method", request.method); put("path", path); put("headers", headers)
        }).toByteArray()
        await(ready(), call)
        val quiet = chain.readTimeoutMillis().toLong().takeIf { it > 0 } ?: 45_000
        val (stream, answer) = synchronized(lock) {
            lastStream += 1
            val stream = lastStream
            val answer = Answer()
            answers[stream] = answer
            val frames = listOf(RelayFrame(stream, RelayFrame.Kind.Open, head)) +
                (0 until body.size step CHUNK).map { RelayFrame(stream, RelayFrame.Kind.Data, body.copyOfRange(it, minOf(it + CHUNK, body.size))) } +
                RelayFrame(stream, RelayFrame.Kind.End)
            if (!post(frames)) fail(stream, IOException("The connection to your Mac was lost."))
            stream to answer
        }
        watch(stream, quiet)
        val got = try { await(answer.head, call) } catch (e: IOException) { stop(stream, e); throw e }
        val source = object : Source {
            private var left = ByteArray(0)
            private var done = false
            override fun read(sink: Buffer, byteCount: Long): Long {
                while (left.isEmpty()) {
                    if (done) return -1
                    when (val chunk = answer.body.poll(250, TimeUnit.MILLISECONDS)) {
                        null -> if (call.isCanceled()) throw IOException("Canceled")
                        is Chunk.Bytes -> left = chunk.bytes
                        Chunk.End -> { done = true; return -1 }
                        is Chunk.Failed -> { done = true; throw chunk.error }
                    }
                }
                val n = minOf(byteCount, left.size.toLong()).toInt()
                sink.write(left, 0, n)
                left = left.copyOfRange(n, left.size)
                return n.toLong()
            }
            override fun timeout() = Timeout.NONE
            // Ending the body early resets the stream, so the Mac stops.
            override fun close() { if (!done) { done = true; stop(stream, IOException("Canceled")) } }
        }
        return Response.Builder().request(request).protocol(Protocol.HTTP_1_1).code(got.status).message("").headers(got.headers)
            .body(source.buffer().asResponseBody(got.headers["content-type"]?.toMediaTypeOrNull(), -1)).build()
    }

    /** Waits for `future` as long as the call isn't canceled; what it failed with as an IOException. */
    private fun <T> await(future: CompletableFuture<T>, call: okhttp3.Call): T {
        while (true) {
            try { return future.get(250, TimeUnit.MILLISECONDS) } catch (_: TimeoutException) {
                if (call.isCanceled()) throw IOException("Canceled")
            } catch (e: ExecutionException) {
                val cause = e.cause
                throw cause as? IOException ?: IOException(cause?.message ?: "The connection to your Mac was lost.", cause)
            }
        }
    }

    /** The person asked (the app came to the front): a socket may open again at once. */
    fun resume() = synchronized(lock) { stopped = null; notBefore = 0 }

    /** The app went to the background: the socket closes, and with it the session, and none opens again until `resume`. */
    fun hangUp() = synchronized(lock) {
        stopped = stopped ?: RunnerError("Open Oby to reach your Mac.")
        drop(IOException("Canceled"))
    }

    // MARK: The session

    /** Done once a session is up, opening a socket and saying hello if none is. */
    private fun ready(): CompletableFuture<Unit> = synchronized(lock) {
        val started = CompletableFuture<Unit>()
        when {
            stopped != null -> started.completeExceptionally(stopped)
            session != null -> started.complete(Unit)
            System.currentTimeMillis() < notBefore -> started.completeExceptionally(unreachable)
            else -> { waiting += started; if (hello == null) sayHello() }
        }
        started
    }

    private fun sayHello() {
        val key = link.macKey
        val hello = key?.let { runCatching { RelayEnvelope.Hello(it) }.getOrNull() }
        if (hello == null) {
            stopped = RunnerError("This pairing's key for your Mac can't be used. Pair again with the code in Oby on your Mac.")
            return drop(stopped!!)
        }
        this.hello = hello
        val socket = socket ?: open()
        // OkHttp holds it until the socket is up.
        socket.send(hello.bytes.toByteString())
        // Waited for from the socket's opening; a socket that doesn't open in time ends anyway.
        wait(hello, if (opened) patience else UPGRADE + patience)
    }

    /** No welcome within a few seconds of the hello going out: start over, on a new socket. */
    private fun wait(hello: RelayEnvelope.Hello, patience: Long = this.patience) {
        val current = generation
        timer.schedule(Runnable {
            synchronized(lock) {
                if (current == generation && session == null && this.hello === hello) {
                    unansweredInARow += 1
                    drop(if (unansweredInARow >= 3) stillUnanswered else unanswered)
                }
            }
        }, patience, TimeUnit.MILLISECONDS)
    }

    private fun open(): WebSocket {
        generation += 1
        opened = false
        val current = generation
        val request = Request.Builder().url(link.socketURL).header("Authorization", "Bearer ${link.token}").build()
        val socket = connect(request, object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                synchronized(lock) {
                    if (current != generation) return
                    opened = true
                    heardAt = System.currentTimeMillis()
                    hello?.let { wait(it) }
                }
            }
            override fun onMessage(webSocket: WebSocket, bytes: ByteString) { heardAt = System.currentTimeMillis(); heard(bytes.toByteArray(), current) }
            // The Sphere's only text is `pong`: it's still there.
            override fun onMessage(webSocket: WebSocket, text: String) { heardAt = System.currentTimeMillis() }
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) { webSocket.close(code, null); ended(current, code, null) }
            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = ended(current, code, null)
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) = ended(current, null, response?.code)
        })
        this.socket = socket
        // The Sphere closes a socket silent for 2 minutes.
        pinging?.cancel(false)
        pinging = timer.scheduleWithFixedDelay(Runnable {
            synchronized(lock) {
                if (current != generation || !opened) return@Runnable
                // Nothing from the Sphere for 2½ pings: the link is gone, though TCP hasn't said so.
                if (System.currentTimeMillis() - heardAt > SILENT) return@Runnable drop(IOException("The connection to your Mac was lost."))
                socket.send("ping")
            }
        }, 30, 30, TimeUnit.SECONDS)
        return socket
    }

    /** Seals `frames` with the next counters and sends them, in this session only; false when they can't go. */
    private fun post(frames: List<RelayFrame>): Boolean {
        val socket = socket ?: return false
        val session = session ?: return false
        for (frame in frames) if (!socket.send(session.seal(frame.encoded).toByteString())) return false
        return true
    }

    private fun heard(envelope: ByteArray, current: Int) {
        synchronized(lock) {
            if (current != generation) return
            val session = session ?: return welcome(envelope)
            when (val opened = session.open(envelope)) {
                RelayEnvelope.Opened.Dropped -> {}
                RelayEnvelope.Opened.Gap -> over()
                is RelayEnvelope.Opened.Frame -> RelayFrame.of(opened.plain)?.let(::take)
            }
        }
    }

    private fun welcome(envelope: ByteArray) {
        // A welcome that doesn't start this hello's session is an earlier hello's, come late, or
        // forged: it changes nothing, and the hello still waits for its own.
        val hello = hello ?: return
        if (envelope.size < 2 || envelope[1] != 2.toByte()) return
        val started = runCatching { hello.welcomed(envelope) }.getOrNull() ?: return
        session = started
        this.hello = null
        backoff = 0
        unansweredInARow = 0
        waiting.forEach { it.complete(Unit) }
        waiting = mutableListOf()
    }

    private fun take(frame: RelayFrame) {
        // Stream 0 is the session's own: obid's `reset` there ends it.
        if (frame.stream == 0) { if (frame.kind == RelayFrame.Kind.Reset) over(); return }
        val answer = answers[frame.stream] ?: return
        answer.heard = System.currentTimeMillis()
        when (frame.kind) {
            RelayFrame.Kind.Open -> {
                if (answer.head.isDone) return
                val head = runCatching {
                    val o = wire.parseToJsonElement(String(frame.payload)).jsonObject
                    val headers = Headers.Builder()
                    (o["headers"] as? JsonObject)?.forEach { (k, v) -> headers.add(k, v.jsonPrimitive.content) }
                    Head(o["status"]!!.jsonPrimitive.int, headers.build())
                }.getOrNull() ?: return fail(frame.stream, IOException("Your Mac's answer couldn't be read."))
                answer.head.complete(head)
            }
            RelayFrame.Kind.Data -> if (answer.body.size >= HELD) stop(frame.stream, IOException("Your Mac's answer came faster than Oby could take it."))
                else answer.body.add(Chunk.Bytes(frame.payload))
            RelayFrame.Kind.End -> {
                if (!answer.head.isDone) return fail(frame.stream, IOException("Your Mac's answer couldn't be read."))
                answers.remove(frame.stream)
                answer.body.add(Chunk.End)
            }
            RelayFrame.Kind.Reset -> fail(frame.stream, IOException(String(frame.payload)))
        }
    }

    /** The session is over (an envelope went missing, or obid said so): the next request says hello again on the same socket. */
    private fun over() {
        session = null
        hello = null
        for (stream in answers.keys.toList()) fail(stream, IOException("The connection to your Mac was lost."))
    }

    private fun fail(stream: Int, error: IOException) {
        val answer = answers.remove(stream) ?: return
        answer.head.completeExceptionally(error)
        answer.body.add(Chunk.Failed(error))
    }

    /** The caller stopped waiting, or the answer can't go on: the Mac stops answering it. */
    private fun stop(stream: Int, error: IOException) {
        synchronized(lock) {
            if (answers[stream] == null) return
            fail(stream, error)
            post(listOf(RelayFrame(stream, RelayFrame.Kind.Reset)))
        }
    }

    /**
     * Nothing from the Mac on a stream for `quiet` milliseconds (its call's read timeout; an event
     * stream and a wait send something every 15 seconds). Before its answer began, the session is
     * stuck: a new hello, on a new socket. After, that answer alone ends.
     */
    private fun watch(stream: Int, quiet: Long) {
        timer.schedule(Runnable {
            synchronized(lock) {
                val answer = answers[stream] ?: return@Runnable
                if (System.currentTimeMillis() - answer.heard < quiet) return@Runnable watch(stream, quiet)
                if (!answer.head.isDone) {
                    fail(stream, IOException("Your Mac didn't answer in time."))
                    drop(IOException("The connection to your Mac was lost."))
                } else stop(stream, IOException("Your Mac didn't answer in time."))
            }
        }, quiet, TimeUnit.MILLISECONDS)
    }

    /** The socket ended: why, as the Sphere's close code or its refusal of the upgrade says. */
    private fun ended(current: Int, code: Int?, status: Int?) {
        synchronized(lock) { endedLocked(current, code, status) }
    }

    private fun endedLocked(current: Int, code: Int?, status: Int?) {
        if (current != generation || socket == null) return
        val error: IOException = when {
            status == 401 || code == 4001 -> forgotten.also { stopped = it }
            // Another socket of this phone's took over: never open again by itself.
            code == 4003 -> replaced.also { stopped = it }
            status == 429 || code == 4008 -> {
                backoff = minOf(maxOf(backoff * 2, 30_000), 300_000)
                notBefore = System.currentTimeMillis() + backoff
                unreachable
            }
            else -> IOException("The connection to your Mac was lost.")
        }
        drop(error)
    }

    /** Closes the socket: its session, what was asked in it and what waited for it end with `error`. */
    private fun drop(error: IOException) {
        generation += 1
        pinging?.cancel(false)
        pinging = null
        socket?.cancel()
        socket = null
        opened = false
        hello = null
        session = null
        for (stream in answers.keys.toList()) fail(stream, error)
        waiting.forEach { it.completeExceptionally(error) }
        waiting = mutableListOf()
    }
}
