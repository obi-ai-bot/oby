import AppKit
import SwiftUI

/// The window's message box (Sangam's Composer.tsx, app.css `.composer-field`): one row of + /
/// text / voice-or-send, 46 points tall, edged like a pill. Once the text runs past a line it
/// stacks: the text full width over the + / voice-or-send row. It grows with what's typed up to
/// `maximumHeight` (#270: about half the window), then scrolls inside, and shrinks back once sent.
nonisolated enum BoxMetrics {
    static let lineHeight = Tokens.composerLine
    /// One row: 32-point buttons, 7 points from the edge (6 inside the 1-point outline).
    static let button: CGFloat = 32
    static let inset: CGFloat = 7
    static var rowHeight: CGFloat { button + 2 * inset }
    /// A line of text, centred on the row.
    static var textInset: CGFloat { (rowHeight - lineHeight) / 2 }
    /// Stacked, the text has 10 points above and below it and 12 either side.
    static let stackedPadding = EdgeInsets(top: 10, leading: 12, bottom: 10, trailing: 12)
    static let radius: CGFloat = 23

    /// The text's width on one row, between + and voice-or-send, in a box `width` wide.
    static func rowWidth(_ width: CGFloat, gauge: Bool = false) -> CGFloat {
        max(1, width - 2 * inset - CGFloat(gauge ? 3 : 2) * (button + 4) - 8)
    }
    /// Stacked, the text's width.
    static func stackedWidth(_ width: CGFloat) -> CGFloat { max(1, width - 2 * inset - stackedPadding.leading - stackedPadding.trailing) }

    /// Whether the box stacks: the text doesn't fit on the row's one line. Measured at the row's
    /// width whichever way it shows, so it never flips back and forth at the edge.
    @MainActor static func stacked(_ text: String, width: CGFloat, gauge: Bool = false) -> Bool {
        ComposerMetrics.lines(text, width: rowWidth(width, gauge: gauge)) > 1
    }

    /// The text's height: a line per line, up to what's left of `maximum` once the box's own room
    /// is taken; past that it scrolls.
    @MainActor static func textHeight(_ text: String, width: CGFloat, maximum: CGFloat, gauge: Bool = false) -> CGFloat {
        guard stacked(text, width: width, gauge: gauge) else { return lineHeight }
        let lines = ComposerMetrics.lines(text, width: stackedWidth(width))
        let room = maximum - 2 * inset - stackedPadding.top - stackedPadding.bottom - button
        return min(max(lineHeight, room), CGFloat(lines) * lineHeight)
    }

    /// The whole box's height for `text`.
    @MainActor static func height(_ text: String, width: CGFloat, maximum: CGFloat) -> CGFloat {
        guard stacked(text, width: width) else { return rowHeight }
        return 2 * inset + stackedPadding.top + textHeight(text, width: width, maximum: maximum) + stackedPadding.bottom + button
    }
}

struct MessageBox: View {
    @Binding var text: String
    @Binding var focused: Bool
    let tray: Tray
    var placeholder = "Message Oby"
    var label = "Message Oby"
    var maximumHeight: CGFloat = 300
    var focusRequest = 0
    let onSend: () -> Void
    let onScreenshot: () -> Void
    let onStop: () -> Bool
    /// Esc in the box with nothing to stop.
    var onEscape: (() -> Void)? = nil
    /// Each account's week, for the gauge by voice-or-send; nil where the box doesn't talk to Oby (no gauge).
    var usage: [UsageRow]? = nil
    /// The + (a screenshot, the clipboard, a file): off where nothing can take them (a task's page).
    var attachments = true
    /// The microphone the voice button listens with (AGENTS.md: a click listens until Stop or another
    /// click, at most 60 seconds, the words left as a draft); nil: the button only says how to talk.
    var voice: Voice? = nil
    /// Whether the talk key is listening: the button waits for it.
    var talkKeyListening = false
    /// Stops Oby's turn under way, from the box's end slot (Sangam, mac-design A14); nil: no turn here.
    var onStopTurn: (() -> Void)? = nil
    /// Whether a turn is under way: then the end slot is Stop.
    var working = false
    /// The tasks + › Attach tasks offers (Sangam's), latest first; nil where the box doesn't talk to
    /// Oby. Other Obys (Sangam's Add other Obys, and @ in the words) wait for sealed Obysphere
    /// messages (#1233).
    var tasks: [Job]? = nil
    @State private var width: CGFloat = 480
    @State private var pickingTasks = false
    @State private var hovering = false
    @State private var listening = false
    @State private var limit: Task<Void, Never>?

