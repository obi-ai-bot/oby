import SwiftUI

/// The side panel on an Obysphere (Sangam's Sidebar.tsx, its `obysphere` section; docs/obysphere.md
/// part 1): its name and how many are in it, its ⋮ (Settings; Leave for anyone but its admin),
/// Knowledge and Invite member, then its members. A member's Oby has no page of its own yet
/// (docs/obysphere.md, step 5): their rows only say who is in it, and an orange dot when their key
/// changed (Home asks about it).
struct SpherePanel: View {
    let model: IslandModel
    let state: ChatState
    @State private var sphere: SphereModel
    @State private var inviting = false
    @State private var leaving = false
    @State private var folded = false

    init(model: IslandModel, state: ChatState) {
        self.model = model
        self.state = state
        _sphere = State(initialValue: SphereModel(feed: model.feed))
    }

    private var name: String { sphere.name ?? sphere.membership?.sphere ?? "Obysphere" }

    /// "9 members", you included; "1 member" alone.
    nonisolated static func count(_ members: Int) -> String { "\(members) member\(members == 1 ? "" : "s")" }

    /// A member as the Obysphere's screens name them: their owner id, capitalised as a name is
    /// (the Obysphere keeps no display names, sphere-api.md).
    nonisolated static func shown(_ owner: String) -> String { owner.prefix(1).uppercased() + owner.dropFirst() }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 4) {
                Text(name).font(Tokens.font(Tokens.textTitle, .semibold)).foregroundStyle(Tokens.ink)
                    .lineLimit(1).frame(height: 32).padding(.horizontal, 10)
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: 0)
                options
            }
            Text(Self.count(sphere.memberCount)).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted)
                .padding(.horizontal, 10).padding(.bottom, 16)
                .opacity(sphere.name == nil ? 0 : 1)
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    PanelRow(symbol: "book", title: "Knowledge") {
                        state.trail.start(.knowledge(name: name, admin: sphere.admin))
                    }
                    PanelRow(symbol: "person.badge.plus", title: "Invite member") { inviting = true }
                        .padding(.bottom, 12)
                    members
                }
                .padding(.bottom, 8)
            }
            .scrollIndicators(.never)
        }
        .padding(.horizontal, 16).padding(.top, state.chrome.bar)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        .background(alignment: .top) { WindowDragArea().frame(height: state.chrome.bar) }
        // Its members and their keys, while it shows; out of it (left, or removed), back Home.
        .task {
            while !Task.isCancelled {
                do { try await sphere.refresh() } catch { sphere.problem = error.localizedDescription }
                if sphere.membership != nil && !sphere.joined {
                    state.section = .home
                    state.sphereChanged += 1
                    return
                }
                try? await Task.sleep(for: .seconds(20))
            }
        }
        .sheet(isPresented: $inviting) { InviteSheet(sphere: sphere, close: { inviting = false }) }
        .confirmationDialog("Leave \(name)?", isPresented: $leaving, titleVisibility: .visible) {
            Button("Leave", role: .destructive) {
                sphere.run {
                    try await sphere.leave()
                    state.section = .home
                    state.sphereChanged += 1
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: {
            Text("You’ll lose access to \(name) and everything its Obys share. To come back, someone has to invite you again.")
        }
    }

    /// Its ⋮: which Obysphere (its name and address), then Settings, and Leave for anyone but its admin.
    private var options: some View {
        Menu {
            Text(name)
            if let address = sphere.address { Text(address) }
            Divider()
            Button("Settings") { state.trail.start(.sphereSettings(name: name)) }
            if sphere.joined && !sphere.admin {
                Divider()
                Button("Leave \(name)", role: .destructive) { leaving = true }
            }
        } label: { IconButton(symbol: "ellipsis", rotated: true) }
            .menuStyle(.button).buttonStyle(.plain).menuIndicator(.hidden).fixedSize()
            .accessibilityLabel("\(name) options")
    }

    /// Members, folding as Home's groups do: each by name, with their Oby's badge.
    private var members: some View {
        VStack(alignment: .leading, spacing: 1) {
            Button { withAnimation(Tokens.warm) { folded.toggle() } } label: {
                HStack(spacing: 4) {
                    Text("Members").font(Tokens.font(Tokens.textSm, .medium)).foregroundStyle(Tokens.muted)
                    Image(systemName: "chevron.down").font(Tokens.font(9, .semibold)).foregroundStyle(Tokens.muted)
                        .rotationEffect(.degrees(folded ? -90 : 0))
                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 10).frame(height: 28).contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityValue(folded ? "Folded" : "Open")
            if !folded {
                if sphere.name == nil && sphere.problem == nil {
                    RowBones()
                } else if sphere.members.isEmpty {
                    Text("Just you so far").font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted)
                        .padding(.horizontal, 10).padding(.vertical, 6)
                } else {
                    ForEach(sphere.members) { MemberRow(member: $0) }
                }
                if let problem = sphere.problem {
                    Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger)
                        .padding(.horizontal, 10).padding(.vertical, 6).fixedSize(horizontal: false, vertical: true)
                }
            }
        }
    }
}

