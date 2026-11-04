// The iPhone and Apple Watch's client following obid, compiled from those apps' own client code:
// tests/swift_client.rs builds this file with apps/shared/Runner.swift and what it needs, and runs
// it against a running obid. Nothing here talks HTTP itself: every request is the apps' `Runner`,
// every answer is read into their own types. What it saw goes to stdout, one line each; the test
// reads them.
//
// OBID_URL is the test listener's address; OBID_HOME is its home, holding the API token file.

import CryptoKit
import Foundation

func say(_ line: String) {
    FileHandle.standardOutput.write(Data((line + "\n").utf8))
}

/// An endpoint obid may not serve: its status, or what failed.
func answered(_ call: () async throws -> Void) async -> String {
    do {
        try await call()
        return "200"
    } catch let error as RunnerError {
        return error.status.map(String.init) ?? error.message
    } catch {
        return "\(error)"
    }
}

let environment = ProcessInfo.processInfo.environment
let home = URL(filePath: environment["OBID_HOME"] ?? "")
guard let token = (try? String(contentsOf: home.appending(path: "api-token"), encoding: .utf8))?.trimmingCharacters(in: .whitespacesAndNewlines),
      let url = URL(string: environment["OBID_URL"] ?? ""), url.host() != nil else {
    say("no token or url")
    exit(1)
}
let runner = Runner(base: url, token: token)

var following: String?
do {
    // Following first, then fetching, as the apps do.
    for try await event in runner.events() {
        switch event {
        case .connected:
            let status = try await runner.status()
            say("status host=\(status.runner.host) version=\(status.runner.version) accounts=\(status.accounts.map(\.id).joined(separator: ",")) clearedAt=\(status.clearedAt.map { _ in "set" } ?? "null") mode=\(status.permissions?.mode ?? "none") cleanup=\(status.cleanup != nil)")
            let list = try await runner.jobs()
            say("jobs \(list.map { "\($0.id):\($0.status.rawValue):\($0.title ?? "")" }.joined(separator: ","))")
            say("inbox \(try await runner.inbox().count)")
            let turns = try await runner.conversation(before: nil, limit: 50)
            say("conversation \(turns.count)")
            // ASK-21: a turn with a question's card, as obid writes it (ask_an_oby's ask_21 holds
            // this file to that), read as the phones read it: its words, whole.
            let carded = URL(fileURLWithPath: #filePath).deletingLastPathComponent().deletingLastPathComponent().appending(path: "fixtures/carded-turn.json")
            let old = try Runner.decoder.decode(RunnerTurn.self, from: Data(contentsOf: carded))
            say("carded \(old.turn.text.hasPrefix("> Billing retries.") && old.turn.text.hasSuffix("wasn't bothered.")) \(old.turn.who.rawValue)")
            if let first = list.first {
                let one = try await runner.job(first.id)
                say("job \(one.id) \(one.status.rawValue) heading=\(one.heading)")
            }
            // A device's approval key, as DeviceKey.publicKey() gives it (X9.63), made here: no
            // Secure Enclave. A key obid doesn't know yet is named in its pairing file first.
            let deviceKey = P256.Signing.PrivateKey().publicKey.x963Representation
            try? Data(deviceKey.base64EncodedString().utf8).write(to: home.appending(path: "pairing"))
            say("devices \(await answered { _ = try await runner.register(name: "Test iPhone", publicKey: deviceKey) })")
            let cleared = try await runner.clear()
            say("cleared \(cleared.formatted(.iso8601))")
            say("ready")
        case .cleared(let date):
            say("event clear \(date.formatted(.iso8601))")
        case .job(let job):
            say("event job \(job.id) \(job.status.rawValue)")
            if following == nil, job.status == .running {
                following = job.id
                let stopped = try await runner.stopAll()
                say("stop-all \(stopped.stopped) \(stopped.jobs.map(\.id).joined(separator: ","))")
            }
            if job.id == following, job.status == .stopped {
                say("done")
                exit(0)
            }
        default:
            say("event other")
        }
    }
    say("events ended")
    exit(1)
} catch {
    say("failed \(error)")
    exit(1)
}