    /// How long one click listens at most.
    static let listenLimit: Duration = .seconds(60)

    private var empty: Bool { text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }
    private var stacked: Bool { BoxMetrics.stacked(text, width: width, gauge: usage != nil) }
    private var textHeight: CGFloat { BoxMetrics.textHeight(text, width: width, maximum: maximumHeight, gauge: usage != nil) }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if attachments && (!tray.items.isEmpty || tray.place != nil || !attachedTasks.isEmpty) { tags.padding(.bottom, 6) }
            if stacked {
                editor.padding(BoxMetrics.stackedPadding)
                HStack(spacing: 4) {
                    plus
                    Spacer(minLength: 0)
                    gauge
                    end
                }
            } else {
                HStack(alignment: .bottom, spacing: 4) {
                    plus
                    editor.padding(.horizontal, 4).padding(.vertical, BoxMetrics.textInset - BoxMetrics.inset)
                    gauge
                    end
                }
            }
        }
        .padding(BoxMetrics.inset)
        .frame(maxWidth: .infinity)
        .onGeometryChange(for: CGFloat.self) { $0.size.width } action: { width = $0 }
        .overlay {
            RoundedRectangle(cornerRadius: BoxMetrics.radius, style: .continuous)
                .strokeBorder(hovering || focused ? Tokens.fieldEdgeOn : Tokens.line, lineWidth: 1)
        }
        .contentShape(RoundedRectangle(cornerRadius: BoxMetrics.radius, style: .continuous))
        .onHover { hovering = $0 }
        .animation(Tokens.fade, value: hovering || focused)
        .overlay(alignment: .top) { if attachments { problem.offset(y: -28) } }
        .popover(isPresented: $pickingTasks, arrowEdge: .top) {
            TaskPicker(tasks: tasks ?? [], picked: tray.tasks, onPick: { job in
                if !tray.tasks.contains(job.id) { tray.tasks.append(job.id) }
                pickingTasks = false
                focused = true
            }, onClose: { pickingTasks = false })
            .frame(width: max(width, 320))
        }
        // Leaving the screen stops listening; what was heard stays as a draft.
        .onDisappear { if listening { stopListening() } }
    }

    /// What was heard, after what's already typed: a draft, never sent by itself.
    static func draft(_ text: String, adding said: String) -> String {
        let said = said.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !said.isEmpty else { return text }
        let typed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return typed.isEmpty ? said : typed + " " + said
    }

    private func toggleListening() {
        guard let voice else { return }
        if listening { return stopListening() }
        guard voice.phase == .idle, !talkKeyListening else { return }
        voice.start(key: .controlOption, context: [])
        listening = true
        limit = Task { @MainActor in
            try? await Task.sleep(for: Self.listenLimit)
            if !Task.isCancelled, listening { stopListening() }
        }
    }

    private func stopListening() {
        limit?.cancel()
        limit = nil
        listening = false
        voice?.finish { said in text = Self.draft(text, adding: said) }
    }

    private var editor: some View {
        ComposerEditor(text: $text, focused: $focused, focusRequest: focusRequest, onSend: onSend, onStop: onStop,
                       lineHeight: BoxMetrics.lineHeight, textColor: NSColor(white: 1, alpha: 0.9), label: label,
                       onEscape: onEscape)
            .frame(height: textHeight)
            .overlay(alignment: .topLeading) {
                if listening {
                    Text(voice.map { $0.words.isEmpty ? "Listening…" : $0.words } ?? "Listening…").font(Tokens.font(Tokens.textLg)).foregroundStyle(Tokens.muted)
                        .lineLimit(1).truncationMode(.head)
                        .frame(height: BoxMetrics.lineHeight)
                        .allowsHitTesting(false)
                } else if text.isEmpty {
                    Text(placeholder).font(Tokens.font(Tokens.textLg)).foregroundStyle(Tokens.muted)
                        .lineLimit(1).truncationMode(.tail)
                        .frame(height: BoxMetrics.lineHeight)
                        .allowsHitTesting(false)
                }
            }
    }

    /// + : a screenshot, what you copied, a file (Sangam's + panel's Add files), and where the box
    /// talks to Oby, tasks (its Attach tasks; its Add other Obys waits for #1233).
    @ViewBuilder private var plus: some View {
        if attachments { plusMenu } else { Color.clear.frame(width: 4, height: BoxMetrics.button) }
    }

    private var plusMenu: some View {
        Menu {
            Button("Screenshot", action: onScreenshot).disabled(tray.capturing)
            Button("Clipboard", action: tray.pasteClipboard)
            Button("File…", action: tray.chooseFile)
            if tasks != nil { Button("Attach tasks") { pickingTasks = true } }
            // Where the box talks to Oby: run its jobs on a teammate's Mac that's free (Sangam's
            // "Run on another Mac", #808), with your own accounts.
            if usage != nil && !tray.macs.isEmpty { placeMenu }
        } label: {
            BoxIcon(symbol: "plus")
        }
        .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
        .accessibilityLabel("Add attachment")
    }

    /// This Mac, then the teammates' Macs: free or busy (and when it's free), never whether their
    /// owner is at them. Picking one puts its tag over the words; one at a time.
    private var placeMenu: some View {
        Menu("Run on another Mac") {
            Button { tray.place = nil } label: {
                Label("This Mac", systemImage: tray.place == nil ? "checkmark" : "laptopcomputer")
            }
            ForEach(tray.macs) { mac in
                Button { tray.place = mac } label: {
                    Label(Self.placeLine(mac), systemImage: tray.place == mac ? "checkmark" : "laptopcomputer")
                }
                .disabled(!mac.free)
            }
            Divider()
            Text("Your job runs there with your own accounts: your logins stay on this Mac.")
        }
    }

    /// "Wellington’s Mac · Free" or "· Busy", and "· unverified" until you've compared
    /// Wellington's key with him (Settings › Obysphere).
    static func placeLine(_ mac: RunnerMac) -> String {
        let unverified = mac.verified == false ? " · unverified" : ""
        return "\(mac.name) · \(mac.free ? "Free" : "Busy")\(unverified)"
    }

    /// Always there where the box talks to Oby, even before an account reports usage (docs/mac-design.md B3).
    @ViewBuilder private var gauge: some View {
        if let usage { UsageGauge(rows: usage, shares: tray.shares) }
    }

    /// What the end slot holds: Stop while a turn is under way, the microphone while the box is
    /// empty or listening, Send once there's text.
    enum EndSlot: Equatable { case voice, send, stop }

    static func endSlot(empty: Bool, listening: Bool, working: Bool) -> EndSlot {
        if working && !listening { return .stop }
        return empty || listening ? .voice : .send
    }

    private var slot: EndSlot { Self.endSlot(empty: empty, listening: listening, working: working && onStopTurn != nil) }

    @ViewBuilder private var end: some View {
        if slot == .stop, let onStopTurn {
            Button(action: onStopTurn) { BoxIcon(symbol: "stop.fill", color: Tokens.ink) }
                .buttonStyle(.plain)
                .help("Stop")
                .accessibilityLabel("Stop")
                .frame(width: BoxMetrics.button)
        } else {
            sendOrVoice
        }
    }

    /// Voice while the box is empty: the talk key does it, so it says how. Send once there's text.
    @ViewBuilder private var sendOrVoice: some View {
        ZStack {
            // The talk key speaks to Oby: where the box tells an agent (no voice given), none.
            if attachments || voice != nil { voiceIcon }
            Button(action: onSend) { BoxIcon(symbol: "arrow.up", color: Tokens.ink) }
                .buttonStyle(.plain)
                .accessibilityLabel("Send")
                .opacity(empty || listening ? 0 : 1).scaleEffect(empty || listening ? 0.4 : 1)
                .disabled(empty || listening)
                .accessibilityHidden(empty || listening)
        }
        .frame(width: BoxMetrics.button)
        .animation(Tokens.spring, value: empty)
    }

    @ViewBuilder private var voiceIcon: some View {
        if voice != nil {
            Button(action: toggleListening) {
                BoxIcon(symbol: listening ? "stop.fill" : "waveform", color: listening ? Tokens.ink : Tokens.muted)
            }
            .buttonStyle(.plain)
            .disabled(talkKeyListening && !listening)
            .help(listening ? "Stop listening" : "Click to talk, or hold fn (or Control-Option)")
            .accessibilityLabel(listening ? "Stop listening" : "Talk")
            .opacity(empty || listening ? 1 : 0).scaleEffect(empty || listening ? 1 : 0.4)
            .accessibilityHidden(!(empty || listening))
        } else {
            BoxIcon(symbol: "waveform")
                .help("Hold fn (or Control-Option) to talk")
                .accessibilityLabel("Hold fn to talk")
                .opacity(empty ? 1 : 0).scaleEffect(empty ? 1 : 0.4)
                .accessibilityHidden(!empty)
        }
    }

    /// The tasks attached, where the box attaches them: each with its title, "Task" once it's gone.
    private var attachedTasks: [(id: String, title: String)] {
        guard let tasks else { return [] }
        return tray.tasks.map { id in (id, tasks.first { $0.id == id }?.heading ?? "Task") }
    }

    /// What goes with the words, as tags over them, wrapping (Sangam's `.composer-tags`).
    private var tags: some View {
        CenteredFlow(spacing: 6, centred: false) {
            if let mac = tray.place {
                HStack(spacing: 6) {
                    Image(systemName: "laptopcomputer").font(Tokens.font(11)).foregroundStyle(Tokens.muted)
                    Text("On \(mac.name)").lineLimit(1)
                    Button { tray.place = nil } label: {
                        Image(systemName: "xmark").font(Tokens.font(9, .bold)).foregroundStyle(Tokens.muted)
                            .frame(width: 18, height: 18).contentShape(Circle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Run it on this Mac instead")
                }
                .font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.item)
                .padding(.leading, 6).padding(.trailing, 3).frame(height: 24)
                .background(Tokens.chipHover, in: Capsule())
            }
            ForEach(tray.items) { item in
                tag(item.label, icon: item.icon) { tray.remove(item) }
            }
            ForEach(attachedTasks, id: \.id) { task in
                tag(task.title, icon: "checklist") { tray.tasks.removeAll { $0 == task.id } }
            }
        }
        .padding(3)
    }

    private func tag(_ label: String, icon: String, remove: @escaping () -> Void) -> some View {
        HStack(spacing: 6) {
            Image(systemName: icon).font(Tokens.font(11)).foregroundStyle(Tokens.muted)
            Text(label).lineLimit(1)
            Button(action: remove) {
                Image(systemName: "xmark").font(Tokens.font(9, .bold)).foregroundStyle(Tokens.muted)
                    .frame(width: 18, height: 18).contentShape(Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Remove \(label)")
        }
        .font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.item)
        .padding(.leading, 6).padding(.trailing, 3).frame(height: 24)
        .background(Tokens.chipHover, in: Capsule())
        .frame(maxWidth: 240, alignment: .leading)
    }

    /// Why the last screenshot or paste didn't work, over the box.
    @ViewBuilder private var problem: some View {
        switch tray.problem {
        case .screenRecording:
            Button("Oby needs Screen Recording to see your screen. Open System Settings") {
                if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture") {
                    _ = NSWorkspace.shared.open(url)
                }
            }
            .buttonStyle(.plain).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.needs)
        case .other(let problem):
            Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger).lineLimit(1)
        case nil:
            EmptyView()
        }
    }
}

