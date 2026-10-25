import AppKit
import SwiftUI
import WebKit

/// The side panel's sections, picked by the icons on its left and the account menu (Sangam's Sidebar.tsx).
nonisolated enum PanelSection: Sendable { case home, conversations, settings }

/// Opens a Settings page in the window: the message box's usage panel asks for Accounts. Equal while
/// it opens on the same window's state, so views reading it don't redraw on every update.
struct OpenSettingsPage: Equatable {
    weak var state: ChatState?
    var animated = true

    func callAsFunction(_ page: SettingsPage, spotlight: SettingsSpotlight? = nil) {
        withAnimation(animated ? Tokens.warm : nil) { state?.openSettings(page, spotlight: spotlight) }
    }

    /// The setting Settings points at, and done with it.
    var spotlight: SettingsSpotlight? { state?.spotlight }
    func clear(_ spotlight: SettingsSpotlight) { if state?.spotlight == spotlight { state?.spotlight = nil } }

    static func == (a: Self, b: Self) -> Bool { a.state === b.state && a.animated == b.animated }
}

/// The one setting a Settings page points at, ringed and scrolled to, when another page sent you
/// there: Pair a device sends you to turn Remote access on, and once it's on, back to Pair a device.
nonisolated enum SettingsSpotlight: Hashable, Sendable { case remoteAccess, pairDevice }

extension EnvironmentValues {
    @Entry var openSettingsPage = OpenSettingsPage()
}

/// The Oby window (Sangam's design, #11 and #270), opened from the menu bar icon, an alert or ⌘O: the
/// side panel, and Home with pages opening over it.
final class ChatWindow: NSObject, NSWindowDelegate {
    private let island: IslandController
    private var window: NSWindow?
    /// What's open over Home, and where the title bar's content starts.
    private let state = ChatState()
    /// Oby on Home's stage: its own page, paused while out of sight.
    private lazy var character = ObyCharacter(query: "embed&size=1.23")
    /// In front while the window's character is still drawing. Nil once the window is showing.
    private var splash: NSPanel?

    init(island: IslandController) { self.island = island }

    /// Shows the window, on the conversation or on a job's page when one is named; on Home, with
    /// Oby and its message box, when Oby was just opened.
    func show(job: String? = nil, home: Bool = false) {
        state.open(job: job)
        if home {
            state.section = .home
            state.newTask()
        }
        island.model.chatWindowVisible = true
        if job == nil { island.model.markChatRead() }
        present()
    }

    /// Shows the window on one of obid's conversations, where its cards wait (an alert's click, #862).
    func show(conversation id: String) {
        show()
        Task { await island.feed.conversations.open(id) }
    }

    /// Shows the window on Settings (⌘, and the menu bar's Settings…): `page`, else General unless one
    /// of its pages shows already.
    func showSettings(_ page: SettingsPage? = nil) {
        state.visible = true
        state.openSettings(page)
        island.model.chatWindowVisible = true
        present()
    }

    private func present() {
        NSApp.activate()
        if let window {
            window.alphaValue = 1
            endSplash(reveal: false)
            window.makeKeyAndOrderFront(nil)
            return
        }
        let window = Self.make(ChatView(model: island.model, state: state, character: character))
        window.delegate = self
        window.setFrameAutosaveName("ObyChat")
        if !window.setFrameUsingName("ObyChat") { window.center() }
        self.window = window
        Self.measureLights(window, into: state)
        // The character only starts once its page is in a window. Keep that window invisible
        // behind the splash until the first frame, so Home doesn't open blank.
        guard !character.ready else {
            window.makeKeyAndOrderFront(nil)
            return
        }
        window.alphaValue = 0
        window.orderFront(nil)
        showSplash()
    }

