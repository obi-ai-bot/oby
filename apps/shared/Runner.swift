import Foundation

/// A job on the runner (docs/runner-api.md).
nonisolated struct Job: Codable, Identifiable, Equatable, Sendable {
    enum Status: String, Codable, Sendable { case queued, running, done, failed, stopped, interrupted }

    var id: String
    var request: String
    var agent: String
    var account: String
    var repo: String
    var branch: String
    var base: String?
    var pr: Int?
    var kind: String
    var task: String
    /// What to call the job (docs/runner-api.md): the brain's short title, else the runner's
    /// fallback. Older runners don't send it.
    var title: String?
    /// Whether `GET /v1/jobs/{id}/report` has a saved research report.
    var report: Bool?
    var status: Status
    var step: String
    var summary: String
    var session: String?
    var created: Date
    var started: Date?
    var finished: Date?
    var commits: Int?
    /// Setting it (a publish from this phone) counts what went out, as the runner does; obid's
    /// `canPublish` from before it no longer holds.
    var published: String? { didSet { if published != nil { pushed = commits; offersPublish = nil } } }
    /// Commits on the pull request when it was last published.
    var pushed: Int?
    /// Why the runner couldn't publish it by itself, or nil (newer runners only).
    var publishError: String?
    /// obid's own `can_publish`, which knows the commit the check passed on (#1772), or nil from an
    /// older obid.
    var offersPublish: Bool? = nil
    /// Newer runners only, so all optional: the issue the work is about, how many rounds the agent
    /// may take and the one it's on, the command that must pass, the stronger model it runs on,
    /// and what the user told the agent along the way, oldest first.
    var issue: Int?
    var rounds: Int?
    var round: Int?
    var check: String?
    var model: String?
    var notes: [String]?
    /// Derived by obid and sent by newer runners; old runners still decode.
    var pipeline: JobPipeline?
    var pullRequest: PullRequestStatus?
    var pullRequestError: String?
    /// When the user let the job go (Dismiss), or nil: it then needs nothing (newer runners only).
    var dismissed: Date?
    /// When it was archived with a task's ⋮, on any app (obid only, `POST /v1/jobs/{id}/archive`),
    /// or nil; obid clears it when the task runs again.
    var archived: Date?
    /// Oby's own review of the pull request (newer runners): the review job under way, the latest
    /// verdict, why none came, and, on a review job, the job it reviews.
    var reviewing: String?
    var review: Review?
    var reviewError: String?
    var reviewOf: String?
    /// Everything the runner sent out for the job, oldest first (newer runners only).
    var outgoing: [Outgoing]?
    /// What its agent asked the user and waits for (the inbox has it), or nil. Older runners don't send it.
    var question: Question?
    /// Oby's merge stopped where a person takes over (the runner derives it, docs/runner-api.md "Mac
    /// check"), or nil from older runners.
    var stoppedMerge: Bool?
    /// Oby's review stopped asking for changes, where a person takes over (the runner derives it,
    /// docs/runner-api.md "Oby's review"), or nil from older runners.
    var stoppedReview: Bool?
    /// Oby's Xcode Cloud recovery stopped, where a person takes over (the runner derives it,
    /// docs/runner-api.md "Xcode Cloud"), or nil from older runners.
    var stoppedCloud: Bool?
    /// How freely its agent works ("ask", "acceptEdits", "plan" or "bypass", `RunnerMode`).
    /// Nil from older runners, and for the runner's default.
    var mode: String?
    /// What its agent waits for the user's OK on, or nil. Older runners don't send it.
    var permission: RunnerPermission?
    /// Every time the user moved it to another tool, account or model, oldest first (newer runners only).
    var moves: [Move]?
    /// What a queued job waits for (#238), or nil: getting ready, about to start, not queued, or an older runner.
    var waiting: Waiting?
    /// The conversation whose words started it (obid only, #493), or nil.
    var conversation: String?
    /// The user's turn whose words started it (obid only): its card shows under them as it starts.
    var turn: String?
    /// What it made to be looked at (obid only, its `.obi/artifacts/`), or nil.
    var artifacts: [ArtifactRef]?
    /// The job this one follows up (obid only): the owner told that one something after it ended,
    /// and its page lists this one (Sangam's design). Nil otherwise.
    var followUpOf: String?
    /// obid only (#395): the routine whose run started it, or nil.
    var routine: RoutineMark?
    /// What its agent asked Tetrix, the session it saved and where the graph fell short, or nil.
    var tetrix: TetrixJobReport?
    /// Why it stopped without ending done, apart from its agent's summary (obid only), or nil.
    var reason: String?
    /// A `done` job that ended with nothing to show (obid only, docs/contracts/tasks.md TASKS-12).
    var nothingDone: Bool?
    /// A grouped account decision owns this interruption; older runners omit it.
    struct AccountLimit: Codable, Equatable, Sendable {
        let decision: String?
        let state: String
        let resetAt: Date?
    }
    var accountLimit: AccountLimit?
    /// Interrupted on obid: its one next step, as its button says it ("One more round", "Try
    /// again"), or nil.
    var nextStep: String?
    /// When obid tries it again by itself, after its account's usage limit resets, or nil; one
    /// this phone can't read is left out, never failing the job.
    var resumesAt: Lenient<Date>?
    /// `resumesAt`, read.
    var resumes: Date? { resumesAt?.value }
    /// A stalled job's why and handoff note (obid only, #773), or nil; one this phone can't read
    /// is left out, never failing the job.
    var stalled: Lenient<Stalled>?
    /// The handoff note a stalled job left, or nil.
    var handoffNote: String? { stalled?.value?.note.flatMap { $0.isEmpty ? nil : $0 } }
    /// obid runs its check again by itself, or stopped trying with Publish kept on its page
    /// (PUBLISHING-31, obid only): either way it doesn't need the owner.
    var unjudged: Lenient<Unjudged>?
    struct Unjudged: Codable, Equatable, Sendable {
        var tries: Int?
    }

    /// Why and where a stalled job stopped (obid's `stalled`, #773), as far as the apps show it:
    /// its handoff note. Every key optional, so a newer obid's object still decodes.
    struct Stalled: Codable, Equatable, Sendable {
        var why: String?
        var note: String?
    }
    /// The job page's facts (#739), each nil until obid serves it: its tools, tokens, share of the
    /// limit windows, the estimate, the check's commands, the secret scan, the diff and what the
    /// current stage is doing (`JobFacts`). Each is `Lenient`: one this phone can't read is left
    /// out, never failing the job (or the list it's in).
    var activity: Lenient<JobActivity>?
    var usage: Lenient<JobUsage>?
    var windowShare: Lenient<WindowShare>?
    var estimate: Lenient<JobEstimate>?
    var checks: Lenient<[CheckRound]>?
    var scan: Lenient<JobScan>?
    var diff: Lenient<JobDiff>?
    var context: Lenient<String>?

    /// The same facts as the Mac reads them, with each stage's times.
    var facts: JobFacts {
        var stages: [String: StageTiming] = [:]
        for stage in pipeline?.stages ?? [] {
            let timing = StageTiming(started: stage.started?.value, finished: stage.finished?.value, waited: stage.waited?.value,
                                     estimate: stage.estimate?.value)
            if timing != StageTiming() { stages[stage.id] = timing }
        }
        return JobFacts(activity: activity?.value, usage: usage?.value, windowShare: windowShare?.value, estimate: estimate?.value,
                        checks: checks?.value, scan: scan?.value, diff: diff?.value, context: context?.value, stages: stages)
    }

    /// What it waits for, as the runner words it: `text` like "after #223 merges" or "runner
    /// update"; `kind` ("job", "files", "update", "cap", "schedule", "limit") stays a string so a
    /// newer runner's kinds still decode.
    struct Waiting: Codable, Equatable, Sendable {
        var kind: String
        var text: String
        var job: String?
        /// Waiting for room while only these accounts' weekly reserves stand in the way (#278), by
        /// id: "Use Personal anyway" shows on each. Newer runners only.
        var reserved: [String]?

        /// obid holds it back until the Mac has room for it (docs/runner-api.md, admission by
        /// memory): `cap`, with these words.
        static let memory = Waiting(kind: "cap", text: "memory frees up")
    }

    /// Queued until what its `startWhen` names happens (#216: `waiting`'s kind `schedule` or
    /// `limit`): it can start now (`Runner.startNow`).
    var waitsToStart: Bool { status == .queued && (waiting.map { ["schedule", "limit"].contains($0.kind) } ?? false) }

    /// One move, as the runner words it: "Codex · Deskree" to "Claude Code · Personal · Fable 5.1".
    struct Move: Codable, Equatable, Sendable {
        var from: String
        var to: String
        var why: String?
        var date: Date
    }

    private enum CodingKeys: String, CodingKey {
        case id, request, agent, account, repo, branch, base, pr, kind, task, title, status, step, summary, session, created,
             started, finished, commits, published, pushed, publishError, issue, rounds, round, check, model, notes, pipeline,
             pullRequest, pullRequestError, dismissed, archived, reviewing, review, reviewError, reviewOf, outgoing, question, mode, permission, moves, waiting, report,
             conversation, turn, artifacts, followUpOf, routine, tetrix, activity, usage, windowShare, estimate, checks, scan, diff, context,
             reason, nothingDone, nextStep, resumesAt, stalled, accountLimit, unjudged
        case stoppedMerge = "mergeNeedsYou"
        case stoppedReview = "reviewNeedsYou"
        case stoppedCloud = "cloudNeedsYou"
        case offersPublish = "canPublish"
    }

    /// "Moved from Codex · Deskree to Claude Code · Personal": its latest move, or nil.
    var movedLine: String? { moves?.last.map { "Moved from \($0.from) to \($0.to)" } }

    /// Its merge stopped: it couldn't merge, or the Mac check failed with no round left.
    var mergeNeedsYou: Bool { dismissed == nil && stoppedMerge == true }

    /// Its Xcode Cloud recovery stopped: Cloud still fails, or it couldn't hand the failure on.
    var cloudNeedsYou: Bool { dismissed == nil && stoppedCloud == true }

    var isActive: Bool { status == .queued || status == .running }
    /// How many rounds it may take and the one it's on.
    var maxRounds: Int { rounds ?? 1 }
    var currentRound: Int { round ?? 1 }
    /// Queued behind something (`waiting`): listed under Waiting, not counted as working (#238).
    var isWaiting: Bool { status == .queued && waiting != nil }
    /// Running, or getting ready to: what every app counts as an agent working.
    var isWorking: Bool { isActive && !isWaiting }
    /// Its agent asked the user something and waits for the answer: its alert stays until then.
    var waitsForAnswer: Bool { question != nil && dismissed == nil && status != .stopped }
    /// Its agent waits for the user's OK (Allow, Always allow or Deny).
    var waitsForPermission: Bool { permission != nil && isActive }

    /// The report's list of what Oby sent out for the job, or nil when nothing went out.
    var sentLines: String? {
        guard let outgoing, !outgoing.isEmpty else { return nil }
        return "Sent by Oby:\n" + outgoing.map { "- \($0.line)" }.joined(separator: "\n")
    }
    var repoName: String { String(repo.split(separator: "/").last ?? Substring(repo)) }
    var agentName: String { ["codex": "Codex", "cursor": "Cursor"][agent] ?? "Claude Code" }

    /// The runner's title, like "Mac runner notifications", else "Review oby #33" or the first line
    /// of what the user asked.
    var heading: String {
        if let title = title?.trimmingCharacters(in: .whitespacesAndNewlines), !title.isEmpty { return title }
        if let number = pr ?? issue { return "\(kind.capitalized) \(repoName) #\(number)" }
        return (task.isEmpty ? request : task).split(separator: "\n").first.map(String.init) ?? request
    }

    /// The one line a finished job is known by; a waiting one says what for.
    var line: String {
        if let waiting, status == .queued { return Self.waitingLine(waiting.text) }
        return switch status {
        case .queued, .running: step
        case .done: summary.split(separator: "\n").first.map(String.init) ?? "Done"
        case .interrupted: reason ?? summary.split(separator: "\n").first.map(String.init) ?? "Needs you"
        case .failed, .stopped: summary.split(separator: "\n").first.map(String.init) ?? status.rawValue.capitalized
        }
    }

    /// Commits not out yet, as obid's `Job::unpublished`: all of them before it's pushed, then later
    /// rounds' ones. Pushed with no pull request (a remote with none, `pullRequests: false`) counts as
    /// out, and so does a push whose pull request alone failed (#627, #1763).
    var unpublished: Int {
        let commits = commits ?? 0
        let out = published != nil ? pushed ?? commits : publishError == nil ? pushed ?? 0 : 0
        return max(0, commits - out)
    }
    /// Whether Publish by hand is offered: what obid says; from an older one, not once the user
    /// dismissed it, never on Oby's own review jobs, and never once its pull request was merged or
    /// closed.
    var canPublish: Bool {
        if let offersPublish { return offersPublish }
        return status == .done && kind != "research" && kind != "review" && base != nil && unpublished > 0 && dismissed == nil && reviewOf == nil
            && (pullRequest?.state ?? "OPEN") == "OPEN"
    }

    /// Oby's review requested changes for the last time it may (a blocker came back) and the pull
    /// request is still open, as the runner says.
    var reviewNeedsYou: Bool { stoppedReview == true }

    /// Done with commits its pull request won't take: merged or closed before they went out.
    var stranded: Bool {
        status == .done && published != nil && unpublished > 0 && reviewOf == nil
            && (pullRequest.map { $0.state != "OPEN" } ?? false)
    }

    /// Waiting for Publish (the runner's own publish failed), failed, interrupted, out of review
    /// rounds, stranded or its merge stopped, and not dismissed: what the runner sends an alert for,
    /// and what Dismiss takes.
    var needsYou: Bool {
        dismissed == nil && ((canPublish && unjudged == nil) || stranded || status == .failed || (status == .interrupted && accountLimit == nil) || reviewNeedsYou || mergeNeedsYou || cloudNeedsYou)
    }

    /// "Round 2 of 5 · Fable 5.1", or empty for a one-round job on the usual model.
    var extras: String {
        var parts: [String] = []
        if let rounds, rounds > 1 { parts.append("Round \(round ?? 1) of \(rounds)") }
        if let model { parts.append(ModelChoice.name(model)) }
        return parts.joined(separator: " · ")
    }
}

