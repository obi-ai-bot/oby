#if DEBUG
import Foundation
import OSLog

/// Debug builds only: launched with `-ObySampleTasks YES`, Home shows three made-up tasks (one
/// running, one that needs you, one done) and two decisions in its stack, so a simulator with no runner jobs can show Sangam's
/// cards and pages. Nothing is sent: a runner's list, when one arrives, replaces them.
/// ponytail: fixed JSON, enough for screenshots; a preview catalog would replace it if one comes.
enum SampleTasks {
    static var on: Bool { UserDefaults.standard.bool(forKey: "ObySampleTasks") }

    static var jobs: [Job] {
        let now = Date.now
        func at(_ minutes: Double) -> String { ISO8601DateFormatter().string(from: now.addingTimeInterval(-minutes * 60)) }
        let stages = { (states: [String]) in
            zip(["Plan", "Work", "Check", "Review", "Merge"], states).map { pair in
                #"{"id": "\#(pair.0.lowercased())", "title": "\#(pair.0)", "state": "\#(pair.1)"}"#
            }.joined(separator: ", ")
        }
        let list = [
            #"{"id": "sample-run", "request": "Make the login screen's empty state say what to do next", "agent": "claude", "account": "default", "repo": "deskree-inc/oby", "branch": "oby/fix-12", "base": "dev", "kind": "fix", "issue": 12, "title": "Login screen empty state", "task": "Fix #12", "status": "running", "step": "Running swift test", "summary": "", "created": "\#(at(12))", "started": "\#(at(12))", "rounds": 4, "round": 2, "check": "swift test", "pipeline": {"stages": [\#(stages(["done", "current", "upcoming", "upcoming", "upcoming"]))], "number": 2, "total": 5}}"#,
            #"{"id": "sample-needs", "request": "Review oby PR #33", "agent": "codex", "account": "default", "repo": "deskree-inc/oby", "branch": "oby/review-33", "base": "dev", "kind": "review", "pr": 33, "title": "Review oby #33", "task": "Review #33", "status": "running", "step": "Waiting for your OK", "summary": "", "created": "\#(at(40))", "started": "\#(at(40))", "permission": {"id": "p1", "tool": "Bash", "title": "Run swift test", "detail": "swift test --filter RunnerTests", "rule": "Bash(swift test:*)"}, "pipeline": {"stages": [\#(stages(["done", "current", "upcoming", "upcoming", "skipped"]))], "number": 2, "total": 5}}"#,
            #"{"id": "sample-done", "request": "Add the About row to Settings", "agent": "claude", "account": "default", "repo": "deskree-inc/oby", "branch": "oby/about", "base": "dev", "kind": "build", "title": "Settings About row", "task": "Add About", "status": "done", "step": "", "summary": "Settings has an About section with the app's version and the Mac it reaches.", "created": "\#(at(36))", "started": "\#(at(36))", "finished": "\#(at(2))", "commits": 2, "published": "https://github.com/deskree-inc/oby/pull/412", "pipeline": {"stages": [\#(stages(["done", "done", "done", "done", "done"]))], "number": 5, "total": 5}, "tetrix": \#(tetrixReport)}"#,
        ]
        return list.compactMap { try? Runner.decoder.decode(Job.self, from: Data($0.utf8)) }
    }

