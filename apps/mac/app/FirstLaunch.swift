import AppKit
import SwiftUI

/// Say hi in two minutes (F14, docs/onboarding.md): hello, hold fn, your agents, GitHub, then Home
/// with a first task in its box. Everything runs on obid, This Mac's engine: the accounts are its,
/// signed in through each tool's own login, and the repositories are the ones its gh can push to.
/// Every step can be skipped, closing the window skips the rest, and the menu shows it again.
final class FirstLaunch: NSObject, NSWindowDelegate {
    static let doneKey = "firstLaunchDone"
    static var done: Bool { UserDefaults.standard.bool(forKey: doneKey) }

    private let island: IslandController
    private var window: NSWindow?

    init(island: IslandController) { self.island = island }

    var isOpen: Bool { window?.isVisible ?? false }

    func show() {
        NSApp.activate()
        if let window {
            window.makeKeyAndOrderFront(nil)
            return
        }
        let view = FirstLaunchView(voice: island.model.voice, feed: island.feed, onDone: { [weak self] in self?.window?.close() })
        let window = NSWindow(contentRect: NSRect(x: 0, y: 0, width: 640, height: 580),
                              styleMask: [.titled, .closable, .fullSizeContentView], backing: .buffered, defer: false)
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.isMovableByWindowBackground = true
        window.appearance = NSAppearance(named: .darkAqua)
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(rootView: view)
        window.delegate = self
        window.center()
        window.makeKeyAndOrderFront(nil)
        self.window = window
    }

    /// Finished, skipped or closed: it doesn't come back by itself, and Oby's window opens on Home.
    func windowWillClose(_ notification: Notification) {
        UserDefaults.standard.set(true, forKey: Self.doneKey)
        window = nil
        island.openWindow(nil)
    }
}

/// What First Launch decides, without a window (apps/mac/tests/FirstLaunchTests.swift).
nonisolated enum FirstLaunchSteps {
    /// A tool on the agents step.
    enum Agent: Equatable {
        /// Signed in on obid: its accounts' names.
        case connected([String])
        /// Not on this Mac.
        case missing
        /// Cursor's CLI without `acp`, which obid runs it with.
        case outdated
        /// obid hasn't answered yet.
        case waiting
        /// obid has an account for it, signed out: its own login signs it in.
        case signIn(Account)
        /// Installed, and obid has no account for it: one is added, then signed in.
        case connect
    }

    static func agent(_ agent: String, installed: Bool, cursorHasACP: Bool = true, engine: Bool, accounts: [AccountState]) -> Agent {
        let mine = accounts.filter { $0.account.agent == agent }
        let signedIn = mine.filter(\.signedIn)
        if engine, !signedIn.isEmpty { return .connected(signedIn.map(\.account.name)) }
        if !installed { return .missing }
        if agent == "cursor", !cursorHasACP { return .outdated }
        if !engine { return .waiting }
        if let out = mine.first { return .signIn(out.account) }
        return .connect
    }

    /// The tools' own logins already on this Mac that obid offers (`availableLogins`) for tools it has
    /// no account for: listed without a click. A tool with accounts keeps them as they are, so routing
    /// never moves to a login the owner didn't pick.
    static func adopt(_ available: [RunnerStatus.AvailableLogin], accounts: [AccountState]) -> [String] {
        let have = Set(accounts.map(\.account.agent))
        var agents: [String] = []
        for login in available where !have.contains(login.agent) && !agents.contains(login.agent) { agents.append(login.agent) }
        return agents
    }

    /// "12 repositories ready".
    static func ready(_ repos: [RunnerRepo]) -> String {
        repos.count == 1 ? "1 repository ready" : "\(repos.count) repositories ready"
    }

    /// The first task Home's box offers: one in the repository obid lists first (the listed ones, then
    /// the one pushed to last), or nothing without one.
    static func suggestion(_ repos: [RunnerRepo]) -> String? {
        guard let repo = repos.first?.repo.split(separator: "/").last else { return nil }
        return "Find a small bug in \(repo) and fix it"
    }
}

struct FirstLaunchView: View {
    enum Step: Int, CaseIterable { case hello, talk, agents, github }

    let voice: Voice
    let feed: RunnerFeed
    let onDone: () -> Void

