import SwiftUI

/// A task at work (docs/contracts/live-task.md, Electron's liveTask.ts): what its card takes from the
/// session. Only what the tool gave: nothing here makes up a to-do, a thought or a time.
nonisolated enum LiveTask {
    /// Its to-do list as the agent last wrote it. None until it makes one.
    static func todos(_ entries: [TranscriptEntry]) -> [TranscriptEntry.Todo]? {
        entries.last { $0.kind == "plan" }?.todos
    }

    /// The last things it did, oldest first.
    static func lastSteps(_ entries: [TranscriptEntry], keep: Int = 3) -> [TranscriptEntry] {
        Array(entries.filter { ["tool", "said", "thought"].contains($0.kind) }.suffix(keep))
    }

    /// A long list on a card: `most` of them around the one it's on, and how many are left out each side.
    static func around(_ todos: [TranscriptEntry.Todo], most: Int = 8) -> (todos: [TranscriptEntry.Todo], before: Int, after: Int) {
        guard todos.count > most else { return (todos, 0, 0) }
        let on = todos.firstIndex { $0.state == "running" } ?? todos.firstIndex { $0.state != "done" } ?? 0
        let start = max(0, min(on - 2, todos.count - most))
        return (Array(todos[start..<(start + most)]), start, todos.count - start - most)
    }

    /// A thought in words: its own, or how long it took, or that it was one.
    static func thought(_ entry: TranscriptEntry) -> String {
        if entry.running { return "Thinking" }
        if let text = entry.text, !text.isEmpty { return text }
        return entry.seconds.map { "Thought for \(JobPageText.took($0))" } ?? "Thought"
    }

    /// What obid says it's on, as a live line under the steps: the stage it's in past its agent's
    /// work (the check, publishing); with nothing from its agent yet, what it waits for or its step;
    /// and a step the session doesn't hold, which is obid's own (RECOVERY-5: "Trying again in 5
    /// minutes (2 of 3)"), so a job being tried again says so.
    static func now(_ job: Job, steps: [TranscriptEntry], list: Bool) -> String? {
        guard job.isActive else { return nil }
        if let stage = job.pipeline?.stages.first(where: { $0.state == .current && $0.id != "work" }) { return stage.title }
        if steps.isEmpty && !list { return job.waiting?.text ?? (job.step.isEmpty ? nil : job.step) }
        let words = steps.map { $0.summary ?? "" }
        return job.step.isEmpty || job.step == "Working" || words.contains(job.step) ? nil : job.step
    }

    /// How an ended task reads on its card (LIVETASK-4).
    static func ended(_ job: Job) -> String {
        switch job.status {
        case .failed: "Failed"
        case .stopped, .interrupted: "Stopped"
        default: job.nothingDone == true ? "Nothing done" : "Done"
        }
    }

    /// Why a stopped or failed task stopped, one line, in obid's words (LIVETASK-12).
    static func why(_ job: Job) -> String? {
        guard [.failed, .stopped, .interrupted].contains(job.status) else { return nil }
        return (job.reason ?? job.summary).split(separator: "\n").first.map(String.init)
    }

    /// The card's first line under its title: where it stands, then who has it, and how long a
    /// running one has gone.
    static func meta(_ job: Job, mark: WorkMark, account: String, now: Date = .now) -> String {
        let repo = job.repo.split(separator: "/").last.map(String.init).flatMap { job.repo.contains("/") ? $0 : nil }
        let stands = mark == .done ? ended(job) : ThreadTaskLine.status(mark)
        let took = job.status == .running ? JobPageText.minutes(now.timeIntervalSince(job.started ?? job.created)) : nil
        return [stands, job.agentName, account, repo, took].compactMap { $0 }.filter { !$0.isEmpty }.joined(separator: " · ")
    }
}

/// A task's work under its card while it goes on (Sangam's board "Oby at work"): what it needs, its
/// to-dos if the agent made a list, the last things it did either way, what obid says it's on.
struct LiveTaskWork: View {
    @Environment(Phone.self) private var phone
    let job: Job
    @State private var session: LiveSession?