    /// What waits on you, for Home's stack: the review's permission and a question on the fix.
    static var inbox: [InboxItem] {
        let now = ISO8601DateFormatter().string(from: .now.addingTimeInterval(-300))
        return [#"{"kind": "permission", "id": "sample-ask-1", "job": "sample-needs", "title": "Run swift test", "text": "Codex wants to run the tests before it reviews.", "options": ["Allow", "Always allow", "Deny"], "created": "\#(now)", "headline": "Codex wants to run swift test", "happened": "It runs RunnerTests before reviewing #33.", "recommended": 0}"#,
                #"{"kind": "question", "id": "sample-ask-2", "job": "sample-run", "title": "Which copy for the empty state?", "text": "Two ways to say it; which one?", "options": ["Sign in to see your tasks", "Nothing here yet"], "created": "\#(now)", "headline": "Which words for the empty state?", "recommended": 0}"#]
            .compactMap { try? Runner.decoder.decode(InboxItem.self, from: Data($0.utf8)) }
    }

    /// Two made-up accounts, one past 80% of its week, for the message box's usage gauge.
    static var accounts: [Account] {
        let resets = ISO8601DateFormatter().string(from: .now.addingTimeInterval(3 * 86_400))
        return [#"{"id": "personal", "agent": "claude", "name": "Personal", "signedIn": true, "sevenDay": 0.84, "sevenDayResets": "\#(resets)", "running": 1, "tetrix": true, "tetrixJobs": 12, "tetrixUsed": 9}"#,
                #"{"id": "deskree", "agent": "codex", "name": "Deskree", "signedIn": true, "sevenDay": 0.12, "sevenDayResets": "\#(resets)", "running": 0, "tetrix": true, "tetrixJobs": 4, "tetrixUsed": 4}"#]
            .compactMap { try? Runner.decoder.decode(Account.self, from: Data($0.utf8)) }
    }

    /// What the finished sample job's agent did with Tetrix.
    static let tetrixReport = #"{"connected": true, "calls": {"search_entities": 2, "get_entity": 1, "save_session": 1}, "context": true, "saved": ["ses_8f2a"], "gaps": {"noResults": ["deskree-inc/oby#412"], "missing": [], "unknownFiles": ["apps/ios/app/SettingsScreen.swift"], "failed": [], "sent": "2026-10-18T14:40:02Z", "session": "ses_c41d", "notSent": null}}"#

    /// A made-up Obysphere, for the side panel.
    static let sphere = RunnerSphere(state: "joined", sphere: "deskree")
    static let members = ["Ernesto", "Dima", "Sangam"].map(RunnerSphereMember.init)

    /// What the running sample's agent did, for its live card and its own conversation.
    static func transcript(_ job: String) -> [TranscriptEntry] {
        guard job == "sample-run" else { return [] }
        let at = Date.now.addingTimeInterval(-300)
        return [TranscriptEntry(n: 1, at: at, kind: "start", summary: "Started on Claude Code"),
                TranscriptEntry(n: 2, at: at, kind: "you", text: "Make the login screen's empty state say what to do next"),
                TranscriptEntry(n: 3, at: at, kind: "tool", tool: "Read", summary: "Reading LoginView.swift", state: "done", seconds: 0.2, activity: "read", file: "apps/ios/app/LoginView.swift"),
                TranscriptEntry(n: 4, at: at, kind: "thought", state: "done", seconds: 8),
                TranscriptEntry(n: 5, at: at, kind: "said", text: "The empty state says nothing about what to do. I'll give it one line and one button."),
                TranscriptEntry(n: 6, at: at, kind: "plan", todos: [.init(text: "Read how the login screen is built", state: "done"), .init(text: "Write the empty state", state: "running"), .init(text: "Run the tests", state: "pending")]),
                TranscriptEntry(n: 7, at: at, kind: "tool", tool: "Edit", summary: "Editing LoginView.swift", state: "done", seconds: 0.1, added: 12, removed: 2, activity: "edit", file: "apps/ios/app/LoginView.swift"),
                TranscriptEntry(n: 8, at: at, kind: "tool", tool: "Bash", summary: "Running swift test", state: "running", activity: "test")]
    }

    /// Two made-up conversations, for the side panel.
    static var conversations: [RunnerConversation] {
        [RunnerConversation(id: "sample-c1", title: "Why does the login screen look empty?", lastActivity: .now.addingTimeInterval(-120),
                            turn: .init(text: "On it. Claude is making the empty state say what to do next.")),
         RunnerConversation(id: "sample-c2", title: "Where is 0.9 at?", lastActivity: .now.addingTimeInterval(-3600),
                            turn: .init(text: "It went out on Tuesday; the About row came after."))]
    }

    /// `-ObySampleRoute new`, `settings`, `task:<id>`, `agent:<id>`, `kind:<review|issues|tasks|routines>`,
    /// `panel:chats` or `panel:tasks`: opens that, for its screenshots. `-ObySamplePair '<QR code's
    /// JSON>'`: pairs as scanning it does, for a Simulator with no camera, on any debug launch.
    @MainActor static func launch(_ phone: Phone) {
        let defaults = UserDefaults.standard
        if let code = defaults.string(forKey: "ObySamplePair"), phone.link == nil {
            Task {
                do { try await phone.pair(scanned: code); Logger.phone.notice("paired") }
                catch { Logger.phone.error("pairing failed: \(error.localizedDescription, privacy: .public)") }
            }
        }
        // `-ObySampleChoose <account>`: the picker's choice, once the app has come to rest, for a Simulator nobody taps on.
        if let account = defaults.string(forKey: "ObySampleChoose") {
            Task {
                while phone.resting == .deciding { try? await Task.sleep(for: .milliseconds(200)) }
                phone.chooseTalk(account, in: phone.resting.conversation)
            }
        }
        guard let route = defaults.string(forKey: "ObySampleRoute") else { return }
        let kind = [WorkType.review, .issues, .tasks, .routines].first { "kind:\($0.id)" == route }
        if route == "new" { phone.newConversation() }
        else if route == "panel:chats" { phone.panel = .chats }
        else if route == "panel:tasks" { phone.panel = .tasks }
        else if route == "settings" { phone.homePath = [.settings] }
        else if route.hasPrefix("task:") { phone.homePath = [.task(String(route.dropFirst(5)))] }
        else if route.hasPrefix("agent:") { phone.homePath = [.agent(String(route.dropFirst(6)))] }
        else if let kind { phone.homePath = [.kind(kind)] }
    }

    /// With `-ObySampleTalk <seconds>` too: the Oby thread opens with a question of yours, and that
    /// many seconds after launch Oby's answer comes, a word at a time, for a screenshot of it coming in.
    @MainActor static func talk(_ phone: Phone) {
        harness(phone)
        let delay = UserDefaults.standard.double(forKey: "ObySampleTalk")
        guard delay > 0 else { return }
        let model = phone.conversation
        phone.rest(on: .conversation(nil))
        model.apply(Turn(id: "sample-ask", who: .user, text: "Where's the login screen task at?"))
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(delay))
            model.apply(Turn(id: "sample-answer", who: .oby, text: """
                Claude is on it in deskree-inc/oby: the empty state now says what to do next, and it's running \
                `swift test` before the review. Two of five stages are done, and nothing needs you yet. I'll tell \
                you when it opens the pull request, or sooner if it gets stuck on a check.
                """, jobs: ["sample-run"]))
        }
    }