/// Wire types only. Pipeline state is derived by obid, never by the phone.
nonisolated struct JobStage: Codable, Equatable, Identifiable, Sendable {
    enum State: String, Codable, Sendable { case done, current, upcoming, failed, skipped }
    var id: String
    var title: String
    var state: State
    var detail: String?
    var reason: String?
    /// Its times (#733), nil until obid serves them; one that can't be read is left out (`Lenient`).
    var started: Lenient<Date>?
    var finished: Lenient<Date>?
    var waited: Lenient<Double>?
    var estimate: Lenient<Span>?
}

nonisolated struct JobPipeline: Codable, Equatable, Sendable {
    var stages: [JobStage]
    var number: Int
    var total: Int
    var label: String { "Stage \(number) of \(total)" }
}

nonisolated struct PullRequestStatus: Codable, Equatable, Sendable {
    var state: String
    var reviewDecision: String?
    var base: String
    var mergedAt: Date?
    var checkedAt: Date
}

/// Oby's verdict on a job's pull request (docs/runner-api.md).
nonisolated struct Review: Codable, Equatable, Sendable {
    /// "approved" or "changesRequested".
    var verdict: String
    var job: String
    var agent: String
    var account: String
    var round: Int
    var date: Date
    /// The check run it went on the pull request as; `comment` from runners before check runs.
    var checkRun: String?
    var comment: String?
    var postError: String?

    var label: String { verdict == "approved" ? "Approved" : "Changes requested" }
}

/// Something the runner sent out for a job (docs/runner-api.md): ids only, never what was sent.
nonisolated struct Outgoing: Codable, Equatable, Sendable {
    /// "push", "pullRequest", "pullRequestEdit", "checkRun", "comment" (before check runs) or "merge";
    /// a newer runner may send others.
    var action: String
    var repo: String
    var id: String
    var branch: String?
    var date: Date

    var line: String {
        switch action {
        case "push": "Pushed \(id.prefix(7)) to \(branch ?? "its branch") in \(repo)"
        case "pullRequest": "Opened pull request \(id)"
        case "pullRequestEdit": "Updated the description of \(id)"
        case "comment": "Commented \(id)"
        case "checkRun": "Posted the check \(id)"
        case "merge": "Squash-merged \(id) into dev and deleted \(branch ?? "its branch")"
        default: "\(action) \(id)"
        }
    }
}

nonisolated struct Account: Codable, Identifiable, Equatable, Sendable {
    var id: String
    var agent: String
    var name: String
    var label: String?
    var signedIn: Bool
    var fiveHour: Double?
    var sevenDay: Double?
    /// When each window resets (docs/runner-api.md). Older runners don't send them.
    var fiveHourResets: Date?
    var sevenDayResets: Date?
    var running: Int
    /// A job of its hit the account's usage limit: no job goes to it before then (newer runners only).
    var fullUntil: Date?
    /// Whether its agents have Tetrix connected (nil: not known yet), and of its recent jobs, how
    /// many used it (newer runners only).
    var tetrix: Bool?
    var tetrixJobs: Int?
    var tetrixUsed: Int?
    /// Whether it gets only jobs sent to it by name, its week at 80%, and until when the user let
    /// it past that (#278); newer runners only.
    var reserved: Bool?
    var reserveLiftedUntil: Date?
    /// The model and efforts its jobs run on, each only when the owner chose it; nil: the tool's
    /// own (obid only, #1574).
    var model: String?
    var effort: String?
    var complexEffort: String?
    /// Who it is, as its tool says (docs/contracts/accounts.md); newer runners only.
    var email: String?
    var org: String?
    var plan: String?

    /// "Personal · Claude Code"
    var title: String { "\(name) · \(Usage.tool(agent))" }

    /// What Settings' usage card shows.
    var usage: Usage.Snapshot {
        Usage.Snapshot(agent: agent, name: name, label: label, fiveHour: fiveHour, sevenDay: sevenDay,
                       fiveHourResets: fiveHourResets, sevenDayResets: sevenDayResets, running: running,
                       reserveLiftedUntil: reserveLiftedUntil, runnerSignedIn: signedIn)
    }
}

nonisolated struct Question: Codable, Equatable, Sendable {
    var text: String
    /// At most 3 (docs/runner-api.md); the user's own words answer too.
    var options: [String]
    /// What each option does, one line each, or nil. Older runners don't send it.
    var details: [String]? = nil
    /// The index of the option the asker recommends, or nil. Older runners don't send it.
    var recommended: Int? = nil
    /// The plan to approve, in markdown (Plan mode), or nil. Older runners don't send it.
    var plan: String? = nil
}

/// Something only the user can answer, waiting in the inbox (docs/runner-api.md).
nonisolated struct InboxItem: Codable, Identifiable, Equatable, Sendable {
    /// `question`, `review`, `failed` or `limit`; more may come.
    var kind: String
    var id: String
    var job: String
    var title: String
    var text: String
    var options: [String]
    /// What each option does, one line each, from newer runners only (#215).
    var details: [String]? = nil
    var created: Date
    var done: Date?
    var later: Date?
    var answer: String?
    /// Oby's plain words for it (#65), from newer runners only: a headline of at most 8 words, what
    /// happened, the option it recommends (an index into `options`, or nil) and why.
    var headline: String?
    var happened: String?
    var recommended: Int?
    var why: String?
    /// On a `confirm` item (obid only, #546), what its Confirm carries out.
    var confirm: RunnerConfirm? = nil
    /// On a `start` item Oby offered in a conversation (obid only, `oby_offer`): that conversation,
    /// and the offer: its card's id (the user's turn it was offered in), obid's words for the job,
    /// the task Oby planned, and whether the turn had read something untrusted first.
    var conversation: String? = nil
    var offer: Offer? = nil

    nonisolated struct Offer: Codable, Equatable, Sendable {
        var id: String
        var what: String
        var task: String
        var untrusted: Bool
    }

    var isOpen: Bool { done == nil }
    /// Those words, for its card.
    var words: InboxWords { InboxWords(headline: headline, happened: happened, recommended: recommended, why: why, confirm: confirm) }
    func waiting(at now: Date = .now) -> Bool { done == nil && (later.map { $0 <= now } ?? true) }
}

/// What `POST /v1/jobs/{id}/delete` answers.
nonisolated struct DeleteAnswer: Decodable, Sendable { var deleted: Bool; var ask: InboxItem? }


extension InboxItem {
    /// Its kind as the runner names it.
    nonisolated func shownKind(_ words: InboxWords? = nil) -> String {
        if words?.confirm != nil { return RunnerConfirm.kind }
        if let type = words?.obi?.type, ObiAsk.named.contains(type) { return type }
        return "\(kind)"
    }

    /// "Failed · Mac runner notifications · waiting 3m": what kind of item, its job, how long it waited.
    nonisolated func line(now: Date = .now, words: InboxWords? = nil) -> String {
        let label = switch shownKind(words) {
        case "question", "agentQuestion": "Question"
        case "review": "Out of reviews"
        case "limit": "Out of usage"
        case "audit": "Issue audit"
        case "merge": "Not merged"
        case "failure": "Oby failure"
        case "start": "Start it?"
        case "permission": "Permission"
        case "review_request": "Review request"
        case "host_approval": "Approve on GitHub?"
        case RunnerConfirm.kind: "Confirm it?"
        default: "Failed"
        }
        return "\(label) · \(title) · \(Elapsed.waiting(since: created, now: now))"
    }

    /// The inbox as the apps show it: what waits now, the longest-waiting first.
    nonisolated static func waiting(_ items: [InboxItem], now: Date = .now) -> [InboxItem] {
        items.filter { $0.waiting(at: now) }.sorted { $0.created < $1.created }
    }
}

/// What the brain made of the user's words (`POST /v1/plans`): the work it planned has started
/// (#269), and the reply carries those jobs. A `plan` from an older runner reads as a reply with
/// nothing started.
nonisolated enum PlanResult: Decodable, Equatable, Sendable {
    case question(said: String, Question)
    /// What Oby said, and the jobs it started for it, 0 to 10.
    case reply(String, started: [Job])
    case none

    private enum Keys: String, CodingKey { case kind, jobs, said, question, text }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: Keys.self)
        switch try c.decode(String.self, forKey: .kind) {
        case "question": self = .question(said: try c.decode(String.self, forKey: .said), try c.decode(Question.self, forKey: .question))
        case "reply", "plan":
            self = .reply(try c.decodeIfPresent(String.self, forKey: .text) ?? "", started: try c.decodeIfPresent([Job].self, forKey: .jobs) ?? [])
        default: self = .none
        }
    }
}

/// What `POST /v1/plans` answers, with the conversation the words joined (obid only, #493).
nonisolated struct PlannedTurn: Decodable, Equatable, Sendable {
    var result: PlanResult
    var conversation: String?

    private enum Keys: String, CodingKey { case conversation }

    init(from decoder: any Decoder) throws {
        result = try PlanResult(from: decoder)
        conversation = try? decoder.container(keyedBy: Keys.self).decodeIfPresent(String.self, forKey: .conversation)
    }
}

