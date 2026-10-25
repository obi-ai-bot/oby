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

    @Test func createSheet() async throws {
        try await save(CreateSphereSheet(feed: RunnerFeed(), email: "doti@gmail.com", done: { _ in }, close: {}), as: "sphere-create-empty")
        try await save(CreateSphereSheet(feed: RunnerFeed(), name: "Deskree Platform", emails: ["ernesto@deskree.com", "sangam@deskree.com"],
                                         email: "doti@deskree.com", done: { _ in }, close: {}), as: "sphere-create-filled")
        try await save(CreateSphereSheet(feed: RunnerFeed(), name: "Deskree Platform", emails: ["ernesto@deskree.com", "not-an-email"],
                                         email: "doti@deskree.com", done: { _ in }, close: {}), as: "sphere-create-wrong-email")
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