/// One of the box's 32-point round icon buttons: muted at rest, ink with a faint fill hovered.
struct BoxIcon: View {
    let symbol: String
    var color = Tokens.muted
    @State private var hovering = false

    var body: some View {
        Image(systemName: symbol).font(Tokens.font(16, .regular))
            .foregroundStyle(hovering ? Tokens.ink : color)
            .frame(width: BoxMetrics.button, height: BoxMetrics.button)
            .background(hovering ? Tokens.hover : .clear, in: Circle())
            .contentShape(Circle())
            .onHover { hovering = $0 }
            .animation(Tokens.fade, value: hovering)
    }
}

/// One account's week, for the message box's gauge.
struct UsageRow: Identifiable, Equatable {
    var id: String
    var agent: String
    var name: String
    var week: Double?
    var resets: Date?
}

extension IslandModel {
    /// Every account where jobs run (the runner's, as Settings › Accounts lists them), with its week:
    /// nil while it's unknown, signed out or reset.
    var usageRows: [UsageRow] {
        feed.accounts.map {
            UsageRow(id: $0.account.id, agent: $0.account.agent, name: $0.account.name,
                     week: Usage.week($0.usage), resets: $0.sevenDayResets)
        }
    }
}

/// How much the accounts have used this week (Sangam's UsageGauge): a gauge with a dot while any
/// is past 80% (yellow) or 90% (red); a click shows each account's week, and the way to Accounts.
private struct UsageGauge: View {
    let rows: [UsageRow]
    /// The shares teammates gave you (#809): Sangam's "Shared with you", under your own accounts.
    var shares: [RunnerShares.Share] = []
    @State private var open = false
    @Environment(\.openSettingsPage) private var openSettings

