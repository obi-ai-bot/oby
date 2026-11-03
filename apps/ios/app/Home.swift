import SwiftUI
import WebKit

/// What opens over where the app rests: a kind of work's page, a task's page, a task's agent's own
/// conversation, Archived, the Inbox (what needs you with no task to show it on) and Settings.
enum Route: Hashable {
    case kind(WorkType)
    case task(String)
    case agent(String)
    case archived
    case inbox
    case settings
}

extension EnvironmentValues {
    /// Opens a route over the stack the view is in.
    @Entry var push: (Route) -> Void = { _ in }
}

extension View {
    /// The way to open them, set on the stack itself so every page in it gets it.
    func pushes(onto path: Binding<[Route]>) -> some View {
        environment(\.push) { path.wrappedValue.append($0) }
    }

    /// Every page a tab's stack can open.
    func routes() -> some View {
        navigationDestination(for: Route.self) { route in
            switch route {
            case .kind(let kind): KindPage(kind: kind)
            case .task(let id): TaskPage(id: id)
            case .agent(let id): AgentPage(id: id)
            case .archived: ArchivedPage()
            case .inbox: InboxPage()
            case .settings: SettingsScreen()
            }
        }
    }

    /// Sangam's dark page: the background behind everything, its ink for the bars' controls.
    func obyPage() -> some View {
        background(Tokens.bg)
            .toolbarBackground(Tokens.bg, for: .navigationBar)
            .scrollContentBackground(.hidden)
    }
}

/// What the app shows (docs/contracts/ios-new-oby.md; the owner's decision of 2026-11-02, "I interact
/// with my Oby on my conversation"): your conversation, or the empty chat with none, and everything
/// else opened over it in one stack. Sangam's panel button at the top left, New conversation one tap
/// away at the top right. Why the Mac can't be reached shows as one line over every screen.
struct ChatRoot: View {
    @Environment(Phone.self) private var phone
    @Environment(\.scenePhase) private var scenePhase
    let character: ObyCharacter

    var body: some View {
        @Bindable var phone = phone
        VStack(spacing: 0) {
            if let notice = phone.notice { ReachLine(notice: notice) }
            NavigationStack(path: $phone.homePath) {
                rest
                    // The empty chat stays the same page: a second one built while the first is
                    // still out (the panel's close animation) puts Oby's one WebPage in two
                    // WebViews, and WebKit stops the app.
                    .id(phone.resting.showsStage ? -1 : phone.restMoves)
                    .toolbar {
                        // One panel button, as Sangam's: while the panel is open, its own puts it away.
                        if phone.panel == nil {
                            ToolbarItem(placement: .topBarLeading) {
                                Button { phone.panel = .chats } label: { Image(systemName: "sidebar.left") }
                                    .accessibilityLabel("Show sidebar")
                            }
                        }
                        if !phone.resting.showsStage {
                            ToolbarItem(placement: .topBarTrailing) {
                                Button { phone.newConversation() } label: { Image(systemName: "square.and.pencil") }
                                    .accessibilityLabel("New conversation")
                            }
                        }
                    }
                    .routes()
            }
            .pushes(onto: $phone.homePath)
        }
        .animation(Tokens.warm, value: phone.notice)
        .onDisappear { phone.cancelVoice() }
        .onChange(of: scenePhase) { if scenePhase != .active { phone.leftForeground(background: scenePhase == .background) } }
        .onChange(of: phone.homePath) { if !phone.homePath.isEmpty { phone.cancelVoice() } }
    }

    @ViewBuilder private var rest: some View {
        // One page from the words that start a conversation on, as obid names it.
        if phone.resting.showsStage {
            EmptyChat(character: character)
        } else {
            ConversationPage(id: phone.resting.conversation, isNew: phone.resting == .starting)
        }
    }
}

