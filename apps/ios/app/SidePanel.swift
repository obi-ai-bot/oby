import SwiftUI
import UIKit

/// What the side panel shows (Sangam's Sidebar.tsx, chat first): its icon strip picks the chats or
/// the tasks; your picture at its foot opens Settings.
enum PanelSection: Hashable { case chats, tasks }

/// Oby on iPhone with no tab bar (the owner's call, 2026-10-18, as Sangam's design has none): your
/// conversation and everything opened over it in one stack, and Sangam's side panel. It opens from
/// the left and pushes the view over: the panel button at the top left, or a swipe from the screen's
/// left edge where the app rests; a tap on what it pushed aside, a swipe back on it, the panel's own button, or
/// VoiceOver's escape closes it. With Reduce Motion it appears and goes without sliding.
struct PhoneRoot: View {
    @Environment(Phone.self) private var phone
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let home: StageCharacter
    let unpaired: StageCharacter
    /// How far a drag has moved the panel from where it rests.
    @GestureState private var drag: CGFloat = 0
    /// How far a swipe from the screen's edge has pulled it out.
    @State private var edge: CGFloat = 0

    var body: some View {
        if phone.needsPairing {
            // A fresh iPhone, or one the Mac stopped taking: Oby, why, and Connect.
            UnpairedScreen(character: unpaired.character)
        } else {
            panelled
        }
    }

    private var panelled: some View {
        GeometryReader { geometry in
            let width = min(Tokens.sidebar + 40, geometry.size.width - 56)
            let open = phone.panel != nil
            let shift = min(width, max(0, (open ? width : 0) + drag + edge))
            ZStack(alignment: .leading) {
                SidePanel(width: width, close: close)
                    .frame(width: width)
                    .offset(x: shift - width)
                    .accessibilityHidden(!open)
                    .accessibilityAction(.escape) { close() }
                ChatRoot(character: home.character)
                    .frame(width: geometry.size.width)
                    // The left edge where the app rests: a swipe right opens the panel. The system's own edge pan,
                    // which cancels the touch under it, so a row the swipe ends on isn't tapped and
                    // a tap near the edge still lands. Only there: over a page the edge
                    // is the system's back swipe.
                    .gesture(EdgeSwipe(enabled: !open && phone.homePath.isEmpty) { edge = $0 } ended: { moved in
                        edge = 0
                        if moved > width / 3, phone.homePath.isEmpty { phone.panel = .chats }
                    })
                    .overlay {
                        // What the panel pushed aside: a tap brings it back, and so does a drag
                        // back on it. Only here, so the panel's rows keep their own swipes.
                        Tokens.bg.opacity(0.4 * shift / width).ignoresSafeArea()
                            .allowsHitTesting(open)
                            .onTapGesture { close() }
                            .gesture(dragging(width))
                            .accessibilityHidden(true)
                    }
                    .clipShape(Rectangle())
                    .offset(x: shift)
                    .accessibilityHidden(open)
            }
            .animation(reduceMotion ? nil : Tokens.warm, value: open)
        }
        .background(Tokens.bg)
    }

    private func dragging(_ width: CGFloat) -> some Gesture {
        DragGesture(minimumDistance: 10)
            .updating($drag) { value, state, _ in state = value.translation.width }
            .onEnded { value in
                let open = phone.panel != nil
                let moved = value.predictedEndTranslation.width
                if !open, moved > width / 3 { phone.panel = .chats }
                if open, moved < -width / 3 { close() }
            }
    }

    private func close() { phone.panel = nil }
}

/// The panel (Sangam's Sidebar.tsx, chat first, on a phone): the icon strip on the far left (Oby's
/// chats, Tasks with its dots, and your picture at its foot for Settings), the column beside it
/// showing one. Search on the title's row for chats, the ⋮ for tasks.
private struct SidePanel: View {
    @Environment(Phone.self) private var phone
    @AppStorage(GroupBy.key) private var groupBy = GroupBy.type
    let width: CGFloat
    let close: () -> Void
    @State private var searching = false
    @State private var problem: String?

