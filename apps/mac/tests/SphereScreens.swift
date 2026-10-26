import AppKit
import SwiftUI
import Testing
@testable import Oby

/// Creating an Obysphere (the strip's +) as pictures, against Sangam's CreateSphereModal, and Settings ›
/// Obysphere's Host your own and the Join screen an oby://join link opens: off unless
/// `OBY_SCREENS` names a folder (`TEST_RUNNER_OBY_SCREENS=… xcodebuild test -only-testing:MacTests/SphereScreens`).
@MainActor
@Suite(.serialized, .enabled(if: ProcessInfo.processInfo.environment["OBY_SCREENS"] != nil))
struct SphereScreens {
    let folder = URL(fileURLWithPath: ProcessInfo.processInfo.environment["OBY_SCREENS"] ?? NSTemporaryDirectory())

    /// Settings › Obysphere in an Obysphere, with Leave at the bottom; and once its admin removed this
    /// Oby, Leave to join another (#1214). The Sphere is faked for this test alone.
    @Test func leaveAnObysphere() async throws {
        let server = RunnerStub { request in
            switch (request.method, request.path) {
            case ("POST", "/v0.1/oauth/token"): (200, #"{"access_token": "owner-token"}"#)
            case ("GET", "/v0.1/spheres/deskree"): (200, #"{"name": "Deskree", "admin_owner": "doti"}"#)
            case ("GET", "/v0.1/spheres/deskree/members"):
                (200, #"{"members": [{"obi": "obi-sangam", "owner": "sangam", "status": "active", "card": {"name": "Sangam's Oby"}}]}"#)
            default: (404, "{}")
            }
        }
        let owner = "tester-\(UUID().uuidString.lowercased().prefix(8))"
        let joined = #"{"state": "joined", "owner": "\#(owner)", "url": "https://\#(server.host)", "sphere": "deskree", "obiId": "obi-\#(owner)"}"#
        let obid = RunnerStub { request in
            switch request.path {
            case "/v1/obid/sphere": (200, joined)
            case "/v1/obid/sphere/pins", "/v1/inbox": (200, "[]")
            default: (404, "{}")
            }
        }
        let session = SphereServer.session
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [RunnerStubProtocol.self] + (configuration.protocolClasses ?? [])
        SphereServer.session = URLSession(configuration: configuration, delegate: NoRedirects(), delegateQueue: nil)
        defer {
            SphereServer.session = session
            try? OwnerKey.store.remove(owner)
        }
        let feed = RunnerFeed()
        feed.stageAlerts(following: obid.runner)
        try await save(settings(SphereSettingsView(feed: feed)), as: "sphere-leave")

        let removed = RunnerStub { request in
            request.path == "/v1/obid/sphere" ? (200, #"{"state": "revoked", "owner": "\#(owner)", "sphere": "deskree"}"#) : (404, "{}")
        }
        let gone = RunnerFeed()
        gone.stageAlerts(following: removed.runner)
        try await save(settings(SphereSettingsView(feed: gone)), as: "sphere-removed")
    }

    @Test func createSheet() async throws {
        try await save(CreateSphereSheet(feed: RunnerFeed(), email: "doti@gmail.com", done: { _ in }, close: {}), as: "sphere-create-empty")
        try await save(CreateSphereSheet(feed: RunnerFeed(), name: "Deskree Platform", emails: ["ernesto@deskree.com", "sangam@deskree.com"],
                                         email: "doti@deskree.com", done: { _ in }, close: {}), as: "sphere-create-filled")
        try await save(CreateSphereSheet(feed: RunnerFeed(), name: "Deskree Platform", emails: ["ernesto@deskree.com", "not-an-email"],
                                         email: "doti@deskree.com", done: { _ in }, close: {}), as: "sphere-create-wrong-email")
    }

    /// Settings › Obysphere for its admin: Remove on each member, and Cancel invite on the team link
    /// (#1217). The Sphere is faked for this test alone.
    @Test func theAdminsMembersAndTeamLink() async throws {
        let owner = "tester-\(UUID().uuidString.lowercased().prefix(8))"
        let id = "deskree-\(UUID().uuidString.lowercased().prefix(8))"
        let server = RunnerStub { request in
            switch (request.method, request.path) {
            case ("POST", "/v0.1/oauth/token"): (200, #"{"access_token": "owner-token"}"#)
            case ("GET", "/v0.1/spheres/\(id)"): (200, #"{"name": "Deskree", "admin_owner": "\#(owner)"}"#)
            case ("GET", "/v0.1/spheres/\(id)/members"):
                (200, #"{"members": [{"obi": "obi-sangam", "owner": "sangam", "status": "active", "card": {"name": "Sangam's Oby"}}, {"obi": "obi-ernesto", "owner": "ernesto", "status": "active", "card": {"name": "Ernesto's Oby"}}]}"#)
            default: (404, "{}")
            }
        }
        let obid = RunnerStub { request in
            switch request.path {
            case "/v1/obid/sphere": (200, #"{"state": "joined", "owner": "\#(owner)", "url": "https://\#(server.host)", "sphere": "\#(id)", "obiId": "obi-\#(owner)"}"#)
            case "/v1/obid/sphere/pins", "/v1/inbox": (200, "[]")
            default: (404, "{}")
            }
        }
        let session = SphereServer.session
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [RunnerStubProtocol.self] + (configuration.protocolClasses ?? [])
        SphereServer.session = URLSession(configuration: configuration, delegate: NoRedirects(), delegateQueue: nil)
        defer {
            SphereServer.session = session
            try? OwnerKey.store.remove(owner)
            try? SphereModel.teamLinks.remove(id)
        }
        let link = InviteLink(server: URL(string: "https://obysphere.deskree.com")!, sphere: id, name: "Deskree", by: owner, id: "inv-1", secret: "s")
        try SphereModel.teamLinks.save(link.link, as: id)
        let feed = RunnerFeed()
        feed.stageAlerts(following: obid.runner)
        try await save(settings(SphereSettingsView(feed: feed)), as: "sphere-admin")
    }

    /// The side panel on an Obysphere, its admin's Invite member, its Knowledge (empty for now) and a
    /// teammate's changed key on Home (docs/obysphere.md, step 2). The Sphere is faked for this test alone.
    @Test func thePanelInviteKnowledgeAndAChangedKey() async throws {
        let owner = "tester-\(UUID().uuidString.lowercased().prefix(8))"
        let id = "deskree-\(UUID().uuidString.lowercased().prefix(8))"
        let server = RunnerStub { request in
            switch (request.method, request.path) {
            case ("POST", "/v0.1/oauth/token"): (200, #"{"access_token": "owner-token"}"#)
            case ("GET", "/v0.1/spheres/\(id)"): (200, #"{"name": "Deskree", "admin_owner": "\#(owner)"}"#)
            case ("GET", "/v0.1/spheres/\(id)/members"):
                (200, #"{"members": [{"obi": "obi-wellington", "owner": "wellington", "status": "active", "card": {"name": "wellington's Oby"}}, {"obi": "obi-ernesto", "owner": "ernesto", "status": "active", "card": {"name": "ernesto's Oby"}}, {"obi": "obi-sangam", "owner": "sangam", "status": "active", "card": {"name": "sangam's Oby"}}]}"#)
            default: (404, "{}")
            }
        }
        let obid = RunnerStub { request in
            switch request.path {
            case "/v1/obid/sphere": (200, #"{"state": "joined", "owner": "\#(owner)", "url": "https://\#(server.host)", "sphere": "\#(id)", "obiId": "obi-\#(owner)"}"#)
            case "/v1/obid/sphere/pins": (200, #"[{"owner": "ernesto", "fingerprint": "ed25519:4f1c9a", "changedTo": "ed25519:9b27e0d4c1a8f3e6"}]"#)
            case "/v1/inbox": (200, "[]")
            default: (404, "{}")
            }
        }
        let session = SphereServer.session
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [RunnerStubProtocol.self] + (configuration.protocolClasses ?? [])
        SphereServer.session = URLSession(configuration: configuration, delegate: NoRedirects(), delegateQueue: nil)
        defer {
            SphereServer.session = session
            try? OwnerKey.store.remove(owner)
        }
        let feed = RunnerFeed()
        feed.stageAlerts(following: obid.runner)
        let model = IslandModel(feed: feed, availability: MacAvailability())
        try await save(SpherePanel(model: model, state: ChatState()).frame(width: Tokens.sidebar - Tokens.rail, height: 420).background(Tokens.bg),
                       as: "sphere-panel")
        let sphere = SphereModel(feed: feed)
        try await sphere.refresh()
        try await save(InviteSheet(sphere: sphere, close: {}), as: "sphere-invite")
        try await save(KnowledgePane(name: "Deskree", admin: true).frame(width: Tokens.column + 48, height: 340).background(Tokens.bg), as: "sphere-knowledge")
        let pin = try #require(await KeyChangeCard.changed(feed).first)
        try await save(KeyChangeCard(pin: pin, feed: feed, head: AnyView(Text(KeyChangeCard.title(pin))), onLater: {}, onDone: {})
                        .padding(24).frame(width: 560).background(Tokens.bg), as: "sphere-key-changed")
    }

    /// On a current server (obi-sphere#27), its admin's settings: the picture and the name to change,
    /// Make admin on each member, and the invited with Cancel invite; and the side panel's Invited.
    /// It writes this Mac's record of addresses: only with `OBY_HOME` set to a folder of its own.
    @Test(.enabled(if: ProcessInfo.processInfo.environment["OBY_HOME"] != nil))
    func aCurrentServersEditsAndInvited() async throws {
        let owner = "tester-\(UUID().uuidString.lowercased().prefix(8))"
        let id = "deskree-\(UUID().uuidString.lowercased().prefix(8))"
        let server = RunnerStub { request in
            switch (request.method, request.path) {
            case ("POST", "/v0.1/oauth/token"): (200, #"{"access_token": "owner-token"}"#)
            case ("GET", "/v0.1/spheres/\(id)"): (200, #"{"name": "Deskree", "admin_owner": "\#(owner)", "admins": ["\#(owner)", "ernesto"]}"#)
            case ("GET", "/v0.1/spheres/\(id)/members"):
                (200, #"{"members": [{"obi": "obi-ernesto", "owner": "ernesto", "status": "active", "card": {"name": "ernesto's Oby"}}, {"obi": "obi-sangam", "owner": "sangam", "status": "active", "card": {"name": "sangam's Oby"}}]}"#)
            case ("GET", "/v0.1/spheres/\(id)/invitations"):
                (200, #"{"sphere": "\#(id)", "invitations": [{"invitation_id": "inv-1", "label": "Invited by email", "max_uses": 1, "uses": 0, "expires_at": "2026-11-01T00:00:00Z"}, {"invitation_id": "inv-2", "label": "Team link", "max_uses": 50, "uses": 2, "expires_at": "2026-11-01T00:00:00Z"}]}"#)
            default: (404, "{}")
            }
        }
        let obid = RunnerStub { request in
            switch request.path {
            case "/v1/obid/sphere": (200, #"{"state": "joined", "owner": "\#(owner)", "url": "https://\#(server.host)", "sphere": "\#(id)", "obiId": "obi-\#(owner)"}"#)
            case "/v1/obid/sphere/pins", "/v1/inbox": (200, "[]")
            default: (404, "{}")
            }
        }
        let session = SphereServer.session
        let configuration = URLSessionConfiguration.ephemeral
        configuration.protocolClasses = [RunnerStubProtocol.self] + (configuration.protocolClasses ?? [])
        SphereServer.session = URLSession(configuration: configuration, delegate: NoRedirects(), delegateQueue: nil)
        defer {
            SphereServer.session = session
            try? OwnerKey.store.remove(owner)
            SphereModel.keepInvited([:])
        }
        SphereModel.keepInvited(["inv-1": "sangam@deskree.com"])
        let feed = RunnerFeed()
        feed.stageAlerts(following: obid.runner)
        try await save(settings(SphereSettingsView(feed: feed)), as: "sphere-admin-current")
        let model = IslandModel(feed: feed, availability: MacAvailability())
        try await save(SpherePanel(model: model, state: ChatState()).frame(width: Tokens.sidebar - Tokens.rail, height: 480).background(Tokens.bg),
                       as: "sphere-panel-invited")
        // The record keeps the addresses of those the server still lists.
        #expect(SphereModel.invited() == ["inv-1": "sangam@deskree.com"])
    }

    /// The Join screen an oby://join link opened, with the trust question for a server Oby doesn't
    /// know (in the link's own words: nothing is asked of it yet); then Host your own once Deploy
    /// copied the token, and once an address is checked.
    @Test func anOpenedJoinLinkAndHostYourOwn() async throws {
        let obid = RunnerStub { request in
            request.path == "/v1/obid/sphere" ? (200, #"{"state": "none", "owner": "wellington"}"#) : (404, "{}")
        }
        let feed = RunnerFeed()
        feed.stageAlerts(following: obid.runner)
        OpenedInvite.shared.invite = InviteLink(opened: URL(string: "oby://join/obi-sphere.acme.workers.dev#sphere=acme&name=Acme&by=sangam&id=inv-1&secret=s")!)
        try await save(settings(SphereSettingsView(feed: feed)), as: "sphere-join-opened")
        let host = HostOwn.shared
        host.made = ("made", 0)
        defer { (host.made, host.address) = (nil, "") }
        try await save(settings(SphereSettingsView(feed: feed)), as: "sphere-host-your-own")
        // Checked, waiting for the owner to say it's theirs.
        host.address = "obi-sphere.acme.workers.dev"
        host.checked = URL(string: "https://obi-sphere.acme.workers.dev")
        try await save(settings(SphereSettingsView(feed: feed)), as: "sphere-host-your-own-confirm")
    }

    private func settings(_ page: some View) -> some View {
        page.environment(\.settingsCards, true).padding(24).frame(width: Tokens.column + 48).background(Tokens.bg)
    }

    private func save(_ page: some View, as name: String) async throws {
        let view = NSHostingView(rootView: page.environment(\.colorScheme, .dark))
        let size = view.fittingSize
        let window = NSWindow(contentRect: NSRect(origin: .zero, size: size), styleMask: [.borderless], backing: .buffered, defer: false)
        window.appearance = NSAppearance(named: .darkAqua)
        window.contentView = view
        view.frame = NSRect(origin: .zero, size: size)
        window.orderBack(nil)
        try await Task.sleep(for: .milliseconds(2000))
        view.layoutSubtreeIfNeeded()
        let image = try #require(view.bitmapImageRepForCachingDisplay(in: view.bounds))
        view.cacheDisplay(in: view.bounds, to: image)
        let png = try #require(image.representation(using: .png, properties: [:]))
        try png.write(to: folder.appending(path: "\(name).png"))
        window.orderOut(nil)
    }
}