/// A member (Sangam's `oby-row`): their picture with their Oby's badge, and their name.
private struct MemberRow: View {
    let member: SphereModel.Member

    private var shown: String { SpherePanel.shown(member.owner) }

    var body: some View {
        HStack(spacing: 10) {
            Text(SidebarSections.initials(shown)).font(Tokens.font(9, .semibold)).foregroundStyle(.white.opacity(0.92))
                .frame(width: 22, height: 22).background(Tokens.avatar(shown), in: Circle())
                .overlay(alignment: .bottomTrailing) {
                    ObyMark(size: 8).frame(width: 11, height: 11).background(Tokens.bg, in: Circle()).offset(x: 3, y: 3)
                }
            Text(shown).font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.item).lineLimit(1)
            Spacer(minLength: 0)
            if member.changedTo != nil {
                Circle().fill(Tokens.needs).frame(width: 6, height: 6)
                    .help("\(shown)’s key changed: compare it with them, on Home")
            }
        }
        .padding(.horizontal, 10).frame(height: 34)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(shown)’s Oby" + (member.changedTo != nil ? ", key changed" : ""))
    }
}

/// Invite member (Sangam's InviteModal.tsx; docs/obysphere.md part 3) over what the Obysphere server
/// has today: the emails get one link for that many people, which the admin sends from their own
/// Mail (the owner's decision of 2026-10-25), only for the admin's work domain when they say so;
/// and the team link, to copy and pass on. The server lists no invitations yet, so nobody shows as
/// Invited. Only its admin invites; anyone else is told who can.
struct InviteSheet: View {
    let sphere: SphereModel
    let close: () -> Void
    @State private var emails: [String] = []
    @State private var draft = ""
    @State private var email: String?
    @State private var onlyDomain = true
    @State private var sending = false
    @State private var sent = false
    @State private var copied = false
    @State private var problem: String?

