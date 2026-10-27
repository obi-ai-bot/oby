import Foundation
import Observation
import OSLog

/// What signs an approval on this Mac: its Secure Enclave key (apps/shared/DeviceKey.swift), or a stand-in in tests.
nonisolated protocol PublishingKey: Sendable {
    var isAvailable: Bool { get }
    /// The public half, X9.63, to register with the runner.
    func publicKey() throws -> Data
    /// Signs a publish of `job` by `action`, the runner's own sentence. The owner's click is the
    /// approval: nothing asks for Touch ID or the password (the owner's decision of 2026-10-24).
    func approve(job: String, action: String, device: String) async throws -> Approval
    /// The same for a change to the runner's connections (`Approval.connectionMessage`).
    func approveConnection(action: String, device: String) async throws -> Approval
    /// The same for the owner's Confirm of an inbox item (`Approval.confirmMessage`, #776).
    func approveConfirm(item: String, title: String, device: String) async throws -> Approval
    /// The same for revoking device `device`, the key this one replaced; this key is device `signer`
    /// (`Approval.revokeMessage`).
    func approveRevoke(device: String, signer: String) async throws -> Approval
    /// The same for a pairing invitation made on the owner's click (`Approval.inviteMessage`).
    func approveInvite(device: String) async throws -> Approval
}

/// The Mac's Secure Enclave key, behind the login keychain's access list (apps/shared/DeviceKey.swift):
/// it signs without asking.
nonisolated struct EnclaveKey: PublishingKey {
    var isAvailable: Bool { DeviceKey.isAvailable }

    func publicKey() throws -> Data { try DeviceKey.publicKey() }

    func approve(job: String, action: String, device: String) async throws -> Approval {
        try DeviceKey.approve(job: job, action: action, device: device)
    }

    func approveConnection(action: String, device: String) async throws -> Approval {
        try DeviceKey.approveConnection(action: action, device: device)
    }

    func approveConfirm(item: String, title: String, device: String) async throws -> Approval {
        try DeviceKey.approveConfirm(item: item, title: title, device: device)
    }

    func approveRevoke(device: String, signer: String) async throws -> Approval {
        try DeviceKey.approveRevoke(device: device, signer: signer)
    }

    func approveInvite(device: String) async throws -> Approval {
        try DeviceKey.approveInvite(device: device)
    }
}

/// This Mac's publishing key with the runner (docs/runner-api.md, "POST /v1/devices"): a second key
/// to the same door as the iPhone's. It registers on every connection, which never adds a key the
/// runner doesn't know. A new key goes in only on the owner's click (`id(enrolling:)`: the first
/// Publish, Confirm or Disconnect, or Settings' Try again): on the runner on this Mac, Oby names it
/// in the runner's pairing; another runner's 403 line says how its owner does, in Settings ›
/// Runner, with Try again. With it, the owner's click on this Mac publishes, confirms or
/// disconnects on the runner.
@Observable
final class MacDevice {
    nonisolated enum State: Equatable, Sendable {
        /// Not asked yet, on this runner.
        case unregistered
        case registered
        /// The runner answered 403: its line says how to open pairing.
        case needsPairing(String)
        /// Anything else that went wrong, said as it came.
        case failed(String)
    }

    private(set) var state = State.unregistered
    @ObservationIgnored let key: any PublishingKey
    @ObservationIgnored private let given: String?
    @ObservationIgnored private let defaults: UserDefaults

    init(key: any PublishingKey = EnclaveKey(), name: String? = nil, defaults: UserDefaults = .standard) {
        self.key = key
        given = name
        self.defaults = defaults
    }

    /// What the runner's log calls this Mac: its name in Sharing settings, asked when it registers.
    var name: String { given ?? Host.current().localizedName ?? "Mac" }

    /// Settings › Runner's word for it.
    var status: String {
        guard key.isAvailable else { return "Needs a Secure Enclave" }
        switch state {
        case .unregistered: return "Not registered yet"
        case .registered: return "Registered"
        case .needsPairing, .failed: return "Not registered"
        }
    }

