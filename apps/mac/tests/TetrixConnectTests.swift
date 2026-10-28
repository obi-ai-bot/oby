import Foundation
import Testing
@testable import Oby

/// Tetrix connected once, on obid, for every agent (docs/runner-api.md, "obid only: Tetrix"): what
/// the Mac reads of it, the only sign-in pages it opens, and what a job's agent did with it.
struct TetrixConnectTests {
    /// The owner's own Tetrix, by its address: obid has no built-in one and no default.
    let own = TetrixStatus.Server(name: "tetrix.example.com", url: URL(string: "https://tetrix.example.com/mcp")!)

    /// The contract's own example, each state, a runner that leaves keys out, and one with nothing
    /// entered yet (no server, nothing broken: it says to enter one).
    @Test func decodesTheStatus() throws {
        let on = try Runner.decoder.decode(TetrixStatus.self, from: Data(#"""
            {"connected": true, "url": "https://tetrix.example.com/mcp", "preset": null, "presets": [],
             "scopes": ["graph:read", "sessions:write", "hitl:write", "offline_access"],
             "connectedAt": "2026-10-18T14:02:11Z", "signIn": null, "error": null}
            """#.utf8))
        #expect(on.state == .connected(own) && on.how == "Signed in to tetrix.example.com. Every agent on every account uses it.")
        let ready = try Runner.decoder.decode(TetrixStatus.self, from: Data(#"{"connected": false, "url": null, "preset": null, "presets": []}"#.utf8))
        #expect(ready.state == .available && ready.server == nil && ready.error == nil)
        #expect(ready.how == TetrixStatus.notConfigured && ready.how.contains("Enter your Tetrix's MCP address"))
        let signing = try Runner.decoder.decode(TetrixStatus.self, from: Data(#"""
            {"connected": false, "url": "https://tetrix.example.com/mcp", "preset": null, "presets": [],
             "signIn": {"url": "https://auth.example.com/authorize?state=x", "expiresAt": "2026-10-18T22:00:00Z"}}
            """#.utf8))
        #expect(signing.state == .signingIn(own))
        let signedOut = try Runner.decoder.decode(TetrixStatus.self, from: Data(#"{"connected": false, "url": "https://tetrix.example.com/mcp", "error": "Tetrix signed Oby out: sign in again."}"#.utf8))
        #expect(signedOut.state == .available && signedOut.how == "Tetrix signed Oby out: sign in again.")
        #expect(try Runner.decoder.decode(TetrixStatus.self, from: Data("{}".utf8)) == TetrixStatus())
        // An older obid that still names a built-in server: the keys are read past, and only `url` counts.
        let older = try Runner.decoder.decode(TetrixStatus.self, from: Data(#"{"connected": false, "url": null, "preset": "x", "presets": [{"id": "x", "name": "X", "url": "https://x.example.com/mcp"}]}"#.utf8))
        #expect(older == TetrixStatus() && older.server == nil)
    }

    /// The Mac opens a sign-in page only over https with no login in it, on the host of the
    /// authorization server obid names (its issuer, which can be on another domain); none named, none opened.
    @Test func opensOnlyTheSignInServersPage() {
        let page = { (text: String, server: String?) in TetrixSignIn(url: URL(string: text)!, server: server.flatMap(URL.init(string:))).opens }
        #expect(page("https://auth.example.com/oidc/auth?client_id=obid", "https://auth.example.com/oidc"))
        #expect(page("https://AUTH.example.com/a", "https://auth.example.com"))
        // A sign-in server on the Tetrix's own host, under a path (a Keycloak realm).
        #expect(page("https://tetrix.example.com/auth/realms/acme/protocol/openid-connect/auth", "https://tetrix.example.com/auth/realms/acme"))
        #expect(!page("https://tetrix.example.com/auth", nil))
        #expect(!page("https://evil.example.com/authorize", "https://auth.example.com"))
        #expect(!page("https://auth.example.com.evil.dev/authorize", "https://auth.example.com"))
        #expect(!page("https://auth.example.com/a", "http://auth.example.com"))
        // The port too: both none, or the same.
        #expect(page("https://auth.example.com:8443/a", "https://auth.example.com:8443/realm"))
        #expect(!page("https://auth.example.com:8444/a", "https://auth.example.com:8443"))
        #expect(!page("https://auth.example.com:8443/a", "https://auth.example.com"))
        #expect(!page("http://auth.example.com/authorize", nil))
        #expect(!page("https://user:pass@auth.example.com/authorize", nil))
        #expect(!page("file:///etc/passwd", nil) && !page("javascript:alert(1)", nil))
        #expect(TetrixSignIn(url: URL(string: "https://Auth.Example.com/a")!).host == "auth.example.com")
    }

    /// Another Tetrix takes what obid takes (https, a host, no login, query or fragment), named by its host.
    @Test func anotherTetrixsAddress() {
        #expect(TetrixStatus.isServerURL("https://tetrix.example.com/mcp") && TetrixStatus.isServerURL("  https://t.example.com  "))
        for bad in ["", "https://", "http://tetrix.example.com/mcp", "tetrix.example.com", "https://me:secret@tetrix.example.com/mcp",
                    "https://tetrix.example.com/mcp?token=x", "https://tetrix.example.com/mcp#a"] {
            #expect(!TetrixStatus.isServerURL(bad), "\(bad)")
        }
        #expect(TetrixStatus.custom("https://tetrix.example.com/mcp") == own)
        #expect(TetrixStatus.custom("http://tetrix.example.com/mcp") == nil && TetrixStatus.custom("") == nil)
    }

    /// A job's `tetrix` as obid fills it, read with the job's other extras, never failing the job; an
    /// older runner's stamped sessions stand for `saved`.
    @Test func readsWhatAJobDidWithTetrix() throws {
        let jobs = #"""
            [{"id": "j1", "tetrix": {"connected": true, "calls": {"search_entities": 2, "get_entity": 1, "save_session": 1}, "rounds": null,
              "reminded": null, "stamps": {"commits": 2, "stamped": 2, "sessions": ["8f2a"]}, "author": null, "context": true, "saved": ["8f2a"],
              "gaps": {"noResults": ["acme/shop#12"], "missing": ["repo:acme/legacy"], "unknownFiles": ["src/new.rs"],
                       "failed": [{"tool": "code_impact", "error": "Tetrix said: timeout"}],
                       "sent": "2026-10-18T14:40:02Z", "session": "c41d", "notSent": null}}},
             {"id": "j2", "tetrix": {"connected": true, "calls": {}, "stamps": {"commits": 1, "stamped": 1, "sessions": ["old"]}}},
             {"id": "j3", "tetrix": "not an object"},
             {"id": "j4", "tetrix": {"calls": {"get_entity": 1}, "gaps": {"noResults": [], "notSent": "The secret scan found something."}}},
             {"id": "j5", "tetrix": {"connected": true, "calls": {}, "unreachable": "Tetrix wasn't reachable, so this job ran without the graph: x."}}]
            """#
        let extras = RunnerJobExtras.of(Data(jobs.utf8))
        let report = try #require(extras["j1"]?.tetrix)
        #expect(report.callsLine == "Asked Tetrix 4 times: search_entities 2, get_entity 1, save_session 1")
        #expect(report.context == true && report.saved == ["8f2a"])
        #expect(report.gaps?.lines == ["No results: acme/shop#12", "Not in the graph: repo:acme/legacy", "A file it didn't know: src/new.rs",
                                       "code_impact failed: Tetrix said: timeout"])
        #expect(report.gaps?.sentLine == "Sent to Tetrix as session c41d")
        #expect(extras["j2"]?.tetrix?.saved == ["old"] && extras["j2"]?.tetrix?.callsLine == "Had Tetrix, but didn't ask it anything")
        #expect(extras["j3"]?.tetrix == TetrixJobReport() && extras["j3"]?.tetrix?.isEmpty == true)
        let notSent = try #require(extras["j4"]?.tetrix)
        #expect(notSent.callsLine == "Asked Tetrix once: get_entity 1" && notSent.gaps?.isEmpty == true)
        #expect(notSent.gaps?.sentLine == "Not sent to Tetrix: The secret scan found something.")
        #expect(TetrixJobReport().isEmpty && !report.isEmpty)
        // Losing Tetrix is never silent: it's a section of its own.
        let lost = try #require(extras["j5"]?.tetrix)
        #expect(lost.unreachable == "Tetrix wasn't reachable, so this job ran without the graph: x." && !lost.isEmpty)
        #expect(report.unreachable == nil)
    }

    /// obid's `tetrix` event keeps the Mac's status current (a Disconnect elsewhere, a sign-in ending).
    @Test func followsTheTetrixEvent() {
        var parser = EventParser()
        var events: [RunnerEvent] = []
        for line in ["event: tetrix", #"data: {"connected": false, "url": "https://tetrix.example.com/mcp", "preset": null, "presets": []}"#, ""] {
            if let event = parser.feed(line) { events.append(event) }
        }
        #expect(events == [.tetrix(TetrixStatus(url: own.url))])
    }

    /// Each account's line once obid holds Tetrix: how many of its jobs used it, or where to connect it.
    @Test func accountLine() {
        #expect(TetrixJobReport.accountLine(connected: true, jobs: 12, used: 9, there: "in Connectors") == "Tetrix: used in 9 of 12 jobs")
        #expect(TetrixJobReport.accountLine(connected: true, jobs: 1, used: 0, there: "in Connectors") == "Tetrix: used in 0 of 1 job")
        #expect(TetrixJobReport.accountLine(connected: true, jobs: 0, used: 0, there: "in Connectors") == "Tetrix: connected")
        #expect(TetrixJobReport.accountLine(connected: false, jobs: 3, used: 3, there: "on your Mac") == "Tetrix: not connected: connect it on your Mac")
        // Settings › Accounts: under every signed-in account, the launch build's too (Tetrix is core).
        var account = AccountState(account: Account(id: "personal", agent: "claude", name: "Personal", payer: "", home: nil), label: nil,
                                   tetrix: true, tetrixJobs: 12, tetrixUsed: 9, runnerSignedIn: true)
        #expect(account.tetrixLine == "Tetrix: used in 9 of 12 jobs")
        account.runnerSignedIn = false
        #expect(account.tetrixLine == nil)
    }

    /// The fake obid: the Mac asks for the status (nothing entered yet), starts a sign-in to the
    /// Tetrix the owner typed, by its address and nothing else, cancels and disconnects; a runner
    /// without Tetrix (404) is nil.
    @Test func talksToObid() async throws {
        let obid = RunnerStub { request in
            switch (request.method, request.path) {
            case ("GET", "/v1/tetrix"): (200, #"{"connected": false, "url": null, "preset": null, "presets": []}"#)
            case ("POST", "/v1/tetrix/sign-in"):
                (200, #"{"url": "https://auth.example.com/authorize?state=s", "expiresAt": "2026-10-18T22:00:00Z", "server": "https://auth.example.com"}"#)
            case ("DELETE", "/v1/tetrix/sign-in"), ("DELETE", "/v1/tetrix"): (204, "")
            default: (404, "")
            }
        }
        let status = try #require(try await obid.runner.tetrix())
        #expect(status.state == .available && status.server == nil)
        let typed = try #require(TetrixStatus.custom("  https://tetrix.example.com/mcp "))
        let signIn = try await obid.runner.startTetrixSignIn(typed)
        #expect(signIn.opens && signIn.server?.host() == "auth.example.com")
        #expect(obid.requests.last?.json as? [String: String] == ["url": "https://tetrix.example.com/mcp"])
        try await obid.runner.cancelTetrixSignIn()
        try await obid.runner.disconnectTetrix()
        #expect(obid.requests.suffix(2).map { "\($0.method) \($0.path)" } == ["DELETE /v1/tetrix/sign-in", "DELETE /v1/tetrix"])
        #expect(try await RunnerStub { _ in (404, "") }.runner.tetrix() == nil)
    }
}
