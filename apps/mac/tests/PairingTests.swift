import Foundation
import Testing
@testable import Oby

struct PairingTests {
    @Test func invitationDecodesTheRunnerDateFormat() throws {
        let json = #"{"code":"123456","expiresAt":"2026-10-13T12:02:00Z","hostname":"oby.example","link":"{\"hostname\":\"oby.example\",\"name\":\"My Mac\"}","name":"My Mac","secret":"one-time","url":"https://oby.example"}"#

        let invitation = try PairingPayloadDecoder.decodeInvitation(Data(json.utf8))

        #expect(invitation.code == "123456")
        #expect(invitation.expiresAt == ISO8601DateFormatter().date(from: "2026-10-13T12:02:00Z"))
    }

    @Test func devicesDecodeTheRunnerDateFormat() throws {
        let json = #"[{"id":"device-1","name":"Alice's iPhone","created":"2026-10-13T10:00:00Z","expiresAt":"2026-11-12T10:00:00Z"}]"#

        let devices = try PairingPayloadDecoder.decodeDevices(Data(json.utf8))

        let device = try #require(devices.first)
        #expect(device.name == "Alice's iPhone")
        #expect(device.created == ISO8601DateFormatter().date(from: "2026-10-13T10:00:00Z"))
        #expect(device.expiresAt == ISO8601DateFormatter().date(from: "2026-11-12T10:00:00Z"))
    }