/// The empty chat (Home's stage, without the chips): Oby, its line and the box, and what Oby is asking
/// you, where it was. Words sent here start a conversation, which opens. While obid's list of
/// conversations is on its way, a thread's bones instead of Oby, so the app doesn't flash it.
private struct EmptyChat: View {
    @Environment(Phone.self) private var phone
    let character: ObyCharacter
    @State private var text = ""
    @FocusState private var typing: Bool

    private var loading: Bool { phone.resting == .deciding && !phone.connection.isOffline }

    var body: some View {
        ScrollView {
            VStack(spacing: 0) {
                if loading {
                    ThreadBones().frame(maxWidth: .infinity)
                } else {
                    stage
                    asks.padding(.top, 22)
                }
            }
            .padding(.horizontal, Tokens.gutter).padding(.top, 6).padding(.bottom, 16)
        }
        .scrollBounceBehavior(.basedOnSize)
        .scrollDismissesKeyboard(.interactively)
        .obyPage()
        .safeAreaInset(edge: .bottom) {
            Composer(text: $text, prompt: HomeLine.placeholder(fullName: HomeLine.owner(ofMac: phone.link?.name) ?? ""),
                     typing: $typing, onSend: send, talkIn: .some(nil))
                .keepsDraft($text, as: Drafts.home)
        }
        // Typing before the list comes: the box is yours, so the app stays here for your words.
        // (Not a draft restored as it opens, which isn't you typing now.)
        .onChange(of: text) { if !text.isEmpty, typing || phone.voice.active { phone.settleOnEmpty() } }
        // Once the panel has slid away (Tokens.warm): focused while it's still out, the box doesn't take it.
        .onChange(of: phone.focusHome, initial: true) {
            guard phone.focusHome else { return }
            phone.focusHome = false
            Task {
                try? await Task.sleep(for: .milliseconds(350))
                // Still showing: a page pushed or the panel opened since keeps the keyboard down.
                if phone.homePath.isEmpty, phone.panel == nil { typing = true }
            }
        }
        .onChange(of: mood, initial: true) { character.mood = mood }
    }

    private var mood: String {
        if case .thinking = phone.talk { return "thinking" }
        return phone.active.contains(where: \.isWorking) ? "working" : "idle"
    }

    /// Oby and the line under it.
    private var stage: some View {
        let work = phone.work
        // A task waiting for memory is active, but not running: the line says it apart.
        let memory = work.count { $0.job.waitsForMemory }
        let running = work.count { $0.status == .running && !$0.job.waitsForMemory }
        let needs = WorkList.needsYou(work, loose: phone.loose.count) + phone.limitDecisions.count
        return VStack(spacing: 0) {
            StageOby(character: character)
            Button {
                if !phone.limitDecisions.isEmpty { phone.homePath.append(.inbox) }
                else if let first = PhoneWork.firstNeeding(work, inbox: phone.answering) { phone.homePath.append(first) }
            } label: {
                Text(HomeLine.attention(running: running, needs: needs, memory: memory))
                    .font(Tokens.font(Tokens.textBase))
                    .foregroundStyle(running + needs + memory == 0 ? Tokens.text : Tokens.muted)
                    .multilineTextAlignment(.center)
            }
            .buttonStyle(.plain)
            .disabled(needs == 0)
            .accessibilityHint(needs > 0 ? "Opens what needs you" : "")
            .padding(.top, 14)
        }
        .frame(maxWidth: .infinity)
    }

    /// What waits on you, as one stack of cards, answered here with a tap (the owner's decision of 2026-10-24).
    @ViewBuilder private var asks: some View {
        let asks = PhoneAsks.over(phone.answering, in: nil)
        VStack(spacing: 12) {
            ForEach(phone.limitDecisions) { AccountLimitDecision(decision: $0) }
            if !asks.isEmpty { AskStack(items: asks) }
        }
    }

    /// `spoken`: the draft has dictated words, so the thread says "You, by voice".
    private func send(spoken: Bool) {
        phone.ask(text, spoken: spoken, new: true)
        text = ""
        typing = false
    }
}

