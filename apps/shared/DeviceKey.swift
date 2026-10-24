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

    /// What a registered device signs to revoke device `device` (docs/runner-api.md,
    /// "DELETE /v1/devices/{id}"): the Mac's new key, for the key it replaced.
    static func revokeMessage(device: String, signedAt: String) -> Data {
        Data("oby-revoke-device\n\(device)\n\(signedAt)".utf8)
    }
}

/// The device's approval key: it lives in the Secure Enclave, and only Oby on this device can
/// use it, so the runner can tell an approval came from the owner's own app, not from a job, another
/// Oby or anything it received. The owner's click is the approval (the owner's decision of
/// 2026-10-24): signing asks for no Touch ID, Face ID or password.
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

    /// Signs the revocation of device `device`, as `approve` signs a publish: this key is the
    /// registered device `signer`.
    static func approveRevoke(device: String, signer: String) throws -> Approval {
        let signedAt = ISO8601DateFormatter().string(from: .now)
        return Approval(device: signer, action: device, signedAt: signedAt,
                        signature: try sign(Approval.revokeMessage(device: device, signedAt: signedAt)))
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