    private var name: String { sphere.name ?? sphere.membership?.sphere ?? "the Obysphere" }
    private var domain: String? { CreateSphereSheet.workDomain(email) }
    private var all: [String] { emails + (draft.trimmingCharacters(in: .whitespaces).isEmpty ? [] : [draft.trimmingCharacters(in: .whitespaces)]) }
    private var ready: Bool { !all.isEmpty && all.allSatisfy(CreateSphereSheet.isEmail) && !sending && !sent }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Invite to \(name)’s Obysphere").font(Tokens.font(17, .semibold)).foregroundStyle(Tokens.ink)
                Spacer()
                IconButton(symbol: "xmark", action: close).keyboardShortcut(.cancelAction).accessibilityLabel("Close")
            }
            .padding(.top, -4).padding(.trailing, -6).padding(.bottom, 16)
            if sphere.admin { invite } else {
                Text("Only \(sphere.adminOwner ?? "its admin"), \(name)’s admin, can invite people for now. Ask them to.")
                    .font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.text).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(20)
        .frame(width: 480)
        .background(Tokens.surface)
        .task { if email == nil { email = await CreateSphereSheet.ownEmail() } }
    }

    @ViewBuilder private var invite: some View {
        label("Email").padding(.bottom, 8)
        EmailTags(emails: $emails, draft: $draft, submit: send).fixedSize(horizontal: false, vertical: true)
        if let domain {
            HStack(spacing: 16) {
                Text("Only allow @\(domain) emails to join").font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.text)
                Spacer()
                Toggle("Only allow @\(domain) emails to join", isOn: $onlyDomain).toggleStyle(.set).labelsHidden()
            }
            .padding(.top, 16)
        }
        label("Invite link").padding(.top, 20).padding(.bottom, 8)
        HStack(spacing: 8) {
            if let link = sphere.teamLink {
                // The secret after the # only through Copy, which conceals it.
                SetCode(link.link.components(separatedBy: "#")[0] + "#…")
                Spacer(minLength: 0)
                Button(copied ? "Copied" : "Copy") {
                    SphereSettingsView.copySecret(link.link)
                    copied = true
                }
                .buttonStyle(.set)
            } else {
                Text("None yet").font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.muted)
                Spacer(minLength: 0)
                Button("Make a link") {
                    sphere.run { _ = try await sphere.makeTeamLink(domain: onlyDomain ? domain : nil); copied = false }
                }
                .buttonStyle(.set)
            }
        }
        if let problem = problem ?? sphere.problem {
            Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger).padding(.top, 16)
                .fixedSize(horizontal: false, vertical: true)
        }
        HStack {
            Spacer()
            Button(action: send) {
                HStack(spacing: 6) {
                    if sending { ProgressView().controlSize(.small) }
                    if sent { Image(systemName: "checkmark") }
                    Text(sent ? "Sent" : "Send invitation")
                }
                .font(Tokens.font(Tokens.textBase, .medium)).foregroundStyle(Tokens.onInk)
                .padding(.horizontal, 14).frame(height: 32)
                .background(Tokens.ink, in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                .opacity(ready || sent ? 1 : 0.3)
            }
            .buttonStyle(.plain).disabled(!ready).keyboardShortcut(.defaultAction)
        }
        .padding(.top, 24)
    }

    private func label(_ text: String) -> some View {
        Text(text).font(Tokens.font(Tokens.textMd, .medium)).foregroundStyle(Tokens.item)
    }

    /// One link for everyone named, opened in a Mail message to them for the admin to send.
    private func send() {
        guard ready else { return }
        let invited = all
        sending = true
        problem = nil
        Task {
            defer { sending = false }
            do {
                let link = try await sphere.invite(invited, domain: onlyDomain ? domain : nil)
                SphereModel.mail(link, to: invited)
                sent = true
                try? await Task.sleep(for: .milliseconds(1200))
                close()
            } catch { problem = error.localizedDescription }
        }
    }
}

/// What an Obysphere's Obys look things up in (Sangam's KnowledgeView.tsx; docs/obysphere.md part
/// 5). No source can be connected yet: one comes after sealed message bodies (step 4, then 6), so
/// this is its empty page, in the design's words.
struct KnowledgePane: View {
    let name: String
    let admin: Bool

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text("Knowledge").font(Tokens.font(28, .semibold)).foregroundStyle(Tokens.ink)
                    .padding(.bottom, 8).accessibilityAddTraits(.isHeader)
                Text("Knowledge systems every Oby in \(name)’s Obysphere has access to.")
                    .font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.muted).padding(.bottom, 24)
                VStack(spacing: 8) {
                    Text("No knowledge sources yet").font(Tokens.font(Tokens.textBase, .semibold)).foregroundStyle(Tokens.ink)
                    Text(admin ? "Connect Tetrix, or any graph or vector database with an MCP server." : "An admin can connect one.")
                        .font(Tokens.font(Tokens.textMd)).foregroundStyle(Tokens.muted).multilineTextAlignment(.center)
                    if admin {
                        Button("Add knowledge source") {}.buttonStyle(.set).disabled(true).padding(.top, 8)
                        Text("Coming: your Tetrix, connected once for everyone here.")
                            .font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.muted)
                    }
                }
                .frame(maxWidth: .infinity).padding(.vertical, 40)
                .background(Tokens.chip, in: RoundedRectangle(cornerRadius: Tokens.askRadius, style: .continuous))
            }
            .frame(maxWidth: Tokens.column, alignment: .leading)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, Tokens.gutter).padding(.top, 16).padding(.bottom, 32)
        }
    }
}

