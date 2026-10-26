import AppKit
import SwiftUI
import UniformTypeIdentifiers

/// Settings › Obysphere: this Mac's Oby in its Obysphere (onboarding.md section 4). The server it
/// uses; join with an invite link ("Join Deskree"), or create one (the strip's +); once in, who else
/// is in it, the team link (an admin's), and what other Obys asked. Oby asks a teammate's Oby for a
/// review by itself, as the repository's workflow says (the owner's decision of 2026-10-25).
// ponytail: one page until Sangam's Obysphere screens (Sidebar.tsx, SphereSettings.tsx) are ported.
struct SphereSettingsView: View {
    @State private var sphere: SphereModel
    @State private var pasted = ""
    @State private var invite: InviteLink?
    /// The owner said they trust a link's server that isn't the one Oby knows.
    @State private var trusted = false
    @State private var creating = false
    @State private var serverText = UserDefaults.standard.string(forKey: SphereModel.serverKey) ?? ""
    /// The member whose key you're about to accept, once you've compared it with them.
    @State private var comparing: SphereModel.Member?
    @State private var leaving = false
    @State private var nameText = ""
    @State private var saved = false
    @State private var deleting = false
    /// The name typed to confirm Delete.
    @State private var typed = ""
    /// Host your own: kept while Oby runs, so the token Deploy… made outlives this page.
    @State private var host = HostOwn.shared
    /// The claim link opened, waiting for the owner to say claim it.
    @State private var claiming: ClaimLink?
    private let feed: RunnerFeed

