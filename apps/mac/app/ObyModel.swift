import Foundation
import Observation

/// What the Oby window, the menu bar and the talk key's pill share: the runner's feed (obid's jobs,
/// the only ones: the app runs none itself, #895), what you're saying now, and Oby's answer to it.
/// (The type keeps its old name; the notch is gone.)
@Observable
final class IslandModel {
    let feed: RunnerFeed
    let availability: MacAvailability
    /// The update waiting to be installed, if any (apps/mac/app/Updater.swift).
    let updater: Updater
    /// What you say while you hold fn.
    let voice: Voice
    /// What goes with the next request: screenshots, what you copied.
    let tray = Tray()
    /// The talk key is down (or hands-free is on), and what you said isn't sent yet.
    var listening = false
    /// Where the talk key's turn is, for the pill over the screen while the window isn't showing it
    /// (apps/mac/app/TalkPill.swift); nil when no turn started by the key is under way.
    var turn: TalkStage?
    /// Oby's reply to the talk key's words, its first lines, while the pill shows it: a few seconds,
    /// or until the next press.
    var reply: String?
    /// The runner's job whose alert's Publish was clicked: its screen starts publishing, once.
    var publishing: String?
    /// Settings' Remote access (apps/mac/app/RemoteAccess.swift), and the vault its Vault page lists:
    /// the app's own; nil in tests, which give Settings made-up ones when they show it.
    var remote: RemoteAccess?
    var vault: Vault?

    /// Oby's answer to your words while the runner's brain plans them (`ask`): thinking, its
    /// question, its reply, or nothing.
    var talk: Desk.Talk = .none
    /// Called with Oby's reply when a turn of `ask` ends, and whether you spoke it (the talk key's
    /// pill shows it then). Not for a turn that was cancelled or replaced.
    @ObservationIgnored var onReply: (_ text: String, _ spoken: Bool) -> Void = { _, _ in }
    /// The plan under way on the runner, until it ends or is cancelled.
    @ObservationIgnored private var planning: Task<Void, Never>?
    /// Counts asks: an answer to a turn that was cancelled or replaced is ignored.
    @ObservationIgnored private var asking = 0
    /// The runner words go to: tests give their own; nil is the feed's.
    @ObservationIgnored var plansOn: Runner?

    @MainActor init(feed: RunnerFeed, updater: Updater = Updater(), availability: MacAvailability, voice: Voice? = nil) {
        self.voice = voice ?? Voice()
        self.feed = feed
        self.availability = availability
        self.updater = updater
        feed.onMacs = { [tray] in tray.macs = $0 }
        feed.onShares = { [tray] in tray.shares = $0 }
    }

    /// Both chat surfaces read and send through this model: the runner's conversation, and your
    /// words on their way to it (`RunnerFeed.echoes`).
    var thread: [Turn] { feed.conversation.thread(with: feed.shownEchoes) }
    /// The accounts a plan's card offers: the runner's.
    var planAccounts: [AccountState] { feed.accounts.filter(\.signedIn) }
    var chatWindowVisible = false
    var readReply: String?
    var unreadReply: Bool { thread.last(where: { $0.who == .oby }).map { $0.id != readReply } ?? false }

    func markChatRead() { readReply = thread.last(where: { $0.who == .oby })?.id }

    func sendChat(_ text: String) {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        let said = tray.note.map { "\(text)\n\n\($0)" } ?? text
        feed.runsOnNext = tray.place?.obi
        // The words show in the thread now, not once the runner lists them (`RunnerFeed.Echo`).
        feed.echo(said)
        ask(said, attachments: tray.paths)
        tray.clear()
    }

