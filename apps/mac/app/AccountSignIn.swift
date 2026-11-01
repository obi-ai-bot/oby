import AppKit
import SwiftUI

/// One of obid's accounts to sign in, as a sheet's item: the sheet shows at once and starts the
/// sign-in itself, so Sign In always answers, and what went wrong shows in front of you.
struct ShownAccountSignIn: Identifiable {
    let account: Account
    var id: String { account.id }

    /// The tool's own login for one of obid's accounts (Settings › Accounts, First Launch): obid starts
    /// it, the Mac opens only the tool's own page, with its one-time code on the clipboard, and the
    /// sheet waits for it. Nothing opens once the sheet has closed.
    static func start(_ account: Account, on feed: RunnerFeed) async throws -> AccountSignIn {
        guard let runner = feed.runner else { throw RunnerError(message: "Oby isn't connected to its runner on this Mac yet.") }
        let signIn = try await runner.startAccountSignIn(account.id)
        guard signIn.opens(for: account.agent) else {
            try? await runner.endAccountSignIn(account.id)
            throw RunnerError(message: "\(feed.name) answered with a page that isn't \((Job.agentNames[account.agent] ?? account.agent))'s own sign-in, so Oby opened nothing.")
        }
        // Cancel ends it on obid (`cancel()` below).
        try Task.checkCancellation()
        if let code = signIn.userCode {
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(code, forType: .string)
        }
        NSWorkspace.shared.open(signIn.url)
        return signIn
    }
}

/// The tool's own login for one of obid's accounts: while obid starts it, a spinner; then, waiting
/// for the person on the tool's page, the code to type there (Codex, already on the clipboard), or a
/// field for the code the page ends with (Claude Code); the page again; Cancel. Oby only hands the
/// code to the tool's login, once.
struct AccountSignInSheet: View {
    let shown: ShownAccountSignIn
    let feed: RunnerFeed
    let close: () -> Void
    @State private var signIn: AccountSignIn?
    @State private var code = ""
    @State private var sending = false
    @State private var problem: String?

    private var tool: String { Job.agentNames[shown.account.agent] ?? shown.account.agent }
    private var signedIn: Bool { feed.accounts.first { $0.account.id == shown.account.id }?.signedIn == true }

    var body: some View {
        VStack(spacing: 12) {
            Text("Sign in to \(tool)").font(Tokens.font(17, .semibold)).foregroundStyle(Tokens.ink)
            if let signIn {
                waiting(signIn)
            } else if problem == nil {
                HStack(spacing: 8) {
                    ProgressView().controlSize(.small)
                    Text("Opening \(tool)'s sign-in").font(Tokens.font(13)).foregroundStyle(Tokens.muted)
                }
            }
            if let problem { Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger).multilineTextAlignment(.center) }
            HStack(spacing: 8) {
                Button("Cancel", action: cancel).buttonStyle(.set).keyboardShortcut(.cancelAction)
                if let signIn {
                    Button("Open the Page Again") { NSWorkspace.shared.open(signIn.url) }.buttonStyle(.set)
                    if signIn.pasteCode {
                        Button("Sign In", action: send).buttonStyle(.set).keyboardShortcut(.defaultAction)
                            .disabled(sending || code.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                    }
                }
            }
            .padding(.top, 6)
            Text("\(tool) keeps the login, in Oby's folder for \(shown.account.name). Oby never reads it; it only learns that it worked.")
                .font(Tokens.font(12)).foregroundStyle(Tokens.muted).multilineTextAlignment(.center)
        }
        .padding(26)
        .frame(width: 420)
        .background(Tokens.surface)
        // Codex and Cursor end by themselves: obid's `accounts` event says it's signed in.
        .onChange(of: signedIn) { if signedIn { close() } }
        .task {
            do {
                signIn = try await ShownAccountSignIn.start(shown.account, on: feed)
            } catch is CancellationError {
                // Closed while it started: Cancel ended it.
            } catch { problem = error.localizedDescription }
        }
    }

    @ViewBuilder private func waiting(_ signIn: AccountSignIn) -> some View {
        if let userCode = signIn.userCode {
            Text("\(tool)'s page opened in your browser. Type this code there; it's on your clipboard.")
                .font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.text).multilineTextAlignment(.center)
            Text(userCode).font(Tokens.mono(30)).foregroundStyle(Tokens.ink).textSelection(.enabled)
                .accessibilityLabel("Code \(userCode.map(String.init).joined(separator: " "))")
        } else if signIn.pasteCode {
            Text("\(tool)'s page opened in your browser. Sign in with the account you want Oby to use, then paste the code it shows.")
                .font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.text).multilineTextAlignment(.center)
            SetField("Code", text: $code, prompt: "Paste the code").onSubmit(send).disabled(sending)
        } else {
            Text("\(tool)'s page opened in your browser. Sign in there with the account you want Oby to use.")
                .font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.text).multilineTextAlignment(.center)
        }
        if !signIn.pasteCode || sending {
            HStack(spacing: 8) {
                ProgressView().controlSize(.small)
                Text("Waiting for \(tool)").font(Tokens.font(13)).foregroundStyle(Tokens.muted)
            }
        }
    }

    private func send() {
        let code = code.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !code.isEmpty, !sending, let runner = feed.runner else { return }
        sending = true
        Task {
            defer { sending = false }
            do {
                _ = try await runner.sendAccountCode(shown.account.id, code)
                close()
            } catch { problem = error.localizedDescription }
        }
    }

    private func cancel() {
        close()
        let id = shown.account.id
        Task { try? await feed.runner?.endAccountSignIn(id) }
    }
}