    private func showSplash() {
        let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 340, height: 360),
                            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.isFloatingPanel = true
        panel.level = .floating
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = true
        panel.hidesOnDeactivate = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        panel.contentView = NSHostingView(rootView: SplashMark(character: character) { [weak self] reduce in
            self?.endSplash(reveal: true, reduce: reduce)
        })
        panel.center()
        panel.orderFrontRegardless()
        splash = panel
    }

    private func endSplash(reveal: Bool, reduce: Bool = true) {
        let panel = splash
        splash = nil
        panel?.orderOut(nil)
        guard reveal, panel != nil, let window else { return }
        if reduce {
            window.alphaValue = 1
        } else {
            NSAnimationContext.runAnimationGroup { context in
                context.duration = 0.22
                window.animator().alphaValue = 1
            }
        }
        window.makeKeyAndOrderFront(nil)
    }

    /// The window around `view`, as Sangam's: a toolbar's title bar, so the traffic lights sit in
    /// the middle of a taller bar that the trail shares (nothing's in the toolbar; it only sets the
    /// height), dark, at least as wide as the view gets.
    static func make(_ view: some View, size: NSSize = NSSize(width: 994, height: 670)) -> NSWindow {
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: size),
                              styleMask: [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        window.title = "Oby"
        window.titlebarAppearsTransparent = true
        window.titleVisibility = .hidden
        window.toolbar = NSToolbar(identifier: "ObyWindow")
        window.toolbarStyle = .unified
        window.isMovableByWindowBackground = false
        window.appearance = NSAppearance(named: .darkAqua)
        window.backgroundColor = NSColor(white: 11 / 255, alpha: 1)
        window.isReleasedWhenClosed = false
        window.minSize = NSSize(width: Tokens.viewMin, height: 520)
        window.contentView = NSHostingView(rootView: view)
        return window
    }

    /// Where macOS put the traffic lights: the title bar is as tall as twice their middle, and its
    /// content starts just right of them. In full screen there are none.
    static func measureLights(_ window: NSWindow, into state: ChatState) {
        // In full screen the lights live in a toolbar window of their own (and hide): no measuring across it.
        guard let zoom = window.standardWindowButton(.zoomButton), zoom.window === window, !zoom.isHidden,
              !window.styleMask.contains(.fullScreen), let bar = zoom.superview, let content = window.contentView else {
            state.chrome = WindowChrome(bar: Tokens.bar, afterLights: 12)
            return
        }
        let frame = content.convert(bar.convert(zoom.frame, to: nil), from: nil)
        let middle = content.isFlipped ? frame.midY : content.bounds.height - frame.midY
        state.chrome = WindowChrome(bar: min(64, max(Tokens.bar, (middle * 2).rounded())), afterLights: (frame.maxX + 10).rounded())
    }

    private func measureLights() { if let window { Self.measureLights(window, into: state) } }

    func windowDidResize(_ notification: Notification) { measureLights() }
    func windowDidEnterFullScreen(_ notification: Notification) { measureLights() }
    func windowDidExitFullScreen(_ notification: Notification) { measureLights() }
    func windowWillClose(_ notification: Notification) {
        window?.alphaValue = 1
        endSplash(reveal: false)
        island.model.chatWindowVisible = false
        state.visible = false
        // Once it's gone: what waited for the owner while it showed it alerts.
        Task { [island] in island.feed.windowLeft() }
    }
    func windowDidMiniaturize(_ notification: Notification) {
        island.model.chatWindowVisible = false
        state.visible = false
        island.feed.windowLeft()
    }
    /// In front again: the alerts for what it shows go.
    func windowDidBecomeKey(_ notification: Notification) {
        if let place = shownPlace { island.feed.seen(place) }
    }
    func windowDidDeminiaturize(_ notification: Notification) {
        island.model.chatWindowVisible = true
        state.visible = true
        if state.trail.top == .talk { island.model.markChatRead() }
    }

    var isOpen: Bool { window?.isVisible ?? false }

    /// What the window's top page is about, for alerts (#862).
    private var shownPlace: AlertPlace? { AlertPlace.shown(state.trail.top, conversation: island.feed.conversations.shown) }

    /// The window is in front, on `place`'s page: an alert for it would only repeat what's on screen.
    func shows(_ place: AlertPlace) -> Bool {
        guard NSApp.isActive, let window, window.isVisible, !window.isMiniaturized else { return false }
        return shownPlace == place
    }

    /// The window is in front on the conversation page: the talk key's words carry on in it (#493).
    var showsConversation: Bool { NSApp.isActive && window?.isVisible == true && state.trail.top == .talk }

    /// The window is in front on Home or the conversation, where what Oby hears shows (#528). Only
    /// the conversation shows what Oby answers (`RunnerFeed.talkShown`).
    var showsTalk: Bool {
        guard NSApp.isActive, window?.isVisible == true else { return false }
        return state.trail.top == nil || state.trail.top == .talk
    }
}

/// The title bar's height and where its content starts, from where macOS put the traffic lights.
nonisolated struct WindowChrome: Equatable, Sendable {
    var bar = Tokens.bar
    var afterLights = Tokens.afterLights
}