/// One of obid's conversations (docs/runner-api.md, "obid only: conversations"), as its list and
/// its event send it. Tolerant: a key missing or unreadable takes a plain default, so a newer
/// obid's still reads. `deleted`: the event saying it's gone.
nonisolated struct RunnerConversation: Decodable, Identifiable, Equatable, Sendable {
    /// What a search found in it: the latest turn that holds the words.
    struct Match: Decodable, Equatable, Sendable {
        var who: String?
        var text: String
    }

    var id: String
    var title: String
    var pinned: Bool
    var created: Date?
    var lastActivity: Date?
    var deleted: Bool
    var turn: Match?
    /// Who it talks through (docs/contracts/conversation-agent.md): the owner's choice (an account's
    /// id, `auto`, or nil to follow the setting), the account its last answer came from, and the one
    /// its turns moved off. obid sends each only once there's something to say.
    var talkAccount: String?
    var talkedOn: String?
    var movedOff: String?
    /// The one conversation that says what other Obys ask (docs/contracts/ask-an-oby.md): the app
    /// never opens on it.
    var fromObys: Bool

    private enum Keys: String, CodingKey { case id, title, pinned, created, lastActivity, deleted, turn, talkAccount, talkedOn, movedOff, fromObys }

    init(id: String, title: String, pinned: Bool = false, created: Date? = nil, lastActivity: Date? = nil,
         deleted: Bool = false, turn: Match? = nil, talkAccount: String? = nil, talkedOn: String? = nil, movedOff: String? = nil,
         fromObys: Bool = false) {
        self.id = id
        self.title = title
        self.pinned = pinned
        self.created = created
        self.lastActivity = lastActivity
        self.deleted = deleted
        self.turn = turn
        self.talkAccount = talkAccount
        self.talkedOn = talkedOn
        self.movedOff = movedOff
        self.fromObys = fromObys
    }

    init(from decoder: any Decoder) throws {
        let keys = try decoder.container(keyedBy: Keys.self)
        id = try keys.decode(String.self, forKey: .id)
        talkAccount = (try? keys.decodeIfPresent(String.self, forKey: .talkAccount)) ?? nil
        talkedOn = (try? keys.decodeIfPresent(String.self, forKey: .talkedOn)) ?? nil
        movedOff = (try? keys.decodeIfPresent(String.self, forKey: .movedOff)) ?? nil
        fromObys = (try? keys.decodeIfPresent(Bool.self, forKey: .fromObys)) ?? false
        title = (try? keys.decodeIfPresent(String.self, forKey: .title)) ?? ""
        pinned = (try? keys.decodeIfPresent(Bool.self, forKey: .pinned)) ?? false
        created = try? keys.decodeIfPresent(Date.self, forKey: .created)
        lastActivity = try? keys.decodeIfPresent(Date.self, forKey: .lastActivity)
        deleted = (try? keys.decodeIfPresent(Bool.self, forKey: .deleted)) ?? false
        turn = try? keys.decodeIfPresent(Match.self, forKey: .turn)
    }

    /// When it last moved: its latest turn, else when it started.
    var latest: Date { lastActivity ?? created ?? .distantPast }
}

nonisolated struct ModelChoice: Codable, Equatable, Sendable {
    var model: String
    var effort: String
    var complexEffort: String? = nil
}

extension ModelChoice {
    /// "Opus 5.5 · Extra high"
    nonisolated var label: String {
        "\(Self.name(model)) · \(Self.effortName(effort))"
    }

    /// "Extra high", "Medium".
    nonisolated static func effortName(_ effort: String) -> String { effort == "xhigh" ? "Extra high" : effort.capitalized }

    /// "Fable 5.1", "GPT-6-Astra", or the id as it is.
    nonisolated static func name(_ model: String) -> String {
        let names = ["claude-opus-5-5": "Opus 5.5", "claude-sonnet-5-5": "Sonnet 5.5", "claude-haiku-5-5": "Haiku 5.5",
                     "claude-fable-5-1": "Fable 5.1", "gpt-6-astra": "GPT-6-Astra", "auto": "Auto"]
        return names[model] ?? model
    }
}

/// `GET /v1/settings/jobs`: how many jobs run at once and, on obid, whether a job past its
/// account's usage limit goes on on another account (`accountFailover`); nil from a runner without it.
nonisolated struct JobSettings: Codable, Equatable, Sendable {
    var maxRunning: Int?
    var accountFailover: Bool?
}

nonisolated struct ModelSettings: Codable, Equatable, Sendable {
    var claude: ModelChoice
    var codex: ModelChoice
    /// Cursor has a model and no effort; an older runner doesn't send it.
    var cursor: CursorModelChoice? = nil
}

nonisolated struct CursorModelChoice: Codable, Equatable, Sendable {
    var model: String
}

nonisolated struct ModelOptions: Codable, Equatable, Sendable {
    struct Model: Codable, Equatable, Sendable {
        var id: String
        var name: String
        var efforts: [String]?
    }
    struct Tool: Codable, Equatable, Sendable {
        var models: [Model]
        var efforts: [String]?
        var complexEfforts: [String]?
    }
    var claude: Tool
    var codex: Tool
    /// Cursor's models, with no efforts; an older runner doesn't send it.
    var cursor: Tool? = nil
}

/// The runner's accounts as the API sends them: the iPhone's own wire type.
typealias RunnerAccount = Account

nonisolated struct RunnerStatus: Decodable, Equatable, Sendable {
    struct Runner: Codable, Equatable, Sendable {
        var version: String
        var host: String
    }
    var runner: Runner
    var accounts: [RunnerAccount]
    var models: ModelSettings
    /// When Clear done was last pressed, on any app, or nil (docs/runner-api.md). Older runners don't send it.
    var clearedAt: Date?
    /// The default mode new jobs get, or nil from older runners.
    var permissions: Permissions?
    var cleanup: Cleanup?
    /// Who the runner's own GitHub calls act as: `app` (Oby's GitHub app) or `owner` (the owner's
    /// `gh` login). Older runners don't send it.
    var github: String?
    /// Whether Oby's review and Mac check go on pull requests as check runs: `ok`, `missing
    /// permission` or `no app`. Older runners don't send it.
    var checks: String?
    /// The tools' own logins obid found on this Mac that aren't accounts (obid only), each listed
    /// with `restoreDefaultLogin`. Older runners don't send it.
    var availableLogins: [AvailableLogin]?
    /// The account new tasks go to (Settings › Agents), or nil for routing's own order (obid only).
    var defaultAccount: String?
    /// Who new conversations talk through (an account's id, nil for Auto), and the account a new
    /// conversation's first turn would use now (obid only, docs/contracts/conversation-agent.md).
    var talkAccount: String?
    var talkNext: String?

    struct AvailableLogin: Codable, Equatable, Sendable {
        var agent: String
        var id: String
    }

    struct Cleanup: Codable, Equatable, Sendable {
        var path: String
        var bytes: Int64
        var lastRun: Date?
    }

    struct Permissions: Codable, Equatable, Sendable {
        var mode: String?
    }
}

/// The Obysphere obid is in, as `GET /v1/obid/sphere` says: only what the side panel shows.
nonisolated struct RunnerSphere: Decodable, Equatable, Sendable {
    /// `none`, `joined` or `revoked`.
    var state: String
    /// Its id: obid knows no other name for it (the invite link's stays with the app that joined).
    var sphere: String?
    /// Who obid acts for there.
    var owner: String? = nil
}

/// Another Oby in that Obysphere (`GET /v1/obid/sphere/members`): the owner it speaks for.
nonisolated struct RunnerSphereMember: Decodable, Equatable, Sendable {
    /// Its id there: one owner can have several Obys.
    var obi: String
    var owner: String
}

/// What `POST /v1/clear` answers, and what the `clear` event carries.
nonisolated struct ClearedAt: Codable, Equatable, Sendable {
    var clearedAt: Date
}

/// What `POST /v1/stop-all` answers: how many jobs stopped, and those jobs.
nonisolated struct StoppedJobs: Decodable, Sendable {
    var stopped: Int
    var jobs: [Job]
}

/// One thing the runner reports while the app is open.
#if !os(watchOS)
/// A grouped owner choice for jobs stopped by one account limit (obid #1563).
nonisolated struct RunnerLimitDecision: Decodable, Identifiable, Equatable, Sendable {
    struct Work: Decodable, Identifiable, Equatable, Sendable { let id: String; let title: String }
    struct Option: Decodable, Equatable, Sendable {
        let kind: String
        let account: String?
        let agent: String?
        let name: String?

        var isSupported: Bool { kind == "resume" || kind == "stop" || (kind == "move" && account != nil && agent != nil && name != nil) }
        var label: String {
            switch kind {
            case "move": "Move all to \(name ?? "Account") · \(agent ?? "")"
            case "resume": "Resume when available"
            case "stop": "Keep stopped"
            default: "Update Oby to use this choice"
            }
        }
    }
    let id: String
    let revision: Int
    let account: String
    let agent: String
    let resetAt: Int?
    let jobs: [Work]
    let options: [Option]
    let problem: String?
}
#endif

nonisolated enum RunnerEvent: Equatable, Sendable {
    /// The runner is following this app now: fetch what's there. Whatever changes meanwhile
    /// arrives after this, so applying it over the fetched lists loses nothing.
    case connected
    case job(Job)
    /// A job was deleted, here or on another app (obid only): it leaves every list.
    case jobDeleted(String)
    case accounts([RunnerAccount])
    #if !os(watchOS)
    case limitDecision
    #endif
    /// Clear done was pressed, here or on another app: jobs that finished by then are hidden.
    case cleared(Date)
    /// An inbox item came, was answered, done or put off, here or on another app.
    case inbox(InboxItem)
    /// A turn of the conversation was added, from any app, or changed (a job dispatched from it):
    /// the iPhone's thread replaces it by id, or add it by time (#11).
    /// With what the app's Turn doesn't keep (obid only): the artifacts it made, and whether it's
    /// the turn a job adds when it ends.
    case turn(Turn, TurnExtras = TurnExtras())
    /// One step of Oby's turn began, went on or ended (#304). Runners from before the step event never send it.
    case step(TurnStep)
    /// One of obid's conversations started, got a turn, was renamed, pinned or deleted (#493).
    case conversation(RunnerConversation)
    /// A piece of Oby's words as its agent writes them (obid only, the conversation harness).
    case delta(TurnDelta)
    /// An artifact was made, got a version, or was deleted (obid only).
    case artifact(ArtifactEvent)
    /// A routine was made, changed, paused, run or deleted (obid only, #395): Settings › Routines
    /// reads them again.
    case routine
    /// Your work on GitHub changed, or obid's gh signed in or out (obid only): the whole list.
    case github(GitHubWork)
    /// A job's session got an entry, one changed, or the agent wrote more words (obid only, #755, #784).
    case transcript(TranscriptEvent)
}

/// One turn of the conversation with Oby: the iPhone reads the runner's turns into this
/// (apps/shared/ConversationModel.swift).
nonisolated struct Turn: Codable, Identifiable, Equatable, Sendable {
    enum Who: String, Codable, Sendable { case user, oby }

    var id: String
    var date: Date
    var who: Who
    var text: String
    /// The jobs this turn started, for a card under Oby's words. Nil on the user's turns and on older ones.
    var jobs: [String]?
    /// The user said it out loud (the iPhone's mic), so the thread says "You, by voice". Nil when
    /// typed, and on turns the runner kept.
    var voice: Bool?
    /// Longer Markdown answer beneath Oby's short reply; nil on older turns.
    var details: String?
    /// The conversation it's in (obid only, #493), or nil from a runner with one thread.
    var conversation: String?

    init(id: String = UUID().uuidString.lowercased(), date: Date = .now, who: Who, text: String,
         jobs: [String]? = nil, voice: Bool? = nil, details: String? = nil, conversation: String? = nil) {
        self.id = id
        self.date = date
        self.who = who
        self.text = text
        self.jobs = jobs
        self.voice = voice
        self.details = details
        self.conversation = conversation
    }
}

