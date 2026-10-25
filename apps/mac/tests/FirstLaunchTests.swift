import Foundation
import Testing
@testable import Oby

/// First launch on obid (#893, docs/onboarding.md): the agents step lists obid's accounts and
/// connects a tool only when it's installed and signed out, the logins already on the Mac are
/// listed without a click, GitHub's step counts the repositories, Home gets a first task, and a new
/// install starts on This Mac (#894).
@MainActor
struct FirstLaunchTests {
    func account(_ id: String, _ agent: String, _ name: String, signedIn: Bool) -> AccountState {
        AccountState(account: Account(id: id, agent: agent, name: name, payer: "", home: nil), label: nil, runnerSignedIn: signedIn)
    }

    @Test func onObidTheAgentsStepListsObidsAccounts() {
        let accounts = [account("default", "claude", "Personal", signedIn: true), account("a1b2c3d4", "claude", "Work", signedIn: true),
                        account("codex", "codex", "Codex", signedIn: true)]
        #expect(FirstLaunchSteps.agent("claude", installed: true, engine: true, accounts: accounts) == .connected(["Personal", "Work"]))
        #expect(FirstLaunchSteps.agent("codex", installed: true, engine: true, accounts: accounts) == .connected(["Codex"]))
        // obid runs a login it has even where the Mac's PATH has no such tool.
        #expect(FirstLaunchSteps.agent("codex", installed: false, engine: true, accounts: accounts) == .connected(["Codex"]))
    }

    @Test func connectIsOnlyForAToolThatsInstalledAndSignedOut() {
        let out = account("e5f6a7b8", "cursor", "Personal", signedIn: false)
        #expect(FirstLaunchSteps.agent("codex", installed: true, engine: true, accounts: []) == .connect)
        #expect(FirstLaunchSteps.agent("codex", installed: false, engine: true, accounts: []) == .missing)
        #expect(FirstLaunchSteps.agent("cursor", installed: true, engine: true, accounts: [out]) == .signIn(out.account))
        #expect(FirstLaunchSteps.agent("cursor", installed: true, cursorHasACP: false, engine: true, accounts: []) == .outdated)
    }

    @Test func theStepWaitsUntilObidAnswers() {
        #expect(FirstLaunchSteps.agent("claude", installed: true, engine: false, accounts: []) == .waiting)
        #expect(FirstLaunchSteps.agent("claude", installed: true, engine: false,
                                       accounts: [account("default", "claude", "Personal", signedIn: true)]) == .waiting)
        #expect(FirstLaunchSteps.agent("claude", installed: false, engine: false, accounts: []) == .missing)
    }

    /// obid's `availableLogins`, for tools it has no account for: routing never moves to a login
    /// the owner didn't pick.
    @Test func loginsAlreadyOnTheMacAreListedOnlyForToolsWithNoAccount() throws {
        let json = #"{"runner": {"version": "0.4.0", "host": "studio"}, "accounts": [], "models": {"claude": {"model": "claude-opus-5-5", "effort": "xhigh"}, "codex": {"model": "gpt-6.1-sol", "effort": "high"}}, "availableLogins": [{"agent": "codex", "id": "codex", "name": "Codex"}, {"agent": "cursor", "id": "cursor", "name": "Cursor"}]}"#
        let found = try #require(try Runner.decoder.decode(RunnerStatus.self, from: Data(json.utf8)).availableLogins)
        #expect(FirstLaunchSteps.adopt(found, accounts: []) == ["codex", "cursor"])
        #expect(FirstLaunchSteps.adopt(found, accounts: [account("e5f6a7b8", "cursor", "Work", signedIn: false)]) == ["codex"])
        let older = #"{"runner": {"version": "0.3.0", "host": "studio"}, "accounts": [], "models": {"claude": {"model": "claude-opus-5-5", "effort": "xhigh"}, "codex": {"model": "gpt-6.1-sol", "effort": "high"}}}"#
        #expect(try Runner.decoder.decode(RunnerStatus.self, from: Data(older.utf8)).availableLogins == nil)
    }

    /// While obid doesn't answer, the steps that need it say why when the login item can't start it.
    @Test func theStepsSayWhyTheEngineDoesntAnswer() {
        #expect(FirstLaunchSteps.waiting(inApplications: false, needsApproval: true, problem: nil) == RunnerLoginItem.moveToApplications)
        #expect(FirstLaunchSteps.waiting(inApplications: true, needsApproval: true, problem: "x").contains("Login Items"))
        #expect(FirstLaunchSteps.waiting(inApplications: true, needsApproval: false, problem: "It failed.") == "Oby’s engine couldn’t start: It failed.")
        #expect(FirstLaunchSteps.waiting(inApplications: true, needsApproval: false, problem: nil) == "Waiting for Oby’s engine on this Mac…")
    }

    @Test func gitHubsStepCountsTheRepositoriesAndHomeGetsAFirstTask() throws {
        let json = #"{"repos": [{"repo": "deskree-inc/oby", "defaultBranch": "dev", "listed": true}, {"repo": "octo-cat/app", "defaultBranch": "main", "listed": false}]}"#
        struct Answer: Decodable { var repos: [RunnerRepo] }
        let repos = try JSONDecoder().decode(Answer.self, from: Data(json.utf8)).repos
        #expect(FirstLaunchSteps.ready(repos) == "2 repositories ready")
        #expect(FirstLaunchSteps.ready(Array(repos.prefix(1))) == "1 repository ready")
        #expect(FirstLaunchSteps.suggestion(repos) == "Find a small bug in oby and fix it")
        #expect(FirstLaunchSteps.suggestion([]) == nil)
    }

    func defaults() throws -> UserDefaults {
        try #require(UserDefaults(suiteName: "oby-first-launch-\(UUID().uuidString)"))
    }

    @Test func thisMacIsTheDefaultOnAFreshInstall() throws {
        let defaults = try defaults()
        RunnerFeed.settleThisMac(defaults)
        #expect(defaults.object(forKey: RunnerFeed.thisMacKey) as? Bool == true)
    }

    @Test func anInstallFollowingARemoteRunnerKeepsIt() throws {
        let defaults = try defaults()
        defaults.set("http://tetrix-lab:8790", forKey: "runnerURL")
        RunnerFeed.settleThisMac(defaults)
        #expect(defaults.object(forKey: RunnerFeed.thisMacKey) as? Bool == false)
    }

    @Test func aChoiceAlreadyMadeIsKept() throws {
        let remote = try defaults()
        remote.set(false, forKey: RunnerFeed.thisMacKey)
        RunnerFeed.settleThisMac(remote)
        #expect(remote.object(forKey: RunnerFeed.thisMacKey) as? Bool == false)
        let here = try defaults()
        here.set(true, forKey: RunnerFeed.thisMacKey)
        here.set("http://tetrix-lab:8790", forKey: "runnerURL")
        RunnerFeed.settleThisMac(here)
        #expect(here.object(forKey: RunnerFeed.thisMacKey) as? Bool == true)
    }
}