@Observable
final class ChatState {
    /// What's open over Home, in the order it was opened.
    var trail = WindowTrail()
    /// Asks the conversation's box, and Home's, to take the keyboard.
    var composerFocusRequest = 0
    var homeFocusRequest = 0
    var chrome = WindowChrome()
    /// The window is on screen (not closed or in the Dock): Oby on Home draws only then.
    var visible = true
    /// Since when, and the job rows opened since: a job created after it and not opened yet is new
    /// (#496), its row and its kind's chip with a blue dot. Both are kept across launches (C8), so a
    /// task made while Oby was closed is new, and one opened before isn't.
    let since: Date
    var seen: Set<String> {
        // ponytail: every row ever opened, a few dozen bytes each; drop the oldest past a few thousand if it grows.
        didSet { defaults.set(Array(seen), forKey: Self.seenKey) }
    }
    @ObservationIgnored private let defaults: UserDefaults
    private static let sinceKey = "newWorkSince"
    private static let seenKey = "newWorkSeen"

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
        if let kept = defaults.object(forKey: Self.sinceKey) as? Date {
            since = kept
        } else {
            since = .now
            defaults.set(since, forKey: Self.sinceKey)
        }
        // Rows of the Desk's own jobs (`mac/…`, before #895) are gone: so are the ids they were seen by.
        seen = Set((defaults.stringArray(forKey: Self.seenKey) ?? []).filter { !$0.hasPrefix("mac/") })
        archiveAfter = defaults.string(forKey: ArchiveAfter.key).flatMap(ArchiveAfter.init) ?? .threeDays
    }
    /// When done tasks leave Home for Archived (Home's ⋮, Archive completed), remembered.
    var archiveAfter: ArchiveAfter {
        didSet { defaults.set(archiveAfter.rawValue, forKey: ArchiveAfter.key) }
    }
    /// The task or conversation being renamed in place: a side panel row's (`TaskRow`'s id), or the
    /// title bar's last crumb (`inBar`).
    var renaming: Renaming?
    struct Renaming: Equatable { var id: String; var inBar = false }
    /// The task Delete task asks about, and the conversation Delete conversation asks about, in
    /// the window's dialogs.
    var deleting: SidebarSections.JobRow?
    var deletingConversation: RunnerConversation?
    /// Why the last Rename, Archive task or Delete task didn't go through, until it's seen.
    var taskProblem: String?
    /// The side panel's folded groups and those showing all their done tasks.
    var folded: Set<String> = []
    var more: Set<String> = []
    /// What the side panel lists: Home's work, or (with obid's conversations, #493) the conversations.
    var section = PanelSection.home
    /// The setting the Settings page showing points at (`SettingsSpotlight`).
    var spotlight: SettingsSpotlight?

    /// A job's page, or the conversation with Oby, from a fresh start.
    func open(job: String? = nil) {
        visible = true
        if let job {
            trail.start(.job(id: job))
        } else {
            trail.start(.talk)
            section = .conversations
            composerFocusRequest += 1
        }
    }

    /// New task: back Home, into its message box.
    func newTask() {
        trail.closeAll()
        homeFocusRequest += 1
    }

    /// A Settings page over Home, with the side panel on Settings' list. With no page named (the
    /// account menu, ⌘,): General, unless one of Settings' pages shows already.
    func openSettings(_ page: SettingsPage? = nil, spotlight: SettingsSpotlight? = nil) {
        section = .settings
        self.spotlight = spotlight
        if let page {
            trail.start(.settings(page))
        } else if case .settings = trail.top {
        } else {
            trail.start(.settings(.general))
        }
    }
}

/// The window: the side panel and Home, the title bar over Home with its trail, and what's open
/// over Home, the top one showing.
struct ChatView: View {
    let model: IslandModel
    let state: ChatState
    var character: ObyCharacter?
    /// The side panel opens as you left it: closed the first time.
    @AppStorage("ObySidebarOpen") private var sidebarOpen = false
    @AppStorage("ObySidebarWidth") private var sidebarWidth = Double(Tokens.sidebar)
    /// While the panel's edge is dragged: its width then, rubber-banding past the limits.
    @State private var dragWidth: CGFloat?
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var panelWidth: CGFloat { dragWidth ?? CGFloat(sidebarWidth) }

    var body: some View {
        ZStack(alignment: .topLeading) {
            HStack(spacing: 0) {
                if sidebarOpen {
                    SidebarView(model: model, state: state, width: panelWidth, onNewTask: { withAnimation(motion) { state.newTask() } })
                        .overlay(alignment: .trailing) { resizeHandle }
                        .transition(.move(edge: .leading))
                }
                HomeView(model: model, state: state, character: character, sidebarOpen: sidebarOpen)
                    .frame(minWidth: Tokens.viewMin, maxWidth: .infinity, maxHeight: .infinity)
            }
            // Right of the traffic lights, over whatever is beneath, whether the panel is open or not.
            IconButton(symbol: "sidebar.left", action: { withAnimation(reduceMotion ? nil : sidebarOpen ? Tokens.easeIn : Tokens.settle) { sidebarOpen.toggle() } })
                .help(sidebarOpen ? "Hide sidebar" : "Show sidebar")
                .accessibilityLabel(sidebarOpen ? "Hide sidebar" : "Show sidebar")
                .padding(.leading, state.chrome.afterLights).padding(.top, (state.chrome.bar - 32) / 2)
        }
        .foregroundStyle(Tokens.text)
        .background(Tokens.bg)
        .preferredColorScheme(.dark)
        .ignoresSafeArea()
        .environment(\.openSettingsPage, OpenSettingsPage(state: state, animated: !reduceMotion))
        .taskDialogs(model: model, state: state)
        // Esc goes back one page.
        .onExitCommand { if !state.trail.isHome { withAnimation(motion) { state.trail.back() } } }
        // Opened: its blue dot goes.
        .onChange(of: state.trail.top, initial: true) { _, top in
            if case .job(let id) = top { state.seen.insert(SidebarSections.JobRow.id(id)) }
            if top == .talk { model.markChatRead() }
        }
        // What the page shows needs no alert any more (#862).
        .onChange(of: AlertPlace.shown(state.trail.top, conversation: model.feed.conversations.shown), initial: true) { _, place in
            if let place { model.feed.seen(place) }
        }
        // A job that left every list: its page goes too.
        .onChange(of: model.feed.jobs.map(\.id)) {
            state.trail.drop { step in
                switch step {
                case .job(let id): !model.feed.jobs.contains { $0.id == id }
                case .session(let id): !model.feed.jobs.contains { $0.id == id }
                default: false
                }
            }
        }
    }