/// One turn of the runner's conversation as `GET /v1/conversation` and the `turn` event send it
/// (docs/runner-api.md), read into the app's Turn.
nonisolated struct RunnerTurn: Decodable, Sendable {
    var id: String
    var who: String
    var text: String
    var at: Date
    var jobs: [String]?
    var details: String?
    /// obid only (#493): the conversation it's in.
    var conversation: String?
    /// obid only: what the turn made to be looked at.
    var artifacts: [ArtifactRef]?
    /// obid only (#351): the turn a job adds to its conversation when it ends, not an answer.
    var ended: Bool?
    /// obid only (#395): the routine whose run the turn tells of.
    var routine: RoutineMark?

    var extras: TurnExtras { TurnExtras(artifacts: artifacts ?? [], ended: ended == true, routine: routine != nil) }

    private enum CodingKeys: String, CodingKey { case id, who, text, at, jobs, details, conversation, artifacts, ended, routine }

    init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        who = try c.decode(String.self, forKey: .who)
        text = try c.decode(String.self, forKey: .text)
        at = try c.decode(Date.self, forKey: .at)
        jobs = try c.decodeIfPresent([String].self, forKey: .jobs)
        details = try c.decodeIfPresent(String.self, forKey: .details)
        conversation = try c.decodeIfPresent(String.self, forKey: .conversation)
        // Tolerant: a list this app can't read leaves the turn without cards, not unread.
        artifacts = try? c.decodeIfPresent([ArtifactRef].self, forKey: .artifacts)
        ended = try? c.decodeIfPresent(Bool.self, forKey: .ended)
        routine = try? c.decodeIfPresent(RoutineMark.self, forKey: .routine)
    }

    var turn: Turn {
        Turn(id: id, date: at, who: Turn.Who(rawValue: who) ?? .oby, text: text,
             jobs: (jobs ?? []).isEmpty ? nil : jobs, details: details, conversation: conversation)
    }
}

/// One step of Oby's turn as the `step` event sends it (docs/runner-api.md, #304): "Moving Vault
/// secrets to Codex · Deskree", running, then done or failed. The apps list a turn's steps under
/// the user's message, then Oby's answer. A step comes again with each change of its `state`, by
/// the same `turn` and `index`.
nonisolated struct TurnStep: Codable, Identifiable, Equatable, Sendable {
    enum State: String, Codable, Sendable { case running, done, failed }

    /// The user's turn this step belongs to.
    var turn: String
    /// Where it stands in the turn, from 1.
    var index: Int
    /// What kind of step, like `move_job`; nil when the runner doesn't say.
    var action: String?
    var text: String
    var state: State
    var at: Date?
    /// The conversation its turn is in (obid only, #493), or nil.
    var conversation: String?
    /// On a tool call of a conversation's agent (`action` "tool", obid only): which tool (`read`,
    /// `web_search`, `bash`, `oby_dispatch`…) and what on (the file, query, URL or command).
    var tool: String?
    var detail: String?

    var id: String { "\(turn)#\(index)" }

    init(turn: String, index: Int, action: String? = nil, text: String, state: State, at: Date? = nil, conversation: String? = nil,
         tool: String? = nil, detail: String? = nil) {
        self.turn = turn
        self.index = index
        self.action = action
        self.text = text
        self.state = state
        self.at = at
        self.conversation = conversation
        self.tool = tool
        self.detail = detail
    }

    private enum CodingKeys: String, CodingKey { case turn, index, action, text, state, at, conversation, tool, detail }

    /// Tolerant of what a newer runner adds: a state this app doesn't know reads as done, and a
    /// time that doesn't parse is left out, so the step still shows.
    init(from decoder: any Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        turn = try c.decode(String.self, forKey: .turn)
        index = try c.decode(Int.self, forKey: .index)
        action = try? c.decodeIfPresent(String.self, forKey: .action)
        text = try c.decode(String.self, forKey: .text)
        state = (try? c.decode(String.self, forKey: .state)).flatMap(State.init(rawValue:)) ?? .done
        at = try? c.decodeIfPresent(Date.self, forKey: .at)
        conversation = try? c.decodeIfPresent(String.self, forKey: .conversation)
        tool = try? c.decodeIfPresent(String.self, forKey: .tool)
        detail = try? c.decodeIfPresent(String.self, forKey: .detail)
    }
}

extension Job {
    /// Queued until the Mac has room for it (obid's admission by memory, docs/runner-api.md):
    /// `Waiting.memory`, the one constant both Jobs match.
    nonisolated var waitsForMemory: Bool { status == .queued && waiting == .memory }

    nonisolated var pullRequestState: String? { pullRequest?.state }
    nonisolated var reviewDecision: String? { pullRequest?.reviewDecision }

    /// What One more round and Try again send (docs/runner-api.md, `POST /v1/jobs/{id}/messages`):
    /// the agent's session picks up where it stopped, in the same worktree and branch.
    nonisolated static let tryAgainWords = "Try again, picking up where you left off. Finish with a short summary as asked."

    /// The one next step of a job that stopped part way and needs you (obid's `interrupted`), or nil.
    nonisolated var nextStepOffered: String? { status == .interrupted && dismissed == nil ? nextStep : nil }

    /// When obid tries it again by itself, as its decision card says it: "At 3:04 PM, or now with
    /// Try again.", or nil.
    nonisolated var resumesLine: String? {
        guard nextStepOffered != nil, let resumes else { return nil }
        return "At \(resumes.formatted(date: .omitted, time: .shortened)), or now with \(nextStep ?? "Try again")."
    }

    /// Its decision card's kind (Sangam's labels): Out of usage when its agent hit a usage limit.
    nonisolated var needsYouKind: String {
        (reason ?? "").range(of: #"(?i)\b(usage|rate)[ -]?limit|hit your limit|limit (reached|exceeded)\b"#, options: .regularExpression) == nil
            ? "Needs you" : "Out of usage"
    }

    /// A finished job can run again for the user's words: it has a session to pick up, or it was
    /// moved since its last round and starts a new one where it went (docs/runner-api.md).
    nonisolated var resumable: Bool { session != nil || (started != nil && !(moves ?? []).isEmpty) }

    /// "After #223 merges", "Runner update": a waiting job's row, under the Waiting heading (#238).
    nonisolated static func waitingLine(_ text: String) -> String { text.prefix(1).uppercased() + text.dropFirst() }

    /// Where the list puts the job (apps/shared/JobList.swift): what needs you, the ones working, the
    /// ones waiting their turn (#238), published ones whose pull request is still open, then
    /// everything else.
    nonisolated var group: JobGroup {
        if needsYou { return .needsYou }
        if isWaiting { return .waiting }
        if isActive { return .working }
        if status == .done, published != nil, (pullRequestState ?? "OPEN") == "OPEN" { return .inReview }
        return .done
    }

    /// Nothing more is to happen for it, so Clear done may take it (`JobList.settled`, #280): it
    /// was dismissed or stopped, its pull request was merged or closed, or it ended with nothing
    /// to publish. `jobs` is the list it's in, where a review job finds the job it reviews.
    nonisolated func settled(among jobs: [Job]) -> Bool {
        let author = reviewOf.flatMap { id in jobs.first { $0.id == id } }
        return JobList.settled(active: isActive || waiting != nil, needsYou: needsYou, letGo: dismissed != nil || status == .stopped,
                               openPullRequest: published != nil && (pullRequestState ?? "OPEN") == "OPEN",
                               author: author.map { $0.settled(among: []) })
    }

    /// Whether the list shows the job after the last Clear done, on any app (apps/shared/JobList.swift).
    nonisolated func listed(after clearedAt: Date?, among jobs: [Job]) -> Bool {
        JobList.lists(settled: settled(among: jobs), finished: finished, clearedAt: clearedAt)
    }

    /// Clear done would take it from the list (apps/shared/JobList.swift).
    nonisolated func clearable(among jobs: [Job]) -> Bool { JobList.clears(settled: settled(among: jobs)) }

    /// The status pill, the same on the iPhone and the Mac: how it's going, how it ended, and
    /// where its pull request is. A job you let go says Dismissed, however it ended.
    nonisolated var badge: JobBadge {
        if dismissed != nil { return JobBadge(text: "Dismissed", tone: .gray) }
        if waitsForPermission { return JobBadge(text: "Awaiting approval", tone: .orange) }
        if waitsForAnswer { return JobBadge(text: "Needs input", tone: .orange) }
        if step == "Waiting for your OK", isActive { return JobBadge(text: "Awaiting approval", tone: .orange) }
        switch status {
        case .queued where waiting != nil: return .waiting
        case .queued: return JobBadge(text: "Starting", tone: .blue)
        case .running: return JobBadge(text: "Working", tone: .blue)
        case .done where canPublish: return JobBadge(text: "Ready to publish", tone: .orange)
        case .done where reviewNeedsYou || mergeNeedsYou || cloudNeedsYou: return JobBadge(text: "Needs input", tone: .orange)
        case .done where pullRequestState == "MERGED": return JobBadge(text: "Merged", tone: .green)
        case .done where pullRequestState == "CLOSED": return JobBadge(text: "PR closed", tone: .gray)
        case .done where reviewDecision == "CHANGES_REQUESTED": return JobBadge(text: "Needs input", tone: .orange)
        case .done where published != nil || pullRequestState == "OPEN": return JobBadge(text: "In review", tone: .blue)
        case .done: return JobBadge(text: "Ready", tone: .green)
        case .failed: return JobBadge(text: "Failed", tone: .red)
        case .stopped: return JobBadge(text: "Stopped", tone: .gray)
        case .interrupted: return JobBadge(text: "Needs you", tone: .orange)
        }
    }

    /// Oby's own verdict on the pull request, next to the pill: "Oby approved", "Oby requested
    /// changes" (with the review's round after the first), "Oby is reviewing", or nil.
    nonisolated var verdictLine: String? {
        guard let review else { return reviewing == nil ? nil : "Oby is reviewing" }
        let verdict = review.label == "Approved" ? "Oby approved" : "Oby requested changes"
        return review.round > 1 ? "\(verdict) (review \(review.round))" : verdict
    }
}

/// What needs the user among these jobs, in one line, like "2 jobs are ready to publish, 1 job
/// failed", or nil when nothing does. Home on the iPhone and the Mac's island both say it, through
/// `Summary.line` (apps/shared/JobList.swift). Dismissed jobs aren't counted.
nonisolated enum NeedsYou {
    static func line(_ jobs: [Job]) -> String? {
        func count(_ n: Int, _ one: String, _ many: String) -> String? { n == 0 ? nil : n == 1 ? "1 job \(one)" : "\(n) jobs \(many)" }
        let needing = jobs.filter(\.needsYou)
        let parts = [
            count(needing.count(where: \.canPublish), "is ready to publish", "are ready to publish"),
            count(needing.count { $0.status == .failed }, "failed", "failed"),
            count(needing.count { $0.status == .interrupted }, "needs you", "need you"),
            count(needing.count(where: \.reviewNeedsYou), "needs your review", "need your review"),
            count(needing.count(where: \.stranded), "wasn't published", "weren't published"),
            count(needing.count(where: \.mergeNeedsYou), "wasn't merged", "weren't merged"),
        ].compactMap(\.self)
        return parts.isEmpty ? nil : parts.joined(separator: ", ")
    }
}

/// How an app's line to the runner is: Settings says it under the runner's address.
nonisolated enum RunnerConnection: Equatable, Sendable {
    case connecting
    case live
    case offline(String)

    var isOffline: Bool { if case .offline = self { true } else { false } }

    /// "Connected to tetrix-lab", "Connecting…", or why it's offline.
    func status(host: String) -> String {
        switch self {
        case .live: "Connected to \(host)"
        case .connecting: "Connecting…"
        case .offline(let why): why
        }
    }
}

nonisolated struct RunnerError: LocalizedError {
    let message: String
    /// The runner's HTTP status, when it answered with an error.
    var status: Int? = nil
    var errorDescription: String? { message }

    // A person never reads "runner" (the owner, 2026-11-01).
    static let unpaired = "Pair with your Mac first."
    static let notLetIn = "Your Mac didn't let this device in."
}

/// obid, reached with a `token` sent as `Authorization: Bearer` on every request, or with none where
/// the address vouches for who's asking.
nonisolated struct Runner: Sendable {
    var base: URL
    var token: String?
    /// What a 401 means here: an iPhone paired with a Mac says its pairing ended (docs/oby-ux.md
    /// row 34); nil says the generic line.
    var refused: String?
    /// An iPhone paired through the Mac's Obysphere (#1278) sends every request sealed through it,
    /// never over HTTP.
    var relay: PhoneRelay?
    /// Signs a request (every one but a read) as words the owner typed in this app
    /// (`DeviceKey.signWords`): given its method, its path and its body (empty when it has none),
    /// the `X-Oby-Words` header, or nil until this device's key is registered. Without it, nothing the app sends counts as the owner's own words for the
    /// secrets a check declares (docs/runner-api.md).
    typealias SignsWords = @Sendable (_ method: String, _ path: String, _ body: Data) -> String?
    var signsWords: SignsWords?

    init(base: URL, token: String? = nil, refused: String? = nil, relay: PhoneRelay? = nil) {
        self.base = base
        self.token = token
        self.refused = refused
        self.relay = relay
    }

    /// Nowhere: what a request goes to with no Mac paired and no runner set (port 9, discard),
    /// so a fresh iPhone talks to nobody (docs/oby-ux.md row 33).
    static let nowhere = URL(string: "http://127.0.0.1:9")!

    /// `request`'s answer, through the relay when there is one.
    private func fetch(_ request: URLRequest) async throws -> (Data, URLResponse) {
        if let relay { return try await relay.data(for: request) }
        try Self.reachable(base)
        return try await URLSession.shared.data(for: request)
    }

    /// Addressed to a Mac's relay with no relay to send through (its keychain item gone, or not the
    /// one the pairing pinned): nothing goes, and the iPhone says to pair again, as for a 401.
    private static func reachable(_ base: URL) throws {
        guard base != RemoteRoute.relayBase else {
            throw RunnerError(message: "This iPhone can't reach your Mac through its Obysphere any more. Pair again with the code in Oby on your Mac.", status: 401)
        }
    }

    /// `request` with the token, when there is one.
    private func signed(_ request: URLRequest) -> URLRequest {
        guard let token else { return request }
        var request = request
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        return request
    }

    static let decoder: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        return decoder
    }()

