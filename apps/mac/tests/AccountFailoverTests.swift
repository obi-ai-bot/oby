import Foundation
import Testing
@testable import Oby

/// Settings › Accounts' switch for moving work past an account's usage limit (the owner's decision
/// of 2026-10-24, #871): read from `GET /v1/settings/jobs`, set with a `PUT` of only
/// `accountFailover`; a runner without the setting shows no switch.
struct AccountFailoverTests {
    @Test func theSwitchReadsAndSetsTheRunnersSetting() async throws {
        let stub = RunnerStub { request in
            switch (request.method, request.path) {
            case ("GET", "/v1/settings/jobs"):
                return (200, #"{"maxRunning": 6, "accountFailover": true}"#)
            case ("PUT", "/v1/settings/jobs"):
                let on = request.json?["accountFailover"] as? Bool ?? true
                return (200, #"{"maxRunning": 6, "accountFailover": \#(on)}"#)
            default:
                return (404, #"{"error": "no"}"#)
            }
        }
        #expect(try await stub.runner.jobSettings().accountFailover == true)
        #expect(try await stub.runner.setAccountFailover(false).accountFailover == false)
        let put = try #require(stub.requests.last)
        #expect(put.method == "PUT")
        // Only the switch: the cap stays as the runner has it.
        #expect(put.json?.keys.sorted() == ["accountFailover"])
        #expect(put.json?["accountFailover"] as? Bool == false)
    }

    /// An older runner, or the Swift one, answers no `accountFailover`: nil, so no switch shows.
    @Test func aRunnerWithoutTheSettingShowsNoSwitch() async throws {
        let stub = RunnerStub { _ in (200, #"{"maxRunning": 6}"#) }
        let settings = try await stub.runner.jobSettings()
        #expect(settings.accountFailover == nil)
        #expect(settings.maxRunning == 6)
    }

    @Test func itSaysWhatItDoes() {
        #expect(AccountsSettingsView.failoverAbout.contains("same tool"))
        #expect(AccountsSettingsView.failoverAbout.contains("by name stays on it"))
    }
}
