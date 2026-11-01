// The watch publishes through the iPhone, or with its own pairing key (apps/watch/WatchLink.swift).
#if os(iOS) || os(macOS)
import CryptoKit
import Foundation
import OSLog
import Security

/// A device's approval for one publish, as the runner checks it (docs/runner-api.md).
nonisolated struct Approval: Encodable, Equatable, Sendable {
    var device: String
    var action: String
    var signedAt: String
    var signature: String

    static func message(job: String, action: String, signedAt: String) -> Data {
        Data("oby-publish\n\(job)\n\(action)\n\(signedAt)".utf8)
    }

    /// What a device signs to change the runner's connections (docs/runner-api.md, "Connection"):
    /// another prefix than a publish's, so neither signature passes for the other.
    static func connectionMessage(action: String, signedAt: String) -> Data {
        Data("oby-connection\n\(action)\n\(signedAt)".utf8)
    }

    /// What a device signs to Confirm inbox item `item`, whose title is `title` (docs/runner-api.md,
    /// "POST /v1/inbox/{id}/answer", #776): its own prefix, so no publish or connection signature
    /// passes for a Confirm, and the item's id, so one tap confirms one item.
    static func confirmMessage(item: String, title: String, signedAt: String) -> Data {
        Data("oby-confirm\n\(item)\n\(title)\n\(signedAt)".utf8)
    }

    static func limitMessage(id: String, revision: Int, choice: String, account: String?, signedAt: String) -> Data {
        Data("oby-limit-decision\n\(id)\n\(revision)\n\(choice)\n\(account ?? "")\n\(signedAt)".utf8)
    }

    static func pairingBindingMessage(id: String, pairing: String, nonce: String, signedAt: String) -> Data {
        Data("oby-device-pairing\n\(id)\n\(pairing)\n\(nonce)\n\(signedAt)".utf8)
    }

    /// What a registered device signs to revoke device `device` (docs/runner-api.md,
    /// "DELETE /v1/devices/{id}"): the Mac's new key, for the key it replaced.
    static func revokeMessage(device: String, signedAt: String) -> Data {
        Data("oby-revoke-device\n\(device)\n\(signedAt)".utf8)
    }

    /// What a device signs with a request that carries words the owner typed in Oby's apps
    /// (docs/runner-api.md, "Words the owner typed"): its method, its path from `/v1/` on and its
    /// body's SHA-256, so the runner can tell them from words anything else holding its token sent
    /// (oby-mcp, a script, an agent), and one signature passes for one request.
    static func wordsMessage(method: String, path: String, body: Data, signedAt: String) -> Data {
        let digest = SHA256.hash(data: body).map { String(format: "%02x", $0) }.joined()
        return Data("oby-words\n\(method)\n\(path)\n\(digest)\n\(signedAt)".utf8)
    }

    /// What the Mac signs on the owner's Pair a device (docs/runner-api.md, "Invitations"): only the
    /// phone such an invitation pairs may add its own key.
    static func inviteMessage(signedAt: String) -> Data {
        Data("oby-pair-invite\n\(signedAt)".utf8)
    }
}