    /// Your words go to the runner's brain (`RunnerFeed.plan`), and Oby thinks until it answers. A
    /// bare "never mind", "stop" or "cancel" only ends the turn under way; any other words replace
    /// it, the runner told first so the old plan starts nothing. `spoken`: you talked, so the talk
    /// key's pill shows the reply. `attachments` go to the jobs the words start.
    func ask(_ words: String, spoken: Bool = false, attachments: [String] = []) {
        let said = words.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !said.isEmpty else { return }
        if Conversation.cancels(said) { return cancel() }
        let replacing = planning
        asking += 1
        let mine = asking
        talk = .thinking(said)
        planning = Task {
            if replacing != nil { await feed.cancelPlan(on: plansOn) }
            replacing?.cancel()
            _ = await replacing?.result
            var result: Desk.Talk
            do { result = try await feed.plan(said, answer: nil, attachments: attachments, on: plansOn) } catch { result = .reply(error.localizedDescription) }
            guard asking == mine else { return }
            talk = result
            if let reply = Desk.reply(in: result) { onReply(reply, spoken) }
        }
    }

    /// You picked one of the brain's options: it plans again with your answer.
    func answer(_ option: String, said: String, question: String) {
        ask(Desk.answered(said, question: question, option: option))
    }

    /// Ends the turn under way: the runner is told, so its plan starts and stops nothing, and its
    /// request stops now, so your words leave the thread without waiting for the runner's answer.
    func cancel() {
        asking += 1
        talk = .none
        Task { await feed.cancelPlan(on: plansOn) }
        planning?.cancel()
        planning = nil
    }

    func stopChatJob(_ id: String) {
        Task { if let job = try? await feed.about(id, { try await $0.stop(id) }) { feed.apply(job) } }
    }

    /// Agents working now on the runner: not the jobs waiting their turn (#238).
    var working: Int { feed.working }
    /// Jobs queued behind something on the runner (`Job.waiting`).
    var waiting: Int { feed.waiting }

    /// Jobs and inbox items that wait for you, each job once: the menu bar's count, with `working`.
    /// It runs on every change to the jobs, so the list is put together once, not once per job (#511).
    var needsYou: Int {
        var waits = Set<String>()
        for job in feed.jobs where job.waitsForYou && job.listed(after: feed.clearedAt, among: feed.jobs) { waits.insert("runner/\(job.id)") }
        for entry in IslandInbox.entries(feed: feed, now: .now) {
            waits.insert(entry.item.job.isEmpty ? entry.id : "runner/\(entry.item.job)")
        }
        // The watcher's and the listener's items (a review ready to approve has its job: once).
        for notice in feed.notices where notice.carded && notice.waiting() {
            waits.insert(notice.job.isEmpty ? "notice/\(notice.id)" : "runner/\(notice.job)")
        }
        return waits.count
    }

    /// What Oby does while the window shows it: listens while you talk, melts into Loading while
    /// the brain thinks (and comes back together when the answer lands), works while agents work.
    var mood: String {
        if listening { return "listening" }
        if case .thinking = talk { return "loading" }
        return working == 0 ? "idle" : "working"
    }

    /// Whether a job is under Archived, not on Home (`ArchiveAfter.archives`): it's settled
    /// (`Job.settled`), and it was archived by hand on obid, an older Clear done took it, or it
    /// finished longer ago than Archive completed says.
    func archived(_ job: Job, after: ArchiveAfter, now: Date = .now) -> Bool {
        after.archives(settled: job.settled(among: feed.jobs), finished: job.finished,
                       archived: feed.extras[job.id]?.archived, clearedAt: feed.clearedAt, now: now)
    }

    /// Rename, Archive task and Delete task are obid's (`PATCH`, `/archive`, `DELETE /v1/jobs/{id}`):
    /// not a runner that isn't obid.
    var managesJobs: Bool { feed.conversationsOn }

    /// The one line the iPhone's Home says too (apps/shared/JobList.swift): who's working, and what
    /// needs you among the jobs listed.
    var summary: String {
        Summary.line(working: working, waiting: waiting, needs: NeedsYou.line(feed.jobs.filter { $0.listed(after: feed.clearedAt, among: feed.jobs) }))
    }
}

extension Job {
    /// It needs an answer, a decision or a Publish: the jobs the window marks "Needs you".
    nonisolated var waitsForYou: Bool { needsYou || waitsForAnswer || waitsForPermission }
}