    static let encoder: JSONEncoder = {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }()

    func status() async throws -> RunnerStatus { try await call("GET", "v1/status") }
    func jobs() async throws -> [Job] { try await call("GET", "v1/jobs?limit=50") }
    func stop(_ id: String) async throws -> Job { try await call("POST", "v1/jobs/\(id)/stop") }
    /// Stop all: every queued and running job on the runner stops now.
    func stopAll() async throws -> StoppedJobs { try await call("POST", "v1/stop-all") }
    /// Lets go of a job that needs the user: it then needs nothing. No signature: nothing goes out.
    func dismiss(_ id: String) async throws -> Job { try await call("POST", "v1/jobs/\(id)/dismiss") }
    /// Renames a job (obid only, `PATCH /v1/jobs/{id}`): one line of 1 to 60 characters.
    func rename(_ id: String, to title: String) async throws -> Job {
        struct Body: Encodable { var title: String }
        return try await call("PATCH", "v1/jobs/\(id)", body: Body(title: title))
    }
    /// Archives a job that ended and needs nothing, or unarchives it (obid only).
    func archive(_ id: String, _ archived: Bool = true) async throws -> Job {
        struct Body: Encodable { var archived: Bool }
        return try await call("POST", "v1/jobs/\(id)/archive", body: Body(archived: archived))
    }
    /// Deletes a job that ended, for good (obid only): its record, worktree and local branch in
    /// obid's clone. Nothing pushed is touched: its branch and pull request stay on GitHub.
    func deleteJob(_ id: String) async throws { try await send("DELETE", "v1/jobs/\(id)") }
    #if !os(watchOS)
    /// The apps' Delete task (obid only, `POST /v1/jobs/{id}/delete`): gone at once, or, with
    /// commits that aren't published or work that isn't committed, obid's Confirm (`ask`) first.
    func deleteAsked(_ id: String) async throws -> DeleteAnswer { try await call("POST", "v1/jobs/\(id)/delete") }
    #endif
    /// Clear done: the runner records now, and every app hides the jobs that finished by then.
    func clear() async throws -> Date {
        let answer: ClearedAt = try await call("POST", "v1/clear")
        return answer.clearedAt
    }

    /// What waits in the inbox, put-off items too. A runner older than the inbox has none (it
    /// doesn't know the path); any other failure throws, so the app tries again.
    func inbox() async throws -> [InboxItem] {
        do {
            return try await call("GET", "v1/inbox")
        } catch let error as RunnerError where error.status == 404 {
            return []
        }
    }

    /// One job, however old: the list has only the latest 50.
    func job(_ id: String) async throws -> Job { try await call("GET", "v1/jobs/\(id)") }

    /// Your work on GitHub as obid's gh finds it (docs/runner-api.md, "obid only: GitHub work"),
    /// or nil from a runner that doesn't list it.
    func gitHubWork() async throws -> GitHubWork? {
        do {
            return try await call("GET", "v1/github/work")
        } catch let error as RunnerError where error.status == 404 {
            return nil
        }
    }

    /// Has obid read GitHub now; answers the feed once the read ends (up to 2 minutes).
    func refreshGitHubWork() async throws -> GitHubWork { try await call("POST", "v1/github/work/refresh", timeout: 150) }

    /// Starts Oby's job on a GitHub item, a review or a fix: the new job, or the one already on it.
    @discardableResult
    func startGitHubWork(_ id: String) async throws -> Job {
        try await call("POST", "v1/github/work/\(id.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? id)/start")
    }
    func report(_ id: String) async throws -> ResearchDocument { try await call("GET", "v1/jobs/\(id)/report") }

    /// One entry's detail (#784): what the call was given, what it printed and its diff.
    func transcriptDetail(_ id: String, _ n: Int) async throws -> TranscriptDetail {
        try TranscriptDetail.read(await send("GET", "v1/jobs/\(id)/transcript/\(n)"))
    }

    /// The job's raw session after line `after` (#381), oldest first: a list, or `{entries: […]}`.
    func transcript(_ id: String, after: Int? = nil) async throws -> TranscriptPage {
        try TranscriptPage.read(await send("GET", "v1/jobs/\(id)/transcript" + (after.map { "?after=\($0)" } ?? "")))
    }
    func fileProposal(_ id: String, index: Int, start: Bool) async throws -> FileProposalResult {
        try await call("POST", "v1/jobs/\(id)/proposals/\(index)/file", body: FileProposalBody(start: start))
    }

    /// Settles an inbox item: an answer (one of its options, or the user's words) goes to its job;
    /// `done` lets it go without one; `later` puts it off until then. Send one of them.
    func settle(_ id: String, answer: String? = nil, done: Bool? = nil, later: Date? = nil) async throws -> InboxItem {
        struct Body: Encodable { var answer: String?; var done: Bool?; var later: Date? }
        return try await call("POST", "v1/inbox/\(Self.inboxSegment(id))/answer", body: Body(answer: answer, done: done, later: later))
    }

    /// An inbox item's id as one path segment: an id from elsewhere (a push) can't reach another route.
    static func inboxSegment(_ id: String) -> String { id.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? id }

    /// Moves a job to another tool, account or model (each nil to keep it or let the runner pick):
    /// the runner answers with the job, or why it couldn't.
    func move(_ id: String, agent: String?, account: String?, model: String?, why: String? = nil) async throws -> Job {
        try await call("POST", "v1/jobs/\(id)/move", body: MoveBody(agent: agent, account: account, model: model, why: why))
    }

    /// "Use anyway" (#278): the account takes automatic work past its weekly reserve until its
    /// week resets, or with `lift` false is reserved again. The runner answers with the account.
    func liftReserve(_ id: String, _ lift: Bool) async throws -> RunnerAccount {
        struct Body: Encodable { var lift: Bool }
        return try await call("POST", "v1/accounts/\(id)/reserve", body: Body(lift: lift))
    }

    /// Starts a job that waits for its `startWhen` now (#216): the runner answers with the job.
    func startNow(_ id: String) async throws -> Job { try await call("POST", "v1/jobs/\(id)/start") }

    /// Tells the job's agent something: the runner answers with the job, or with the follow-up a
    /// finished one gets (`followUpOf` set), except Try again's words, which pick its session up again.
    /// `conversation`: said on the task's page, the words and Oby's answer join the job's
    /// conversation (obid only); `attachments` go with the follow-up.
    func tell(_ id: String, _ text: String, conversation: Bool = false, attachments: [String] = []) async throws -> Job {
        struct Body: Encodable { var text: String; var conversation: Bool?; var attachments: [String]? }
        return try await call("POST", "v1/jobs/\(id)/messages",
                              body: Body(text: text, conversation: conversation ? true : nil, attachments: attachments.isEmpty ? nil : attachments))
    }
    func jobSettings() async throws -> JobSettings { try await call("GET", "v1/settings/jobs") }

    /// Turns obid's account failover on or off (`PUT /v1/settings/jobs` with only `accountFailover`).
    func setAccountFailover(_ on: Bool) async throws -> JobSettings {
        struct Body: Encodable { var accountFailover: Bool }
        return try await call("PUT", "v1/settings/jobs", body: Body(accountFailover: on))
    }

    func models() async throws -> ModelSettings { try await call("GET", "v1/settings/models") }
    func modelOptions() async throws -> ModelOptions { try await call("GET", "v1/settings/models/options") }

    /// How long a plan's request waits: a turn that moves jobs and answers takes longer than the brain alone (#304).
    static let planTimeout: TimeInterval = 120

    func plan(_ said: String, answer: (question: String, option: String)? = nil, attachments: [String] = []) async throws -> PlanResult {
        try await planned(said, answer: answer, attachments: attachments).result
    }