    private var motion: Animation? { reduceMotion ? nil : Tokens.warm }

    /// The panel's right edge: drag to resize, double-click for the default width.
    private var resizeHandle: some View {
        Color.clear.frame(width: 8).contentShape(Rectangle())
            .offset(x: 1)
            .pointerStyle(.columnResize)
            .gesture(DragGesture(minimumDistance: 1, coordinateSpace: .global)
                .onChanged { drag in dragWidth = PanelWidth.dragging(CGFloat(sidebarWidth) + drag.translation.width) }
                .onEnded { drag in
                    let settled = PanelWidth.settled(CGFloat(sidebarWidth) + drag.translation.width)
                    withAnimation(reduceMotion ? nil : Tokens.settle) { dragWidth = nil; sidebarWidth = Double(settled) }
                })
            .onTapGesture(count: 2) { withAnimation(reduceMotion ? nil : Tokens.settle) { sidebarWidth = Double(Tokens.sidebar) } }
            .accessibilityLabel("Resize side panel")
    }
}

/// Home (Sangam's Home.tsx): the title bar and its trail, Oby's stage with what's running and the
/// chips under it, what waits on you (the owner's decision of 2026-10-24), the message box at the
/// foot, and the pages that open over it.
private struct HomeView: View {
    let model: IslandModel
    let state: ChatState
    let character: ObyCharacter?
    let sidebarOpen: Bool
    @State private var text = ""
    @State private var typing = false
    @State private var height: CGFloat = 600
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var work: [WorkList.Item<SidebarSections.JobRow>] { model.work(state) }
    private var away: Bool { !state.trail.isHome }