    /// Why it isn't registered, as the runner said it (a 403's one line), for Settings to show
    /// with Try again; nil otherwise.
    var problem: String? {
        switch state {
        case .needsPairing(let line), .failed(let line): line
        case .unregistered, .registered: nil
        }
    }

    /// The runner's pairing file when the runner is on this Mac (`$OBY_HOME/pairing`, RunnerFeed
    /// sets it): Oby names a new key there on the owner's click. Nil for another runner, whose
    /// owner names it there by hand.
    var pairing: URL?

    /// The id the runner gave this Mac's key, by the runner's address, as the iPhone keeps it.
    func id(on runner: Runner) -> String? { defaults.string(forKey: Self.idKey(runner)) }

    /// Another runner: what the last one said doesn't hold there.
    func reset() { state = .unregistered }

    /// Registers the key and returns its id, on every connection: the runner keeps a known key's
    /// id, and a reinstalled runner learns it anew. A key the runner doesn't know (403) isn't added
    /// here, where nobody clicked: the runner on this Mac waits for the first Publish, Confirm or
    /// Disconnect (`id(enrolling:)`); another runner's line is kept for Settings to show. The 403
    /// is thrown too.
    @discardableResult
    func register(with runner: Runner) async throws -> String {
        try await registering(on: runner) { try await runner.register(name: name, publicKey: try key.publicKey()) }
    }

    /// The id to sign with, on the owner's click: the one kept, else this Mac's key registered now,
    /// and added to the runner when it doesn't know it (`enroll`). Settings' Try again too.
    func id(enrolling runner: Runner) async throws -> String {
        if let id = id(on: runner) { return id }
        return try await registering(on: runner) { try await enroll(with: runner) }
    }

    private func registering(on runner: Runner, _ register: () async throws -> String) async throws -> String {
        guard key.isAvailable else { throw RunnerError(message: Self.noKey) }
        do {
            let id = try await register()
            defaults.set(id, forKey: Self.idKey(runner))
            state = .registered
            await revokeReplaced(on: runner, signer: id)
            return id
        } catch let error as RunnerError where error.status == 403 {
            defaults.removeObject(forKey: Self.idKey(runner))
            state = pairing == nil ? .needsPairing(error.message) : .unregistered
            throw error
        } catch {
            state = .failed(error.localizedDescription)
            throw error
        }
    }

    /// Adds this Mac's key to a runner, after the owner's click. A key it knows, or one its owner
    /// named in its pairing by hand, registers as it is. Else, on the runner on this Mac, Oby names
    /// this key in its pairing, which the runner uses up at once (and Oby removes, whatever
    /// happened).
    private func enroll(with runner: Runner) async throws -> String {
        let publicKey = try key.publicKey()
        do {
            return try await runner.register(name: name, publicKey: publicKey)
        } catch let refused as RunnerError where refused.status == 403 {
            guard let pairing else { throw refused }
            defer { try? FileManager.default.removeItem(at: pairing) }
            try RunnerLoginItem.writePrivately(Data(publicKey.base64EncodedString().utf8), to: pairing)
            return try await runner.register(name: name, publicKey: publicKey)
        }
    }

    /// The keys this one replaced (`replacedKeys`), revoked once it's registered, signed by it. Each
    /// is forgotten here only once the runner has revoked it, or doesn't know it (404): one that
    /// didn't go is tried again on the next connection.
    private func revokeReplaced(on runner: Runner, signer id: String) async {
        for replaced in Self.replacedKeys(runner) {
            guard let old = defaults.string(forKey: replaced) else { continue }
            guard old != id else {
                defaults.removeObject(forKey: replaced)
                continue
            }
            do {
                try await runner.revokeDevice(old, approval: try await key.approveRevoke(device: old, signer: id))
                defaults.removeObject(forKey: replaced)
            } catch let error as RunnerError where error.status == 404 {
                defaults.removeObject(forKey: replaced)
            } catch {
                Logger.feed.error("revoking this Mac's old publishing key failed: \(error.localizedDescription, privacy: .public)")
            }
        }
    }

