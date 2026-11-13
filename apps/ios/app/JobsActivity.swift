import ActivityKit
import Foundation
import OSLog

/// Keeps the Live Activity in step with the runner's jobs: one activity while any agent works,
/// ended with how the last job finished. The app updates the activity while it runs and nothing
/// pushes it: while the app is suspended it stays as it was, and goes stale.
enum JobsActivity {
    private static var current: Activity<ObyActivity>? { open.first }

    private static var open: [Activity<ObyActivity>] {
        Activity<ObyActivity>.activities.filter { $0.activityState == .active || $0.activityState == .stale }
    }

    static func sync(_ jobs: [Job]) async {
        #if DEBUG
        // `-ObySampleQuiet YES`: a Live Activity's first start asks for notifications too.
        if UserDefaults.standard.bool(forKey: "ObySampleQuiet") { return }
        #endif
        // An activity left from an earlier launch and the app's own can meet: keep one.
        for extra in open.dropFirst() { await extra.end(nil, dismissalPolicy: .immediate) }
        let active = jobs.filter(\.isActive)
        // A job working first, and only those count as working: not the ones waiting their turn (#238).
        if let job = active.first(where: \.isWorking) ?? active.first {
            let state = state(job, working: active.count(where: \.isWorking))
            if let current {
                guard current.content.state != state || current.content.staleDate != nil else { return }
                await current.update(ActivityContent(state: state, staleDate: nil))
            } else if ActivityAuthorizationInfo().areActivitiesEnabled {
                let content = ActivityContent(state: state, staleDate: nil)
                do {
                    _ = try Activity.request(attributes: ObyActivity(), content: content)
                } catch {
                    Logger.phone.error("starting the Live Activity failed: \(error.localizedDescription, privacy: .public)")
                }
            }
        } else if let current,
                  let last = jobs.max(by: { ($0.finished ?? .distantPast) < ($1.finished ?? .distantPast) }) {
            // The Lock Screen keeps how it ended for 15 minutes after the job finished.
            let end = (last.finished ?? .now).addingTimeInterval(15 * 60)
            await current.end(ActivityContent(state: state(last, working: 0), staleDate: nil), dismissalPolicy: .after(end))
        }
    }

    /// The app stopped following the runner: a minute later the Lock Screen says the step may be old.
    static func markStale() {
        guard let current, current.content.staleDate == nil else { return }
        Task { await current.update(ActivityContent(state: current.content.state, staleDate: .now.addingTimeInterval(60))) }
    }

    private static func state(_ job: Job, working: Int) -> ObyActivity.ContentState {
        ObyActivity.ContentState(working: working, title: job.heading, line: job.line.isEmpty ? "Starting…" : job.line,
                                 started: job.started ?? job.created, status: job.isWaiting ? "waiting" : job.status.rawValue, agent: job.agent)
    }
}
