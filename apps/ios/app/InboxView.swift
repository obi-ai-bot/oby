import SwiftUI

/// One thing only you can answer (`GET /v1/inbox`), as Sangam's decision block (AskBlock, the
/// shared AskCard): its kind, Oby's headline, what happened, the command or plan, the choices with
/// the recommended one filled, Later, and your own words. Answered here, it goes from the Mac's
/// window too. Shown on a task's card and page, in its conversation, and, with no task on Home, in the Inbox.
struct InboxDecision: View {
    @Environment(\.openURL) private var openURL
    @Environment(Phone.self) private var phone
    let item: InboxItem
    /// A link to the task, when it isn't what's around the block already.
    var openTask = false
    /// A card of its own; off inside a task's card, which is one already.
    var inset = true
    /// Over the decision, above a line (Home's stack: its task and where it is in the stack), naming the task.
    var head: AnyView? = nil
    @State private var busy = false
    @State private var problem: String?
    @State private var words = ""

    private var job: Job? { phone.jobs.first { $0.id == item.job } }
    private var permission: RunnerPermission? { item.kind == "permission" ? job?.permission : nil }
    /// A confirm item (obid only, #546): its title is what Confirm does, why it waits under it, the
    /// words it would send quoted apart; Confirm or Cancel, nothing recommended, no words.
    private var confirming: Bool { item.kind == RunnerConfirm.kind }
    private var takesWords: Bool { !item.job.isEmpty && !confirming }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let head {
                head
                Rectangle().fill(Tokens.divider).frame(height: 1).padding(.vertical, 12)
            }
            let check = PhoneAsk.check(item), merge = PhoneAsk.merge(item)
            AskCard(kind: PhoneAsk.label(item, plan: job?.question?.plan != nil), failed: AskKind.failed(item.kind),
                    source: openTask && head == nil ? job?.heading : nil,
                    title: check.map { _ in PhoneAsk.checkTitle(item) } ?? (confirming ? item.title : merge?.title ?? item.words.title(item.text)),
                    happened: confirming ? InboxWords.plain(item.text) : merge.map(\.happened) ?? item.words.body,
                    behindDetails: confirming || merge != nil ? nil : item.words.details(item.text),
                    detail: check ?? (confirming ? item.confirm?.words : permission?.detail),
                    plan: item.kind == "question" ? job?.question?.plan : nil,
                    options: check != nil ? PhoneAsk.checkOptions(item) : CardOption.inbox(item.options, kind: item.kind, question: job?.question, permission: permission,
                                              words: item.words, details: item.details, does: item.title),
                    busy: busy || phone.waitsToSend(answer: item.id),
                    onChoose: choose,
                    onLater: { settle(later: .now.addingTimeInterval(3600)) })
            if phone.waitsToSend(answer: item.id) {
                Text(ReachNotice.waiting).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted).padding(.top, 10)
            } else if takesWords { OwnWords(words: $words, prompt: item.kind == "permission" ? "No, and tell the agent why…" : "Answer in your own words…", busy: busy, send: sendWords) }
            if openTask, !item.job.isEmpty {
                NavigationLink(value: Route.task(item.job)) {
                    Text("Open task").font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted)
                }
                .padding(.top, 10)
            }
            if let problem { Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger).padding(.top, 8) }
        }
        .modifier(AskInset(on: inset))
    }

    private func choose(_ option: CardOption) {
        if option.answer == "Open the PR", let url = PublishedLink.url(job?.published) { openURL(url); return }
        settle(answer: option.answer)
    }

    /// A permission's words go with a Deny, for the agent; anything else's are the answer.
    private func sendWords(_ text: String) {
        run {
            if let job, permission != nil { try await phone.answer(job, .deny, message: text) } else { try await phone.settle(item.id, answer: text) }
            words = ""
        }
    }

    private func settle(answer: String? = nil, later: Date? = nil) {
        run { try await phone.settle(item.id, answer: answer, later: later) }
    }

    private func run(_ work: @escaping () async throws -> Void) {
        busy = true
        Task {
            defer { busy = false }
            do { try await work(); problem = nil } catch { problem = error.localizedDescription }
        }
    }
}

