import SwiftUI

/// Home, once Oby's engine on this Mac hasn't answered for a few seconds (after first launch, nothing
/// else says so): why, as first launch says it, and the way on: Login Items, then Try Again.
struct EngineDownLine: View {
    let feed: RunnerFeed
    private let loginItem = RunnerLoginItem.shared
    @State private var shown = false
    /// How long a drop lasts before the line shows (the pictures' is shorter).
    static var after: Duration = .seconds(5)

    var body: some View {
        // A view that's always there, so the task runs while the line is hidden.
        VStack(spacing: 0) {
            Color.clear.frame(height: 0)
            if shown {
                VStack(alignment: .leading, spacing: 8) {
                    Label(FirstLaunchSteps.waiting(inApplications: RunnerLoginItem.mayRegister(), needsApproval: loginItem.needsApproval,
                                                   problem: loginItem.problem,
                                                   otherwise: "Oby’s engine on this Mac isn’t answering, so nothing runs."),
                          systemImage: "exclamationmark.triangle")
                        .font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.needs)
                        .fixedSize(horizontal: false, vertical: true)
                    HStack(spacing: 8) {
                        Button("Open Login Items Settings") { RunnerLoginItem.openSettings() }.buttonStyle(.set)
                        Button("Try Again") {
                            loginItem.register()
                            feed.reconnect()
                        }
                        .buttonStyle(.set)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        // A moment's drop (obid restarting, waking from sleep) says nothing.
        .task(id: feed.down) {
            shown = false
            guard feed.down else { return }
            try? await Task.sleep(for: Self.after)
            shown = !Task.isCancelled && feed.down
        }
        // Back from System Settings.
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in loginItem.refresh() }
    }
}

/// Connection failures stay visible beside the conversation they affect.
struct RunnerOfflineNotice: View {
    let name: String
    var body: some View {
        Label("\(name) isn't reachable. Oby will reconnect automatically.", systemImage: "wifi.exclamationmark")
            .font(OBYStyle.caption).foregroundStyle(.orange)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, OBYStyle.Space.lg).padding(.vertical, OBYStyle.Space.sm)
    }
}
