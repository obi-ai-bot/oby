import Foundation
import LocalAuthentication
import Observation
import OSLog

/// What signs a publish on this Mac: its Secure Enclave key (apps/shared/DeviceKey.swift), or a stand-in in tests.
nonisolated protocol PublishingKey: Sendable {
    var isAvailable: Bool { get }
    /// The public half, X9.63, to register with the runner. It doesn't ask for anything.
    func publicKey() throws -> Data
    /// Asks for Touch ID or the Mac's password with the action as the reason, then signs it; throws
    /// when the user cancels or it fails, so nothing is signed.
    func approve(job: String, action: String, device: String) async throws -> Approval
    /// The same for a change to the runner's connections (`Approval.connectionMessage`).
    func approveConnection(action: String, device: String) async throws -> Approval
    /// The same for the owner's Confirm of an inbox item, its title the reason (`Approval.confirmMessage`, #776).
    func approveConfirm(item: String, title: String, device: String) async throws -> Approval
    /// The same for revoking device `device`, the key this one replaced; this key is device `signer`
    /// (`Approval.revokeMessage`).
    func approveRevoke(device: String, signer: String) async throws -> Approval
}

/// The Mac's Secure Enclave key, behind Touch ID or the Mac's password.
nonisolated struct TouchIDKey: PublishingKey {
    /// Makes the context that asks; a test's stands in for macOS's prompt.
    var context: @Sendable () -> LAContext = { LAContext() }

    var isAvailable: Bool { DeviceKey.isAvailable }

    /// Touch ID, with the Mac's password beside it; the password alone where Touch ID can't be used
    /// (no sensor, or a closed laptop). The key accepts either (`.userPresence`), so a Touch ID-only
    /// prompt would guard nothing: five wrong fingers lock Touch ID and macOS asks for the password.
    static let policy = LAPolicy.deviceOwnerAuthentication

    /// Whether Touch ID can approve right now, for the button's words.
    static var touchID: Bool { LAContext().canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: nil) }

    /// "Publish with Touch ID", or "with your password" where Touch ID can't be used.
    static var publish: String { touchID ? "Publish with Touch ID" : "Publish with your password" }

    func publicKey() throws -> Data { try DeviceKey.publicKey() }

    func approve(job: String, action: String, device: String) async throws -> Approval {
        try await asked(action, refused: RunnerError(message: "Not published: it wasn't approved on this Mac.")) {
            try DeviceKey.approve(job: job, action: action, device: device, context: $0)
        }
    }

    func approveConnection(action: String, device: String) async throws -> Approval {
        try await asked(action, refused: RunnerError(message: "Nothing changed: it wasn't approved on this Mac.")) {
            try DeviceKey.approveConnection(action: action, device: device, context: $0)
        }
    }

    func approveConfirm(item: String, title: String, device: String) async throws -> Approval {
        try await asked(title, refused: ConfirmCancelled()) {
            try DeviceKey.approveConfirm(item: item, title: title, device: device, context: $0)
        }
    }

    func approveRevoke(device: String, signer: String) async throws -> Approval {
        try await asked(MacDevice.revoking, refused: RunnerError(message: "This Mac's old publishing key wasn't removed: it wasn't approved on this Mac.")) {
            try DeviceKey.approveRevoke(device: device, signer: signer, context: $0)
        }
    }

    /// Asks first, so a cancel throws `refused`; the key then signs with that same answer, without asking again.
    private func asked(_ action: String, refused: some Error,
                       sign: @escaping @Sendable (LAContext) throws -> Approval) async throws -> Approval {
        let reason = Self.reason(action)
        return try await Task.detached { [context] in
            let asking = context()
            guard (try? await asking.evaluatePolicy(Self.policy, localizedReason: reason)) == true else { throw refused }
            // The key signs with that answer or not at all: no second prompt without the runner's sentence.
            asking.interactionNotAllowed = true
            return try sign(asking)
        }.value
    }

    /// macOS shows "Oby is trying to <reason>.", so the sentence goes in lowercase, without its
    /// period, as for this Mac's own jobs (Publish.touchID).
    static func reason(_ action: String) -> String {
        action.prefix(1).lowercased() + action.dropFirst().dropLast(action.hasSuffix(".") ? 1 : 0)
    }
}