/// One owner choice for every job in obid's account-limit episode. The server's options and
/// revision are authoritative; arbitrary new providers require no client-specific branching.
struct AccountLimitDecision: View {
    @Environment(Phone.self) private var phone
    let decision: RunnerLimitDecision
    @State private var busy = false
    @State private var problem: String?

    private var source: String { phone.accounts.first { $0.id == decision.account }?.name ?? decision.agent }
    private var happened: String {
        let reset = decision.resetAt.map { Date(timeIntervalSince1970: TimeInterval($0)).formatted(date: .abbreviated, time: .shortened) }
        let usage = phone.accounts.first { $0.id == decision.account }?.fiveHour.map { " Five-hour usage: \(Int(($0 * 100).rounded()))%." } ?? ""
        let tasks = decision.jobs.count == 1 ? "1 task is waiting" : "\(decision.jobs.count) tasks are waiting"
        return tasks + " · " + (reset.map { "resets \($0)" } ?? "reset time unknown") + usage
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            AskCard(kind: "Usage limit", shown: decision.jobs.map(\.title).joined(separator: "\n"),
                    title: "\(source) reached its limit", happened: happened,
                    options: decision.options.enumerated().filter { $0.element.isSupported }.map {
                        CardOption(label: $0.element.label, answer: String($0.offset))
                    }, busy: busy, onChoose: choose)
            if busy { Text("Saving your choice…").font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted).padding(.top, 8) }
            if let problem = problem ?? decision.problem {
                Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger).padding(.top, 8)
                    .accessibilityLabel("Couldn’t save your choice. \(problem)")
            }
        }
        .modifier(AskInset(on: true))
    }

    private func choose(_ option: CardOption) {
        guard !busy, let index = Int(option.answer), decision.options.indices.contains(index) else { return }
        busy = true
        problem = nil
        Task {
            defer { busy = false }
            do { try await phone.answerLimit(decision, option: decision.options[index]) }
            catch { problem = error.localizedDescription }
        }
    }
}

/// A job's own question or permission when the runner has no inbox item for it (an older runner):
/// the same block, answered on the job.
struct JobDecision: View {
    @Environment(Phone.self) private var phone
    let job: Job
    var inset = true
    @State private var busy = false
    @State private var problem: String?
    @State private var words = ""

    var body: some View {
        if let permission = job.permission, job.isActive {
            block {
                AskCard(kind: permission.isQuestion ? AskKind.label("agentQuestion") : "Permission", title: permission.title, detail: permission.detail,
                        options: CardOption.permission(permission), busy: busy,
                        onChoose: { option in
                            guard let decision = RunnerPermission.Decision(rawValue: option.answer) else { return }
                            run { try await phone.answer(job, decision) }
                        })
                // On the agent's question (#858) obid takes a Deny with words as its answer.
                OwnWords(words: $words, prompt: permission.isQuestion ? "Answer in your own words…" : "No, and tell the agent why…", busy: busy) { text in run { try await phone.answer(job, .deny, message: text); words = "" } }
            }
            .id(permission.id)
        } else if job.waitsForAnswer, let question = job.question {
            block {
                AskCard(kind: AskKind.label("question", plan: question.plan != nil), title: question.text, plan: question.plan,
                        options: question.cardOptions, busy: busy, onChoose: { option in run { try await phone.tell(job.id, option.answer) } })
                OwnWords(words: $words, prompt: "Answer in your own words…", busy: busy) { text in run { try await phone.tell(job.id, text); words = "" } }
            }
            .id(question.text)
        }
    }

    private func block(@ViewBuilder _ content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            content()
            if let problem { Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger).padding(.top, 8) }
        }
        .modifier(AskInset(on: inset))
    }

    private func run(_ work: @escaping () async throws -> Void) {
        busy = true
        Task {
            defer { busy = false }
            do { try await work(); problem = nil } catch { problem = error.localizedDescription }
        }
    }
}

/// Whether a job waits on a decision the page or card can show: an inbox item, else its own.
extension Phone {
    func decision(for job: Job) -> InboxItem? {
        InboxItem.waiting(inbox).first { $0.job == job.id }
    }
}

/// A decision as a card of its own (radius 18, the chip colour), or bare inside another card.
private struct AskInset: ViewModifier {
    let on: Bool