    var body: some View {
        VStack(spacing: 0) {
            // Over the pages: their scroll views reach up under the bar, and would take its clicks.
            TitleBar(model: model, state: state, sidebarOpen: sidebarOpen).zIndex(1)
            ZStack {
                VStack(spacing: 0) {
                    stage
                        .opacity(away ? 0 : 1).scaleEffect(away && !reduceMotion ? 0.98 : 1)
                    if !away {
                        HomeDecisions(model: model, height: height, onOpen: { state.trail.start(.job(id: $0)) })
                            .frame(maxWidth: Tokens.homeBox - 2 * Tokens.gutter)
                            .padding(.horizontal, Tokens.gutter).padding(.bottom, 12)
                    }
                    if model.listening && !away {
                        ListeningLine(voice: model.voice).frame(maxWidth: Tokens.homeBox - 2 * Tokens.gutter)
                            .padding(.horizontal, Tokens.gutter).padding(.bottom, 8)
                    }
                    box
                        .opacity(away ? 0 : 1).offset(y: away && !reduceMotion ? 12 : 0)
                }
                .allowsHitTesting(!away)
                .accessibilityHidden(away)
                if let top = state.trail.top {
                    page(top)
                        .id(top)
                        .transition(.asymmetric(insertion: .opacity.combined(with: .offset(y: reduceMotion ? 0 : 28)),
                                                removal: .opacity.combined(with: .offset(y: reduceMotion ? 0 : 12))))
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .animation(reduceMotion ? .easeOut(duration: 0.2) : Tokens.warm, value: state.trail)
        .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { height = $0 }
        .onChange(of: away || !state.visible, initial: true) { _, hidden in character?.pause(hidden) }
        .onChange(of: model.mood, initial: true) { _, mood in character?.mood = mood }
        .onChange(of: state.homeFocusRequest) { typing = true }
        // A page over Home: the hidden box lets go of the keyboard, so nothing typed lands in it.
        .onChange(of: away) { if away { typing = false } }
    }

    /// Oby, a little above the middle (as a face at eye level), what's going on under it, and the
    /// chips: every kind of work, always, in Home's order (Sangam, mac-design B7), whatever the side
    /// panel groups by.
    private var stage: some View {
        let groups = WorkList.kinds(work)
        // A task waiting for memory is active, but not running: the line says it apart.
        let memory = work.count { $0.job.job.waitsForMemory }
        let running = work.count { $0.status == .running && !$0.job.job.waitsForMemory }, needs = work.count { $0.status == .needs }
        return EyeLevel {
            VStack(spacing: 16) {
                StagedOby(model: model, character: character)
                    .padding(.bottom, -8)
                Text(HomeLine.attention(running: running, needs: needs, memory: memory))
                    .font(Tokens.font(Tokens.textBase))
                    .foregroundStyle(running + needs + memory == 0 ? Tokens.text : Tokens.muted)
                if !groups.isEmpty {
                    CenteredFlow(spacing: 8) {
                        ForEach(groups) { group in
                            HomeChip(group: group) { if let kind = group.type { state.trail.start(.group(kind)) } }
                        }
                    }
                    .frame(maxWidth: 440)
                    .accessibilityElement(children: .contain)
                    .accessibilityLabel("Your tasks")
                }
            }
        }
        .padding(.horizontal, Tokens.gutter).padding(.bottom, 24)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    /// Asking Oby: the conversation opens on it.
    private var box: some View {
        MessageBox(text: $text, focused: $typing, tray: model.tray,
                   placeholder: HomeLine.placeholder(fullName: NSFullUserName()),
                   maximumHeight: height * 0.5, focusRequest: state.homeFocusRequest,
                   onSend: ask, onScreenshot: screenshot, onStop: { false }, usage: model.usageRows,
                       voice: model.voice, talkKeyListening: model.listening)
            .keepsDraft($text, as: Drafts.home)
            .frame(maxWidth: Tokens.homeBox - 2 * Tokens.gutter)
            .padding(.horizontal, Tokens.gutter).padding(.bottom, Tokens.gutter)
    }

    private func back() { withAnimation(reduceMotion ? nil : Tokens.warm) { state.trail.back() } }

    /// Asked in Home's box: it goes to Oby, and the conversation opens on it.
    private func ask() {
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        // Home's box starts a conversation of its own (obid keeps many, #493).
        model.feed.conversations.startNew()
        model.sendChat(text)
        text = ""
        state.open()
    }

    private func screenshot() {
        Task {
            await model.tray.screenshot()
            typing = true
        }
    }

    @ViewBuilder private func page(_ step: WindowStep) -> some View {
        switch step {
        case .talk:
            ConversationPage(model: model, composerFocusRequest: state.composerFocusRequest,
                             onOpenJob: { state.trail.push(.job(id: $0)) }, onBack: back)
        case .job(let id):
            JobPane(model: model, id: id, onBack: back,
                    onOpenJob: { state.trail.push(.job(id: $0)) },
                    onConversation: { state.trail.push(.talk) },
                    onSession: { state.trail.push(.session(id: id)) }).takesKeys()
        case .session(let id):
            SessionPane(model: model, id: id, onBack: back).takesKeys()
        case .group(let kind):
            GroupPane(model: model, state: state, kind: kind, items: work.filter { $0.type == kind },
                      onOpenJob: { state.trail.push(.job(id: $0)) },
                      onOpenArchive: { state.trail.push(.archived) },
                      onConnectGitHub: { state.openSettings(.connectors) }, onClose: back)
                .takesKeys()
        case .archived:
            ArchivePane(items: model.archive(state),
                        loading: model.feed.connection == .connecting && !model.feed.showingKept,
                        onOpenJob: { state.trail.push(.job(id: $0)) })
                .takesKeys()
        case .settings(let page):
            SettingsPane(model: model, page: page)
        }
    }
}

/// Everything waiting on the owner, in one place on Home (the owner's decision of 2026-10-24): each
/// permission, Confirm, question and Start as the conversation's own card (Sangam's AskBlock), the
/// oldest first, answered right here (Allow, Always allow, Deny, Confirm…) without opening its task.
/// It hugs its cards, scrolling past what fits between Oby and the message box.
private struct HomeDecisions: View {
    let model: IslandModel
    /// Home's height: the cards get what Oby's stage and the box leave.
    let height: CGFloat
    let onOpen: (_ job: String) -> Void
    @State private var content: CGFloat = 0

    /// What the title bar, Oby's stage (smaller while something waits) and the message box need,
    /// measured on macOS 27.
    // ponytail: a measured reserve; measure the stage and the box if either changes size.
    private static let reserve: CGFloat = 400
    /// At least one card's head and buttons, however small the window.
    private static let least: CGFloat = 140

    /// The inbox's decisions that wait now, here and on the runner, the oldest first.
    static func entries(_ model: IslandModel, now: Date) -> [IslandInbox.Entry] {
        IslandInbox.entries(feed: model.feed, now: now).filter(\.item.isDecision)
    }

    /// A runner's permission it only names in its job (an older runner's, #134).
    static func permissions(_ model: IslandModel, now: Date) -> [RunnerFeed.PermissionRecord] {
        model.feed.chatPermissions.filter { $0.waits(at: now) }
    }

    /// Whether anything waits on the owner now.
    static func waits(_ model: IslandModel, now: Date) -> Bool {
        !entries(model, now: now).isEmpty || !permissions(model, now: now).isEmpty
    }

    var body: some View {
        // Every half minute: what was put off comes back.
        TimelineView(.periodic(from: .now, by: 30)) { context in
            let entries = Self.entries(model, now: context.date)
            let permissions = Self.permissions(model, now: context.date)
            if !entries.isEmpty || !permissions.isEmpty {
                ScrollView {
                    VStack(spacing: 12) {
                        ForEach(entries) { entry in
                            InlineDecision(entry: entry, feed: model.feed, onOpen: onOpen)
                        }
                        ForEach(permissions) { record in
                            InlinePermission(record: record, feed: model.feed)
                        }
                    }
                    .onGeometryChange(for: CGFloat.self) { $0.size.height } action: { content = $0 }
                }
                .scrollIndicators(.automatic)
                .frame(height: min(content, max(Self.least, height - Self.reserve)))
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Needs you")
            }
        }
    }
}

/// Oby on Home's stage, a little smaller while something waits on you, so its cards fit under it.
/// A view of its own: only it and the cards follow the inbox, never Home and the pages over it.
private struct StagedOby: View {
    let model: IslandModel
    let character: ObyCharacter?

    var body: some View {
        let waits = HomeDecisions.waits(model, now: .now)
        OnStage(character: character).frame(width: waits ? 200 : 280, height: waits ? 150 : 220)
    }
}

/// Oby on Home's stage: the live character, hidden until its first frame is drawn so nothing
/// the page paints while it loads can flash.
private struct OnStage: View {
    let character: ObyCharacter?

    var body: some View {
        if let character {
            WebView(character.page)
                .webViewContentBackground(.hidden)
                .opacity(character.ready ? 1 : 0)
                .allowsHitTesting(false)
                .accessibilityElement()
                .accessibilityLabel("Oby")
        } else {
            ObyFace(size: 96)
        }
    }
}

/// A kind of work's chip on Home: a blue dot ahead of its name while it holds a new task, an
/// orange one when something in it needs you; both can show.
private struct HomeChip: View {
    let group: WorkList.Group<SidebarSections.JobRow>
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 8) {
                if group.fresh || group.needs > 0 {
                    HStack(spacing: 4) {
                        if group.fresh { Circle().fill(Tokens.unread).frame(width: 6, height: 6).accessibilityLabel("New") }
                        if group.needs > 0 { Circle().fill(Tokens.needs).frame(width: 6, height: 6).accessibilityLabel("Needs you") }
                    }
                }
                Text(group.label).font(Tokens.font(Tokens.textBase)).lineLimit(1)
            }
            .foregroundStyle(hovering ? Tokens.ink : Tokens.text)
            .padding(.horizontal, 16).frame(height: 34)
            .background(hovering ? Tokens.pillHover : Tokens.pill, in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .animation(Tokens.fade, value: hovering)
        .accessibilityElement(children: .combine)
    }
}

/// The title bar: back to Home and the trail (each earlier step goes back to it, the last is
/// where you are), and on a task's page the way into its conversation.
private struct TitleBar: View {
    let model: IslandModel
    let state: ChatState
    let sidebarOpen: Bool

    var body: some View {
        HStack(spacing: 0) {
            if !state.trail.isHome {
                IconButton(symbol: "chevron.left", action: { state.trail.closeAll() })
                    .help("Back to Home").accessibilityLabel("Back to Home")
                    .padding(.trailing, 8)
                HStack(spacing: 2) {
                    ForEach(Array(state.trail.steps.enumerated()), id: \.element) { index, step in
                        if index > 0 {
                            Image(systemName: "chevron.right").font(Tokens.font(11, .light)).foregroundStyle(Tokens.muted.opacity(0.6))
                                .frame(width: 22).accessibilityHidden(true)
                        }
                        let last = index == state.trail.steps.count - 1
                        if last, let renaming = renamingHere {
                            RenameField(label: renaming.label, title: WindowLabels.label(step, model: model), font: Tokens.font(Tokens.textMd),
                                        limit: renaming.limit, onDone: renaming.done)
                                .frame(maxWidth: 280)
                        } else {
                            Crumb(title: WindowLabels.label(step, model: model), here: last) { state.trail.back(to: index) }
                            if last { more }
                        }
                    }
                }
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Where you are")
            }
            Spacer(minLength: 16)
            // An update is downloaded (apps/mac/app/Updater.swift): Restart installs it and relaunches,
            // after this Mac's check if one runs.
            if let pill = model.updater.pill {
                Button(pill) { model.updater.install() }
                    .buttonStyle(ChipButtonStyle())
                    .disabled(model.updater.waiting)
                    .help("Installs the update and relaunches Oby. While this Mac runs a check for the runner, it waits for the check to end.")
                    .padding(.trailing, state.trail.isHome ? Tokens.gutter : 8)
            }
            if case .job(let id) = state.trail.top, model.hasConversation(of: id) {
                Button("Go to conversation") {
                    model.showConversation(of: id)
                    state.trail.push(.talk)
                }
                    .buttonStyle(ChipButtonStyle())
                    .padding(.trailing, Tokens.gutter)
            }
        }
        .padding(.leading, sidebarOpen ? Tokens.gutter : state.chrome.afterLights + 40)
        .frame(height: state.chrome.bar)
        .background { WindowDragArea() }
    }
}

extension TitleBar {
    /// The task on top, when a task's page is, as its side panel row has it.
    private var shownTask: SidebarSections.JobRow? {
        guard case .job(let id) = state.trail.top,
              let job = model.feed.jobs.first(where: { $0.id == id }) else { return nil }
        return SidebarSections.JobRow(job: job)
    }

    /// The conversation on top, when one of obid's is (#493), not a new one yet.
    private var shownTalk: RunnerConversation? {
        guard state.trail.top == .talk, model.feed.conversationsOn else { return nil }
        let list = model.feed.conversations
        return list.shown.flatMap { id in list.items.first { $0.id == id } }
    }

    /// The last crumb renamed in place (Sangam's `crumb-rename`): its field's label and what it does.
    private var renamingHere: (label: String, limit: Int, done: (String?) -> Void)? {
        if let task = shownTask, state.renaming == .init(id: task.id, inBar: true) {
            return ("Task name", 60, { state.renamed(task.job, to: $0, feed: model.feed) })
        }
        if let talk = shownTalk, state.renaming == .init(id: talk.id, inBar: true) {
            return ("Conversation name", 100, { title in
                state.renaming = nil
                if let title { Task { await model.feed.conversations.rename(talk.id, to: title) } }
            })
        }
        return nil
    }

    /// The last crumb's ⋮ on a task's or a conversation's page: its menu, as on its side panel row.
    @ViewBuilder private var more: some View {
        if let task = shownTask {
            barMenu("Task options") {
                TaskMenu(row: task, model: model, state: state, onOpenChat: { state.trail.push(.talk) },
                         onRename: { state.renaming = .init(id: task.id, inBar: true) })
            }
        } else if let talk = shownTalk {
            barMenu("Conversation options") {
                ConversationMenu(conversation: talk, list: model.feed.conversations,
                                 onRename: { state.renaming = .init(id: talk.id, inBar: true) },
                                 onDelete: { state.deletingConversation = talk })
            }
        }
    }

    private func barMenu(_ label: String, @ViewBuilder _ items: @escaping () -> some View) -> some View {
        Menu(content: items) { IconButton(symbol: "ellipsis", rotated: true, size: 28) }
            .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
            .padding(.leading, 8)
            .accessibilityLabel(label)
    }
}

/// One step of the trail: a link back to it, or (`here`) where you are.
private struct Crumb: View {
    let title: String
    let here: Bool
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(title).font(Tokens.font(Tokens.textMd)).lineLimit(1)
                .foregroundStyle(here ? Tokens.text : hovering ? Tokens.ink : Tokens.muted)
                .padding(.horizontal, 6).frame(height: 26)
                .background(!here && hovering ? Tokens.hover : .clear, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                // An earlier step: at most 200 points, a shorter name only as wide as it is.
                .frame(maxWidth: here ? nil : 200, alignment: .leading).fixedSize(horizontal: !here, vertical: false)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(here)
        .padding(.horizontal, -6)
        .onHover { hovering = $0 }
        .animation(Tokens.fade, value: hovering)
        .accessibilityAddTraits(here ? .isHeader : [])
    }
}

/// A small pill button on the chip colour (the trail's "Go to conversation").
struct ChipButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View { Styled(configuration: configuration) }

    private struct Styled: View {
        let configuration: Configuration
        @State private var hovering = false

        var body: some View {
            configuration.label.font(Tokens.font(Tokens.textMd)).lineLimit(1)
                .foregroundStyle(hovering ? Tokens.ink : Tokens.text)
                .padding(.horizontal, 12).frame(height: 28)
                .background(hovering || configuration.isPressed ? Tokens.chipHover : Tokens.chip, in: Capsule())
                .contentShape(Capsule())
                .onHover { hovering = $0 }
                .animation(Tokens.fade, value: hovering)
        }
    }
}

/// What each step is called in the trail.
enum WindowLabels {
    static func label(_ step: WindowStep, model: IslandModel) -> String {
        switch step {
        case .group(let kind): kind.label
        case .archived: "Archived"
        case .settings(let page): page.label
        case .talk: model.feed.conversationsOn ? model.feed.conversations.shownTitle : "Oby"
        case .job(let id):
            model.feed.jobs.first { $0.id == id }?.heading ?? "Task"
        case .session: "Session"
        }
    }
}

extension View {
    /// A page over Home takes the keyboard as it opens (the box under it lets go), so Esc reaches
    /// the window's back, whatever was clicked to open it.
    func takesKeys() -> some View { modifier(TakesKeys()) }
}

private struct TakesKeys: ViewModifier {
    @FocusState private var focused: Bool

    func body(content: Content) -> some View {
        content.focusable().focusEffectDisabled().focused($focused)
            .onAppear { DispatchQueue.main.async { focused = true } }
    }
}

/// One view a little above the middle: the room over it and under it split 1 to 1.6.
struct EyeLevel: Layout {
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let size = subviews.first?.sizeThatFits(ProposedViewSize(width: proposal.width, height: nil)) ?? .zero
        return CGSize(width: proposal.width ?? size.width, height: proposal.height ?? size.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let view = subviews.first else { return }
        let size = view.sizeThatFits(ProposedViewSize(width: bounds.width, height: nil))
        let room = max(0, bounds.height - size.height)
        view.place(at: CGPoint(x: bounds.midX, y: bounds.minY + room / 2.6 + size.height / 2), anchor: .center,
                   proposal: ProposedViewSize(width: bounds.width, height: size.height))
    }
}

/// The bubble hugs its text while its row fills the column; long text gets at most 75%.
struct MessageBubbleLayout: Layout {
    var trailing: Bool
    private func content(_ width: CGFloat, subviews: Subviews) -> CGSize {
        subviews[0].sizeThatFits(ProposedViewSize(width: min(width * 0.75, OBYStyle.threadWidth * 0.75), height: nil))
    }
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? OBYStyle.threadWidth
        return CGSize(width: width, height: content(width, subviews: subviews).height)
    }
    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let size = content(bounds.width, subviews: subviews)
        subviews[0].place(at: CGPoint(x: trailing ? bounds.maxX - size.width : bounds.minX, y: bounds.minY),
                          proposal: ProposedViewSize(size))
    }
}

