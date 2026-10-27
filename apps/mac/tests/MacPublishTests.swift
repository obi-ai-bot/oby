import CryptoKit
import Foundation
import Synchronization
import Testing
@testable import Oby

/// Publishing a runner's job from the Mac (#152): this Mac's key registers with the runner, the
/// runner's sentence is what the owner's click approves (no Touch ID or password, the owner's
/// decision of 2026-10-24), and nothing goes out without a signature.
@MainActor
struct MacPublishTests {
    /// The runner's sentence, as it says it: shown and signed exactly so.
    let sentence = "Push oby/fix-152 to deskree-inc/oby and open a pull request into dev."

    /// The bytes this Mac's key signs must be exactly what the runner checks (Runner/Devices.swift).
    @Test func approvalMessage() {
        let message = Approval.message(job: "ab12cd34", action: sentence, signedAt: "2026-10-11T09:30:00Z")
        #expect(String(decoding: message, as: UTF8.self)
                == "oby-publish\nab12cd34\nPush oby/fix-152 to deskree-inc/oby and open a pull request into dev.\n2026-10-11T09:30:00Z")
    }

    /// Each connection registers again, with this Mac's name and its key; the runner's id is kept.
    @Test func registersOnEachConnection() async throws {
        let stub = RunnerStub { _ in (200, #"{"id": "d7c1"}"#) }
        let key = FakeKey()
        let device = MacDevice(key: key, name: "Wellington's MacBook Air", defaults: stub.defaults)
        #expect(try await device.register(with: stub.runner) == "d7c1")
        #expect(try await device.register(with: stub.runner) == "d7c1")
        #expect(device.state == .registered && device.status == "Registered" && device.problem == nil)
        #expect(device.id(on: stub.runner) == "d7c1")
        let sent = stub.requests
        #expect(sent.map(\.method) == ["POST", "POST"] && sent.allSatisfy { $0.path == "/v1/devices" })
        let body = try #require(sent.first?.json)
        #expect(body["name"] as? String == "Wellington's MacBook Air")
        #expect(body["publicKey"] as? String == key.signing.publicKey.x963Representation.base64EncodedString())
    }

    /// Pairing closed: the runner's 403 line is kept for Settings › Runner, not swallowed.
    @Test func pairingClosedIsShown() async throws {
        let line = "To add this phone, run `touch /var/lib/oby/pairing` on the runner, then try again within 10 minutes."
        let stub = RunnerStub { _ in (403, #"{"error": "\#(line)"}"#) }
        let device = MacDevice(key: FakeKey(), name: "Mac", defaults: stub.defaults)
        let error = await #expect(throws: RunnerError.self) { try await device.register(with: stub.runner) }
        #expect(error?.status == 403)
        #expect(device.state == .needsPairing(line) && device.problem == line && device.status == "Not registered")
        #expect(device.id(on: stub.runner) == nil)
    }

    /// Publish shows the runner's sentence unchanged, and that sentence is what's signed and sent.
    @Test func publishSignsTheRunnersSentence() async throws {
        let stub = RunnerStub { [sentence] request in
            switch (request.method, request.path) {
            case ("GET", "/v1/jobs/ab12cd34/publish"): (200, #"{"action": "\#(sentence)"}"#)
            case ("POST", "/v1/devices"): (200, #"{"id": "d7c1"}"#)
            case ("POST", "/v1/jobs/ab12cd34/publish"): (200, #"{"url": "https://github.com/deskree-inc/oby/pull/200"}"#)
            default: (404, #"{"error": "No."}"#)
            }
        }
        let key = FakeKey()
        let device = MacDevice(key: key, name: "Mac", defaults: stub.defaults)
        let action = try await stub.runner.publishAction("ab12cd34")
        #expect(action == sentence)
        let url = try await device.publish("ab12cd34", action: action, with: stub.runner)
        #expect(url.absoluteString == "https://github.com/deskree-inc/oby/pull/200")

        let publish = try #require(stub.requests.last { $0.method == "POST" && $0.path == "/v1/jobs/ab12cd34/publish" })
        let approval = try #require(publish.json)
        #expect(approval["device"] as? String == "d7c1" && approval["action"] as? String == sentence)
        // The runner's check, as Runner/Devices.swift makes it: the signature over those exact bytes.
        let signedAt = try #require(approval["signedAt"] as? String)
        let der = try #require((approval["signature"] as? String).flatMap { Data(base64Encoded: $0) })
        let signature = try P256.Signing.ECDSASignature(derRepresentation: der)
        #expect(key.signing.publicKey.isValidSignature(signature, for: Approval.message(job: "ab12cd34", action: sentence, signedAt: signedAt)))
    }

    /// A key that can't sign sends nothing: no publish reaches the runner.
    @Test func unsignedSendsNothing() async throws {
        let stub = RunnerStub { request in
            request.path == "/v1/devices" ? (200, #"{"id": "d7c1"}"#) : (200, #"{"url": "https://github.com/deskree-inc/oby/pull/200"}"#)
        }
        let device = MacDevice(key: FakeKey(fails: true), name: "Mac", defaults: stub.defaults)
        let error = await #expect(throws: RunnerError.self) { try await device.publish("ab12cd34", action: sentence, with: stub.runner) }
        #expect(error?.message == FakeKey.failure)
        #expect(!stub.requests.contains { $0.path.hasSuffix("/publish") })
    }

    /// A Mac that registered a Touch ID-only key (its id kept as `device.<host>`) or a Touch ID or
    /// password key (`device.presence.<host>`) registers its new key, which signs without asking,
    /// on its next publish, signs with the new id, and revokes the old ones. An old id is forgotten
    /// only once revoked (or unknown to the runner, 404): one whose revoke failed is tried again on
    /// the next connection.
    @Test func olderKeysRegisterAgain() async throws {
        let failing = Switch()
        let stub = RunnerStub { request in
            switch (request.method, request.path) {
            case ("POST", "/v1/devices"): (200, #"{"id": "n3w1"}"#)
            case ("DELETE", "/v1/devices/0ld1"): failing.on.withLock { $0 } ? (500, #"{"error": "Not now."}"#) : (200, #"{"revoked": "0ld1"}"#)
            case ("POST", "/v1/jobs/ab12cd34/publish"): (200, #"{"url": "https://github.com/deskree-inc/oby/pull/200"}"#)
            default: (404, #"{"error": "No."}"#)
            }
        }
        stub.defaults.set("0ld1", forKey: "device.\(stub.host)")
        stub.defaults.set("0ld2", forKey: "device.presence.\(stub.host)")
        let device = MacDevice(key: FakeKey(), name: "Mac", defaults: stub.defaults)
        _ = try await device.publish("ab12cd34", action: sentence, with: stub.runner)
        #expect(stub.requests.map { "\($0.method) \($0.path)" }
                == ["POST /v1/devices", "DELETE /v1/devices/0ld1", "DELETE /v1/devices/0ld2", "POST /v1/jobs/ab12cd34/publish"])
        #expect(stub.requests.last?.json?["device"] as? String == "n3w1" && device.id(on: stub.runner) == "n3w1")
        #expect(stub.defaults.string(forKey: "device.\(stub.host)") == "0ld1", "its revoke failed: kept to try again")
        #expect(stub.defaults.string(forKey: "device.presence.\(stub.host)") == nil, "the runner doesn't know it")

        failing.on.withLock { $0 = false }
        try await device.register(with: stub.runner)
        #expect(stub.requests.suffix(2).map { "\($0.method) \($0.path)" } == ["POST /v1/devices", "DELETE /v1/devices/0ld1"])
        #expect(stub.defaults.string(forKey: "device.\(stub.host)") == nil)
    }

    /// The runner on this Mac doesn't know this Mac's key: connecting adds nothing and names
    /// nothing in its pairing. The owner's first Publish names the key in the runner's pairing for
    /// that one registration, and signs with the id it gets.
    @Test func firstPublishAddsTheKey() async throws {
        let pairing = FileManager.default.temporaryDirectory.appending(path: "pairing-\(UUID().uuidString)")
        let key = FakeKey()
        let named = key.signing.publicKey.x963Representation.base64EncodedString()
        let stub = RunnerStub { request in
            switch (request.method, request.path) {
            case ("POST", "/v1/devices"):
                (try? String(contentsOf: pairing, encoding: .utf8)) == named ? (200, #"{"id": "d7c1"}"#) : (403, #"{"error": "To add this device, …"}"#)
            case ("POST", "/v1/jobs/ab12cd34/publish"): (200, #"{"url": "https://github.com/deskree-inc/oby/pull/200"}"#)
            default: (404, #"{"error": "No."}"#)
            }
        }
        let device = MacDevice(key: key, name: "Mac", defaults: stub.defaults)
        device.pairing = pairing
        await #expect(throws: RunnerError.self) { try await device.register(with: stub.runner) }
        #expect(device.state == .unregistered && device.problem == nil)
        #expect(!FileManager.default.fileExists(atPath: pairing.path))

        _ = try await device.publish("ab12cd34", action: sentence, with: stub.runner)
        #expect(!FileManager.default.fileExists(atPath: pairing.path), "the pairing doesn't stay open")
        #expect(stub.requests.map(\.path) == ["/v1/devices", "/v1/devices", "/v1/devices", "/v1/jobs/ab12cd34/publish"])
        #expect(stub.requests.last?.json?["device"] as? String == "d7c1" && device.state == .registered)
    }

    /// A Mac moving from its Touch ID-only key (#1375, its id kept as `device.<host>`) and its Touch
    /// ID or password key (`device.presence.<host>`) to a runner that doesn't know the new one: the
    /// new key goes in by the pairing, then revokes the old ones, asking for nothing.
    @Test func newKeyRevokesTheOneItReplaced() async throws {
        let pairing = FileManager.default.temporaryDirectory.appending(path: "pairing-\(UUID().uuidString)")
        let key = FakeKey()
        let named = key.signing.publicKey.x963Representation.base64EncodedString()
        let stub = RunnerStub { request in
            switch (request.method, request.path) {
            case ("POST", "/v1/devices"):
                return (try? String(contentsOf: pairing, encoding: .utf8)) == named ? (200, #"{"id": "n3w1"}"#) : (403, #"{"error": "No."}"#)
            case ("DELETE", let path) where path.hasPrefix("/v1/devices/0ld"):
                let old = String(path.dropFirst("/v1/devices/".count))
                guard let signed = request.json?["approval"] as? [String: String], signed["device"] == "n3w1",
                      let der = signed["signature"].flatMap({ Data(base64Encoded: $0) }),
                      let signature = try? P256.Signing.ECDSASignature(derRepresentation: der),
                      key.signing.publicKey.isValidSignature(signature, for: Approval.revokeMessage(device: old, signedAt: signed["signedAt"] ?? ""))
                else { return (403, #"{"error": "No."}"#) }
                return (200, #"{"revoked": "\#(old)"}"#)
            case ("POST", "/v1/jobs/ab12cd34/publish"): return (200, #"{"url": "https://github.com/deskree-inc/oby/pull/200"}"#)
            default: return (404, #"{"error": "No."}"#)
            }
        }
        stub.defaults.set("0ld1", forKey: "device.\(stub.host)")
        stub.defaults.set("0ld2", forKey: "device.presence.\(stub.host)")
        let device = MacDevice(key: key, name: "Mac", defaults: stub.defaults)
        device.pairing = pairing
        _ = try await device.publish("ab12cd34", action: sentence, with: stub.runner)
        #expect(stub.requests.map { "\($0.method) \($0.path)" }
                == ["POST /v1/devices", "POST /v1/devices", "DELETE /v1/devices/0ld1", "DELETE /v1/devices/0ld2", "POST /v1/jobs/ab12cd34/publish"])
        #expect(stub.requests.allSatisfy { $0.method != "DELETE" || ($0.json?["approval"] as? [String: String])?["device"] == "n3w1" })
        #expect(stub.requests.last?.json?["device"] as? String == "n3w1")
        #expect(stub.defaults.string(forKey: "device.\(stub.host)") == nil && stub.defaults.string(forKey: "device.presence.\(stub.host)") == nil)
    }

    /// The owner's Confirm (#776): the click signs the item's own title, over its id, and that's
    /// what goes with the answer.
    @Test func confirmSignsTheItemsTitle() async throws {
        let title = "Delete Vault tests (its worktree and local branch go)"
        let item = #"{"id": "i9", "kind": "confirm", "job": "ab12cd34", "title": "\#(title)", "text": "", "options": ["Confirm", "Cancel"], "created": "2026-10-22T09:00:00Z", "done": "2026-10-22T09:01:00Z", "later": null, "answer": "Confirm", "confirm": {"action": "delete_job", "target": "ab12cd34", "payload": null}}"#
        let stub = RunnerStub { request in
            request.path == "/v1/devices" ? (200, #"{"id": "d7c1"}"#) : (200, item)
        }
        let key = FakeKey()
        let device = MacDevice(key: key, name: "Mac", defaults: stub.defaults)
        let answered = try await device.confirm("i9", title: title, with: stub.runner)
        #expect(answered.id == "i9")
        let sent = try #require(stub.requests.last { $0.path == "/v1/inbox/i9/answer" }?.json)
        #expect(sent["answer"] as? String == "Confirm")
        let approval = try #require(sent["approval"] as? [String: Any])
        #expect(approval["device"] as? String == "d7c1" && approval["action"] as? String == title)
        let signedAt = try #require(approval["signedAt"] as? String)
        let der = try #require((approval["signature"] as? String).flatMap { Data(base64Encoded: $0) })
        let signature = try P256.Signing.ECDSASignature(derRepresentation: der)
        #expect(key.signing.publicKey.isValidSignature(signature, for: Approval.confirmMessage(item: "i9", title: title, signedAt: signedAt)))
        #expect(String(decoding: Approval.confirmMessage(item: "i9", title: "T", signedAt: "S"), as: UTF8.self) == "oby-confirm\ni9\nT\nS")
    }

    /// The owner's typed words go signed (`X-Oby-Words`, docs/runner-api.md): the runner's sentence
    /// over the method, the path and the body's exact bytes, so nothing else holding its token passes
    /// for them. A read carries none; a POST with no body signs an empty one.
    @Test func typedWordsAreSigned() async throws {
        #expect(String(decoding: Approval.wordsMessage(method: "POST", path: "/v1/plans", body: Data("{}".utf8), signedAt: "S"), as: UTF8.self)
                == "oby-words\nPOST\n/v1/plans\n44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a\nS")
        let stub = RunnerStub { _ in (200, "{}") }
        let key = FakeKey()
        var runner = stub.runner
        runner.signsWords = { method, path, body in
            let signature = try? key.signing.signature(for: Approval.wordsMessage(method: method, path: path, body: body, signedAt: "S"))
            return signature.map { "d7c1 S \($0.derRepresentation.base64EncodedString())" }
        }
        try await runner.send("POST", "v1/plans", body: ["said": "Fix the flaky test"])
        try await runner.send("GET", "v1/jobs?limit=50")
        try await runner.send("POST", "v1/memory/suggestions/s1/remember")
        let plan = try #require(stub.requests.first { $0.path == "/v1/plans" })
        let parts = try #require(plan.words?.split(separator: " ").map(String.init))
        #expect(parts.count == 3 && parts[0] == "d7c1" && parts[1] == "S")
        let der = try #require(Data(base64Encoded: parts[2]))
        let signature = try P256.Signing.ECDSASignature(derRepresentation: der)
        #expect(key.signing.publicKey.isValidSignature(signature, for: Approval.wordsMessage(method: "POST", path: "/v1/plans", body: plan.body, signedAt: "S")))
        #expect(stub.requests.first { $0.path == "/v1/jobs" }?.words == nil)
        // Remember has no body: its signature is over an empty one.
        let remember = try #require(stub.requests.first { $0.path == "/v1/memory/suggestions/s1/remember" })
        let words = try #require(remember.words?.split(separator: " ").map(String.init))
        let empty = try P256.Signing.ECDSASignature(derRepresentation: try #require(Data(base64Encoded: words[2])))
        #expect(key.signing.publicKey.isValidSignature(empty, for: Approval.wordsMessage(method: "POST", path: "/v1/memory/suggestions/s1/remember", body: Data(), signedAt: "S")))
    }

    /// Pair a device, the owner's click: this Mac's key signs the invitation (`oby-pair-invite`) as
    /// its registered device, so only the phone it pairs may add its own key.
    @Test func thePairingInvitationIsSignedByThisMac() async throws {
        let stub = RunnerStub { _ in (200, #"{"id": "d7c1"}"#) }
        let key = FakeKey()
        let device = MacDevice(key: key, name: "Mac", defaults: stub.defaults)
        let approval = try await device.approveInvite(with: stub.runner)
        #expect(approval.device == "d7c1")
        let der = try #require(Data(base64Encoded: approval.signature))
        let signature = try P256.Signing.ECDSASignature(derRepresentation: der)
        #expect(key.signing.publicKey.isValidSignature(signature, for: Approval.inviteMessage(signedAt: approval.signedAt)))
        #expect(String(decoding: Approval.inviteMessage(signedAt: "S"), as: UTF8.self) == "oby-pair-invite\nS")
    }

    /// A key that can't sign confirms nothing, so the card stays open with why.
    @Test func unsignedConfirmsNothing() async throws {
        let stub = RunnerStub { request in request.path == "/v1/devices" ? (200, #"{"id": "d7c1"}"#) : (500, #"{"error": "No."}"#) }
        let device = MacDevice(key: FakeKey(fails: true), name: "Mac", defaults: stub.defaults)
        await #expect(throws: RunnerError.self) { try await device.confirm("i9", title: "Delete it", with: stub.runner) }
        #expect(!stub.requests.contains { $0.path.hasSuffix("/answer") })
        // A Mac with no Secure Enclave says where to confirm, and sends nothing either.
        let none = MacDevice(key: FakeKey(isAvailable: false), name: "Mac", defaults: stub.defaults)
        let error = await #expect(throws: RunnerError.self) { try await none.confirm("i9", title: "Delete it", with: stub.runner) }
        #expect(error?.message == MacDevice.noKey)
    }

    /// A Mac without a Secure Enclave neither registers nor publishes, and says why.
    @Test func noSecureEnclaveNoKey() async {
        let stub = RunnerStub { _ in (200, #"{"id": "d7c1"}"#) }
        let device = MacDevice(key: FakeKey(isAvailable: false), name: "Mac", defaults: stub.defaults)
        await #expect(throws: RunnerError.self) { try await device.publish("ab12cd34", action: sentence, with: stub.runner) }
        #expect(stub.requests.isEmpty && device.status == "Needs a Secure Enclave")
    }

    /// A job older than the latest 50 (an inbox item's Open job, or an alert's Publish) is fetched
    /// by its id, so its screen and its Publish open; one already listed isn't asked for again.
    @Test func olderJobIsFetched() async throws {
        let job = #"{"account":"default","agent":"claude","base":"dev","branch":"oby/fix-152","commits":2,"created":"2026-10-01T09:00:00Z","finished":"2026-10-01T09:20:00Z","id":"0ld0b152","kind":"fix","publishError":"GitHub's rate limit","repo":"deskree-inc/oby","request":"fix oby #152","session":"s","status":"done","step":"","summary":"Done.","task":"Fix #152."}"#
        let stub = RunnerStub { request in request.path == "/v1/jobs/0ld0b152" ? (200, job) : (404, #"{"error": "No."}"#) }
        let feed = RunnerFeed()
        try await feed.fetch("0ld0b152", from: stub.runner)
        let fetched = try #require(feed.jobs.first { $0.id == "0ld0b152" })
        #expect(fetched.canPublish && fetched.publishError == "GitHub's rate limit")
        try await feed.fetch("0ld0b152", from: stub.runner)
        #expect(stub.requests.count == 1 && feed.jobs.count == 1)

        // Settings › Runner changes while a fetch is on its way: the old runner's job doesn't land
        // in the new runner's list, where its actions would go to the new runner.
        let gate = DispatchSemaphore(value: 0)
        let slow = RunnerStub { _ in gate.wait(); return (200, job.replacingOccurrences(of: "0ld0b152", with: "07he7152")) }
        let switching = RunnerFeed()
        let fetching = Task { try await switching.fetch("07he7152", from: slow.runner) }
        for _ in 0..<500 where slow.requests.isEmpty { try await Task.sleep(for: .milliseconds(10)) }
        #expect(slow.requests.count == 1)
        switching.forget()
        gate.signal()
        try await fetching.value
        #expect(switching.jobs.isEmpty)
    }
}

/// A stub's on or off, shared with its answers.
nonisolated final class Switch: Sendable {
    let on = Mutex(true)
}

/// Signs as the Mac's key does, with a software key, or fails as a key that can't sign.
nonisolated struct FakeKey: PublishingKey {
    var isAvailable = true
    var fails = false
    let signing = P256.Signing.PrivateKey()
    static let failure = "Couldn't sign."

    func publicKey() throws -> Data { signing.publicKey.x963Representation }

    func approve(job: String, action: String, device: String) async throws -> Approval {
        if fails { throw RunnerError(message: Self.failure) }
        let signedAt = ISO8601DateFormatter().string(from: .now)
        let signature = try signing.signature(for: Approval.message(job: job, action: action, signedAt: signedAt))
        return Approval(device: device, action: action, signedAt: signedAt, signature: signature.derRepresentation.base64EncodedString())
    }

    func approveConnection(action: String, device: String) async throws -> Approval {
        if fails { throw RunnerError(message: Self.failure) }
        let signedAt = ISO8601DateFormatter().string(from: .now)
        let signature = try signing.signature(for: Approval.connectionMessage(action: action, signedAt: signedAt))
        return Approval(device: device, action: action, signedAt: signedAt, signature: signature.derRepresentation.base64EncodedString())
    }

    func approveConfirm(item: String, title: String, device: String) async throws -> Approval {
        if fails { throw RunnerError(message: Self.failure) }
        let signedAt = ISO8601DateFormatter().string(from: .now)
        let signature = try signing.signature(for: Approval.confirmMessage(item: item, title: title, signedAt: signedAt))
        return Approval(device: device, action: title, signedAt: signedAt, signature: signature.derRepresentation.base64EncodedString())
    }

    func approveInvite(device: String) async throws -> Approval {
        if fails { throw RunnerError(message: Self.failure) }
        let signedAt = ISO8601DateFormatter().string(from: .now)
        let signature = try signing.signature(for: Approval.inviteMessage(signedAt: signedAt))
        return Approval(device: device, action: "", signedAt: signedAt, signature: signature.derRepresentation.base64EncodedString())
    }

    func approveRevoke(device: String, signer: String) async throws -> Approval {
        let signedAt = ISO8601DateFormatter().string(from: .now)
        let signature = try signing.signature(for: Approval.revokeMessage(device: device, signedAt: signedAt))
        return Approval(device: signer, action: device, signedAt: signedAt, signature: signature.derRepresentation.base64EncodedString())
    }
}

/// A runner on a host of its own, answered by `answer`, that keeps every request it got.
/// URLSession.shared asks RunnerStub, registered once, for every request: it takes only stubs' hosts.
nonisolated final class RunnerStub: Sendable {
    struct Request: Sendable {
        var method: String
        var path: String
        var body: Data
        var authorization: String? = nil
        /// The signature over words the owner typed (`X-Oby-Words`), when there is one.
        var words: String? = nil

        var json: [String: Any]? { (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] }
    }

    let host = "\(UUID().uuidString.lowercased()).runner.test"
    let answer: @Sendable (Request) -> (Int, String)
    private let got = Mutex<[Request]>([])

    var runner: Runner { Runner(base: URL(string: "https://\(host)")!) }
    var requests: [Request] { got.withLock { $0 } }
    /// Not the app's defaults: the device id is kept by the runner's host, each stub's its own.
    var defaults: UserDefaults { UserDefaults(suiteName: "com.deskree.oby.tests")! }

    init(answer: @escaping @Sendable (Request) -> (Int, String)) {
        self.answer = answer
        _ = Self.registered
        Self.stubs.withLock { $0[host] = self }
    }

    fileprivate func take(_ request: URLRequest) -> (Int, Data) {
        let body = request.httpBody ?? request.httpBodyStream.map(Self.read) ?? Data()
        let got = Request(method: request.httpMethod ?? "GET", path: request.url?.absoluteURL.path() ?? "", body: body,
                          authorization: request.value(forHTTPHeaderField: "Authorization"),
                          words: request.value(forHTTPHeaderField: "X-Oby-Words"))
        self.got.withLock { $0.append(got) }
        let (status, text) = answer(got)
        return (status, Data(text.utf8))
    }

    private static func read(_ stream: InputStream) -> Data {
        stream.open()
        defer { stream.close() }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        while stream.hasBytesAvailable {
            let count = stream.read(&buffer, maxLength: buffer.count)
            guard count > 0 else { break }
            data.append(buffer, count: count)
        }
        return data
    }

    fileprivate static let stubs = Mutex<[String: RunnerStub]>([:])
    private static let registered: Void = { URLProtocol.registerClass(RunnerStubProtocol.self) }()
}

nonisolated final class RunnerStubProtocol: URLProtocol {
    override class func canInit(with request: URLRequest) -> Bool {
        request.url?.host().map { host in RunnerStub.stubs.withLock { $0[host] != nil } } ?? false
    }

    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override func startLoading() {
        guard let url = request.url, let host = url.host(), let stub = RunnerStub.stubs.withLock({ $0[host] }) else {
            client?.urlProtocol(self, didFailWithError: URLError(.cannotFindHost))
            return
        }
        let (status, data) = stub.take(request)
        let response = HTTPURLResponse(url: url, statusCode: status, httpVersion: "HTTP/1.1", headerFields: ["Content-Type": "application/json"])!
        client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }

    override func stopLoading() {}
}