    /// The same, in one of obid's conversations (#493): `conversation` names the one the words
    /// join, `new` starts one; neither, the latest. The answer says which they joined (nil from a
    /// runner with one thread).
    /// `id`: the id the app gave the words (its echo's, a lower-case UUID), which obid's turn of them
    /// takes; sent again with it (Try again after a lost answer), obid adds nothing (obid only).
    /// `runsOn`: the teammate's Obi whose Mac the jobs these words start run on (obid only, #808).
    /// `jobs`: the tasks attached to the words (Sangam's Attach tasks, obid only, #1173), left out when none.
    func planned(_ said: String, answer: (question: String, option: String)? = nil, attachments: [String] = [],
                 conversation: String? = nil, new: Bool = false, id: String? = nil, runsOn: String? = nil,
                 jobs: [String] = []) async throws -> PlannedTurn {
        struct Answer: Encodable { var question: String; var option: String }
        struct Body: Encodable { var said: String; var answer: Answer?; var attachments: [String]; var conversation: String?; var new: Bool?; var id: String?; var runsOn: String?; var jobs: [String]? }
        let body = Body(said: said, answer: answer.map { Answer(question: $0.question, option: $0.option) }, attachments: attachments,
                        conversation: conversation, new: new ? true : nil, id: id, runsOn: runsOn, jobs: jobs.isEmpty ? nil : jobs)
        return try await call("POST", "v1/plans", body: body, timeout: Self.planTimeout)
    }

    // MARK: Routines (obid only, #395)

    /// obid's routines, by name, or nil from a runner that has none (the Swift runner).
    func routines() async throws -> [RunnerRoutine]? {
        do {
            return try await call("GET", "v1/routines")
        } catch let error as RunnerError where error.status == 404 {
            return nil
        }
    }

    /// Pauses (false) or resumes (true) a routine; obid answers with it.
    func setRoutine(_ id: String, enabled: Bool) async throws -> RunnerRoutine {
        struct Body: Encodable { var enabled: Bool }
        return try await call("PATCH", "v1/routines/\(id)", body: Body(enabled: enabled))
    }

    /// Run now: its job starts, and its next time stays as it was.
    func runRoutine(_ id: String) async throws {
        try await send("POST", "v1/routines/\(id)/run")
    }

    /// Forgets a routine; the jobs its runs started stay.
    func deleteRoutine(_ id: String) async throws {
        try await send("DELETE", "v1/routines/\(id)")
    }

    // MARK: Memory (obid only, #378)

    /// obid's memories and the suggestions waiting for an answer, or nil from a runner with no
    /// memory (the Swift runner).
    func memory() async throws -> RunnerMemoryList? {
        do {
            return try await call("GET", "v1/memory")
        } catch let error as RunnerError where error.status == 404 {
            return nil
        }
    }

    /// Find what I keep repeating: obid reads the owner's conversations and the reviews on merged
    /// pull requests, and answers with what it could suggest. It takes a while.
    func findMemories() async throws -> RunnerMemoryFound {
        try await call("POST", "v1/memory/find", timeout: 150)
    }

    /// Remember: the owner's yes to a suggestion; obid answers with the memory it saved.
    func remember(_ suggestion: String) async throws -> RunnerMemory {
        try await call("POST", "v1/memory/suggestions/\(suggestion)/remember")
    }

    /// Not this: the suggestion goes, and nothing is saved.
    func dismissSuggestion(_ suggestion: String) async throws {
        try await send("POST", "v1/memory/suggestions/\(suggestion)/dismiss")
    }

    func deleteMemory(_ id: String) async throws {
        try await send("DELETE", "v1/memory/\(id)")
    }

    // MARK: Conversations (obid only, #493)

    /// obid's conversations, pinned first, then newest; with `query`, the ones that hold its words.
    /// Nil from a runner that keeps one thread (it doesn't know the path).
    func conversations(query: String? = nil, limit: Int = 100) async throws -> [RunnerConversation]? {
        try await conversationsAnswer(query: query, limit: limit).map(Self.conversations)
    }

    /// `conversations`' answer as it came, or nil (404): what the Mac keeps for its next launch.
    func conversationsAnswer(query: String? = nil, limit: Int = 100) async throws -> Data? {
        var path = "v1/conversations?limit=\(limit)"
        if let query, let q = query.addingPercentEncoding(withAllowedCharacters: .alphanumerics) { path += "&q=\(q)" }
        do {
            return try await send("GET", path)
        } catch let error as RunnerError where error.status == 404 {
            return nil
        }
    }

    /// The conversations in a conversations answer.
    static func conversations(_ data: Data) throws -> [RunnerConversation] {
        try decoder.decode([RunnerConversation].self, from: data)
    }

    /// Renames or pins a conversation, or chooses who it talks through (an account's id, or `auto`);
    /// nil keeps each. obid answers with it.
    func updateConversation(_ id: String, title: String? = nil, pinned: Bool? = nil, talkAccount: String? = nil) async throws -> RunnerConversation {
        struct Body: Encodable { var title: String?; var pinned: Bool?; var talkAccount: String? }
        return try await call("PATCH", "v1/conversations/\(id)", body: Body(title: title, pinned: pinned, talkAccount: talkAccount))
    }

    /// The Obysphere obid is in (obid only).
    func sphere() async throws -> RunnerSphere { try await call("GET", "v1/obid/sphere") }

    /// The other Obys in it (obid only).
    func sphereMembers() async throws -> [RunnerSphereMember] { try await call("GET", "v1/obid/sphere/members") }

    /// Forgets a conversation and its turns; the jobs it started stay.
    func deleteConversation(_ id: String) async throws {
        try await send("DELETE", "v1/conversations/\(id)")
    }

    /// Stop: the turn Oby is working on ends where it is (`POST /v1/conversation/stop`, #304). A
    /// runner from before it answers 404. On obid, `conversation` names the one whose turn stops;
    /// without it obid stops the latest turn under way, in whichever conversation.
    func stopConversation(_ conversation: String? = nil) async throws {
        try await send("POST", "v1/conversation/stop", body: conversation.map(TurnTarget.init))
    }

    func cancelPlan(_ conversation: String? = nil) async {
        _ = try? await send("POST", "v1/plans/cancel", body: conversation.map(TurnTarget.init))
    }

    /// The body naming the conversation a stop or a cancel is for (obid only, #493).
    private struct TurnTarget: Encodable { var conversation: String }

    func dispatch(_ job: Job) async throws -> Job {
        struct Body: Encodable { var job: DispatchedJob; var mode: String? }
        return try await call("POST", "v1/jobs", body: Body(job: DispatchedJob(job: job), mode: Self.mode(of: job)))
    }

    static func mode(of job: Job) -> String? {
        return job.mode
    }

    // MARK: Permissions (#134)

    /// Changes a job's mode; a permission it waits on that the new mode allows is answered.
    func setMode(_ id: String, _ mode: RunnerMode) async throws -> Job {
        struct Body: Encodable { var mode: RunnerMode }
        return try await call("POST", "v1/jobs/\(id)/mode", body: Body(mode: mode))
    }

    /// Answers the permission a job waits on. Words go with a deny, for the agent.
    func answer(_ job: String, permission: String, _ decision: RunnerPermission.Decision, message: String? = nil) async throws -> Job {
        struct Body: Encodable { var id: String; var decision: RunnerPermission.Decision; var message: String? }
        return try await call("POST", "v1/jobs/\(job)/permission", body: Body(id: permission, decision: decision, message: message))
    }

    /// The mode new jobs get when a dispatch doesn't pick one.
    func setDefaultMode(_ mode: RunnerMode) async throws {
        struct Body: Encodable { var mode: RunnerMode }
        try await send("PUT", "v1/settings/permissions", body: Body(mode: mode))
    }

    #if !os(watchOS)
    /// Settings › Agents (obid only): an account's model, `effort` or `complexEffort`, only the keys
    /// changed (`PATCH /v1/accounts/{id}`); answers the account.
    func tuneAccount(_ id: String, _ change: [String: String]) async throws -> Account {
        try await call("PATCH", "v1/accounts/\(id)", body: change)
    }

    /// The account new tasks go to (`PUT /v1/settings/jobs` with only `defaultAccount`, as Electron's).
    func setDefaultAccount(_ id: String) async throws {
        struct Body: Encodable { var defaultAccount: String }
        try await send("PUT", "v1/settings/jobs", body: Body(defaultAccount: id))
    }

    /// Who new conversations talk through (`PUT /v1/settings/jobs` with only `talkAccount`): an
    /// account's id, or `auto`.
    func setTalkAccount(_ id: String) async throws {
        struct Body: Encodable { var talkAccount: String }
        try await send("PUT", "v1/settings/jobs", body: Body(talkAccount: id))
    }
    #endif

    /// The rules Always allow (or Deny) saved.
    func permissionRules() async throws -> [RunnerRule] { try await call("GET", "v1/permissions/rules") }

    func removeRule(_ id: String) async throws {
        try await send("DELETE", "v1/permissions/rules/\(id)")
    }

    /// The exact sentence publishing would carry out.
    func publishAction(_ id: String) async throws -> String {
        struct Answer: Decodable { var action: String }
        let answer: Answer = try await call("GET", "v1/jobs/\(id)/publish")
        return answer.action
    }

    // Approvals come from the iPhone's or the Mac's approval key (apps/shared/DeviceKey.swift): the owner's click.
    #if !os(watchOS)
    /// The owner's Confirm of a confirm item, with the device's signature over its title (#776).
    func settle(_ id: String, answer: String, approval: Approval) async throws -> InboxItem {
        struct Body: Encodable { var answer: String; var approval: Approval }
        return try await call("POST", "v1/inbox/\(Self.inboxSegment(id))/answer", body: Body(answer: answer, approval: approval))
    }

    func publish(_ id: String, approval: Approval) async throws -> URL {
        struct Answer: Decodable { var url: URL }
        let answer: Answer = try await call("POST", "v1/jobs/\(id)/publish", body: approval)
        return answer.url
    }
    #endif

    /// The Apple Watches paired with the runner, for its Watch API. The Mac lists and removes them
    /// too; pairing one takes the iPhone, with the watch at hand.
    func watches() async throws -> [PairedWatch] { try await call("GET", "v1/watches") }

    func removeWatch(_ id: String) async throws {
        struct Answer: Decodable { var removed: String }
        let _: Answer = try await call("DELETE", "v1/watches/\(id)")
    }

    #if os(iOS)

    /// Pairs a watch's key, approved with this phone's key, and hands the runner the Access service token
    /// for its vault. Answers the watch's id and the Watch API's address.
    func pairWatch(name: String, publicKey: Data, clientID: String, clientSecret: String, approval: PairingApproval) async throws -> (id: String, url: URL) {
        struct Body: Encodable { var name: String; var publicKey: String; var accessClientId: String; var accessClientSecret: String; var approval: PairingApproval }
        struct Answer: Decodable { var id: String; var url: URL }
        let answer: Answer = try await call("POST", "v1/watches", body: Body(name: name, publicKey: publicKey.base64EncodedString(),
                                                                          accessClientId: clientID, accessClientSecret: clientSecret, approval: approval))
        return (answer.id, answer.url)
    }
    #endif

    /// Registers a device's publishing key (docs/runner-api.md, "POST /v1/devices"): a key the
    /// runner doesn't know needs the owner's presence (a paired phone's own pairing, or the runner's
    /// pairing naming it), else 403.
    func register(name: String, publicKey: Data) async throws -> String {
        struct Body: Encodable { var name: String; var publicKey: String }
        struct Answer: Decodable { var id: String }
        let answer: Answer = try await call("POST", "v1/devices", body: Body(name: name, publicKey: publicKey.base64EncodedString()))
        return answer.id
    }

    #if !os(watchOS)
    /// Forgets device `id`'s publishing key, with a registered key's signature for it ("DELETE /v1/devices/{id}").
    func revokeDevice(_ id: String, approval: Approval) async throws {
        struct Body: Encodable { var approval: Approval }
        struct Answer: Decodable { var revoked: String }
        let _: Answer = try await call("DELETE", "v1/devices/\(id)", body: Body(approval: approval))
    }
    #endif