    var body: some View {
        let entries = session?.entries ?? []
        let list = LiveTask.todos(entries)
        let steps = LiveTask.lastSteps(entries)
        let line = LiveTask.now(job, steps: steps, list: list != nil)
        let needs = ThreadTaskLine.mark(job) == .needs
        // A list with none in hand now: the last things it did go under the whole list.
        let under = !(list?.contains { $0.state == "running" } ?? false)
        VStack(alignment: .leading, spacing: 6) {
            if needs, let headline = phone.decision(for: job).map({ $0.headline ?? $0.title }) ?? job.question?.text {
                Text(headline).font(Tokens.font(Tokens.textSm, .medium)).foregroundStyle(Tokens.needsInk).lineLimit(2)
            }
            if let list {
                let shown = LiveTask.around(list)
                if shown.before > 0 { more("\(shown.before) done before these") }
                ForEach(Array(shown.todos.enumerated()), id: \.offset) { _, todo in
                    TodoLine(todo: todo, job: job)
                    if todo.state == "running" { stepLines(steps).padding(.leading, 18) }
                }
                if shown.after > 0 { more("\(shown.after) more to do") }
            }
            if list == nil || under { stepLines(steps) }
            if let line { Sheen(text: line, size: Tokens.textSm).lineLimit(1) }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onAppear { if session == nil { session = phone.hold(session: job.id) } }
        .onDisappear { if session != nil { phone.letGo(session: job.id); session = nil } }
        // Fetched again at each of the job's events, so a lost one fills by itself; a moment after, as they come in bursts.
        .task(id: "\(session != nil) \(job.status.rawValue) \(job.step)") {
            try? await Task.sleep(for: .seconds(session?.loaded == true ? 2 : 0))
            guard !Task.isCancelled else { return }
            await session?.catchUp()
        }
    }

    private func more(_ words: String) -> some View {
        Text(words).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted)
    }

    @ViewBuilder private func stepLines(_ steps: [TranscriptEntry]) -> some View {
        ForEach(steps) { LiveStepLine(entry: $0) }
    }
}

/// One to-do, the one it's on lit: at work with the sheen, orange while it needs you, red once it failed.
private struct TodoLine: View {
    let todo: TranscriptEntry.Todo
    let job: Job

    var body: some View {
        let on = todo.state == "running"
        let needs = ThreadTaskLine.mark(job) == .needs
        let working = on && !needs && job.isActive
        HStack(spacing: 8) {
            Group {
                if todo.state == "done" {
                    Image(systemName: "checkmark").font(Tokens.font(10, .semibold)).foregroundStyle(Tokens.check)
                } else if working {
                    WorkMarkView(mark: .working, dot: 7)
                } else if on {
                    Circle().fill(needs ? Tokens.needs : Tokens.danger).frame(width: 7, height: 7)
                } else {
                    Circle().strokeBorder(Tokens.check, lineWidth: 1.5).frame(width: 7, height: 7)
                }
            }
            .frame(width: 12, height: 12)
            if working {
                Sheen(text: todo.text, size: Tokens.textSm).lineLimit(1)
            } else {
                Text(todo.text).font(Tokens.font(Tokens.textSm)).lineLimit(1)
                    .foregroundStyle(todo.state == "done" ? Tokens.muted : on ? Tokens.ink : Tokens.text)
            }
        }
        .accessibilityElement(children: .combine)
        .accessibilityValue(todo.state == "done" ? "Done" : on ? (working ? "Working" : "Needs attention") : "To do")
    }
}

/// One of the last things it did: its thinking and its words as a line each, a tool call with its
/// mark, its verb and what it ran or touched.
private struct LiveStepLine: View {
    let entry: TranscriptEntry

    var body: some View {
        switch entry.kind {
        case "thought":
            if entry.running { Sheen(text: LiveTask.thought(entry), size: Tokens.textSm).lineLimit(1) }
            else { Text(LiveTask.thought(entry)).font(Tokens.font(Tokens.textSm)).italic().foregroundStyle(Tokens.muted).lineLimit(1) }
        case "said":
            Text(entry.words.replacing(/\s+/, with: " ")).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.text).lineLimit(1)
        default:
            let (verb, target) = entry.verbAndTarget
            HStack(spacing: 8) {
                Group {
                    if entry.running { WorkMarkView(mark: .working, dot: 6) }
                    else if entry.state == "declined" { Circle().strokeBorder(Tokens.check, lineWidth: 1.5).frame(width: 6, height: 6) }
                    else if entry.failed { Circle().fill(Tokens.danger).frame(width: 6, height: 6) }
                    else { Image(systemName: "checkmark").font(Tokens.font(9, .semibold)).foregroundStyle(Tokens.check) }
                }
                .frame(width: 10, height: 10)
                HStack(spacing: 4) {
                    Text(verb).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted).fixedSize()
                    if let target { Text(target).font(Tokens.mono(11)).foregroundStyle(Tokens.item).lineLimit(1).truncationMode(.middle) }
                }
                if entry.state == "declined" { Text("Declined").font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted) }
            }
            .accessibilityElement(children: .combine)
        }
    }
}