/// What Oby asks that the stack shows: the kinds that wait on the owner's own answer, once each. An
/// offer made in the conversation on screen is in its thread, where it was made.
nonisolated enum PhoneAsks {
    /// A permission, a Confirm, a question (Oby's or the agent's, #858), a plan, a Start.
    static let kinds: Set<String> = ["permission", RunnerConfirm.kind, "question", "agentQuestion", "plan", "start"]

    static func over(_ answering: [InboxItem], in conversation: String?) -> [InboxItem] {
        PhoneOffer.once(answering.filter { kinds.contains($0.kind) && !($0.offer != nil && conversation != nil && $0.conversation == conversation) })
    }
}

/// What Oby is asking, over your conversation, answered there in one click (CHATFIRST-8). Taller than
/// there's room for, it scrolls, so its choices stay reachable.
struct AsksOver: View {
    @Environment(Phone.self) private var phone
    let asks: [InboxItem]
    let typing: Bool

    var body: some View {
        if !asks.isEmpty || !phone.limitDecisions.isEmpty {
            let stack = VStack(spacing: 12) {
                ForEach(phone.limitDecisions) { AccountLimitDecision(decision: $0) }
                if !asks.isEmpty { AskStack(items: asks) }
            }
            .padding(.horizontal, Tokens.gutter).padding(.vertical, 8)
            ViewThatFits(in: .vertical) {
                stack
                ScrollView { stack }.scrollIndicators(.never)
            }
            // ponytail: fixed heights; a measured share of the screen if these ever crowd a small iPhone
            .frame(maxHeight: typing ? 220 : 420)
            .background(Tokens.bg)
        }
    }
}

/// Oby on its stage (`.oby-frame`), the same face on the empty chat and on the unpaired screen.
struct StageOby: View {
    @Environment(\.colorScheme) private var colorScheme
    let character: ObyCharacter

    var body: some View {
        WebView(character.page)
            .webViewContentBackground(.hidden)
            .opacity(character.ready ? 1 : 0)
            .allowsHitTesting(false)
            .frame(width: 160, height: 157)
            // White in both themes: on light, a soft shadow keeps its shape (the design's `--oby-glow`).
            .shadow(color: .black.opacity(colorScheme == .light ? 0.10 : 0), radius: 11, y: 10)
            .accessibilityElement()
            .accessibilityLabel("Oby")
    }
}

/// Everything waiting on you, as the Mac's Home has it (its HomeDecisions, Sangam's AskStack, #1442):
/// a stack of cards, the oldest on top. The top card is the whole decision, answered right here,
/// under a head with its task and where it is in the stack ("2/5") between the arrows that go round
/// it; the next two peek out under it. Answered (once the runner confirms), it lifts away and the
/// one after takes its place; put off with Later, it leaves until it's due again.
struct AskStack: View {
    @Environment(Phone.self) private var phone
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let items: [InboxItem]
    /// The card on top, and where it was: gone, the one that took its place.
    @State private var at = HomeStack.Place()
    /// How the top card leaves: lifted when answered, sunk the way you went when you go round.
    @State private var exit = HomeStack.Exit.lift

    var body: some View {
        let index = HomeStack.index(of: at, in: items.map(\.cardID))
        let top = items[index]
        let peeks = min(2, items.count - 1)
        ZStack {
            InboxDecision(item: top, openTask: true, head: AnyView(head(top, index: index)))
                .id(top.cardID)
                .transition(reduceMotion ? .opacity : .asymmetric(insertion: HomeStack.Exit.rise, removal: exit.transition))
        }
        // The next two, a step further back each, peeking out under it.
        .background(alignment: .bottom) {
            ZStack {
                ForEach(Array((1...2).reversed().filter { $0 <= peeks }), id: \.self) { depth in
                    RoundedRectangle(cornerRadius: Tokens.askRadius, style: .continuous)
                        .fill(Tokens.chip.mix(with: Tokens.bg, by: 0.25 * Double(depth) - 0.1))
                        .scaleEffect(1 - 0.05 * CGFloat(depth), anchor: .bottom)
                        .offset(y: 8 * CGFloat(depth))
                }
            }
            .accessibilityHidden(true)
        }
        .padding(.bottom, 8 * CGFloat(peeks))
        .animation(reduceMotion ? .easeOut(duration: 0.2) : Tokens.settle, value: top.cardID)
        .animation(reduceMotion ? nil : Tokens.warm, value: peeks)
        // Kept in step with the list: a card coming back above it moves its place, not the card.
        .onChange(of: HomeStack.Place(id: top.cardID, index: index), initial: true) { _, place in at = place }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Needs you")
    }