    /// Server-Sent Events until the connection drops; the caller reconnects.
    func events() -> AsyncThrowingStream<RunnerEvent, Error> {
        AsyncThrowingStream { stream in
            let task = Task {
                do {
                    var request = URLRequest(url: base.appending(path: "v1/events"))
                    request.setValue("text/event-stream", forHTTPHeaderField: "Accept")
                    request.timeoutInterval = 60
                    var parser = EventParser()
                    var line = Data()
                    // Split on newlines only: summaries may hold U+2028, which `lines` would break at.
                    func feed(_ byte: UInt8) {
                        guard byte == 0x0A else { line.append(byte); return }
                        if let event = parser.feed(String(decoding: line, as: UTF8.self)) { stream.yield(event) }
                        line.removeAll(keepingCapacity: true)
                    }
                    if let relay {
                        let (response, body) = try await relay.send(signed(request))
                        try Self.check(response, nil, refused: refused)
                        stream.yield(.connected)
                        for try await chunk in body { chunk.forEach(feed) }
                    } else {
                        try Self.reachable(base)
                        let (bytes, response) = try await URLSession.shared.bytes(for: signed(request))
                        try Self.check(response, nil, refused: refused)
                        // The runner answers once it follows this connection: from here, every change arrives.
                        stream.yield(.connected)
                        for try await byte in bytes { feed(byte) }
                    }
                    stream.finish()
                } catch {
                    stream.finish(throwing: error)
                }
            }
            stream.onTermination = { _ in task.cancel() }
        }
    }

    #if os(iOS)
    /// Hands the runner what it pushes alerts with, through Deskree's relay: the relay's `pushId` and
    /// `pushSecret`, and the public key it seals each alert to (apps/ios/push/SealedPush.swift).
    /// Never the device token, which only the relay has. Answers whether the runner pushes.
    func registerNotifications(_ apns: PushRelay.HandOver) async throws -> Bool {
        struct Body: Encodable { var apns: PushRelay.HandOver }
        struct Answer: Decodable { var push: Bool }
        let answer: Answer = try await call("POST", "v1/notifications", body: Body(apns: apns))
        return answer.push
    }
    #endif

    func call<Answer: Decodable>(_ method: String, _ path: String, body: (any Encodable)? = nil, timeout: TimeInterval = 45, signs: Bool = false) async throws -> Answer {
        try Self.decoder.decode(Answer.self, from: try await send(method, path, body: body, timeout: timeout, signs: signs))
    }

    #if !os(watchOS)
    func limitDecisions() async throws -> [RunnerLimitDecision] {
        try await call("GET", "v1/limit-decisions")
    }

    struct DevicePairingChallenge: Decodable, Sendable { let pairing: String; let nonce: String }

    func devicePairingChallenge(_ id: String) async throws -> DevicePairingChallenge {
        try await call("GET", "v1/devices/\(Self.inboxSegment(id))/pairing")
    }

    func bindDevicePairing(_ id: String, challenge: DevicePairingChallenge, approval: Approval) async throws {
        struct Body: Encodable { let pairing: String; let nonce: String; let approval: Approval }
        _ = try await send("POST", "v1/devices/\(Self.inboxSegment(id))/pairing", body: Body(pairing: challenge.pairing, nonce: challenge.nonce, approval: approval))
    }

    func answerLimit(_ id: String, revision: Int, choice: String, account: String?, approval: Approval) async throws {
        struct Body: Encodable { let revision: Int; let choice: String; let account: String?; let approval: Approval }
        _ = try await send("POST", "v1/limit-decisions/\(Self.inboxSegment(id))/answer", body: Body(revision: revision, choice: choice, account: account, approval: approval))
    }
    #endif

    /// The answer's body, unread: for a call whose answer says nothing the app needs.
    @discardableResult
    /// `json`, when given, is the body as it is (an Obi-to-Obi object keeps its snake_case keys).
    /// `signs`: a read signed too, as a write is: one that takes something (a check, `nextCheck`).
    func send(_ method: String, _ path: String, body: (any Encodable)? = nil, json: Data? = nil, timeout: TimeInterval = 45,
              signs: Bool = false) async throws -> Data {
        var request = URLRequest(url: URL(string: path, relativeTo: base.appending(path: "/"))!)
        request.httpMethod = method
        request.timeoutInterval = timeout
        if let token { request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization") }
        if let body = try json ?? body.map({ try Self.encoder.encode($0) }) {
            request.httpBody = body
            request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        }
        // Everything but a read, a body or none (Remember has none): the path as the runner reads
        // it, without the query.
        if signs || (method != "GET" && method != "HEAD") {
            let route = "/" + (path.split(separator: "?", maxSplits: 1).first.map(String.init) ?? path)
            if let signed = signsWords?(method, route, request.httpBody ?? Data()) {
                request.setValue(signed, forHTTPHeaderField: "X-Oby-Words")
            }
        }
        let (data, response) = try await fetch(signed(request))
        try Self.check(response, data, refused: refused)
        return data
    }


    /// One page of the runner's conversation, oldest first (#11): the latest `limit` turns, or the
    /// ones before the turn `before` names. A runner older than the thread has none.
    /// On obid, `id` names the conversation; without it, the latest.
    func conversation(id: String? = nil, before: String? = nil, limit: Int = 50) async throws -> [Turn] {
        try await conversationTurns(id: id, before: before, limit: limit).map(\.turn)
    }

    /// The same page as the runner sends it, with what the app's Turn doesn't keep (the artifacts).
    func conversationTurns(id: String? = nil, before: String? = nil, limit: Int = 50) async throws -> [RunnerTurn] {
        var query = "limit=\(limit)"
        if let id, let id = id.addingPercentEncoding(withAllowedCharacters: .alphanumerics) { query += "&id=\(id)" }
        if let before, let id = before.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) { query += "&before=\(id)" }
        do {
            return try await call("GET", "v1/conversation?\(query)")
        } catch let error as RunnerError where error.status == 404 && before == nil {
            return []
        }
    }

    /// An artifact obid keeps (docs/runner-api.md, "obid only: artifacts"), or nil when there's no
    /// such one (or the runner keeps none).
    func artifact(_ id: String) async throws -> RunnerArtifact? {
        guard let id = id.addingPercentEncoding(withAllowedCharacters: .alphanumerics) else { return nil }
        do {
            return try await call("GET", "v1/artifacts/\(id)")
        } catch let error as RunnerError where error.status == 404 {
            return nil
        }
    }

    /// One file of an artifact's version, with obid's answer as it is (its status and headers,
    /// the Content-Security-Policy among them): the viewer passes it on to its web view.
    func artifactFile(_ ref: ArtifactRef, path: String) async throws -> (Data, HTTPURLResponse) {
        let segments = ([ref.id, String(ref.version)] + path.split(separator: "/").map(String.init))
            .map { $0.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed.subtracting(["/"])) ?? $0 }
        var request = URLRequest(url: URL(string: "v1/artifacts/" + segments.joined(separator: "/"), relativeTo: base.appending(path: "/"))!)
        request.timeoutInterval = 45
        let (data, response) = try await fetch(signed(request))
        guard let http = response as? HTTPURLResponse else { throw RunnerError(message: "Oby's answer wasn't HTTP.") }
        return (data, http)
    }

    /// `refused`: what a 401 says (a paired iPhone's pairing ended), else the generic line.
    static func check(_ response: URLResponse, _ data: Data?, refused: String? = nil) throws {
        guard let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) else { return }
        if http.statusCode == 401, let refused { throw RunnerError(message: refused, status: 401) }
        struct Problem: Decodable { var error: String }
        if let data, let problem = try? JSONDecoder().decode(Problem.self, from: data) { throw RunnerError(message: problem.error, status: http.statusCode) }
        throw RunnerError(message: http.statusCode == 401 ? RunnerError.notLetIn : "Oby answered \(http.statusCode).",
                          status: http.statusCode)
    }
}

nonisolated struct ResearchDocument: Codable, Equatable, Sendable {
    var job: String
    var commit: String
    var markdown: String
    var proposals: [ResearchProposal]
}

nonisolated struct ResearchProposal: Codable, Equatable, Identifiable, Sendable {
    var index: Int
    var repo: String
    var title: String
    var body: String
    var labels: [String]
    var filed: String?
    var filedAt: Date?
    var id: Int { index }
}

nonisolated struct FileProposalBody: Encodable, Sendable { var start: Bool }
nonisolated struct FileProposalResult: Decodable, Sendable { var issue: String; var jobId: String? }

/// The job `POST /v1/jobs` sends back. Swift leaves nils out, but a missing check keeps the plan's,
/// so no check and the usual model go as null.
nonisolated struct DispatchedJob: Encodable {
    var job: Job

    private enum Nulls: String, CodingKey { case check, model }

    func encode(to encoder: any Encoder) throws {
        try job.encode(to: encoder)
        var nulls = encoder.container(keyedBy: Nulls.self)
        if job.check == nil { try nulls.encodeNil(forKey: .check) }
        if job.model == nil { try nulls.encodeNil(forKey: .model) }
    }
}

/// A watch paired with the runner, as `GET /v1/watches` lists it.
nonisolated struct PairedWatch: Codable, Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    var registered: Date
}

/// An app or API the runner's agents use through its broker, as `GET /v1/connections` lists it
/// (docs/runner-api.md, "Connection"): which vault secret it sends, by name, never its value.
nonisolated struct ConnectedApp: Decodable, Identifiable, Equatable, Sendable {
    var name: String
    /// A remote MCP server's URL, or nil for an API.
    var mcp: String?
    /// An API's base URL, or nil for an MCP server.
    var api: String?
    /// The vault secret's name.
    var secret: String
    /// Where the secret goes: `Authorization` sends it as a bearer token, any other header as it is.
    var header: String?
    /// Whether the runner's vault has the secret now.
    var secretSaved: Bool
    var lastUsed: Date?
    var id: String { name }
}

/// obid's GitHub login, as `GET /v1/github` says: whether it has a gh, whether gh is signed in and
/// as whom, and the sign-in under way. obid never passes on the token.
nonisolated struct GitHubLogin: Decodable, Equatable, Sendable {
    var gh: Bool
    var signedIn: Bool
    var user: String?
    var signIn: GitHubSignIn?
}

/// A tool's own login for one of obid's accounts, under way (`GET /v1/accounts/{id}/sign-in`): its
/// page, the one-time code to type there (Codex), or whether the page ends with a code to send back
/// (Claude Code).
nonisolated struct AccountSignIn: Decodable, Equatable, Sendable {
    var account: String
    var url: URL
    var userCode: String?
    var pasteCode: Bool
    var expiresAt: String?

    /// The tool's own sites, as obid takes the link from its login: the Mac opens nothing else.
    static let sites = ["claude": ["claude.com", "claude.ai", "anthropic.com"], "codex": ["openai.com", "chatgpt.com"],
                        "cursor": ["cursor.com", "cursor.sh"]]

    /// Whether it's a page the Mac opens for `agent`: https on the tool's own site, with no login or port.
    func opens(for agent: String) -> Bool {
        guard url.scheme == "https", url.user == nil, url.password == nil, url.port == nil,
              let host = url.host()?.lowercased() else { return false }
        return (Self.sites[agent] ?? []).contains { host == $0 || host.hasSuffix("." + $0) }
    }
}

/// A repository a job may start in, as `GET /v1/repos` lists it.
nonisolated struct RunnerRepo: Decodable, Equatable, Sendable {
    var repo: String
    var defaultBranch: String
    var listed: Bool
}