    /// Publishes a runner's job with exactly `action`, the runner's own sentence the user saw: the
    /// key signs it, then the runner checks the signature and pushes.
    func publish(_ job: String, action: String, with runner: Runner) async throws -> URL {
        let device = try await id(enrolling: runner)
        let approval = try await key.approve(job: job, action: action, device: device)
        return try await runner.publish(job, approval: approval)
    }

    /// Removes the runner's connection `name` (Settings › Connectors): the runner's own sentence,
    /// signed as a publish is.
    /// The `oby-connection` signature also approves adding a connection, so this Mac signs only
    /// the sentence that removes this one (`removal`): anything else is refused, unsigned.
    func disconnect(_ name: String, with runner: Runner) async throws {
        let device = try await id(enrolling: runner)
        let action = try await runner.disconnectAction(name)
        guard action == Self.removal(name) else {
            throw RunnerError(message: "The runner asked this Mac to approve something other than disconnecting \(name), so nothing was signed.")
        }
        let approval = try await key.approveConnection(action: action, device: device)
        try await runner.disconnect(name, approval: approval)
    }

    /// The owner's Confirm of inbox item `item` (#776): the key signs its title, then the runner
    /// checks the signature and carries it out. A Mac with no Secure Enclave can't confirm.
    func confirm(_ item: String, title: String, with runner: Runner) async throws -> InboxItem {
        guard key.isAvailable else { throw RunnerError(message: Self.noKey) }
        let device = try await id(enrolling: runner)
        let approval = try await key.approveConfirm(item: item, title: title, device: device)
        return try await runner.settle(item, answer: "Confirm", approval: approval)
    }

    /// The owner's click on Pair a device: this Mac's key signs the invitation, so the phone it
    /// pairs may add its own key (docs/runner-api.md, "Invitations"). Without it a phone pairs but
    /// approves nothing.
    func approveInvite(with runner: Runner) async throws -> Approval {
        try await key.approveInvite(device: try await id(enrolling: runner))
    }

    /// What a publish or a Confirm says on a Mac with no Secure Enclave to keep its key (#776):
    /// none that runs macOS 26, which needs Apple silicon or a T2 chip.
    nonisolated static let noKey = "This Mac has no Secure Enclave to sign with: approve it on your iPhone."

    /// The runner's sentence for removing connection `name` (Runner/Connections.swift, `Connection.removal`).
    nonisolated static func removal(_ name: String) -> String { "Disconnect \(name)." }

    /// Signs the words the owner types on this Mac for `runner` with this Mac's key, as its id there
    /// is when the request goes (`DeviceKey.signWords`): nothing before the runner knows the key.
    nonisolated static func signsWords(for runner: Runner) -> Runner.SignsWords {
        let key = idKey(runner)
        return { method, path, body in
            guard DeviceKey.isAvailable, let device = UserDefaults.standard.string(forKey: key) else { return nil }
            return try? DeviceKey.signWords(method: method, path: path, body: body, device: device)
        }
    }

    /// The id of the key that signs without asking (the owner's decision of 2026-10-24).
    nonisolated private static func idKey(_ runner: Runner) -> String { "device.approval.\(runner.base.host() ?? runner.base.absoluteString)" }
    /// The ids of the keys `idKey`'s replaced, revoked once the new one is added (`enroll`): Touch ID
    /// only (`device.<host>`, until 2026-10-23), then Touch ID or the password (`device.presence.<host>`,
    /// until 2026-10-24).
    private static func replacedKeys(_ runner: Runner) -> [String] {
        let host = runner.base.host() ?? runner.base.absoluteString
        return ["device.\(host)", "device.presence.\(host)"]
    }
}