    /// Anything in the tasks needing you, GitHub's work waiting on you included (CHATFIRST-7).
    private var tasksNeed: Bool {
        WorkList.needsYou(phone.work, loose: phone.loose.count) + phone.limitDecisions.count > 0
            || !(phone.gitHubWork?.toDo(.review, later: phone.gitHubLater) ?? []).isEmpty
            || !(phone.gitHubWork?.toDo(.issues, later: phone.gitHubLater) ?? []).isEmpty
    }

    var body: some View {
        let section = phone.panel ?? .chats
        HStack(spacing: 0) {
            VStack(spacing: 8) {
                strip(.chats, "bubble.left.and.bubble.right", "Oby", on: section == .chats, dot: nil)
                strip(.tasks, "checklist", "Tasks", on: section == .tasks,
                      dot: tasksNeed ? Tokens.needs : phone.work.contains(where: \.fresh) ? Tokens.unread : nil)
                Spacer()
                Button { open(.settings) } label: { OwnerPicture(name: HomeLine.owner(ofMac: phone.link?.name)) }
                    .buttonStyle(.plain)
                    .frame(width: 44, height: 44)
                    .accessibilityLabel("Settings")
            }
            .padding(.vertical, 8)
            .frame(width: Tokens.rail)
            .overlay(alignment: .trailing) { Rectangle().fill(Tokens.edge).frame(width: 1) }
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 0) {
                    Text(section == .chats ? "Chats" : "Tasks")
                        .font(Tokens.font(Tokens.textTitle, .semibold)).foregroundStyle(Tokens.ink)
                        .accessibilityAddTraits(.isHeader)
                    Spacer()
                    if section == .chats {
                        Button { withAnimation(Tokens.warm) { searching.toggle() } } label: { barIcon("magnifyingglass") }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Search")
                    } else {
                        tasksMenu
                    }
                    // Sangam's panel button, here to put it away again.
                    Button(action: close) { barIcon("sidebar.left") }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Hide sidebar")
                }
                .padding(.leading, 12).padding(.trailing, 4).padding(.top, 4).padding(.bottom, 8)
                switch section {
                case .chats: ChatsColumn(searching: searching, needsYou: !phone.answering.isEmpty, startNew: { phone.newConversation() })
                case .tasks: TasksColumn(open: open, groupBy: groupBy, problem: problem)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(Tokens.bg)
        // obid has no event for the Sphere's directory: read as the panel opens.
        .task(id: phone.panel) { if phone.panel != nil { await phone.readSphere() } }
    }

    private func barIcon(_ symbol: String) -> some View {
        Image(systemName: symbol).font(Tokens.font(17)).foregroundStyle(Tokens.muted)
            .frame(width: 44, height: 44).contentShape(Rectangle())
    }

    /// Tasks' ⋮ (Sangam's): by kind of work or by status, and Clear done.
    private var tasksMenu: some View {
        Menu {
            Picker("Group by", selection: $groupBy) {
                ForEach(GroupBy.allCases, id: \.self) { Text($0.label).tag($0) }
            }
            .pickerStyle(.inline)
            Divider()
            // Through the runner (it keeps the time), so every app clears too.
            Button("Clear done") {
                Task { do { try await phone.clear(); problem = nil } catch { problem = error.localizedDescription } }
            }
            .disabled(!phone.visibleJobs.contains { $0.clearable(among: phone.jobs) })
        } label: { barIcon("ellipsis") }
            .accessibilityLabel("Tasks options")
    }

    private func strip(_ section: PanelSection, _ icon: String, _ label: String, on: Bool, dot: Color?) -> some View {
        Button { phone.panel = section } label: {
            Image(systemName: on && section == .chats ? "\(icon).fill" : icon).font(Tokens.font(17))
                .foregroundStyle(on ? Tokens.ink : Tokens.railIcon)
                .frame(width: 36, height: 36)
                .background(on ? Tokens.active : .clear, in: RoundedRectangle(cornerRadius: Tokens.iconRadius, style: .continuous))
                .overlay(alignment: .topTrailing) {
                    if let dot { Circle().fill(dot).frame(width: 7, height: 7).offset(x: -3, y: 4) }
                }
                .frame(width: 44, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
        .accessibilityValue(dot == Tokens.needs ? "Needs you" : dot == nil ? "" : "New")
        .accessibilityAddTraits(on ? .isSelected : [])
    }

    /// Opens a page over where the app rests and closes the panel, as Sangam's does.
    private func open(_ route: Route) {
        phone.homePath = [route]
        phone.panel = nil
    }
}

/// The panel's Tasks (Sangam's side panel, chat first): New task, the kinds of work (Home's chips) with
/// their dots, then each group's open tasks and its latest done ones; Archived under them.
private struct TasksColumn: View {
    @Environment(Phone.self) private var phone
    let open: (Route) -> Void
    let groupBy: GroupBy
    let problem: String?
    @State private var more: Set<String> = []

    var body: some View {
        let groups = WorkList.groups(phone.work, by: groupBy)
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 0) {
                // New task goes to the empty chat's box, as his does.
                Button { phone.newConversation() } label: {
                    Label("New task", systemImage: "plus")
                        .padding(.horizontal, 12).frame(maxWidth: .infinity, minHeight: 44, alignment: .leading)
                        .contentShape(Rectangle())
                }
                ForEach(WorkList.kinds(phone.work, routines: Launch.everything)) { kind in
                    let toDo = kind.type.map { $0 == .review || $0 == .issues ? !(phone.gitHubWork?.toDo($0, later: phone.gitHubLater) ?? []).isEmpty : false } ?? false
                    Button { if let type = kind.type { open(.kind(type)) } } label: {
                        HStack(spacing: 8) {
                            Text(kind.label).lineLimit(1)
                            if kind.fresh { Circle().fill(Tokens.unread).frame(width: 6, height: 6).accessibilityLabel("New") }
                            if kind.needs > 0 || toDo { Circle().fill(Tokens.needs).frame(width: 6, height: 6).accessibilityLabel("Needs you") }
                        }
                        .padding(.horizontal, 12).frame(maxWidth: .infinity, minHeight: 40, alignment: .leading)
                        .contentShape(Rectangle())
                    }
                }
                if let problem { Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger).padding(.horizontal, 12) }
                if groups.isEmpty {
                    if !phone.loaded && phone.connection == .connecting {
                        RowBones()
                    } else {
                        Text(!phone.loaded ? "Reaching your Mac…" : phone.archived.isEmpty ? "No tasks yet. They’ll show up here once you give me one." : "All caught up. Finished tasks are in Archived.")
                            .font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted).padding(.horizontal, 12).padding(.top, 12)
                    }
                }
                ForEach(groups) { group in
                    let folded = phone.panelFolded.contains(group.id)
                    let shown = WorkList.shown(group, more: more.contains(group.id))
                    // The chevron and the row fold it, his way.
                    Button { withAnimation(Tokens.warm) { phone.panelFolded.formSymmetricDifference([group.id]) } } label: {
                        HStack(spacing: 4) {
                            Text(group.label).padding(.trailing, 2)
                            Image(systemName: "chevron.down").font(Tokens.font(9, .semibold)).rotationEffect(.degrees(folded ? 180 : 0))
                            Spacer(minLength: 4)
                            if folded && group.needs > 0 { Circle().fill(Tokens.needs).frame(width: 6, height: 6) }
                            if group.count > 0 { Text("\(group.count)").monospacedDigit() }
                        }
                        .font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.label)
                        .padding(.horizontal, 12).frame(minHeight: 44).contentShape(Rectangle())
                    }
                    .accessibilityLabel("\(folded ? "Show" : "Hide") \(group.label)")
                    .accessibilityValue(group.count > 0 ? "\(group.count) \(group.countLabel)" : "")
                    .accessibilityAddTraits(.isHeader)
                    if !folded {
                        ForEach(shown.rows) { item in
                            Button { open(.task(item.id)) } label: { PanelTaskRow(item: item) }
                        }
                        if shown.hidden > 0 {
                            Button(more.contains(group.id) ? "Show less" : "Show \(shown.hidden) more") {
                                if more.contains(group.id) { more.remove(group.id) } else { more.insert(group.id) }
                            }
                            .font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted)
                            .padding(.horizontal, 12).frame(minHeight: 36)
                        }
                    }
                }
                // What needs you with no task to show it on: only when there's some.
                if !phone.loose.isEmpty {
                    Button { open(.inbox) } label: {
                        HStack {
                            Text("Inbox")
                            Spacer()
                            Text("\(phone.loose.count)").monospacedDigit().foregroundStyle(Tokens.needsInk)
                        }
                    }
                    .padding(.horizontal, 12).padding(.top, 16)
                }
            }
            .font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.item)
            .buttonStyle(.plain)
            .padding(.bottom, 24)
        }
        .refreshable { await phone.reload() }
        // The panel's foot: the done tasks that left the lists, once there are some.
        if !phone.archived.isEmpty {
            ArchivedFoot(count: phone.archived.count) { open(.archived) }
        }
    }
}