struct ThinkingLine: View {
    /// Steps of the runner's turn run: "Working" instead of "Thinking" (#304).
    var working = false
    /// Why Stop didn't go through, or nil.
    var problem: String? = nil
    let onStop: () -> Void
    var body: some View {
        HStack(spacing: OBYStyle.Space.sm) {
            HStack(spacing: OBYStyle.Space.xs) {
                ForEach(0..<3) { _ in Circle().fill(OBYStyle.tertiary).frame(width: 4, height: 4) }
            }.accessibilityHidden(true)
            Text(working ? "Working" : "Thinking").font(OBYStyle.small).foregroundStyle(OBYStyle.tertiary)
            StopButton(action: onStop)
            if let problem {
                Text(problem).font(OBYStyle.caption).foregroundStyle(.orange)
            }
            Spacer()
        }
        .padding(.vertical, OBYStyle.Space.sm)
    }
}

/// A job's page over Home (Sangam's TaskView): a runner's job open here stays across new lists,
/// and one older than its latest 50 is fetched.
private struct JobPane: View {
    let model: IslandModel
    let id: String
    var onBack: (() -> Void)? = nil
    var onOpenJob: (String) -> Void = { _ in }
    /// Its conversation, made the one shown, over the page.
    var onConversation: () -> Void = {}
    /// Full session: the job's session over its page.
    var onSession: (() -> Void)? = nil
    @State private var problem: String?
    /// The page's own attachments: never what waits in Home's box.
    @State private var tray = Tray()