    /// What `tailscale serve status --json` printed on the owner's Mac (2026-10-20), its names changed.
    static let serve = #"""
    {"TCP": {"443": {"HTTPS": true}, "8443": {"HTTPS": true}},
     "Web": {"mac.tail0000.ts.net:443": {"Handlers": {"/oberon": {"Proxy": "http://127.0.0.1:7448"}}},
             "mac.tail0000.ts.net:8443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:8810"}}}},
     "AllowFunnel": {"mac.tail0000.ts.net:443": true}}
    """#

    static func tailnet(_ json: String, port: Int = 8810) -> PairingRoute.Tailnet {
        PairingRoute.tailnet(serveStatus: Data(json.utf8), port: port)
    }

    /// Funnel on :443 goes to another server there, not the gateway: the tailnet route stands.
    @Test func theTailnetRouteIsTheServeURLThatProxiesToTheGateway() {
        #expect(Self.tailnet(Self.serve) == .route("mac.tail0000.ts.net:8443"))
    }

    @Test func aFunnelURLIsNeverTheRoute() {
        let json = #"{"Web": {"mac.tail0000.ts.net:443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:8810"}}}}, "AllowFunnel": {"mac.tail0000.ts.net:443": true}}"#
        #expect(Self.tailnet(json) == .funnel)
    }

    @Test func aFunnelToTheGatewayOnAnyPathRulesOutTheTailnetRoute() {
        let json = #"""
        {"Web": {"mac.tail0000.ts.net:443": {"Handlers": {"/oby": {"Proxy": "http://127.0.0.1:8810"}}},
                 "mac.tail0000.ts.net:8443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:8810"}}}},
         "AllowFunnel": {"mac.tail0000.ts.net:443": true}}
        """#
        #expect(Self.tailnet(json) == .funnel)
    }

    @Test func aForegroundFunnelToTheGatewayRulesOutTheTailnetRoute() {
        let json = #"""
        {"Web": {"mac.tail0000.ts.net:8443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:8810"}}}},
         "Foreground": {"session-1": {"TCP": {"443": {"HTTPS": true}},
                                      "Web": {"mac.tail0000.ts.net:443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:8810"}}}},
                                      "AllowFunnel": {"mac.tail0000.ts.net:443": true}}}}
        """#
        #expect(Self.tailnet(json) == .funnel)
    }

    @Test func aTCPFunnelToTheGatewayRulesOutTheTailnetRoute() {
        let json = #"""
        {"TCP": {"10000": {"TCPForward": "127.0.0.1:8810"}},
         "Web": {"mac.tail0000.ts.net:8443": {"Handlers": {"/": {"Proxy": "http://127.0.0.1:8810"}}}},
         "AllowFunnel": {"mac.tail0000.ts.net:10000": true}}
        """#
        #expect(Self.tailnet(json) == .funnel)
    }

    @Test func aServeURLForAnotherPortOrPathIsNotTheRoute() {
        #expect(Self.tailnet(Self.serve, port: 9999) == .nothing)
        #expect(Self.tailnet(#"{"Web": {"mac.tail0000.ts.net:8443": {"Handlers": {"/oby": {"Proxy": "http://127.0.0.1:8810"}}}}}"#) == .nothing)
    }

    @Test func aTCPForwardThatEndsTLSIsARoute() {
        #expect(Self.tailnet(#"{"TCP": {"8443": {"TCPForward": "127.0.0.1:8810", "TerminateTLS": "mac.tail0000.ts.net"}}}"#)
            == .route("mac.tail0000.ts.net:8443"))
        #expect(Self.tailnet(#"{"TCP": {"8443": {"TCPForward": "127.0.0.1:8810"}}}"#) == .nothing)
    }

    @Test func noTailscaleMeansNoTailnetRoute() async {
        #expect(await PairingRoute.tailnet(port: 8810, clis: ["/nonexistent/tailscale"]) == .nothing)
        #expect(Self.tailnet("{}") == .nothing)
        #expect(Self.tailnet("not json") == .nothing)
    }

    @Test func remoteAccessComesFirstThenTheTailnet() {
        #expect(PairingRoute.routes(remote: "oby-abcdefghijklmnop.tetrix.pro", tailscale: "mac.tail0000.ts.net:8443")
            == [.remote("oby-abcdefghijklmnop.tetrix.pro"), .tailscale("mac.tail0000.ts.net:8443")])
        #expect(PairingRoute.routes(remote: nil, tailscale: "mac.tail0000.ts.net:8443") == [.tailscale("mac.tail0000.ts.net:8443")])
        #expect(PairingRoute.routes(remote: nil, tailscale: nil).isEmpty)
        #expect(PairingRoute.tailscale("mac.tail0000.ts.net:8443").label == "Over Tailscale: mac.tail0000.ts.net:8443")
    }

    @Test func theGatewayPortIsObidsOrNoneWhenItHasNoGateway() {
        #expect(PairingRoute.gatewayPort(config: Data(#"{"gateway": "127.0.0.1:9910"}"#.utf8)) == 9910)
        #expect(PairingRoute.gatewayPort(config: nil) == 8810)
        #expect(PairingRoute.gatewayPort(config: Data("{}".utf8)) == nil)
        #expect(PairingRoute.gatewayPort(config: Data(#"{"gateway": "127.0.0.1:70000"}"#.utf8)) == nil)
        #expect(PairingRoute.gatewayPort(config: Data(#"{"gateway": "127.0.0.1:0"}"#.utf8)) == nil)
    }
    /// In an Obysphere the Mac asks obid for no route: it pairs the phone through the Sphere's relay,
    /// with no Tailscale or Remote access (#1377). With a route, its URL, as before.
    @MainActor @Test func inAnObysphereTheInvitationNeedsNoRoute() async throws {
        let invitation = #"{"code":"123456","expiresAt":"2026-10-26T12:02:00Z","hostname":"My Mac","link":"{}","name":"My Mac","secret":"s","url":"https://obysphere.example"}"#
        let obid = RunnerStub { request in
            switch request.path {
            case "/v1/obid/sphere": (200, #"{"state": "joined", "owner": "wellington", "url": "https://obysphere.example", "sphere": "acme", "obiId": "obi-wellington"}"#)
            case "/v1/pairing/invite": (200, invitation)
            default: (404, "{}")
            }
        }
        #expect(await PairingView.inSphere(obid.runner))
        let approval = Approval(device: "mac-1", action: "", signedAt: "2026-10-26T12:00:00Z", signature: "sig")
        let made = try await PairingView.invite(obid.runner, name: "My Mac", route: nil, approval: approval)
        #expect(made.url == "https://obysphere.example")
        let body = try #require(obid.requests.last?.json)
        #expect(body["url"] == nil)
        #expect(body["hostname"] == nil)
        #expect(body["name"] as? String == "My Mac")
        #expect((body["approval"] as? [String: Any])?["device"] as? String == "mac-1")

        _ = try await PairingView.invite(obid.runner, name: "My Mac", route: .tailscale("mac.tail0000.ts.net:8443"), approval: approval)
        #expect(obid.requests.last?.json?["url"] as? String == "https://mac.tail0000.ts.net:8443")
        #expect(obid.requests.last?.json?["hostname"] as? String == "mac.tail0000.ts.net:8443")
    }

    /// Out of an Obysphere obid's own words say what to do, and no runner means no Sphere.
    @MainActor @Test func outOfAnObysphereObidSaysToJoinOne() async throws {
        let obid = RunnerStub { request in
            request.path == "/v1/obid/sphere" ? (200, #"{"state": "none", "owner": "wellington"}"#)
                : (409, #"{"error": "Join or host an Obysphere first (Settings › Obysphere)."}"#)
        }
        #expect(await !PairingView.inSphere(obid.runner))
        #expect(await !PairingView.inSphere(nil))
        let approval = Approval(device: "mac-1", action: "", signedAt: "2026-10-26T12:00:00Z", signature: "sig")
        let refused = await #expect(throws: RunnerError.self) {
            try await PairingView.invite(obid.runner, name: "My Mac", route: nil, approval: approval)
        }
        #expect(refused?.message == "Join or host an Obysphere first (Settings › Obysphere).")
    }
}