/// Archived and its count at the panel's foot, under a line, 44 tall (the Mac's ArchivedRow,
/// Sangam's `.sidebar-archived`).
private struct ArchivedFoot: View {
    let count: Int
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack {
                Text("Archived").font(Tokens.font(Tokens.textBase))
                Spacer(minLength: 4)
                Text("\(count)").font(Tokens.font(Tokens.textSm)).monospacedDigit()
            }
            .foregroundStyle(Tokens.label)
            .padding(.horizontal, 12).frame(height: 44)
            .overlay(alignment: .top) { Tokens.line.frame(height: 1) }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .padding(.bottom, 2)
        .accessibilityLabel("Archived, \(count) task\(count == 1 ? "" : "s")")
    }
}

/// A task in the panel (Sangam's TaskRow): its agent's mark, its title, its one mark.
private struct PanelTaskRow: View {
    let item: WorkList.Item<Job>

    var body: some View {
        HStack(spacing: 10) {
            AgentMark(agent: item.job.agent).font(Tokens.font(12, .semibold)).frame(width: 16).foregroundStyle(Tokens.ink.opacity(0.7))
            Text(item.job.heading).font(Tokens.font(Tokens.textBase))
                .foregroundStyle(item.status == .done ? Tokens.doneTitle : Tokens.item).fadeOut()
            WorkMarkView(mark: item.mark).frame(width: 12, height: 12)
        }
        .padding(.horizontal, 12).frame(height: 36)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityValue(item.mark.rawValue)
    }
}