    var body: some View {
        let dot = UsageDot.of(weeks: rows.map(\.week))
        Button { open.toggle() } label: {
            BoxIcon(symbol: "gauge.with.dots.needle.33percent")
                .overlay(alignment: .topTrailing) {
                    if dot != .none {
                        Circle().fill(dot == .over ? Tokens.over : Tokens.full).frame(width: 6, height: 6)
                            .offset(x: -4, y: 5).accessibilityHidden(true)
                    }
                }
        }
        .buttonStyle(.plain)
        .help("Usage this week")
        .accessibilityLabel("Usage this week" + (dot == .none ? "" : ", an account is nearly full"))
        .popover(isPresented: $open, arrowEdge: .top) { panel }
    }

    private var panel: some View {
        VStack(alignment: .leading, spacing: 2) {
            HStack {
                Text("Usage this week").font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.label)
                Spacer()
                IconButton(symbol: "gearshape", size: 28) { open = false; openSettings(.accounts) }
                    .help("Manage accounts").accessibilityLabel("Manage accounts")
            }
            .padding(.leading, 8)
            if rows.isEmpty {
                Text("No accounts yet").font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.muted).padding(8)
            }
            ForEach(rows) { row in
                let used = row.week ?? 0
                let level = UsageDot.of(weeks: [row.week])
                HStack(alignment: .top, spacing: 10) {
                    AgentMark(agent: row.agent).font(Tokens.font(13)).frame(width: 16).foregroundStyle(Tokens.muted)
                    VStack(alignment: .leading, spacing: 6) {
                        HStack(alignment: .firstTextBaseline, spacing: 10) {
                            Text("\(Usage.tool(row.agent)) · \(row.name)").font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.ink).fadeOut()
                            if let resets = row.resets {
                                Text(Usage.resetText(resets)).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted).fixedSize()
                            }
                            Text(row.week == nil ? "–" : Usage.percent(used)).font(Tokens.font(Tokens.textBase)).monospacedDigit()
                                .foregroundStyle(level == .over ? Tokens.over : level == .high ? Tokens.full : Tokens.text)
                                .frame(minWidth: 32, alignment: .trailing)
                        }
                        SetMeter(used: used * 100)
                    }
                }
                .padding(8)
            }
            if !shares.isEmpty {
                Text("Shared with you").font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.label).padding(.leading, 8).padding(.top, 8)
            }
            ForEach(shares) { share in
                // The bar is your share of its month, filled with what you've used of it.
                let used = Double(share.share - share.left) / Double(max(share.share, 1))
                HStack(alignment: .top, spacing: 10) {
                    Image(systemName: "person.crop.circle").font(Tokens.font(13)).frame(width: 16).foregroundStyle(Tokens.muted)
                    VStack(alignment: .leading, spacing: 6) {
                        HStack(alignment: .firstTextBaseline, spacing: 10) {
                            Text(share.name).font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.ink).fadeOut()
                            Text("Your \(share.share)% of its month").font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted).fixedSize()
                            Text("\(share.left)% left").font(Tokens.font(Tokens.textBase)).monospacedDigit().foregroundStyle(Tokens.text)
                                .frame(minWidth: 32, alignment: .trailing)
                        }
                        SetMeter(used: used * 100)
                    }
                }
                .padding(8)
            }
        }
        .padding(.horizontal, 8).padding(.top, 8).padding(.bottom, 20)
        .frame(width: 340)
        .background(Tokens.surface)
    }
}