    private var feed: RunnerFeed { model.feed }
    private var job: Job? { feed.jobs.first { $0.id == id } }

    var body: some View {
        Group {
            if let job {
                TaskPage(job: job, actions: TaskActions.of(job, model: model), tray: tray,
                         publishNow: model.publishing == job.id, onPublishNow: { model.publishing = nil }, onBack: onBack,
                         followUps: ThreadTaskLine.followUps(of: job.id, in: feed.jobs), onOpenJob: onOpenJob,
                         onSession: onSession, usage: model.usageRows, voice: model.voice, talkKeyListening: model.listening,
                         onTold: { _ in
                             // Sangam's tellFromTask: its conversation opens on the words and Oby's answer.
                             guard TaskPage.afterTold(hasConversation: model.hasConversation(of: job.id)) == .conversation else { return false }
                             model.showConversation(of: job.id)
                             onConversation()
                             return true
                         })
            } else if let problem {
                Text(problem).font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.danger)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                CardBones(rows: 1).frame(maxWidth: Tokens.column).frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    .padding(.horizontal, Tokens.gutter).padding(.top, 24)
            }
        }
        .onChange(of: id, initial: true) { old, new in
            if old != new { feed.showing(old, false) }
            feed.showing(new, true)
            Task {
                do { try await feed.fetch(new) } catch { problem = error.localizedDescription }
            }
        }
        .onDisappear { feed.showing(id, false) }
    }
}