/// This Mac's publishing key with the runner (docs/runner-api.md, "POST /v1/devices"): a second key
/// to the same door as the iPhone's. It registers on every connection, which never adds a key the
/// runner doesn't know. A new key goes in only on the owner's click (`id(enrolling:)`: the first
/// Publish, Confirm or Disconnect, or Settings' Try again): on the runner on this Mac, Oby names it
/// in the runner's pairing; another runner's 403 line says how its owner does, in Settings ›
/// Runner, with Try again. With it, a runner's job publishes from here.
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

    init(key: any PublishingKey = TouchIDKey(), name: String? = nil, defaults: UserDefaults = .standard) {
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
        case .registered: return "Registered, Touch ID or password"
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
    /// happened); the key it replaced (`replacedKey`, #1375) is then revoked, signed by this one.
    private func enroll(with runner: Runner) async throws -> String {
        let publicKey = try key.publicKey()
        do {
            return try await runner.register(name: name, publicKey: publicKey)
        } catch let refused as RunnerError where refused.status == 403 {
            guard let pairing else { throw refused }
            defer { try? FileManager.default.removeItem(at: pairing) }
            try RunnerLoginItem.writePrivately(Data(publicKey.base64EncodedString().utf8), to: pairing)
            let id = try await runner.register(name: name, publicKey: publicKey)
            if let old = defaults.string(forKey: Self.replacedKey(runner)), old != id {
                defaults.removeObject(forKey: Self.replacedKey(runner))
                do { try await runner.revokeDevice(old, approval: try await key.approveRevoke(device: old, signer: id)) } catch {
                    Logger.feed.error("revoking this Mac's old publishing key failed: \(error.localizedDescription, privacy: .public)")
                }
            }
            return id
        }
    }

    /// What the prompt says when this Mac's new key revokes the one it replaced.
    nonisolated static let revoking = "Remove this Mac's old publishing key."

    /// Publishes a runner's job with exactly `action`, the runner's own sentence the user saw:
    /// Touch ID or the password signs it, then the runner checks the signature and pushes. A cancel
    /// throws before anything is sent.
    func publish(_ job: String, action: String, with runner: Runner) async throws -> URL {
        let device = try await id(enrolling: runner)
        let approval = try await key.approve(job: job, action: action, device: device)
        return try await runner.publish(job, approval: approval)
    }

    /// Removes the runner's connection `name` (Settings › Connectors): the runner's own sentence,
    /// signed after Touch ID or the password, as a publish is. A cancel throws before anything changes.
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

    /// The owner's Confirm of inbox item `item` (#776): Touch ID or the password with its title as
    /// the reason, then the runner checks the signature and carries it out. A cancel throws
    /// `ConfirmCancelled` before anything is sent; a Mac with no Secure Enclave can't confirm.
    func confirm(_ item: String, title: String, with runner: Runner) async throws -> InboxItem {
        guard key.isAvailable else { throw RunnerError(message: Self.noKey) }
        let device = try await id(enrolling: runner)
        let approval = try await key.approveConfirm(item: item, title: title, device: device)
        return try await runner.settle(item, answer: "Confirm", approval: approval)
    }

    /// What a publish or a Confirm says on a Mac with no Secure Enclave to keep its key (#776):
    /// none that runs macOS 26, which needs Apple silicon or a T2 chip.
    nonisolated static let noKey = "This Mac has no Secure Enclave to sign with: approve it on your iPhone."

    /// The runner's sentence for removing connection `name` (Runner/Connections.swift, `Connection.removal`).
    nonisolated static func removal(_ name: String) -> String { "Disconnect \(name)." }

    /// The id of the key that takes Touch ID or the password: a Touch ID-only key's, kept as
    /// `device.<host>` until 2026-10-23, doesn't sign any more, so that Mac registers its new key once.
    private static func idKey(_ runner: Runner) -> String { "device.presence.\(runner.base.host() ?? runner.base.absoluteString)" }
    /// The id of the key `idKey`'s replaced, revoked once the new one is added (`enroll`).
    private static func replacedKey(_ runner: Runner) -> String { "device.\(runner.base.host() ?? runner.base.absoluteString)" }
}

/// Touch ID or the password was cancelled on a Confirm: nothing was signed or sent, and the card stays as it was.
nonisolated struct ConfirmCancelled: Error {}