/// The agent's own conversation (Sangam's board "The agent's own conversation"), a page over the chat
/// on a phone: its to-dos on top, from the start what it said, did and thought and what you told it,
/// tool lines that open to their detail, the decision it waits on, and the box to tell it something.
/// Told something once it's finished, it starts a follow-up, which this then follows.
struct AgentPage: View {
    @Environment(Phone.self) private var phone
    @Environment(\.dismiss) private var dismiss
    @Environment(\.push) private var push
    let id: String
    @State private var followed: String?
    @State private var session: LiveSession?
    @State private var note = ""
    @FocusState private var typing: Bool
    @State private var sending = false
    @State private var problem: String?
    /// Asked obid for a job its list doesn't have.
    @State private var looked = false

    private var shown: String { followed ?? id }
    private var job: Job? { phone.jobs.first { $0.id == shown } }

    var body: some View {
        Group {
            if let session, let job {
                SessionView(session: session, agent: job.agent, meta: "", live: job.isActive, compact: true, title: false, conversation: true)
                    .task(id: job.isActive) { await session.follow(live: job.isActive) }
                    .safeAreaInset(edge: .top, spacing: 0) { todos(session, job) }
                    .safeAreaInset(edge: .bottom, spacing: 0) { foot(job) }
            } else if looked && job == nil {
                ContentUnavailableView("This task isn't on your Mac any more.", systemImage: "questionmark.folder")
                    .foregroundStyle(Tokens.text)
            } else {
                ProgressView()
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Tokens.bg)
        .navigationTitle(job?.heading ?? "")
        .navigationSubtitle(job.map { TaskMeta.line($0, account: phone.accountName($0.account)) } ?? "")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            if let job {
                if job.status == .running {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button("Stop", role: .destructive) {
                            Task { do { try await phone.stop(job.id) } catch { problem = error.localizedDescription } }
                        }
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button { push(.task(job.id)) } label: { Image(systemName: "arrow.up.right") }
                        .accessibilityLabel("Open the task’s page")
                }
            }
        }
        .task(id: shown) {
            hold()
            phone.markSeen(shown)
            phone.showing(shown, true)
            await phone.fetch(shown)
            looked = true
        }
        // Deleted while it's open: it closes (LIVETASK-15).
        .onChange(of: job == nil) { if job == nil, looked, phone.loaded { dismiss() } }
        .onDisappear { release() }
    }

    private func hold() {
        release()
        session = phone.hold(session: shown)
        heldFor = shown
    }

    private func release() {
        if let heldFor { phone.letGo(session: heldFor); phone.showing(heldFor, false) }
        heldFor = nil
        session = nil
    }

    /// The session held now, let go of when another is followed or the page goes.
    @State private var heldFor: String?

    /// Its to-dos on top, once, the one it's on lit.
    @ViewBuilder private func todos(_ session: LiveSession, _ job: Job) -> some View {
        if let list = LiveTask.todos(session.entries) {
            let shown = LiveTask.around(list)
            VStack(alignment: .leading, spacing: 4) {
                if shown.before > 0 { Text("\(shown.before) done before these").font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted) }
                ForEach(Array(shown.todos.enumerated()), id: \.offset) { _, todo in TodoLine(todo: todo, job: job) }
                if shown.after > 0 { Text("\(shown.after) more to do").font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.horizontal, Tokens.gutter).padding(.vertical, 8)
            .background(Tokens.bg)
            .overlay(alignment: .bottom) { Tokens.line.frame(height: 1) }
        }
    }

    /// The decision it waits on, over the box; then the box.
    private func foot(_ job: Job) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if let item = phone.decision(for: job) {
                InboxDecision(item: item).padding(.horizontal, Tokens.gutter)
            } else if (job.permission != nil && job.isActive) || job.waitsForAnswer {
                JobDecision(job: job).padding(.horizontal, Tokens.gutter)
            }
            if let problem {
                Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger).padding(.horizontal, Tokens.gutter + 12)
            }
            Composer(text: $note, prompt: job.isActive ? "Tell \(job.agentName)…" : "Ask for a follow-up…", typing: $typing, onSend: { _ in tell(job.id) })
                .keepsDraft($note, as: Drafts.task(job.id))
                .disabled(sending)
        }
        .background(Tokens.bg)
    }

    /// What you tell it goes to its agent (`POST /v1/jobs/{id}/messages`, LIVETASK-7).
    private func tell(_ id: String) {
        guard case .send(let text) = ChatSend.typed(note), !sending else {
            if note.count > ChatSend.most { problem = ChatSend.tooLong }
            return
        }
        sending = true
        Task {
            defer { sending = false }
            do {
                let told = try await phone.tell(id, text, conversation: true)
                note = ""
                typing = false
                problem = nil
                if told != id { followed = told }
            } catch { problem = error.localizedDescription }
        }
    }
}