/// gh's own sign-in under way: GitHub's device page, the code to type there, when it runs out.
nonisolated struct GitHubSignIn: Decodable, Equatable, Sendable {
    var url: URL
    var userCode: String
    var expiresAt: String

    /// The only page the Mac opens for it.
    static let devicePage = URL(string: "https://github.com/login/device")!

    /// Whether it's what gh's sign-in gives: exactly GitHub's device page, and a code of two groups
    /// of four capitals or digits. The Mac opens and copies nothing else a runner answers.
    var isGitHubs: Bool {
        url == Self.devicePage && userCode.wholeMatch(of: /[A-Z0-9]{4}-[A-Z0-9]{4}/) != nil
    }
}

/// A plug-in obid found, as `GET /v1/obid/plugins` lists it.
nonisolated struct RunnerPlugin: Decodable, Identifiable, Equatable, Sendable {
    var id: String
    var name: String
    var version: String?
    var kinds: [String]
    var enabled: Bool
    /// `disabled`, `starting`, `ready` or `failed`.
    var state: String
    var lastError: String?
}

/// What `POST /v1/jobs/{id}/move` takes: nils are left out.
nonisolated struct MoveBody: Encodable, Sendable {
    var agent: String?
    var account: String?
    var model: String?
    var why: String?
}

/// What the job screens' "Move to…" offers, on the iPhone, the Mac and the Watch (docs/runner-api.md,
/// `POST /v1/jobs/{id}/move`).
nonisolated enum MoveChoice {
    static let tools = [("claude", "Claude Code"), ("codex", "Codex"), ("cursor", "Cursor")]
    /// Claude Code's models. Codex's and Cursor's come from the runner's options; Codex's strongest when they don't.
    static let claudeModels = ["claude-fable-5-1", "claude-opus-5-5", "claude-sonnet-5-5", "claude-haiku-5-5"]
    static let codexStrongest = "gpt-6-astra"
    /// The model that drops the job's own for the tool's from Settings › Models.
    static let toolDefault = "default"
    /// Past this share of either window, an account takes a moved job only when the user picks it.
    static let room = 0.8

    /// "5h 42% · week 61%", "No usage yet", or "Signed out".
    static func usage(signedIn: Bool, fiveHour: Double?, sevenDay: Double?) -> String {
        guard signedIn else { return "Signed out" }
        let parts = [fiveHour.map { "5h \(Int(($0 * 100).rounded()))%" }, sevenDay.map { "week \(Int(($0 * 100).rounded()))%" }].compactMap { $0 }
        return parts.isEmpty ? "No usage yet" : parts.joined(separator: " · ")
    }

    /// Past 80% of either window: Oby won't pick it by itself.
    static func busy(fiveHour: Double?, sevenDay: Double?) -> Bool { max(fiveHour ?? 0, sevenDay ?? 0) >= room }

    /// What a move does to a job in this state, said before it runs.
    static func effect(active: Bool, running: Bool) -> String {
        running ? "Its agent stops, and the round starts over there with the work so far."
            : active ? "It starts there." : "Its next round runs there: tell it something, or Try again."
    }
}

/// obid's `job` event for a job that was deleted.
private nonisolated struct DeletedJob: Decodable { var id: String; var deleted: Bool }

/// Turns Server-Sent Events lines into events. Unknown events and pings are skipped.
nonisolated struct EventParser {
    private var name = ""
    private var data = ""

    mutating func feed(_ raw: String) -> RunnerEvent? {
        let line = raw.hasSuffix("\r") ? String(raw.dropLast()) : raw
        if line.isEmpty {
            defer { name = ""; data = "" }
            let json = Data(data.utf8)
            switch name {
            case "job":
                if let gone = try? Runner.decoder.decode(DeletedJob.self, from: json), gone.deleted { return .jobDeleted(gone.id) }
                return (try? Runner.decoder.decode(Job.self, from: json)).map(RunnerEvent.job)
            case "accounts": return (try? Runner.decoder.decode([RunnerAccount].self, from: json)).map(RunnerEvent.accounts)
            #if !os(watchOS)
            case "limitDecision": return .limitDecision
            #endif
            case "clear": return (try? Runner.decoder.decode(ClearedAt.self, from: json)).map { .cleared($0.clearedAt) }
            case "inbox": return (try? Runner.decoder.decode(InboxItem.self, from: json)).map(RunnerEvent.inbox)
            case "turn": return (try? Runner.decoder.decode(RunnerTurn.self, from: json)).map { RunnerEvent.turn($0.turn, $0.extras) }
            case "step": return (try? Runner.decoder.decode(TurnStep.self, from: json)).map(RunnerEvent.step)
            case "conversation": return (try? Runner.decoder.decode(RunnerConversation.self, from: json)).map(RunnerEvent.conversation)
            case "delta": return (try? Runner.decoder.decode(TurnDelta.self, from: json)).map(RunnerEvent.delta)
            case "artifact": return (try? Runner.decoder.decode(ArtifactEvent.self, from: json)).map(RunnerEvent.artifact)
            case "routine": return .routine
            case "github": return (try? Runner.decoder.decode(GitHubWork.self, from: json)).map(RunnerEvent.github)
            case "transcript": return (try? Runner.decoder.decode(TranscriptEvent.self, from: json)).map(RunnerEvent.transcript)
            default: return nil
            }
        }
        if line.hasPrefix("event:") { name = line.dropFirst(6).trimmingCharacters(in: .whitespaces) }
        if line.hasPrefix("data:") { data += (data.isEmpty ? "" : "\n") + line.dropFirst(5).trimmingCharacters(in: .whitespaces) }
        return nil
    }
}

/// Which routine's run a job is (docs/runner-api.md, "obid only: routines (#395)").
nonisolated struct RoutineMark: Codable, Equatable, Sendable {
    var id: String
    var name: String
    var scheduled: Date?
    var catchUp: Bool?

    /// What a job's card and page say: "From routine: Review open PRs" (Sangam's design).
    var line: String { "From routine: \(name)" }
}

/// Teammates' open pull requests that change a job's files (obid only: docs/runner-api.md,
/// "obid only: teammates' pull requests"). Titles are what people wrote: never shown as obid's words.
nonisolated struct TeammatePRs: Codable, Equatable, Sendable {
    struct PR: Codable, Equatable, Sendable {
        var number: Int
        var author: String
        var files: [String]
        /// After the job's commits: the code both change, empty when only files are shared.
        var clash: [String]?
    }
    var prs: [PR]

    /// What a job's page says under its head: "Also changed in @alice's #812 (src/jobs.rs)", or
    /// "Same code changed in …" once the check after its commits found some; nil when none.
    var line: String? {
        guard !prs.isEmpty else { return nil }
        let same = prs.contains { !($0.clash ?? []).isEmpty }
        let who = prs.map { "@\($0.author)'s #\($0.number)" }
        var files: [String] = []
        for file in prs.flatMap(\.files) where !files.contains(file) { files.append(file) }
        let named = who.count < 3 ? who.joined(separator: " and ") : who.dropLast().joined(separator: ", ") + " and " + who.last!
        let shown = files.prefix(3).joined(separator: ", ") + (files.count > 3 ? " and \(files.count - 3) more" : "")
        return "\(same ? "Same code changed in" : "Also changed in") \(named) (\(shown))"
    }
}

/// A routine as obid lists it: a saved request it runs on a schedule, a job each time.
/// One memory obid keeps (docs/runner-api.md, "obid only: memory").
nonisolated struct RunnerMemory: Codable, Identifiable, Equatable, Sendable {
    var id: String
    /// `user` or `project`.
    var scope: String
    var repo: String?
    var kind: String
    var title: String
    var text: String
    /// Where it came from, in obid's words: "You said it in 3 conversations".
    var from: String
    var created: Date
}

/// A memory obid could keep, waiting for Remember or Not this.
nonisolated struct RunnerMemorySuggestion: Codable, Identifiable, Equatable, Sendable {
    struct Source: Codable, Equatable, Sendable {
        var quote: String
        /// `You, in “Release planning”` or `A review on obi-ai-bot/obi#12`.
        var place: String
        var link: URL?
    }

    var id: String
    var scope: String
    var repo: String?
    var title: String
    var text: String
    var from: String
    var sources: [Source]
}

nonisolated struct RunnerMemoryList: Codable, Equatable, Sendable {
    var memories: [RunnerMemory]
    var suggestions: [RunnerMemorySuggestion]
}

nonisolated struct RunnerMemoryFound: Codable, Equatable, Sendable {
    struct Read: Codable, Equatable, Sendable {
        var conversations: Int
        var pullRequests: Int
    }

    var suggestions: [RunnerMemorySuggestion]
    var read: Read
    var problems: [String]
}

nonisolated struct RunnerRoutine: Codable, Identifiable, Equatable, Sendable {
    struct Run: Codable, Equatable, Sendable {
        var scheduled: Date
        var at: Date
        /// `started`, `skipped`, `missed` or `failed`.
        var outcome: String
        var reason: String?
        var job: String?
        var catchUp: Bool?
        /// Its job's end once it ended: `done`, `failed`, `stopped`, `interrupted`.
        var ended: String?
    }

    var id: String
    var name: String
    var repo: String
    var kind: String
    var task: String
    /// obid's words for its schedule: "Weekdays at 9:00".
    var when: String
    var zone: String
    var enabled: Bool
    var nextRun: Date?
    var failuresInARow: Int?
    var lastRun: Run?
    var conversation: String?

    var repoName: String { String(repo.split(separator: "/").last ?? Substring(repo)) }

    /// Under its name: the schedule and repository, then when it runs next (Sangam's "Starts
    /// Monday at 9") or that it's paused, then how the last run went.
    func about(now: Date = .now) -> String {
        var parts = ["\(when) · \(repoName)"]
        if let next = nextRun, enabled {
            parts.append("Next " + Self.day(next, now: now))
        } else {
            parts.append("Paused")
        }
        if let last = lastRun {
            switch (last.outcome, last.ended) {
            case ("started", nil): parts.append("Running now")
            case ("started", "done"): parts.append("Last run done")
            case ("started", "failed"): parts.append("Last run failed")
            case ("started", let end?): parts.append("Last run \(end)")
            case ("skipped", _): parts.append("Last run skipped")
            case ("missed", _): parts.append("Missed a run")
            default: parts.append("Last run couldn't start")
            }
        }
        return parts.joined(separator: " · ")
    }

    /// Its next run as Home's Routines page says it (Sangam's "Starts Monday at 9"), or nil while
    /// paused.
    func starts(now: Date = .now) -> String? {
        guard enabled, let next = nextRun else { return nil }
        let day = Self.day(next, now: now)
        return "Starts " + day
    }

    /// The routines waiting for their next run, soonest first.
    static func upcoming(_ routines: [RunnerRoutine]) -> [RunnerRoutine] {
        routines.filter { $0.enabled && $0.nextRun != nil }.sorted { ($0.nextRun ?? .distantFuture) < ($1.nextRun ?? .distantFuture) }
    }

    /// "today at 9:00", "tomorrow at 9:00", "Monday at 9:00", or "Oct 30 at 9:00", in this device's zone.
    static func day(_ date: Date, now: Date = .now, calendar: Calendar = .current) -> String {
        let time = date.formatted(.dateTime.hour().minute())
        if calendar.isDate(date, inSameDayAs: now) { return "today at \(time)" }
        if let tomorrow = calendar.date(byAdding: .day, value: 1, to: now), calendar.isDate(date, inSameDayAs: tomorrow) {
            return "tomorrow at \(time)"
        }
        if date.timeIntervalSince(now) < 6 * 24 * 3600 { return "\(date.formatted(.dateTime.weekday(.wide))) at \(time)" }
        return "\(date.formatted(.dateTime.month(.abbreviated).day())) at \(time)"
    }
}