    init(feed: RunnerFeed) {
        self.feed = feed
        _sphere = State(initialValue: SphereModel(feed: feed))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if sphere.joined { joined } else { notJoined }
        }
        .disabled(sphere.busy)
        // An oby://join link opened (AppDelegate): its Join screen, as if pasted here and opened.
        .onChange(of: OpenedInvite.shared.invite, initial: true) { _, opened in
            guard let opened else { return }
            OpenedInvite.shared.invite = nil
            pasted = opened.link
            Task {
                // The membership first: on a page just opened it isn't read yet.
                try? await sphere.refresh()
                if sphere.joined { sphere.problem = "This Oby is in an Obysphere already: leave it first, at the bottom of this page." } else { open(opened) }
            }
        }
        // An oby://claim link opened (AppDelegate): asked once, then claimed and Create (`HostOwn.claim`).
        .onChange(of: OpenedInvite.shared.claim, initial: true) { _, opened in
            guard let opened else { return }
            OpenedInvite.shared.claim = nil
            Task {
                try? await sphere.refresh()
                if sphere.joined { sphere.problem = "This Oby is in an Obysphere already: leave it first, at the bottom of this page." } else { claiming = opened }
            }
        }
        .alert("Claim the Obysphere at \(claiming?.host ?? "")?", isPresented: Binding(get: { claiming != nil }, set: { if !$0 { claiming = nil } }),
               presenting: claiming) { link in
            Button("Claim") {
                Task {
                    guard await host.claim(link) else { return }
                    serverText = link.server.absoluteString
                    creating = true
                }
            }
            Button("Cancel", role: .cancel) {}
        } message: { link in
            Text("Oby makes its operator token, sends it to \(link.host) once with the link's code, keeps it in your Keychain under \(link.host), and makes it your Obysphere server. Claim it only if you just started that server.")
        }
        // While the page is open: the membership, the members and what waits, every 20 seconds.
        .task {
            while !Task.isCancelled {
                do { try await sphere.refresh() } catch { sphere.problem = error.localizedDescription }
                try? await Task.sleep(for: .seconds(20))
            }
        }
    }

    // MARK: Not in one yet

    @ViewBuilder private var notJoined: some View {
        if let removed = sphere.membership.flatMap({ $0.state == "revoked" ? $0.sphere ?? "your Obysphere" : nil }) {
            SetGroup("Removed from \(removed)", footer: "Its admin removed this Oby. Leave it to join another Obysphere, or this one again with a new link.") {
                SetRow("") { Button("Leave \(removed)") { sphere.run { try await sphere.leave() } }.buttonStyle(.set) }
            }
        }
        if let invite {
            // "Join Deskree" (onboarding.md section 4).
            // The name and who sent it are only what the link says; the server is what joining trusts.
            SetGroup("Join \(invite.name)", problem: sphere.problem) {
                SetRow(invite.by.isEmpty ? "The link says you're invited." : "The link says \(invite.by) invited you.",
                       about: "\(invite.name)'s Obys can ask yours for a review or a question. You decide what it answers."
                           + (invite.members.map { " \($0 == 1 ? "1 Oby is" : "\($0) Obys are") in it." } ?? ""))
                if let domain = invite.domain {
                    SetRow("For @\(domain) accounts", about: "Oby asks you to sign in with Google first, so \(invite.host) can check you're at \(domain).")
                }
                SetRow("Server") { SetCode(invite.host).textSelection(.enabled) }
                if let warning = invite.warning {
                    SetRow(about: warning) {
                        Toggle("I trust \(invite.host)", isOn: $trusted).toggleStyle(.checkbox)
                    }
                }
                SetRow("") {
                    HStack {
                        Button("Not now") { self.invite = nil }.buttonStyle(.set)
                        Button("Join \(invite.name)") { sphere.run { try await sphere.join(invite) } }
                            .buttonStyle(.set).disabled(invite.warning != nil && !trusted)
                    }
                }
            }
            .onChange(of: trusted) { if trusted { preview() } }
        } else {
            SetGroup("Join an Obysphere", footer: "Paste the invite link someone in it sent you.", problem: sphere.problem) {
                SetRow("Invite link") { SetField("Invite link", text: $pasted, prompt: "https://…/join#…", width: 320) }
                SetRow("") {
                    Button("Open") {
                        guard let found = InviteLink(pasted) else {
                            sphere.problem = "That isn't an Obysphere invite link with a public https address."
                            return
                        }
                        open(found)
                    }
                    .buttonStyle(.set).disabled(pasted.isEmpty)
                }
            }
            SetGroup("Create an Obysphere", footer: "You're its admin. Everyone in it can ask everyone for a review or a question; each Oby still asks its owner.") {
                SetRow("", about: SphereModel.server == nil ? "Set the server below first." : nil) {
                    Button("Create an Obysphere…") { creating = true }.buttonStyle(.set).disabled(!sphere.canCreate)
                }
            }
            hostYourOwn
        }
        SetGroup("Server", footer: "Where your Obysphere lives: Deskree's for Deskree's people, or your own (obi-sphere's docs/run-your-own.md). Empty: the one this Oby came with, if any.") {
            SetRow("Address") {
                SetField("Obysphere server", text: $serverText, prompt: (Bundle.main.object(forInfoDictionaryKey: "ObySphereURL") as? String).flatMap { $0.isEmpty ? nil : $0 } ?? "https://sphere.example.com", width: 280)
                    .onSubmit(saveServer)
            }
            SetRow("") { Button("Save", action: saveServer).buttonStyle(.set).disabled(serverText == (UserDefaults.standard.string(forKey: SphereModel.serverKey) ?? "")) }
        }
        .sheet(isPresented: $creating) {
            CreateSphereSheet(feed: feed, done: { _ in
                creating = false
                Task { try? await sphere.refresh() }
            }, close: { creating = false })
        }
    }

    /// The Join screen for `found`, in the link's own words until the Sphere may be asked.
    private func open(_ found: InviteLink) {
        trusted = false
        invite = found
        preview()
    }

    /// What the Sphere says of the invite shown (its name, who's in it, a Workspace limit): for a
    /// server Oby doesn't know, only once "I trust" is ticked (`SphereModel.shown`).
    private func preview() {
        guard let found = invite else { return }
        sphere.run {
            let seen = await sphere.shown(found, trusted: trusted)
            if invite?.id == found.id { invite = seen }
        }
    }

    /// Host your own (Sangam's design has no screen for it; `HostOwn`): Deploy… copies the token and
    /// opens Cloudflare's deploy page; the address is checked on Return or Check, without the token,
    /// and used once you say it's yours; then Create. The token is never on screen.
    private var hostYourOwn: some View {
        SetGroup("Host your own", footer: "Oby keeps the token in this Mac's Keychain under the address you confirm, and sends it only there. One never used is forgotten when Oby quits or you deploy again; if Cloudflare has it already, deploy again and paste the new one over SPHERE_OPERATOR_TOKEN in your Worker's Settings › Variables and Secrets.", problem: host.problem) {
            SetRow("Deploy to Cloudflare", about: host.made != nil
                   ? "Oby copied your Obysphere's operator token, on this Mac only: paste it where Cloudflare asks for SPHERE_OPERATOR_TOKEN."
                   : "Your own Obysphere, on your Cloudflare account. Oby copies the token it needs, for you to paste there.") {
                Button("Deploy…") { NSWorkspace.shared.open(host.deploy()) }.buttonStyle(.set)
            }
            SetRow("Your Obysphere's address", about: "Once Cloudflare is done, your Worker's address.") {
                SetField("Your Obysphere's address", text: $host.address, prompt: "https://obi-sphere.you.workers.dev", width: 280)
                    .onSubmit { Task { await host.check() } }
            }
            if let checked = host.checked, let name = checked.host() {
                SetRow("Use \(name)?", about: host.made == nil ? "It becomes your Obysphere server."
                       : "Oby sends it the token it copied, keeps the token in your Keychain under \(name), and makes it your Obysphere server.") {
                    HStack {
                        Button("Not mine") { host.checked = nil }.buttonStyle(.set)
                        Button("Use it") {
                            Task {
                                guard await host.use() else { return }
                                serverText = checked.absoluteString
                                creating = true
                            }
                        }
                        .buttonStyle(.set)
                    }
                }
            } else {
                SetRow("") { Button("Check") { Task { await host.check() } }.buttonStyle(.set).disabled(host.address.isEmpty) }
            }
        }
        .disabled(host.busy)
    }

    private func saveServer() {
        let text = serverText.trimmingCharacters(in: .whitespacesAndNewlines).trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        if text.isEmpty {
            UserDefaults.standard.removeObject(forKey: SphereModel.serverKey)
        } else if let url = URL(string: text), let problem = SphereServer.problem(url) {
            sphere.problem = problem
            return
        } else {
            UserDefaults.standard.set(text, forKey: SphereModel.serverKey)
        }
        serverText = text
        sphere.problem = nil
    }

    /// nspasteboard.org's marker for a secret on the clipboard.
    static let concealed = NSPasteboard.PasteboardType("org.nspasteboard.ConcealedType")

    /// `secret` on this Mac's clipboard only (no Universal Clipboard), concealed: clipboard managers
    /// that honour it don't keep it. Answers the clipboard's change count, to clear it later if it's
    /// still this.
    @discardableResult
    static func copySecret(_ secret: String, to pasteboard: NSPasteboard = .general) -> Int {
        pasteboard.prepareForNewContents(with: .currentHostOnly)
        pasteboard.setString(secret, forType: .string)
        pasteboard.setString("", forType: concealed)
        return pasteboard.changeCount
    }

    /// Its id from its name (Sangam's `slugOf`): lowercase letters, digits and single hyphens.
    static func slug(_ name: String) -> String {
        let words = name.lowercased().folding(options: .diacriticInsensitive, locale: nil)
            .split { !($0.isASCII && ($0.isLetter || $0.isNumber)) }
        return String(words.joined(separator: "-").prefix(30))
    }

    /// "ernesto · key ed25519:…", the fingerprint to compare with them; or, when the Obysphere shows
    /// another key for them, what to do before your Oby trusts them again.
    static func memberAbout(_ member: SphereModel.Member) -> String {
        let who = member.active ? member.owner : "\(member.owner) · removed"
        if let new = member.changedTo {
            return "\(who) · their key changed to \(new). Your Oby doesn't trust them until you compare it with \(member.owner), in person or by phone, and accept it."
        }
        return member.fingerprint.map { "\(who) · key \($0)\(member.verified ? "" : " · not compared yet")" } ?? who
    }

    /// Why a key reads "not compared yet", while one does: it works as it is, and comparing makes sure.
    static func compareNote(_ members: [SphereModel.Member]) -> String? {
        guard members.contains(where: { !$0.verified && $0.fingerprint != nil && $0.changedTo == nil }) else { return nil }
        return "Oby trusts each teammate’s key from when it first saw it, and that’s enough to work together. To be sure it’s really them, Verify key and compare it with them in person or by phone."
    }

    // MARK: In one

    @ViewBuilder private var joined: some View {
        let name = sphere.name ?? sphere.membership?.sphere ?? "Obysphere"
        // Its picture, name and address (Sangam's first group): an admin changes the first two, each
        // kept as it's made; its address stays as it was made (the owner's decision of 2026-10-25).
        // A server from before obi-sphere#27 keeps neither.
        let editable = sphere.admin && sphere.current
        SetGroup(footer: !sphere.admin ? "Only its admins can change these."
                         : editable ? "Its address stays as it was made."
                         : "A new picture and name come once its Obysphere server is updated. Its address stays as it was made.",
                 problem: sphere.problem) {
            SetRow("Avatar", about: sphere.admin ? "A square image, at least 128px" : nil) {
                if editable {
                    Button(action: pickPicture) { SphereTile(name: name, image: SphereTile.kept, size: 40) }
                        .buttonStyle(.plain).help("Change its picture").accessibilityLabel("Change its picture")
                    if SphereTile.kept != nil {
                        Button("Remove") { sphere.run { try await sphere.removePicture() } }.buttonStyle(.set)
                    }
                } else {
                    SphereTile(name: name, image: SphereTile.kept, size: 40)
                }
            }
            SetRow("Name", about: saved ? "Saved" : nil) {
                if editable {
                    SetField("Name", text: $nameText, prompt: name, width: 220).onSubmit(rename)
                } else {
                    Text(name).font(Tokens.font(Tokens.textBase)).foregroundStyle(Tokens.text)
                }
            }
            // Under its label, where a long address wraps.
            if let address = sphere.address { SetRow("Obysphere URL", about: address) }
        }
        .onChange(of: sphere.name, initial: true) { nameText = sphere.name ?? "" }
        // Everyone in it, you first; an admin removes the others. Keys stay underneath (the owner's
        // decision of 2026-10-25): a changed one asks on Home, and here; comparing one on purpose
        // comes with the member's own page (docs/obysphere.md, step 5).
        SetGroup("Members · \(sphere.memberCount)",
                 footer: sphere.membership?.lastError.map { "Last problem with the Obysphere: \($0)" } ?? Self.compareNote(sphere.members)) {
            SetRow("\(Profile.shared.name) (you)", about: [sphere.owner, sphere.ownFingerprint.map { "your key \($0)" }].compactMap(\.self).joined(separator: " · ")) {
                role(sphere.owner ?? "", you: true)
            }
            ForEach(sphere.members) { member in
                SetRow(SpherePanel.shown(member), about: Self.memberAbout(member)) {
                    if member.changedTo != nil {
                        Button("Compare new key") { comparing = member }.buttonStyle(.set)
                    } else {
                        role(member.owner, you: false)
                    }
                    if sphere.admin {
                        Button("Remove", role: .destructive) { sphere.run { try await sphere.remove(member) } }.buttonStyle(.set)
                    }
                }
            }
            // Who hasn't joined yet: an admin cancels their invite (Sangam's "Invited").
            ForEach(sphere.invitations ?? []) { invitation in
                SetRow(invitation.shown) {
                    Text("Invited").font(Tokens.font(Tokens.textMd)).foregroundStyle(Tokens.muted)
                    Button("Cancel invite") { sphere.run { try await sphere.cancel(invitation) } }.buttonStyle(.set)
                }
            }
        }
        .confirmationDialog("Does \(comparing?.owner ?? "your teammate") read you this key?",
                            isPresented: Binding(get: { comparing != nil }, set: { if !$0 { comparing = nil } }),
                            titleVisibility: .visible) {
            Button("It matches") {
                if let member = comparing {
                    comparing = nil
                    Task { try? await sphere.acceptKey(member) }
                }
            }
            Button("Cancel", role: .cancel) { comparing = nil }
        } message: {
            Text("\(comparing?.changedTo ?? comparing?.fingerprint ?? "")\n\nCompare it with \(comparing?.owner ?? "them") in person or by phone, every character. Your Oby trusts their Oby with this key.")
        }
        // Last, set apart: an admin deletes it, anyone else leaves (Sangam's design), its name typed
        // to confirm a delete, so it isn't done by a slip.
        if sphere.admin {
            SetGroup {
                SetRow("Delete Obysphere", about: "Deletes \(name) for all \(sphere.memberCount) members. You can’t undo this."
                       + (sphere.current ? "" : " It comes once its Obysphere server is updated.")) {
                    Button("Delete", role: .destructive) { typed = ""; deleting = true }.buttonStyle(.set).disabled(!sphere.current)
                }
            }
            .alert("Delete \(name)?", isPresented: $deleting) {
                TextField("Type \(name) to confirm", text: $typed)
                Button("Delete Obysphere", role: .destructive) {
                    guard typed == name else { return }
                    sphere.run { try await sphere.deleteSphere() }
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This deletes \(name) for all \(sphere.memberCount) members, cancels open invites and erases everything its Obys shared. You can’t undo this. Type its name to confirm.")
            }
        }
        if !sphere.admin {
            SetGroup {
                SetRow("Leave \(name)", about: "You’ll lose access until someone invites you back.") {
                    Button("Leave", role: .destructive) { leaving = true }.buttonStyle(.set)
                }
            }
            .confirmationDialog("Leave \(name)?", isPresented: $leaving, titleVisibility: .visible) {
                Button("Leave", role: .destructive) { sphere.run { try await sphere.leave() } }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("You’ll lose access to \(name) and everything its Obys share. To come back, someone has to invite you again.")
            }
        }
    }

    /// Someone's role: an admin, or a member; another admin makes them one or takes it away, at
    /// least one always (the server refuses the last).
    @ViewBuilder private func role(_ owner: String, you: Bool) -> some View {
        let isAdmin = sphere.admins.contains(owner)
        let label = Text(isAdmin ? "Admin" : "Member").font(Tokens.font(Tokens.textMd)).foregroundStyle(Tokens.muted)
        if sphere.admin && sphere.current && !(you && sphere.admins.count < 2) {
            Menu {
                Button(isAdmin ? (you ? "Stop being an admin" : "Remove as admin") : "Make admin") {
                    sphere.run { try await sphere.setAdmin(owner, !isAdmin) }
                }
            } label: { label }
                .menuStyle(.button).buttonStyle(.plain).fixedSize()
        } else {
            label
        }
    }

    /// Its picture, from an image the admin picks: cut square and sent for everyone.
    private func pickPicture() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.image]
        guard panel.runModal() == .OK, let url = panel.url, let image = NSImage(contentsOf: url) else { return }
        sphere.run { try await sphere.setPicture(image) }
    }

    /// Its new name, kept as soon as it's given (Return), and "Saved" under it for a moment.
    private func rename() {
        let name = nameText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !name.isEmpty, name.count <= 100, name != sphere.name else { nameText = sphere.name ?? ""; return }
        sphere.run {
            try await sphere.rename(name)
            saved = true
            try? await Task.sleep(for: .seconds(2))
            saved = false
        }
    }
}