/// An Obysphere's settings over Home, from its ⋮ (Sangam's SphereSettings.tsx), under its name
/// ("Deskree settings"), while the side panel stays on the Obysphere: Settings › Obysphere's page.
struct SphereSettingsPane: View {
    let model: IslandModel
    let name: String

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text("\(name) settings").font(Tokens.font(28, .semibold)).foregroundStyle(Tokens.ink)
                    .padding(.bottom, 16).accessibilityAddTraits(.isHeader)
                SphereSettingsView(feed: model.feed)
            }
            .frame(maxWidth: Tokens.column, alignment: .leading)
            .frame(maxWidth: .infinity)
            .padding(.horizontal, Tokens.gutter).padding(.top, 16).padding(.bottom, 32)
        }
        .environment(\.settingsCards, true)
        .takesKeys()
    }
}

/// A teammate's Oby whose key changed, as a card on Home (the owner's decision of 2026-10-25,
/// docs/obysphere.md part 4): obid trusts it again only once the owner compared the new key with
/// them and says it matches. Pins are obid's own record (`GET /v1/obid/sphere/pins`).
struct KeyChangeCard: View {
    let pin: SphereModel.Pin
    let feed: RunnerFeed
    let head: AnyView
    var keyboardEnabled = false
    var focusRequest = 0
    let onLater: () -> Void
    let onDone: () -> Void
    @State private var busy = false
    @State private var problem: String?

    static func title(_ pin: SphereModel.Pin) -> String { "\(SpherePanel.shown(pin.owner))’s Oby has a new key" }

    /// The pins whose key changed; none when obid can't say.
    static func changed(_ feed: RunnerFeed) async -> [SphereModel.Pin] {
        guard let runner = feed.runner else { return [] }
        let pins: [SphereModel.Pin] = (try? await runner.call("GET", "v1/obid/sphere/pins")) ?? []
        return pins.filter { $0.changedTo != nil }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            CardHead(head: head)
            AskCard(kind: "New key", shown: pin.changedTo, title: Self.title(pin),
                    happened: "Your Oby doesn’t trust their Oby until you compare this key with \(SpherePanel.shown(pin.owner)), in person or by phone, every character.",
                    options: [CardOption(label: "It matches", answer: "It matches", detail: "Your Oby trusts their Oby with this key from now on.")],
                    busy: busy, keyboardEnabled: keyboardEnabled, focusRequest: focusRequest,
                    onChoose: { _ in accept() }, onLater: onLater)
            if let problem { Text(problem).font(Tokens.font(Tokens.textSm)).foregroundStyle(Tokens.danger).padding(.top, 8) }
        }
        .padding(16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Tokens.chip, in: RoundedRectangle(cornerRadius: Tokens.askRadius, style: .continuous))
    }

    private func accept() {
        guard !busy, let fingerprint = pin.changedTo, let runner = feed.runner else { return }
        busy = true
        problem = nil
        Task {
            defer { busy = false }
            do {
                _ = try await runner.send("POST", "v1/obid/sphere/pins/\(pin.owner)", json: try JSONSerialization.data(withJSONObject: ["fingerprint": fingerprint]))
                onDone()
            } catch { problem = error.localizedDescription }
        }
    }
}