/// Sangam's + › Attach tasks: a search over your tasks, over the box. Typing narrows it, the arrows
/// move the lit row (the first match until they do), Return attaches it, Esc closes it; a task
/// attached already has a check.
struct TaskPicker: View {
    let tasks: [Job]
    let picked: [String]
    let onPick: (Job) -> Void
    let onClose: () -> Void
    @State private var query = ""
    @State private var lit = 0

    /// The tasks whose title holds what's typed, in their order (Sangam's search).
    nonisolated static func matches(_ tasks: [Job], _ query: String) -> [Job] {
        let typed = query.trimmingCharacters(in: .whitespaces).lowercased()
        return typed.isEmpty ? tasks : tasks.filter { $0.heading.lowercased().contains(typed) }
    }

    /// The row lit after an arrow, round from the last to the first and back.
    nonisolated static func moved(_ lit: Int, down: Bool, count: Int) -> Int {
        guard count > 0 else { return 0 }
        return (min(lit, count - 1) + (down ? 1 : count - 1)) % count
    }

    var body: some View {
        let found = Self.matches(tasks, query)
        let at = min(lit, max(found.count - 1, 0))
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Image(systemName: "magnifyingglass").font(Tokens.font(12)).foregroundStyle(Tokens.muted)
                TaskSearch(text: $query, placeholder: "Search tasks") { key in
                    switch key {
                    case .up, .down: lit = Self.moved(lit, down: key == .down, count: found.count)
                    case .pick: if found.indices.contains(at) { onPick(found[at]) }
                    case .close: onClose()
                    }
                }
                .onChange(of: query) { lit = 0 }
            }
            .padding(.horizontal, 8).frame(height: 32)
            Rectangle().fill(Tokens.surfaceEdge).frame(height: 1)
            if found.isEmpty {
                // ponytail: Sangam's list says "Nothing matches" even with no tasks at all; with none, it says so.
                Text(tasks.isEmpty ? "No tasks yet" : "Nothing matches “\(query.trimmingCharacters(in: .whitespaces))”")
                    .font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted).padding(.horizontal, 10).padding(.vertical, 8)
            } else {
                ScrollViewReader { proxy in
                    ScrollView {
                        VStack(spacing: 0) {
                            ForEach(Array(found.enumerated()), id: \.element.id) { index, job in
                                row(job, lit: index == at).onHover { if $0 { lit = index } }
                            }
                        }
                    }
                    .frame(height: min(CGFloat(found.count) * 36, 240))
                    .onChange(of: at) { proxy.scrollTo(found[at].id) }
                }
                .accessibilityElement(children: .contain)
                .accessibilityLabel("Tasks")
            }
        }
        .padding(4)
        .background(Tokens.surface)
    }

    private func row(_ job: Job, lit: Bool) -> some View {
        let mark = ThreadTaskLine.mark(job)
        let attached = picked.contains(job.id)
        return Button { onPick(job) } label: {
            HStack(spacing: 10) {
                WorkMarkView(mark: mark).frame(width: 16)
                Text(job.heading).font(Tokens.font(Tokens.textBase)).foregroundStyle(lit ? Tokens.ink : Tokens.item).fadeOut()
                if attached { Image(systemName: "checkmark").font(Tokens.font(11, .semibold)).foregroundStyle(Tokens.muted) }
            }
            .padding(.horizontal, 10).frame(height: 36)
            .background(lit ? Tokens.hover : .clear, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(job.heading), \(ThreadTaskLine.status(mark))")
        .accessibilityAddTraits(attached ? .isSelected : [])
    }
}

/// The picker's search, in AppKit: a SwiftUI TextField's field editor keeps ↑ and ↓ for itself, so
/// the list never hears them. It takes the keys as it lands in its window (the popover's).
private struct TaskSearch: NSViewRepresentable {
    enum Key { case up, down, pick, close }

    @Binding var text: String
    let placeholder: String
    let onKey: (Key) -> Void

    final class Field: NSTextField {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            window?.makeFirstResponder(self)
        }
    }

    final class Keys: NSObject, NSTextFieldDelegate {
        var parent: TaskSearch
        init(_ parent: TaskSearch) { self.parent = parent }

        func controlTextDidChange(_ note: Notification) { parent.text = (note.object as? NSTextField)?.stringValue ?? "" }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            let key: Key? = switch selector {
            case #selector(NSResponder.moveUp(_:)): .up
            case #selector(NSResponder.moveDown(_:)): .down
            case #selector(NSResponder.insertNewline(_:)): .pick
            // A field editor's Esc would offer completions.
            case #selector(NSResponder.cancelOperation(_:)): .close
            default: nil
            }
            guard let key else { return false }
            parent.onKey(key)
            return true
        }
    }

    func makeCoordinator() -> Keys { Keys(self) }

    func makeNSView(context: Context) -> Field {
        let field = Field()
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = .systemFont(ofSize: Tokens.textMd)
        field.textColor = NSColor(Tokens.ink)
        field.placeholderAttributedString = NSAttributedString(string: placeholder, attributes: [
            .foregroundColor: NSColor(Tokens.muted), .font: NSFont.systemFont(ofSize: Tokens.textMd),
        ])
        field.setAccessibilityLabel(placeholder)
        field.delegate = context.coordinator
        return field
    }

    func updateNSView(_ field: Field, context: Context) {
        context.coordinator.parent = self
        if field.stringValue != text { field.stringValue = text }
    }
}