    /// The card's head (the demo's `tc-head`): its task's agent and name, and where it is in the
    /// stack between the arrows that go round it (none with only one).
    private func head(_ item: InboxItem, index: Int) -> some View {
        let job = phone.jobs.first { $0.id == item.job }
        let many = items.count > 1
        return HStack(spacing: 8) {
            if let job {
                AgentMark(agent: job.agent).font(Tokens.font(13, .semibold)).frame(width: 16).foregroundStyle(Tokens.muted)
            } else {
                ObyMark(size: 16)
            }
            // An offer goes under the conversation it was made in (its job isn't one until it starts).
            let conversation = item.offer == nil ? nil : phone.conversations?.first { $0.id == item.conversation }?.title
            Text(job?.heading ?? conversation.flatMap { $0.isEmpty ? nil : $0 } ?? item.title).font(Tokens.font(Tokens.textBase, .semibold)).foregroundStyle(Tokens.ink)
                .lineLimit(1).fadeOut()
            Spacer(minLength: 0)
            HStack(spacing: 0) {
                if many { arrow("chevron.left", "Previous") { go(-1, from: index) } }
                Text("\(index + 1)/\(items.count)").font(Tokens.font(Tokens.textSm)).monospacedDigit()
                    .foregroundStyle(Tokens.text).frame(minWidth: 28)
                    .accessibilityLabel("\(index + 1) of \(items.count)")
                if many { arrow("chevron.right", "Next") { go(1, from: index) } }
            }
            .padding(.vertical, -10)
        }
    }

    private func arrow(_ symbol: String, _ label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol).font(Tokens.font(13, .semibold)).foregroundStyle(Tokens.muted)
                .frame(width: 36, height: 44).contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }

    /// Round the stack by one: the card on top sinks back the way you went (set a moment before it
    /// goes, so it leaves that way), the next comes up.
    private func go(_ step: Int, from index: Int) {
        let next = HomeStack.step(index, by: step, count: items.count)
        guard next != index else { return }
        exit = step > 0 ? .next : .prev
        DispatchQueue.main.async {
            at = HomeStack.Place(id: items[next].cardID, index: next)
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.6) { exit = .lift }
        }
    }
}

/// One kind of work's page (Sangam's GroupView.tsx): its name, Running and Completed tabs with
/// their counts (Review PR and Issues have To do first, for GitHub's items no job works on yet), a
/// card per task under the open tab, and under Completed the way to Archived.
struct KindPage: View {
    @Environment(Phone.self) private var phone
    @Environment(\.push) private var push
    let kind: WorkType
    /// The tab picked, or nil for the one it opens on.
    @State private var picked: WorkList.Tab?
    /// Issues and Review PR each remember which projects, repositories and milestones to show. Empty shows every one.
    @AppStorage("githubProjects.issues") private var issuesProjects = ""
    @AppStorage("githubProjects.review") private var reviewProjects = ""
    @AppStorage("githubRepos.issues") private var issuesRepos = ""
    @AppStorage("githubRepos.review") private var reviewRepos = ""
    @AppStorage("githubMilestones.issues") private var issuesMilestones = ""
    @AppStorage("githubMilestones.review") private var reviewMilestones = ""