/// The device's approval key: it lives in the Secure Enclave, so it never leaves this device, and
/// the runner can tell an approval came from the owner's own app, not from a job, another Oby or
/// anything it received. The owner's click is the approval (the owner's decision of 2026-10-24):
/// signing asks for no Touch ID, Face ID or password. What keeps other code on the device from
/// signing is where its wrapped blob is kept: on the iPhone, Oby's own keychain, which no other app
/// reads; on the Mac, a generic password in the login keychain, whose access list makes macOS ask
/// the owner before any other app reads it, and which a boxed agent can't reach at all. A program
/// that runs as the owner and that the owner lets read it could sign.
// ponytail: the Mac's data-protection keychain, with an access group, would bind the blob to Oby
// alone; it needs Developer ID's provisioning profile (F4, as Engine/Vault.swift's vault).
nonisolated enum DeviceKey {
    static var isAvailable: Bool { SecureEnclave.isAvailable }

    /// The public half, to register with the runner.
    static func publicKey() throws -> Data {
        try key().publicKey.x963Representation
    }

    /// Signs one publish of `job` by `action`, the runner's own sentence.
    static func approve(job: String, action: String, device: String) throws -> Approval {
        let signedAt = ISO8601DateFormatter().string(from: .now)
        return Approval(device: device, action: action, signedAt: signedAt,
                        signature: try sign(Approval.message(job: job, action: action, signedAt: signedAt)))
    }

    /// Signs one change to the runner's connections, as `approve` signs a publish.
    static func approveConnection(action: String, device: String) throws -> Approval {
        let signedAt = ISO8601DateFormatter().string(from: .now)
        return Approval(device: device, action: action, signedAt: signedAt,
                        signature: try sign(Approval.connectionMessage(action: action, signedAt: signedAt)))
    }

    /// Signs the owner's Confirm of inbox item `item`: `action` is the item's title, word for word (#776).
    static func approveConfirm(item: String, title: String, device: String) throws -> Approval {
        let signedAt = ISO8601DateFormatter().string(from: .now)
        return Approval(device: device, action: title, signedAt: signedAt,
                        signature: try sign(Approval.confirmMessage(item: item, title: title, signedAt: signedAt)))
    }

    static func approveLimit(id: String, revision: Int, choice: String, account: String?, device: String) throws -> Approval {
        let signedAt = ISO8601DateFormatter().string(from: .now)
        return Approval(device: device, action: choice, signedAt: signedAt,
                        signature: try sign(Approval.limitMessage(id: id, revision: revision, choice: choice, account: account, signedAt: signedAt)))
    }

    static func approvePairingBinding(id: String, pairing: String, nonce: String) throws -> Approval {
        let signedAt = ISO8601DateFormatter().string(from: .now)
        return Approval(device: id, action: "bind-pairing", signedAt: signedAt,
                        signature: try sign(Approval.pairingBindingMessage(id: id, pairing: pairing, nonce: nonce, signedAt: signedAt)))
    }

    /// Signs the revocation of device `device`, as `approve` signs a publish: this key is the
    /// registered device `signer`.
    static func approveRevoke(device: String, signer: String) throws -> Approval {
        let signedAt = ISO8601DateFormatter().string(from: .now)
        return Approval(device: signer, action: device, signedAt: signedAt,
                        signature: try sign(Approval.revokeMessage(device: device, signedAt: signedAt)))
    }

    /// Signs a pairing invitation, made on the owner's click, as device `device`.
    static func approveInvite(device: String) throws -> Approval {
        let signedAt = ISO8601DateFormatter().string(from: .now)
        return Approval(device: device, action: "", signedAt: signedAt, signature: try sign(Approval.inviteMessage(signedAt: signedAt)))
    }

    /// Signs a request that carries words the owner typed, as device `device`: the value of its
    /// `X-Oby-Words` header, `"<device> <signedAt> <signature>"`.
    static func signWords(method: String, path: String, body: Data, device: String) throws -> String {
        // To the millisecond: two of the same request in one second are two signatures.
        let format = ISO8601DateFormatter()
        format.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let signedAt = format.string(from: .now)
        let signature = try sign(Approval.wordsMessage(method: method, path: path, body: body, signedAt: signedAt))
        return "\(device) \(signedAt) \(signature)"
    }

    /// ECDSA P-256 with SHA-256, DER, base64: what the runner checks.
    static func sign(_ message: Data) throws -> String {
        try key().signature(for: message).derRepresentation.base64EncodedString()
    }

    #if os(macOS)
    // The Mac's login keychain, as its vault's (Engine/Vault.swift): it has no passcode class.
    private static let protection = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
    private static let noKey = "This Mac can't make its approval key."
    /// The keys it replaces: Touch ID only (`.biometryCurrentSet`, until 2026-10-23), then Touch ID
    /// or the password (`.userPresence`, until 2026-10-24). Nothing signs with them any more.
    private static let replaced = ["publish-key", "publish-key.presence"]
    #else
    private static let protection = kSecAttrAccessibleWhenPasscodeSetThisDeviceOnly
    private static let noKey = "This iPhone can't make its approval key."
    /// The Face ID key it replaces (`.biometryCurrentSet`, until 2026-10-24).
    private static let replaced = ["publish-key"]
    #endif
    /// A key with no presence check: a new id with the runner, so each device registers it once
    /// (MacDevice.idKey, Phone.deviceKey).
    private static let account = "approval-key"

    private static func key() throws -> SecureEnclave.P256.Signing.PrivateKey {
        if let saved = read(account) {
            return try SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: saved)
        }
        var error: Unmanaged<CFError>?
        guard let access = SecAccessControlCreateWithFlags(nil, protection, [.privateKeyUsage], &error) else {
            throw error?.takeRetainedValue() ?? RunnerError(message: noKey)
        }
        let key: SecureEnclave.P256.Signing.PrivateKey
        do {
            key = try SecureEnclave.P256.Signing.PrivateKey(accessControl: access)
        } catch {
            throw RunnerError(message: noKey)
        }
        try save(key.dataRepresentation)
        for old in replaced {
            let deleted = SecItemDelete(item(old) as CFDictionary)
            if deleted != errSecSuccess && deleted != errSecItemNotFound {
                Logger(subsystem: "com.deskree.oby", category: "device-key")
                    .error("couldn't delete the old approval key \(old, privacy: .public): \(deleted, privacy: .public)")
            }
        }
        return key
    }

    // The key itself never leaves the Secure Enclave: the Keychain keeps the wrapped blob only
    // this device's enclave can use.
    private static func item(_ account: String) -> [String: Any] {
        [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "com.deskree.oby", kSecAttrAccount as String: account]
    }

    private static func read(_ account: String) -> Data? {
        var query = item(account)
        query[kSecReturnData as String] = true
        var found: CFTypeRef?
        return SecItemCopyMatching(query as CFDictionary, &found) == errSecSuccess ? found as? Data : nil
    }

    private static func save(_ blob: Data) throws {
        var add = item(account)
        add[kSecValueData as String] = blob
        add[kSecAttrAccessible as String] = protection
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw RunnerError(message: "Couldn't keep the approval key (\(status)).") }
    }
}
#endif
