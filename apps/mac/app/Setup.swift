import AppKit
import ApplicationServices
import Observation

/// Accessibility, which the talk key needs to notice fn in any app. The microphone asks on the
/// first hold (apps/mac/app/Voice.swift) and notifications when the first task finishes
/// (`RunnerFeed.post`): each is asked when it's needed, and each on its own.
@Observable
final class Permissions {
    private(set) var accessibility = false

    // macOS doesn't say whether Accessibility was refused, only whether it's allowed.
    func refresh() { accessibility = AXIsProcessTrusted() }

    /// macOS's prompt the first time, System Settings after. The talk key's tap installs by itself
    /// once it's allowed.
    func askAccessibility() {
        guard !AXIsProcessTrusted() else { return refresh() }
        if UserDefaults.standard.bool(forKey: "accessibilityAsked") {
            if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility") {
                NSWorkspace.shared.open(url)
            }
        } else {
            UserDefaults.standard.set(true, forKey: "accessibilityAsked")
            AXIsProcessTrustedWithOptions([kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary)
        }
        refresh()
    }
}

/// The coding tools installed on this Mac. Their accounts are obid's (`RunnerFeed.accounts`):
/// obid finds each tool's own login and signs new ones in through the tool's own login.
@Observable
final class Tools {
    static let all = ["claude", "codex", "cursor"]

    /// The tools whose CLI is on the login shell's PATH, by agent.
    private(set) var installed: Set<String> = []
    private(set) var looking = false
    /// Whether the installed Cursor CLI has `acp` (`cursor-agent acp --help` exits 0), which obid
    /// runs it with; true until known.
    private(set) var cursorHasACP = true

    func refresh() async {
        looking = true
        defer { looking = false }
        var found: Set<String> = []
        for (agent, command) in [("claude", "claude"), ("codex", "codex"), ("cursor", "cursor-agent")] where await Self.onPath(command) {
            found.insert(agent)
        }
        installed = found
        if found.contains("cursor") {
            cursorHasACP = (try? await Shell.run(["cursor-agent", "acp", "--help"], timeout: 15))?.status == 0
        }
    }

    /// What the setup row says to install Cursor's CLI, and to update one without `acp`.
    static let installCursor = "curl https://cursor.com/install -fsS | bash"
    static let updateCursor = "cursor-agent update"

    nonisolated static func onPath(_ command: String) async -> Bool {
        (try? await Shell.run(["which", command], timeout: 10))?.status == 0
    }
}