    /// GitHub's part of the page: only Review PR and Issues have one.
    private var gitHub: Bool { kind == .review || kind == .issues }
    private var connect: GitHubWork.Connect { gitHub ? .init(phone.gitHubWork) : .unknown }
    /// Routines only: each routine's next run.
    private var upcoming: [RunnerRoutine] { kind == .routines ? RunnerRoutine.upcoming(phone.routines) : [] }
    /// To do is where the page opens, unless a job runs or needs you, only when it has something:
    /// GitHub's items, or Connect GitHub.
    private var opensOnToDo: Bool { gitHub && (!toDo.isEmpty || connect == .signedOut) }
    private var toDo: [GitHubWork.Column] { gitHub ? phone.gitHubWork?.toDo(kind, later: phone.gitHubLater) ?? [] : [] }

    /// GitHub's sign-in is the Mac's alone: a paired phone never signs the Mac in to someone's GitHub.
    static let connectLine = "See the pull requests waiting for your review and the issues assigned to you here, and hand any of them to Oby in one tap. Connect GitHub on your Mac, in Settings › Connectors."

    var body: some View {
        let items = WorkList.ranked(phone.work.filter { $0.type == kind })
        let tab = picked ?? WorkList.firstTab(items, toDo: opensOnToDo)
        let columns = GitHubWork.columns(toDo, picked: GitHubWork.projectSet(projectRaw.wrappedValue), repos: GitHubWork.projectSet(repoRaw.wrappedValue), milestones: GitHubWork.projectSet(milestoneRaw.wrappedValue))
        ScrollView {
            VStack(alignment: .leading, spacing: 8) {
                Text(kind.label).font(Tokens.font(28, .semibold)).foregroundStyle(Tokens.ink)
                    .padding(.bottom, 12).accessibilityAddTraits(.isHeader)
                LineTabs(tab: tab, counts: WorkList.counts(items, toDo: gitHub ? columns.reduce(0) { $0 + $1.items.count } : nil, upcoming: upcoming.count)) {
                    picked = $0
                }
                .padding(.bottom, 8)
                if gitHub && tab == .todo {
                    WorkFilters(columns: toDo, projects: projectRaw, repos: repoRaw, milestones: milestoneRaw)
                }
                let cards = WorkList.items(items, in: tab)
                if tab == .todo {
                    GitHubToDo(kind: kind, work: phone.gitHubWork, columns: columns,
                               connect: connect == .signedOut ? ConnectGitHubCard(line: Self.connectLine, connect: nil) : nil,
                               loading: !phone.loaded && phone.connection == .connecting,
                               start: { try await phone.startGitHubWork($0.id) }, later: { phone.gitHubLater.insert($0.id) })
                }
                // A routine waiting for its next run says when (Sangam's tasks.ts).
                if tab == .running { ForEach(upcoming) { UpcomingRoutineCard(routine: $0) } }
                ForEach(cards) { TaskCard(item: $0) }
                if tab != .todo && cards.isEmpty && (tab == .done || upcoming.isEmpty) {
                    if !phone.loaded && phone.connection == .connecting {
                        CardBones(rows: 2)
                    } else {
                        Text(tab == .done ? "Nothing completed yet" : "Nothing running")
                            .font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.muted).padding(.vertical, 8).padding(.horizontal, 4)
                    }
                }
                if tab == .done {
                    Button("Older ones are in Archived") { push(.archived) }
                        .font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted)
                        .padding(.top, 8).frame(maxWidth: .infinity)
                }
            }
            .padding(.horizontal, Tokens.gutter).padding(.top, 8).padding(.bottom, 32)
        }
        .obyPage()
        .navigationBarTitleDisplayMode(.inline)
        // The tab it opens on stays put while you're on it, so a task arriving doesn't switch it.
        // On Review PR and Issues it waits for GitHub's list, so it doesn't settle on an empty Running first.
        .onAppear { if !gitHub || phone.gitHubWork != nil { picked = picked ?? WorkList.firstTab(items, toDo: opensOnToDo) } }
        .onChange(of: phone.gitHubWork == nil) { if picked == nil { picked = WorkList.firstTab(items, toDo: opensOnToDo) } }
        .refreshable { await phone.reload() }
        // Opening Review PR or Issues looks at GitHub again; the list arrives as an event.
        .task { if gitHub { await phone.refreshGitHubWork() } }
    }

    private var projectRaw: Binding<String> { stored($issuesProjects, $reviewProjects) }
    private var repoRaw: Binding<String> { stored($issuesRepos, $reviewRepos) }
    private var milestoneRaw: Binding<String> { stored($issuesMilestones, $reviewMilestones) }

    private func stored(_ issues: Binding<String>, _ review: Binding<String>) -> Binding<String> {
        switch kind {
        case .issues: issues
        case .review: review
        default: .constant("")
        }
    }
}