    /// With `-ObySampleHarness <seconds>` too: that many seconds after launch, a turn as the
    /// conversation harness runs it, as obid's events would bring it (through `Phone`'s own event
    /// path): tool lines running, Oby's words streaming in, then its answer with an artifact
    /// (`ArtifactFixture`, served as obid serves it) and the job it started, whose card follows its
    /// `job` events.
    @MainActor static func harness(_ phone: Phone) {
        let delay = UserDefaults.standard.double(forKey: "ObySampleHarness")
        guard delay > 0 else { return }
        phone.rest(on: .conversation(nil))
        let asked = "Make me a page for the login screen's empty state, then fix it in the app."
        phone.sample(.turn(Turn(id: "sample-harness", who: .user, text: asked)))
        func step(_ index: Int, _ tool: String, _ text: String, _ detail: String, _ state: TurnStep.State) {
            phone.sample(.step(TurnStep(turn: "sample-harness", index: index, action: "tool", text: text, state: state, tool: tool, detail: detail)))
        }
        func job(_ step: String) {
            guard var job = phone.jobs.first(where: { $0.id == "sample-run" }) else { return }
            job.step = step
            job.status = .running
            phone.sample(.job(job))
        }
        let answer = """
            I read how the login screen is built, looked up how good empty states are worded, and made a page \
            you can try: it says what to do next and offers the one button that gets you there. Claude is now \
            putting the same words into the app in deskree-inc/oby.
            """
        Task { @MainActor in
            try? await Task.sleep(for: .seconds(delay))
            step(1, "read", "Reading LoginView.swift", "apps/ios/app/LoginView.swift", .running)
            try? await Task.sleep(for: .seconds(1.5))
            step(1, "read", "Reading LoginView.swift", "apps/ios/app/LoginView.swift", .done)
            step(2, "read", "Reading EmptyState.swift", "apps/ios/app/EmptyState.swift", .done)
            step(3, "web_search", "Reading the web", "empty state copy guidelines", .running)
            try? await Task.sleep(for: .seconds(1.5))
            step(3, "web_search", "Reading the web", "empty state copy guidelines", .done)
            step(4, "bash", "Running ls apps/ios/app", "ls apps/ios/app", .done)
            step(5, "write", "Editing index.html", "empty-state/index.html", .done)
            // The job it starts shows at once, under the words (obid's `turn`), before the answer lists it.
            step(6, "oby_dispatch", "Starting a job", "deskree-inc/oby", .running)
            if var started = phone.jobs.first(where: { $0.id == "sample-run" }) {
                started.turn = "sample-harness"
                started.status = .queued
                started.step = "Getting ready"
                phone.sample(.job(started))
            }
            step(6, "oby_dispatch", "Starting a job", "deskree-inc/oby", .done)
            // `-ObySampleHold <seconds>`: it stays a while at work, then halfway through its words, for screenshots.
            let hold = UserDefaults.standard.double(forKey: "ObySampleHold")
            try? await Task.sleep(for: .seconds(4 + hold))
            job("Reading LoginView.swift")
            for (index, word) in WordReveal.words(answer).enumerated() {
                if index == 20 { try? await Task.sleep(for: .seconds(hold)) }
                phone.sample(.delta(TurnDelta(conversation: nil, turn: "sample-harness", text: String(word))))
                try? await Task.sleep(for: .seconds(0.3))
            }
            phone.sample(.artifact(ArtifactEvent(id: ArtifactFixture.sample.id, version: 1, title: ArtifactFixture.sample.title, kind: "html")))
            phone.sample(.turn(Turn(id: "sample-harness-answer", who: .oby, text: answer, jobs: ["sample-run"]),
                               TurnExtras(artifacts: [ArtifactRef(id: ArtifactFixture.sample.id, version: 1)])))
            for line in ["Reading LoginView.swift", "Editing LoginView.swift", "Running swift test"] {
                job(line)
                try? await Task.sleep(for: .seconds(1.5))
            }
        }
    }
}
#endif
