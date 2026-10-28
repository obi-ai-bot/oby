import Foundation

/// Tetrix, connected once on obid for every agent of every account (docs/runner-api.md, "obid only:
/// Tetrix"). obid is Tetrix's OAuth client and holds the login: the apps only learn whether it's
/// connected and to which server, and the Mac opens Tetrix's own sign-in page in the browser. No
/// token ever reaches them. Not on the gateway: the iPhone sees it through accounts and jobs.
nonisolated struct TetrixStatus: Decodable, Equatable, Sendable {
    /// The Tetrix obid signs in to: the MCP address the owner typed, named by its host. Oby has
    /// no built-in Tetrix and no default: until the owner enters theirs, Oby runs without one.
    struct Server: Equatable, Sendable, Identifiable {
        var name: String
        var url: URL
        var id: URL { url }
    }

    /// What it does, as Sangam's Connectors says it.
    static let about = "Project knowledge and blast radius"
    /// The row's line while no Tetrix has been entered yet.
    static let notConfigured = "Not connected. Enter your Tetrix's MCP address: every agent on every account uses it, and you sign in once, on Tetrix's own page."

    var connected: Bool
    /// The MCP server obid signed in to, or last tried; nil until the owner enters one.
    var url: URL?
    /// When the sign-in finished, or nil.
    var connectedAt: Date?
    /// The sign-in under way, or nil.
    var signIn: TetrixSignIn?
    /// Why the last sign-in or refresh failed, in obid's words, or nil.
    var error: String?

    private enum Keys: String, CodingKey { case connected, url, connectedAt, signIn, error }

    init(connected: Bool = false, url: URL? = nil, connectedAt: Date? = nil, signIn: TetrixSignIn? = nil, error: String? = nil) {
        (self.connected, self.url, self.connectedAt, self.signIn, self.error) = (connected, url, connectedAt, signIn, error)
    }

    init(from decoder: any Decoder) throws {
        let keys = try decoder.container(keyedBy: Keys.self)
        connected = (try? keys.decodeIfPresent(Bool.self, forKey: .connected)) ?? false
        url = try? keys.decodeIfPresent(URL.self, forKey: .url)
        connectedAt = try? keys.decodeIfPresent(Date.self, forKey: .connectedAt)
        signIn = try? keys.decodeIfPresent(TetrixSignIn.self, forKey: .signIn)
        error = try? keys.decodeIfPresent(String.self, forKey: .error)
    }

    /// The server it's signed in to or signing in to: the owner's address, by its host.
    var server: Server? {
        url.map { Server(name: $0.host() ?? "Tetrix", url: $0) }
    }

    /// What the connector row shows (Sangam's Connectors: Connected, or Connect).
    enum State: Equatable, Sendable {
        case connected(Server)
        case signingIn(Server)
        case available
    }

    var state: State {
        if connected, let server { return .connected(server) }
        if signIn != nil, let server { return .signingIn(server) }
        return .available
    }

    /// "Signed in to tetrix.example.com. Every agent on every account uses it.", or what's under
    /// way. A refresh obid couldn't do says so while it's still signed in. With no Tetrix entered
    /// yet, it says to enter one: nothing is wrong, Oby just runs without Tetrix until then.
    var how: String {
        switch state {
        case .connected(let server): error ?? "Signed in to \(server.name). Every agent on every account uses it."
        case .signingIn(let server): "Signing in to \(server.name) in your browser…"
        case .available: error ?? Self.notConfigured
        }
    }

    /// Whether `text` is an address obid takes (docs/runner-api.md, Connection): https, with a host,
    /// no login, query or fragment.
    static func isServerURL(_ text: String) -> Bool {
        guard let url = URL(string: text.trimmingCharacters(in: .whitespaces)) else { return false }
        return url.scheme?.lowercased() == "https" && url.host()?.isEmpty == false && url.user() == nil && url.password() == nil
            && url.query() == nil && url.fragment() == nil
    }

    /// The owner's Tetrix as typed, named by its host so the row says where it is; nil for text
    /// that isn't an address obid takes.
    static func custom(_ text: String) -> Server? {
        guard isServerURL(text), let url = URL(string: text.trimmingCharacters(in: .whitespaces)), let host = url.host() else { return nil }
        return Server(name: host, url: url)
    }
}