    @State private var permissions = Permissions()
    @State private var tools = Tools()
    @State private var step: Step
    /// The last thing you said while trying fn.
    @State private var heard = ""
    @State private var problem: String?
    /// A tool's own login on obid, under way in its sheet.
    @State private var signingIn: ShownAccountSignIn?
    @State private var gitHub: GitHubLogin?
    @State private var gitHubSignIn: GitHubSignIn?
    @State private var repos: [RunnerRepo]?

    /// `step`, `gitHub` and `repos`: where it starts, for its pictures (apps/mac/tests/WindowScreens.swift).
    init(voice: Voice, feed: RunnerFeed, step: Step = .hello, gitHub: GitHubLogin? = nil, repos: [RunnerRepo]? = nil,
         onDone: @escaping () -> Void) {
        self.voice = voice
        self.feed = feed
        self.onDone = onDone
        _step = State(initialValue: step)
        _gitHub = State(initialValue: gitHub)
        _repos = State(initialValue: repos)
    }

    /// obid on this Mac answers (live only once it has).
    private var engine: Bool { feed.connection == .live }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Group {
                switch step {
                case .hello: hello
                case .talk: talk
                case .agents: agents
                case .github: github
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            footer
        }
        .padding(.horizontal, 36)
        .padding(.top, 44)
        .padding(.bottom, 24)
        .frame(width: 640, height: 580)
        // Accessibility has no callback: look again every couple of seconds while the window is up.
        .task {
            while !Task.isCancelled {
                permissions.refresh()
                try? await Task.sleep(for: .seconds(2))
            }
        }
        .task(id: step) {
            if step == .agents { await tools.refresh() }
            if step == .github { await watchGitHub() }
        }
        // Once obid answers, the logins it found on this Mac become its accounts.
        .task(id: engine) { await adopt() }
        // Back from a tool's page or Terminal.
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            if step == .agents { Task { await tools.refresh() } }
        }
        .onChange(of: voice.words) { _, words in
            if !words.isEmpty { heard = words }
        }
        .sheet(item: $signingIn) { shown in
            AccountSignInSheet(shown: shown, feed: feed) { signingIn = nil }
        }
    }

    // MARK: Steps

    private var hello: some View {
        VStack(alignment: .leading, spacing: 16) {
            Image(nsImage: .obyMenuBarIcon)
                .resizable().renderingMode(.template).frame(width: 72, height: 72)
                .padding(.bottom, 8)
            title("Hi, I'm Oby.")
            text("I hand your coding work to the agents you already use: Claude Code, Codex and Cursor. They run on this Mac, with your own logins, in their own copies of your repositories, and I tell you when they're done.")
            text("Two minutes and we're set. You can skip any step.")
        }
    }

    private var talk: some View {
        VStack(alignment: .leading, spacing: 16) {
            title("Hold fn to talk")
            text("Hold fn (or Control-Option), say hi, and let go. Oby listens only while you hold it. fn + Space keeps listening until you press fn again.")
            if !permissions.accessibility {
                card(symbol: "keyboard", "Accessibility", lines: ["So Oby notices you holding fn (or Control-Option) in any app. It reads no other keys."]) {
                    Button("Allow") { permissions.askAccessibility() }
                }
            }
            HStack(spacing: 12) {
                Image(systemName: voice.phase == .listening ? "waveform" : "mic")
                    .font(.system(size: 22)).frame(width: 32)
                    .symbolEffect(.variableColor, isActive: voice.phase == .listening)
                Text(voice.phase == .listening ? (voice.words.isEmpty ? "Listening…" : voice.words)
                     : heard.isEmpty ? "Try it now." : "You said “\(heard)”. Oby answers in its window.")
                    .font(.system(size: 15, weight: .medium))
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
            caption("The first time you hold it, macOS asks for the microphone.")
        }
    }

    private var agents: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                title("Your agents")
                Spacer()
                if tools.looking { ProgressView().controlSize(.small) }
            }
            text(engine ? "Oby found these on your Mac. Each signs in with its own login, so Oby never sees your password."
                        : "Waiting for Oby’s engine on this Mac…")
            VStack(spacing: 10) {
                ForEach(Tools.all, id: \.self) { toolRow($0) }
            }
            caption("One is enough to start. A second login of the same tool, like a work account, goes in Settings › Accounts.")
            if let problem {
                Text(problem).font(.caption).foregroundStyle(.red)
            }
        }
    }

    private var github: some View {
        VStack(alignment: .leading, spacing: 14) {
            title("Connect GitHub")
            text("So Oby can list your repositories and open a pull request when an agent’s work passes its check. GitHub’s own sign-in does it; Oby never holds the key.")
            if !engine {
                text("Waiting for Oby’s engine on this Mac…")
            } else if let gitHub, gitHub.signedIn {
                card(symbol: "checkmark.circle", "GitHub", lines: [gitHub.user.map { "Signed in as \($0)" } ?? "Signed in",
                                                                   repos.map(FirstLaunchSteps.ready) ?? "Reading your repositories…"]) {
                    Label("Connected", systemImage: "checkmark.circle.fill").font(.system(size: 13)).foregroundStyle(.green)
                }
            } else if let gitHubSignIn {
                card(symbol: "person.badge.key", "Type this code on GitHub’s page", lines: ["Copied. Paste it on the page that opens."]) {
                    Button("Open the Page Again") { NSWorkspace.shared.open(GitHubSignIn.devicePage) }
                }
                Text(gitHubSignIn.userCode).font(.system(size: 30, weight: .semibold, design: .monospaced)).textSelection(.enabled)
                    .accessibilityLabel("Code \(gitHubSignIn.userCode.map(String.init).joined(separator: " "))")
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Waiting for GitHub").font(.system(size: 13)).foregroundStyle(.secondary)
                }
            } else {
                card(symbol: "arrow.triangle.branch", "GitHub", lines: ["Not connected"]) {
                    Button("Open GitHub", action: signInToGitHub)
                }
            }
            if let problem {
                Text(problem).font(.caption).foregroundStyle(.red)
            }
        }
    }

    // MARK: Pieces

    private var footer: some View {
        HStack(spacing: 12) {
            HStack(spacing: 6) {
                ForEach(Step.allCases, id: \.self) { each in
                    Circle().fill(.white.opacity(each == step ? 0.9 : 0.25)).frame(width: 6, height: 6)
                }
            }
            Spacer()
            if step != .hello {
                Button("Back") { move(-1) }.buttonStyle(.borderless)
            }
            Button(step == .github ? "Skip and finish" : "Skip") {
                if step == .github { onDone() } else { move(1) }
            }
            .keyboardShortcut(.cancelAction)
            Button(primary) {
                if step == .github { onDone() } else { move(1) }
            }
            .buttonStyle(.borderedProminent)
            .keyboardShortcut(.defaultAction)
        }
        .controlSize(.large)
        .padding(.top, 16)
    }

    private var primary: String {
        switch step {
        case .hello: "Let's go"
        case .github: "Finish"
        default: "Continue"
        }
    }

    private func move(_ by: Int) {
        problem = nil
        step = Step(rawValue: step.rawValue + by) ?? step
    }

    private func card(symbol: String, _ name: String, lines: [String], @ViewBuilder buttons: () -> some View) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: symbol).font(.system(size: 18)).frame(width: 28).padding(.top, 2)
            VStack(alignment: .leading, spacing: 4) {
                Text(name).font(.system(size: 14, weight: .semibold))
                ForEach(lines, id: \.self) { line in
                    Text(line).font(.system(size: 12)).foregroundStyle(.secondary)
                }
            }
            .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 12)
            buttons()
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(.white.opacity(0.06), in: RoundedRectangle(cornerRadius: 12))
    }

    private func toolRow(_ agent: String) -> some View {
        let row = FirstLaunchSteps.agent(agent, installed: tools.installed.contains(agent), cursorHasACP: tools.cursorHasACP,
                                         engine: engine, accounts: feed.accounts)
        let tool = Job.agentNames[agent] ?? agent
        let line: String = switch row {
        case .connected(let names): names.joined(separator: ", ")
        case .missing: agent == "cursor" ? "Install the Cursor CLI: `\(Tools.installCursor)`, and Oby finds it." : "Not installed. Install \(tool), and Oby finds it."
        case .outdated: "Update the Cursor CLI: `\(Tools.updateCursor)`. Oby needs its `acp` command."
        case .waiting: "Found on this Mac."
        case .signIn(let account): "\(account.name): signed out"
        case .connect: "Found. Connect an account so Oby can send it jobs."
        }
        return card(symbol: Usage.symbol(agent), tool, lines: [line]) {
            switch row {
            case .connected:
                Label("Connected", systemImage: "checkmark.circle.fill").font(.system(size: 13)).foregroundStyle(.green)
            case .missing, .outdated:
                // Installing is never automatic: the command is for the user to run.
                if agent == "cursor" {
                    Button("Copy Command") {
                        NSPasteboard.general.clearContents()
                        NSPasteboard.general.setString(row == .missing ? Tools.installCursor : Tools.updateCursor, forType: .string)
                    }
                }
            case .waiting:
                Button("Connect") {}.disabled(true)
            case .signIn(let account):
                Button("Sign In") { signIn(account) }
            case .connect:
                Button("Connect") { connect(agent) }
            }
        }
    }

    // MARK: obid

    /// A tool with no account on obid: a "Personal" one, then its own login.
    private func connect(_ agent: String) {
        Task {
            do {
                guard let runner = feed.runner else { throw RunnerError(message: "Oby isn't connected to its engine on this Mac yet.") }
                let added = try await runner.addAccount(agent: agent, name: "Personal", payer: "")
                signingIn = try await ShownAccountSignIn.start(added.account, on: feed)
                problem = nil
            } catch { problem = error.localizedDescription }
        }
    }

    private func signIn(_ account: Account) {
        Task {
            do {
                signingIn = try await ShownAccountSignIn.start(account, on: feed)
                problem = nil
            } catch { problem = error.localizedDescription }
        }
    }

    private func adopt() async {
        guard engine, let runner = feed.runner else { return }
        for agent in FirstLaunchSteps.adopt(feed.availableLogins, accounts: feed.accounts) {
            // obid's `accounts` event brings it to the row.
            _ = try? await runner.restoreDefaultLogin(agent: agent)
        }
    }

    /// gh's own sign-in on obid: GitHub's device page opens with its code on the clipboard.
    private func signInToGitHub() {
        Task {
            do {
                guard let runner = feed.runner else { throw RunnerError(message: "Oby isn't connected to its engine on this Mac yet.") }
                let signIn = try await runner.startGitHubSignIn()
                guard signIn.isGitHubs else {
                    try? await runner.endGitHubSignIn()
                    throw RunnerError(message: "Oby’s engine answered with something other than GitHub’s sign-in page and code, so Oby opened nothing.")
                }
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(signIn.userCode, forType: .string)
                gitHubSignIn = signIn
                NSWorkspace.shared.open(GitHubSignIn.devicePage)
                problem = nil
            } catch { problem = error.localizedDescription }
        }
    }

    /// While GitHub's step shows: whether gh is signed in, and once it is, the repositories it can push
    /// to (obid reads GitHub's list again right after a sign-in). The first one's task waits in Home's box.
    private func watchGitHub() async {
        while !Task.isCancelled {
            if engine, let runner = feed.runner, let login = try? await runner.gitHubLogin() {
                if gitHubSignIn != nil, !login.signedIn, login.signIn == nil { problem = "GitHub’s sign-in ended without signing in." }
                gitHub = login
                if login.signedIn || login.signIn == nil { gitHubSignIn = nil }
                if login.signedIn, let listed = try? await runner.jobRepos() {
                    repos = listed
                    if let task = FirstLaunchSteps.suggestion(listed), Drafts.text(Drafts.home).isEmpty { Drafts.keep(task, as: Drafts.home) }
                }
            }
            try? await Task.sleep(for: .seconds(2))
        }
    }

    // MARK: Text

    private func title(_ words: String) -> some View {
        Text(words).font(.system(size: 30, weight: .bold))
    }

    private func text(_ words: String) -> some View {
        Text(words).font(.system(size: 15)).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
    }

    private func caption(_ words: String) -> some View {
        Text(words).font(.system(size: 12)).foregroundStyle(.tertiary).fixedSize(horizontal: false, vertical: true)
    }
}