/// Your picture until there's a photo (Sangam's OwnerAvatar): your initial on a colour picked from
/// your name, or a person with no name to go by.
struct OwnerPicture: View {
    let name: String?

    var body: some View {
        Group {
            if let name, let initial = name.first {
                Text(String(initial)).font(Tokens.font(11, .semibold)).foregroundStyle(.white.opacity(0.92))
                    .frame(width: 28, height: 28).background(Tokens.avatar(name), in: Circle())
            } else {
                Image(systemName: "person.crop.circle").font(Tokens.font(22)).foregroundStyle(Tokens.railIcon)
                    .frame(width: 28, height: 28)
            }
        }
    }
}

/// A swipe in from the screen's left edge (UIScreenEdgePanGestureRecognizer): how far it has come,
/// then where it ended, a fling counting a little further.
private struct EdgeSwipe: UIGestureRecognizerRepresentable {
    var enabled: Bool
    var changed: (CGFloat) -> Void
    var ended: (CGFloat) -> Void

    func makeUIGestureRecognizer(context: Context) -> UIScreenEdgePanGestureRecognizer {
        let recognizer = UIScreenEdgePanGestureRecognizer()
        recognizer.edges = .left
        return recognizer
    }

    func updateUIGestureRecognizer(_ recognizer: UIScreenEdgePanGestureRecognizer, context: Context) {
        recognizer.isEnabled = enabled
    }

    func handleUIGestureRecognizerAction(_ recognizer: UIScreenEdgePanGestureRecognizer, context: Context) {
        let moved = max(0, recognizer.translation(in: recognizer.view).x)
        switch recognizer.state {
        case .began, .changed: changed(moved)
        case .ended: ended(moved + recognizer.velocity(in: recognizer.view).x * 0.15)
        default: ended(0)
        }
    }
}