/// Tetrix's sign-in under way on obid (`POST /v1/tetrix/sign-in`): the authorization server's page
/// to open in the browser, which hands the login back to obid's loopback page, never to the app.
/// `server` is the authorization server's issuer, as obid found it in the MCP server's metadata (it
/// can be on another domain: Logto, Keycloak); obid answers only pages on its host.
nonisolated struct TetrixSignIn: Codable, Equatable, Sendable {
    var url: URL
    var expiresAt: Date?
    var server: URL?

    /// Whether the Mac may open it: https with no login in it, on the host and port of the
    /// authorization server obid names. Anything else is never opened; the sheet shows the person
    /// which host it is.
    var opens: Bool {
        guard url.scheme?.lowercased() == "https", url.user() == nil, url.password() == nil, let host, let server,
              server.scheme?.lowercased() == "https" else { return false }
        return server.host()?.lowercased() == host && server.port == url.port
    }

    /// The host the sign-in page is on, as the sheet shows it.
    var host: String? { url.host()?.lowercased() }
}

/// What a job's agent did with Tetrix through obid (`tetrix` on a job, docs/runner-api.md): its
/// calls by tool, the sessions it saved, and the report back to Tetrix of where the graph fell short.
/// Everything here is plain text from the agent or Tetrix: shown as written, never as Markdown or a
/// link that opens by itself.
nonisolated struct TetrixJobReport: Codable, Equatable, Sendable {
    /// Where the graph fell short (null until the job ends), and whether it went back to Tetrix.
    struct Gaps: Codable, Equatable, Sendable {
        struct Failure: Codable, Equatable, Sendable {
            var tool: String
            var error: String
        }
        var noResults: [String] = []
        var missing: [String] = []
        var unknownFiles: [String] = []
        var failed: [Failure] = []
        /// When obid sent them to Tetrix, and as which session; or why it didn't.
        var sent: Date?
        var session: String?
        var notSent: String?

        private enum Keys: String, CodingKey { case noResults, missing, unknownFiles, failed, sent, session, notSent }

        init(noResults: [String] = [], missing: [String] = [], unknownFiles: [String] = [], failed: [Failure] = [],
             sent: Date? = nil, session: String? = nil, notSent: String? = nil) {
            (self.noResults, self.missing, self.unknownFiles, self.failed) = (noResults, missing, unknownFiles, failed)
            (self.sent, self.session, self.notSent) = (sent, session, notSent)
        }

        init(from decoder: any Decoder) throws {
            let keys = try decoder.container(keyedBy: Keys.self)
            noResults = (try? keys.decodeIfPresent([String].self, forKey: .noResults)) ?? []
            missing = (try? keys.decodeIfPresent([String].self, forKey: .missing)) ?? []
            unknownFiles = (try? keys.decodeIfPresent([String].self, forKey: .unknownFiles)) ?? []
            failed = (try? keys.decodeIfPresent([Failure].self, forKey: .failed)) ?? []
            sent = try? keys.decodeIfPresent(Date.self, forKey: .sent)
            session = try? keys.decodeIfPresent(String.self, forKey: .session)
            notSent = try? keys.decodeIfPresent(String.self, forKey: .notSent)
        }

        var isEmpty: Bool { noResults.isEmpty && missing.isEmpty && unknownFiles.isEmpty && failed.isEmpty }

        /// One line per gap, labelled: "No results: acme/shop#12".
        var lines: [String] {
            noResults.map { "No results: \($0)" } + missing.map { "Not in the graph: \($0)" }
                + unknownFiles.map { "A file it didn't know: \($0)" } + failed.map { "\($0.tool) failed: \($0.error)" }
        }

        /// "Sent to Tetrix as session c41d…", or why not.
        var sentLine: String? {
            if let notSent { return "Not sent to Tetrix: \(notSent)" }
            guard sent != nil else { return nil }
            return "Sent to Tetrix" + (session.map { " as session \($0)" } ?? "")
        }
    }

    var connected: Bool?
    var calls: [String: Int]
    /// Whether obid wrote Tetrix's context for the job before its agent started.
    var context: Bool?
    /// The session ids `save_session` answered the agent.
    var saved: [String]
    var gaps: Gaps?
    /// "Tetrix wasn't reachable, so this job ran without the graph: …", in obid's words, while it
    /// couldn't reach Tetrix the last time the job tried; nil otherwise.
    var unreachable: String?

    private enum Keys: String, CodingKey { case connected, calls, context, saved, gaps, stamps, unreachable }
    private struct Stamps: Decodable { var sessions: [String]? }

    init(connected: Bool? = nil, calls: [String: Int] = [:], context: Bool? = nil, saved: [String] = [], gaps: Gaps? = nil,
         unreachable: String? = nil) {
        (self.connected, self.calls, self.context, self.saved, self.gaps) = (connected, calls, context, saved, gaps)
        self.unreachable = unreachable
    }

    /// Never fails: a `tetrix` that isn't an object reads as an empty report, so it can't take the
    /// job, or the iPhone's whole list, down with it.
    init(from decoder: any Decoder) throws {
        let keys = try? decoder.container(keyedBy: Keys.self)
        connected = try? keys?.decodeIfPresent(Bool.self, forKey: .connected)
        calls = (try? keys?.decodeIfPresent([String: Int].self, forKey: .calls)) ?? [:]
        context = try? keys?.decodeIfPresent(Bool.self, forKey: .context)
        gaps = try? keys?.decodeIfPresent(Gaps.self, forKey: .gaps)
        unreachable = try? keys?.decodeIfPresent(String.self, forKey: .unreachable)
        // A runner without `saved` still names the sessions its commits were stamped with.
        saved = (try? keys?.decodeIfPresent([String].self, forKey: .saved))
            ?? (try? keys?.decodeIfPresent(Stamps.self, forKey: .stamps))?.sessions ?? []
    }

    func encode(to encoder: any Encoder) throws {
        var keys = encoder.container(keyedBy: Keys.self)
        try keys.encodeIfPresent(connected, forKey: .connected)
        try keys.encode(calls, forKey: .calls)
        try keys.encodeIfPresent(context, forKey: .context)
        try keys.encode(saved, forKey: .saved)
        try keys.encodeIfPresent(gaps, forKey: .gaps)
        try keys.encodeIfPresent(unreachable, forKey: .unreachable)
    }

    /// Nothing worth a section: it wasn't asked, saved nothing and has no gaps to tell.
    var isEmpty: Bool { calls.isEmpty && saved.isEmpty && context != true && (gaps?.isEmpty ?? true) && unreachable == nil }

    /// "Asked Tetrix 4 times: search_entities 2, get_entity 1, save_session 1", busiest first.
    var callsLine: String {
        let total = calls.values.reduce(0, +)
        guard total > 0 else { return connected == true ? "Had Tetrix, but didn't ask it anything" : "Didn't ask Tetrix" }
        let tools = calls.sorted { ($0.value, $1.key) > ($1.value, $0.key) }.map { "\($0.key) \($0.value)" }
        return "Asked Tetrix \(total == 1 ? "once" : "\(total) times"): " + tools.joined(separator: ", ")
    }

    /// What an account row says once obid holds Tetrix for everyone: "Tetrix: used in 9 of 12 jobs",
    /// or where to connect it (`there`: "in Connectors", "on your Mac").
    static func accountLine(connected: Bool, jobs: Int, used: Int, there: String) -> String {
        guard connected else { return "Tetrix: not connected: connect it \(there)" }
        return jobs > 0 ? "Tetrix: used in \(used) of \(jobs) job\(jobs == 1 ? "" : "s")" : "Tetrix: connected"
    }
}

#if os(macOS)
extension Runner {
    /// obid's Tetrix, or nil from a runner that doesn't hold one (404).
    func tetrix() async throws -> TetrixStatus? {
        do {
            return try await call("GET", "v1/tetrix")
        } catch let error as RunnerError where error.status == 404 {
            return nil
        }
    }

    /// Starts signing obid in to `server`, by its address. A second one ends the one under way.
    func startTetrixSignIn(_ server: TetrixStatus.Server) async throws -> TetrixSignIn {
        struct Body: Encodable { var url: URL }
        return try await call("POST", "v1/tetrix/sign-in", body: Body(url: server.url))
    }

    /// Ends the sign-in under way, if any.
    func cancelTetrixSignIn() async throws {
        try await send("DELETE", "v1/tetrix/sign-in")
    }

    /// Signs obid out of Tetrix: agents stop getting its tools.
    func disconnectTetrix() async throws {
        try await send("DELETE", "v1/tetrix")
    }
}
#endif