/// Archived (Sangam's ArchiveView.tsx), kept by this iPhone: the done tasks that left Home, most
/// recently done first, with a search over them.
struct ArchivedPage: View {
    @Environment(Phone.self) private var phone
    @Environment(\.push) private var push
    @State private var query = ""

    var body: some View {
        let all = phone.archived.sorted { ($0.finished ?? $0.created) > ($1.finished ?? $1.created) }
        let q = query.trimmingCharacters(in: .whitespaces)
        let shown = all.filter { q.isEmpty || "\($0.heading) \($0.agentName) \($0.repoName)".localizedCaseInsensitiveContains(q) }
        List {
            ForEach(shown) { job in
                Button { push(.task(job.id)) } label: {
                    HStack(spacing: 10) {
                        AgentMark(agent: job.agent).font(Tokens.font(12, .semibold)).frame(width: 16).foregroundStyle(Tokens.muted)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(job.heading).font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.item).lineLimit(1)
                            Text(TaskMeta.line(job, account: phone.accountName(job.account))).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted).lineLimit(1)
                        }
                        Spacer(minLength: 8)
                        Text(ConversationOrder.ago(job.finished ?? job.created)).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted)
                    }
                    .frame(minHeight: 44)
                }
                .listRowBackground(Tokens.surface)
            }
            if shown.isEmpty {
                Text(all.isEmpty ? "Nothing archived yet" : "Nothing archived matches “\(q)”")
                    .font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.muted).listRowBackground(Color.clear)
            }
        }
        .searchable(text: $query, prompt: "Search archived tasks")
        .obyPage()
        .navigationTitle("Archived")
        .navigationBarTitleDisplayMode(.inline)
    }
}

/// The Inbox (docs/oby-ux.md row 10): what only you can answer and has no task on Home (an audit,
/// an Oby failure, a task older than the list), the longest-waiting first. The rest live with
/// their task: on its card, its page and in its conversation.
struct InboxPage: View {
    @Environment(Phone.self) private var phone

    var body: some View {
        // Every half minute: what was put off comes back.
        TimelineView(.periodic(from: .now, by: 30)) { context in
            let items = PhoneOffer.once(PhoneWork.loose(InboxItem.waiting(phone.inbox, now: context.date), among: phone.work))
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Inbox").font(Tokens.font(28, .semibold)).foregroundStyle(Tokens.ink).padding(.bottom, 8)
                        .accessibilityAddTraits(.isHeader)
                    ForEach(phone.limitDecisions) { decision in AccountLimitDecision(decision: decision) }
                    ForEach(items, id: \.cardID) { item in
                        VStack(alignment: .leading, spacing: 6) {
                            Text(item.line(now: context.date)).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted).lineLimit(1)
                            InboxDecision(item: item, openTask: true)
                        }
                    }
                    if items.isEmpty && phone.limitDecisions.isEmpty {
                        Text("Nothing needs you").font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.muted)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, Tokens.gutter).padding(.top, 8).padding(.bottom, 32)
            }
        }
        .obyPage()
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await phone.reload() }
    }
}