    func body(content: Content) -> some View {
        if on {
            content.padding(16).frame(maxWidth: .infinity, alignment: .leading)
                .background(Tokens.chip, in: RoundedRectangle(cornerRadius: Tokens.askRadius, style: .continuous))
        } else {
            content.frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

/// Under a decision: your own words instead of a choice, sent with the arrow.
private struct OwnWords: View {
    @Binding var words: String
    let prompt: String
    let busy: Bool
    let send: (String) -> Void

    private var typed: String? { ChatThread.words(words) }

    var body: some View {
        HStack(spacing: 6) {
            TextField(prompt, text: $words, axis: .vertical)
                .lineLimit(1...4)
                .font(Tokens.font(Tokens.textMd)).foregroundStyle(Tokens.ink)
                .submitLabel(.send)
                .onSubmit { if let typed { send(typed) } }
            if let typed {
                Button { send(typed) } label: {
                    Image(systemName: "arrow.up").font(Tokens.font(13, .semibold)).foregroundStyle(Tokens.ink)
                        .frame(width: 28, height: 28).background(Tokens.active, in: Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Send your answer")
            }
        }
        .padding(.leading, 12).padding(.trailing, 4).padding(.vertical, 4)
        .frame(minHeight: 36)
        .overlay { RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(Tokens.line) }
        .padding(.top, 12)
        .disabled(busy)
    }
}

/// What a decision's card says on the iPhone where the Electron app's `itemAsk` (live.ts) says it
/// otherwise than the shared AskKind, which the frozen Mac app keeps: Sangam's labels (ASK_LABEL)
/// for a Confirm and an Oby failure, a merge card labelled by what it is, and a repository's check.
nonisolated enum PhoneAsk {
    static func label(_ item: InboxItem, plan: Bool) -> String {
        if check(item) != nil { return "Check" }
        switch item.kind {
        // Not merged, Ready to merge, Needs a teammate: what its title says before the task's
        case "merge": return item.title.components(separatedBy: ": ").first.flatMap { $0.isEmpty ? nil : $0 } ?? AskKind.label("merge")
        case RunnerConfirm.kind: return "Confirm?"
        case "failure": return "Failed"
        default: return AskKind.label(item.kind, plan: plan)
        }
    }

    /// Oby's ask for a repository's check (`set_check`): the command, or nil.
    static func check(_ item: InboxItem) -> String? {
        guard item.kind == RunnerConfirm.kind, item.confirm?.action == "set_check" else { return nil }
        return InboxWords.plain(item.confirm?.payload?["check"])
    }

    static func checkTitle(_ item: InboxItem) -> String { "Use this as \(item.confirm?.target ?? "")’s check before publishing?" }

    /// Use it (obid's Confirm, signed: never recommended, as no Confirm is), and No check, or Keep it
    /// for a repository that has one (Cancel).
    /// ponytail: no Edit here; its `POST /v1/repos/check` isn't on the phone's gateway, so it's the Mac's
    static func checkOptions(_ item: InboxItem) -> [CardOption] {
        [CardOption(label: "Use it", answer: RunnerConfirm.confirm),
         CardOption(label: item.confirm?.payload?["was"] != nil ? "Keep it" : "No check", answer: RunnerConfirm.cancel)]
    }

    /// A merge card with no headline of the model's: obid's reason leads, its first sentence (without
    /// the label's words again) over the rest, then every other hold, one a line (MERGING-21).
    static func merge(_ item: InboxItem) -> (title: String, happened: String?)? {
        guard item.kind == "merge", InboxWords.plain(item.headline) == nil, !item.text.isEmpty else { return nil }
        var holds = item.text.components(separatedBy: "\n")
        let line = holds.removeFirst()
        var first = line, rest = ""
        if let stop = line.range(of: ". ") {
            first = String(line[..<line.index(after: stop.lowerBound)])
            rest = String(line[stop.upperBound...])
        }
        if let said = first.range(of: "^Not merged( yet)?: ", options: .regularExpression) {
            let tail = first[said.upperBound...]
            first = tail.prefix(1).uppercased() + tail.dropFirst()
        }
        let happened = ([rest] + holds).filter { !$0.isEmpty }.joined(separator: "\n")
        return (first, happened.isEmpty ? nil : happened)
    }
}
