import Foundation
import Testing
@testable import Oby

/// docs/contracts/ios-new-oby.md, each test named after its scenario.
@MainActor
struct NewObyTests {
    private let now = Date(timeIntervalSince1970: 1_800_000_000)

    private func conversation(_ id: String, minutesAgo: Double, pinned: Bool = false) -> RunnerConversation {
        RunnerConversation(id: id, title: id, pinned: pinned, lastActivity: now.addingTimeInterval(-minutesAgo * 60))
    }

    private func account(_ id: String, agent: String = "claude", signedIn: Bool = true, week: Double? = nil, session: Double? = nil,
                         sessionResets: Date? = nil, fullUntil: Date? = nil) -> Account {
        Account(id: id, agent: agent, name: id, signedIn: signedIn, fiveHour: session, sevenDay: week, fiveHourResets: sessionResets, running: 0, fullUntil: fullUntil)
    }

    private func job(_ extra: String = "", status: String = "running", step: String = "Working") throws -> Job {
        let json = #"{"id": "j", "request": "Fix it", "agent": "claude", "account": "a", "repo": "acme/app", "branch": "b", "kind": "fix", "task": "Fix it", "status": "\#(status)", "step": "\#(step)", "summary": "", "created": "2026-11-03T10:00:00Z"\#(extra)}"#
        return try Runner.decoder.decode(Job.self, from: Data(json.utf8))
    }

    private func tool(_ n: Int, _ summary: String, state: String = "done", tool: String = "Read") -> TranscriptEntry {
        TranscriptEntry(n: n, kind: "tool", tool: tool, summary: summary, state: state)
    }

    private func item(_ kind: String, id: String, job: String = "j", offerIn: String? = nil) throws -> InboxItem {
        let offer = offerIn.map { #", "conversation": "\#($0)", "offer": {"id": "\#(id)", "what": "Fix it", "task": "Fix it", "untrusted": false}"# } ?? ""
        let json = #"{"kind": "\#(kind)", "id": "\#(id)", "job": "\#(job)", "title": "t", "text": "x", "options": [], "created": "2026-11-03T10:00:00Z"\#(offer)}"#
        return try Runner.decoder.decode(InboxItem.self, from: Data(json.utf8))
    }

    // MARK: Chat first

    @Test func iosnew_1_it_rests_on_what_moved_last_and_is_decided_once() {
        #expect(PhoneWork.resting([conversation("old", minutesAgo: 90), conversation("new", minutesAgo: 1)]) == .conversation("new"))
        // A pinned one that hasn't moved doesn't count as having moved.
        #expect(PhoneWork.resting([conversation("pinned", minutesAgo: 90, pinned: true), conversation("new", minutesAgo: 1)]) == .conversation("new"))
        #expect(PhoneWork.resting([]) == .empty)
        #expect(PhoneWork.resting(nil) == .conversation(nil))
        // Its stage shows only with nothing to rest on.
        #expect(Resting.empty.showsStage && Resting.deciding.showsStage)
        #expect(!Resting.starting.showsStage && !Resting.conversation("a").showsStage)
    }

    private func newPhone() throws -> Phone {
        Phone(offlineQueue: OfflineQueue(defaults: try #require(UserDefaults(suiteName: "NewObyTests.\(UUID())"))))
    }

    @Test func iosnew_1_a_conversation_that_arrives_later_doesnt_move_it() throws {
        let phone = try newPhone()
        #expect(phone.resting == .deciding)
        phone.sample(conversations: [conversation("old", minutesAgo: 90), conversation("new", minutesAgo: 1)])
        #expect(phone.resting == .conversation("new"))
        phone.sample(.conversation(conversation("newer", minutesAgo: 0)))
        #expect(phone.resting == .conversation("new"))
        // Listed again, as every connection does: still decided.
        phone.sample(conversations: [conversation("old", minutesAgo: 90), conversation("new", minutesAgo: 1), conversation("newer", minutesAgo: 0)])
        #expect(phone.resting == .conversation("new"))
    }

    @Test func iosnew_2_new_conversation_creates_nothing_and_a_deleted_one_goes_to_the_latest() throws {
        let phone = try newPhone()
        phone.sample(conversations: [conversation("a", minutesAgo: 1), conversation("b", minutesAgo: 9)])
        phone.homePath = [.settings]
        phone.newConversation()
        #expect(phone.resting == .empty)
        #expect(phone.focusHome && phone.homePath.isEmpty)
        #expect(phone.conversations?.count == 2)
        phone.rest(on: .conversation("a"))
        phone.sample(.conversation(RunnerConversation(id: "a", title: "", deleted: true)))
        #expect(phone.resting == .conversation("b"))
        // Another one going doesn't move you, and the last one going leaves the empty chat.
        phone.sample(.conversation(conversation("c", minutesAgo: 0)))
        phone.sample(.conversation(RunnerConversation(id: "c", title: "", deleted: true)))
        #expect(phone.resting == .conversation("b"))
        phone.sample(.conversation(RunnerConversation(id: "b", title: "", deleted: true)))
        #expect(phone.resting == .empty)
    }

    /// Words on the empty chat: obid's name for their conversation is adopted in the same page.
    @Test func iosnew_2_words_start_a_conversation_and_its_name_is_adopted() throws {
        let phone = try newPhone()
        phone.sample(connection: .live)
        phone.sample(conversations: [])
        #expect(phone.resting == .empty)
        phone.ask("Fix the route", new: true)
        #expect(phone.resting == .starting && phone.startingNew)
        let moves = phone.restMoves
        // Named as its turn begins, before the request (no runner here) can answer.
        phone.sample(.conversation(RunnerConversation(id: "named", title: "Fix the route")))
        #expect(phone.resting == .conversation("named"))
        #expect(phone.current == "named" && !phone.startingNew)
        #expect(phone.restMoves == moves)
    }

    /// Before obid's list is in, words still start their own conversation, never its latest.
    @Test func iosnew_2_words_before_the_list_start_their_own_conversation() throws {
        let queue = OfflineQueue(defaults: try #require(UserDefaults(suiteName: "NewObyTests.\(UUID())")))
        let phone = Phone(offlineQueue: queue)
        #expect(phone.resting == .deciding && phone.conversations == nil)
        phone.ask("Fix the route", new: true)
        #expect(phone.resting == .starting)
        #expect(queue.messages.first?.new == true)
        // The list that comes later doesn't move it.
        phone.sample(conversations: [conversation("old", minutesAgo: 1)])
        #expect(phone.resting == .starting)
    }

    @Test func iosnew_2_typing_before_the_list_keeps_the_box() throws {
        let phone = try newPhone()
        phone.settleOnEmpty()
        #expect(phone.resting == .empty)
        phone.sample(conversations: [conversation("a", minutesAgo: 1)])
        #expect(phone.resting == .empty)
        // Once it has come to rest, it means nothing.
        let settled = try newPhone()
        settled.sample(conversations: [conversation("a", minutesAgo: 1)])
        settled.settleOnEmpty()
        #expect(settled.resting == .conversation("a"))
    }

    @Test func iosnew_3_the_panel_lists_pinned_first_then_five_and_the_one_youre_in() {
        let list = (1...8).map { conversation("c\($0)", minutesAgo: Double($0)) } + [conversation("p", minutesAgo: 100, pinned: true)]
        let five = PhoneWork.chats(list, resting: "c1", all: false)
        #expect(five.rows.map(\.id) == ["p", "c1", "c2", "c3", "c4"])
        #expect(five.hidden == 4)
        let inside = PhoneWork.chats(list, resting: "c8", all: false)
        #expect(inside.rows.map(\.id) == ["p", "c1", "c2", "c3", "c4", "c8"])
        #expect(inside.hidden == 3)
        let all = PhoneWork.chats(list, resting: nil, all: true)
        #expect(all.rows.count == 9 && all.hidden == 0)
    }

    @Test func iosnew_4_only_a_sphere_obid_is_in_shows_and_only_teammates() throws {
        let joined = #"{"state": "joined", "url": "http://127.0.0.1:8787", "sphere": "e2e", "owner": "wellington", "card": {"a": 1}, "tokenExpiresAt": "2026-10-14T13:00:05Z"}"#
        let sphere = try Runner.decoder.decode(RunnerSphere.self, from: Data(joined.utf8))
        #expect(sphere == RunnerSphere(state: "joined", sphere: "e2e", owner: "wellington"))
        let listed = #"[{"obi": "obi-1", "owner": "ernesto", "accepts": ["question"], "sealed": true, "verified": false}, {"obi": "obi-2", "owner": "ernesto"}, {"obi": "obi-3", "owner": "wellington"}]"#
        let members = try Runner.decoder.decode([RunnerSphereMember].self, from: Data(listed.utf8))
        let phone = try newPhone()
        phone.take(sphere: sphere, members: members)
        #expect(phone.sphere?.sphere == "e2e")
        // Your own other Mac isn't a teammate; two Obys of one owner are two rows.
        #expect(phone.sphereMembers.map(\.obi) == ["obi-1", "obi-2"])
        // Members that couldn't be read keep what was read.
        phone.take(sphere: sphere, members: nil)
        #expect(phone.sphereMembers.count == 2)
        // One obid isn't in shows nothing, and takes its people with it.
        for state in ["revoked", "none"] {
            phone.take(sphere: RunnerSphere(state: state, sphere: "e2e"), members: members)
            #expect(phone.sphere == nil && phone.sphereMembers.isEmpty)
        }
    }

    // MARK: Who Oby talks through

    @Test func iosnew_5_each_accounts_percentage_and_dot() {
        #expect(TalkChoice.percent(account("a", week: 0.36, session: 0.86), now: now) == 86)
        #expect(TalkChoice.percent(account("a", week: 0.36, session: 0.12), now: now) == 36)
        // Past its reset, the session counts nothing.
        #expect(TalkChoice.percent(account("a", week: 0.2, session: 0.9, sessionResets: now.addingTimeInterval(-60)), now: now) == 20)
        // With no week to go by too: its figure is over.
        #expect(TalkChoice.percent(account("a", session: 0.9, sessionResets: now.addingTimeInterval(-60)), now: now) == 0)
        #expect(TalkChoice.percent(account("a", session: 0.9), now: now) == 90)
        #expect(TalkChoice.usageWords(account("a", week: 0.36), now: now) == "36%")
        #expect(TalkChoice.usageWords(account("a", week: 0.36, fullUntil: now.addingTimeInterval(60)), now: now) == "Full")
        #expect(TalkChoice.usageWords(account("c", agent: "cursor", week: 0.5), now: now) == "No usage")
        #expect(TalkChoice.usageWords(account("a"), now: now) == "No usage")
        #expect(TalkChoice.dot(account("a", week: 0.8), now: now) == .none)
        #expect(TalkChoice.dot(account("a", week: 0.81), now: now) == .high)
        #expect(TalkChoice.dot(account("a", week: 0.91), now: now) == .over)
        #expect(TalkChoice.dot(account("a", week: 0.1, fullUntil: now.addingTimeInterval(60)), now: now) == .over)
        #expect(TalkChoice.dot(account("a", week: 0.1, fullUntil: now.addingTimeInterval(-60)), now: now) == .none)
    }

    @Test func iosnew_6_the_picker_names_the_right_account() {
        let a = account("a"), b = account("b", agent: "codex")
        let held = account("h", fullUntil: now.addingTimeInterval(60))
        let accounts = [a, b, held]
        func named(_ chosen: String?, talkedOn: String? = nil, next: String? = nil, movedOff: String? = nil) -> String? {
            TalkChoice.shown(accounts, chosen: chosen, talkedOn: talkedOn, next: next, movedOff: movedOff, now: now)?.id
        }
        #expect(named("b") == "b")
        // Moved off it, or it can't run: where it last talked.
        #expect(named("b", talkedOn: "a", movedOff: "b") == "a")
        #expect(named("h", talkedOn: "b") == "b")
        // Auto: where it last talked, else what obid says is next, each only while it can run.
        #expect(named(nil, talkedOn: "b", next: "a") == "b")
        #expect(named(nil, next: "b") == "b")
        #expect(named(nil, talkedOn: "h", next: "b") == "b")
        #expect(named(nil) == "a")
        // None can run: where it last talked.
        #expect(TalkChoice.shown([held], chosen: nil, talkedOn: "h", next: nil, movedOff: nil, now: now)?.id == "h")
        // A conversation's own choice beats the setting; `auto` is Auto whatever the setting says.
        #expect(TalkChoice.chosen(own: "b", setting: "a") == "b")
        #expect(TalkChoice.chosen(own: nil, setting: "a") == "a")
        #expect(TalkChoice.chosen(own: "auto", setting: "a") == nil)
        #expect(TalkChoice.chosen(own: nil, setting: nil) == nil)
    }

    @Test func iosnew_7_the_list_has_every_connected_account_by_tool() {
        var work = account("w")
        work.email = "doti@deskree.com"
        work.org = "Deskree"
        work.plan = "team"
        let accounts = [account("c", agent: "cursor"), account("x", agent: "codex"), work, account("p"), account("out", signedIn: false)]
        #expect(TalkChoice.list(accounts).map(\.id) == ["w", "p", "x", "c"])
        #expect(TalkChoice.rowTitle(work, among: accounts) == "Claude · w")
        #expect(TalkChoice.rowTitle(accounts[1], among: accounts) == "Codex")
        #expect(TalkChoice.who(work) == "doti@deskree.com · Deskree · Team")
        #expect(TalkChoice.who(accounts[1]) == "Codex")
    }

    @Test func iosnew_8_a_choice_goes_where_it_was_made_and_a_refusal_goes_back() async throws {
        let phone = try newPhone()
        phone.sample(conversations: [conversation("c", minutesAgo: 1)])
        // In a conversation it's that conversation's, named at once; the setting stays.
        phone.chooseTalk("b", in: "c")
        #expect(phone.conversations?.first?.talkAccount == "b")
        #expect(phone.talkSetting == nil)
        // No runner to take it: it goes back and says why.
        for _ in 0..<50 where phone.talkProblem == nil { try await Task.sleep(for: .milliseconds(100)) }
        #expect(phone.talkProblem != nil)
        #expect(phone.conversations?.first?.talkAccount == nil)
        // On the empty chat it's the setting.
        phone.chooseTalk("b", in: nil)
        #expect(phone.talkSetting == "b")
        #expect(phone.conversations?.first?.talkAccount == nil)
        phone.talkProblem = nil
        for _ in 0..<50 where phone.talkProblem == nil { try await Task.sleep(for: .milliseconds(100)) }
        #expect(phone.talkSetting == nil)
    }

    // MARK: A task's card and its agent's conversation

    @Test func iosnew_9_a_running_cards_list_and_steps() throws {
        let older = TranscriptEntry(n: 2, kind: "plan", todos: [.init(text: "old", state: "running")])
        let newer = TranscriptEntry(n: 5, kind: "plan", todos: [.init(text: "a", state: "done"), .init(text: "b", state: "running")])
        let entries = [TranscriptEntry(n: 1, kind: "you", text: "Fix it"), older, tool(3, "Reading a.ts"), tool(4, "Reading b.ts"), newer,
                       TranscriptEntry(n: 6, kind: "said", text: "On it"), tool(7, "Running ls", state: "running", tool: "Bash")]
        #expect(LiveTask.todos(entries)?.map(\.text) == ["a", "b"])
        #expect(LiveTask.todos([tool(1, "Reading a.ts")]) == nil)
        #expect(LiveTask.lastSteps(entries).map(\.n) == [4, 6, 7])
        let long = (0..<12).map { TranscriptEntry.Todo(text: "\($0)", state: $0 < 6 ? "done" : $0 == 6 ? "running" : "pending") }
        let shown = LiveTask.around(long)
        #expect(shown.todos.count == 8 && shown.before == 4 && shown.after == 0)
        #expect(shown.todos.contains { $0.state == "running" })
        let short = LiveTask.around(Array(long.prefix(3)))
        #expect(short.todos.count == 3 && short.before == 0 && short.after == 0)
        // With no steps and no list: what obid says it's on; past its agent's work: the stage.
        #expect(LiveTask.now(try job(step: "Getting ready"), entries: []) == "Getting ready")
        let stages = #", "pipeline": {"stages": [{"id": "work", "title": "Work", "state": "done"}, {"id": "check", "title": "Check", "state": "current"}], "number": 2, "total": 2}"#
        #expect(LiveTask.now(try job(stages, step: "Reading a.ts"), entries: [tool(1, "Reading a.ts")]) == "Check")
        // A step its session already shows isn't said again.
        #expect(LiveTask.now(try job(step: "Reading a.ts"), entries: [tool(1, "Reading a.ts")]) == nil)
        // Older than the card's last three steps too.
        let more = [tool(1, "Reading a.ts"), tool(2, "b"), tool(3, "c"), tool(4, "d")]
        #expect(LiveTask.now(try job(step: "Reading a.ts"), entries: more) == nil)
    }

    @Test func iosnew_10_only_what_a_tool_gave_and_steps_read_as_they_stand() {
        #expect(LiveTask.thought(TranscriptEntry(n: 1, kind: "thought", state: "running")) == "Thinking")
        #expect(LiveTask.thought(TranscriptEntry(n: 1, kind: "thought", state: "done", seconds: 8)) == "Thought for 8s")
        #expect(LiveTask.thought(TranscriptEntry(n: 1, kind: "thought", text: "Its words", state: "done", seconds: 8)) == "Its words")
        #expect(LiveTask.thought(TranscriptEntry(n: 1, kind: "thought", state: "done")) == "Thought")
        #expect(tool(1, "Reading a.ts", state: "running").verbAndTarget.verb == "Reading a.ts")
        #expect(tool(1, "Reading a.ts").verbAndTarget.verb == "Read a.ts")
        #expect(tool(1, "Running ls", tool: "Bash").verbAndTarget == ("Ran", "ls"))
        let refused = tool(1, "Running rm", state: "declined", tool: "Bash")
        #expect(refused.verbAndTarget.verb == "Running")
        #expect(refused.meta == "Declined")
    }

    @Test func iosnew_11_an_ended_card_says_how_and_why() throws {
        #expect(LiveTask.ended(try job(status: "done")) == "Done")
        #expect(LiveTask.ended(try job(#", "nothingDone": true"#, status: "done")) == "Nothing done")
        #expect(LiveTask.ended(try job(status: "failed")) == "Failed")
        #expect(LiveTask.ended(try job(status: "stopped")) == "Stopped")
        #expect(LiveTask.ended(try job(status: "interrupted")) == "Stopped")
        #expect(LiveTask.why(try job(#", "reason": "Oby restarted\nand more""#, status: "interrupted")) == "Oby restarted")
        #expect(LiveTask.why(try job(#", "reason": "Why""#, status: "done")) == nil)
        let meta = LiveTask.meta(try job(status: "stopped"), mark: .done, account: "Personal", now: now)
        #expect(meta == "Stopped · Claude Code · Personal · app")
    }

    @Test func iosnew_12_a_session_fills_its_holes_and_nothing_undoes_an_event() async throws {
        #expect(LiveSession.knownUpTo([]) == nil)
        #expect(LiveSession.knownUpTo([tool(1, "a"), tool(2, "b"), tool(5, "c")]) == 2)
        #expect(LiveSession.knownUpTo([tool(3, "a")]) == nil)
        #expect(LiveSession.knownUpTo([tool(3, "a"), tool(4, "b")], trimmed: true) == 4)
        // An ended call stays ended; words stay as far as they got.
        let late = LiveSession.kept([tool(1, "a")], adding: [tool(1, "a", state: "running")]).entries
        #expect(late[0].state == "done")
        let said = TranscriptEntry(n: 2, kind: "said", text: "All of it")
        let partial = TranscriptEntry(n: 2, kind: "said", text: "All")
        #expect(LiveSession.kept([said], adding: [partial]).entries[0].text == "All of it")
        // A session goes to the job it's for; one held twice stays until both let go.
        let phone = try newPhone()
        let first = phone.hold(session: "j")
        let again = phone.hold(session: "j")
        #expect(first === again)
        phone.letGo(session: "j")
        #expect(phone.hold(session: "j") === first)
        phone.letGo(session: "j")
        phone.letGo(session: "j")
        #expect(phone.hold(session: "j") !== first)
    }

    /// `catchUp` asks from the last entry with none missing before it, so a lost event comes too.
    @Test func iosnew_12_a_read_starts_where_the_entries_have_no_hole() async throws {
        var asked: [Int?] = []
        let session = LiveSession(job: "j", load: { after in
            asked.append(after)
            return TranscriptPage(entries: after == 2 ? [self.tool(3, "lost")] : [])
        }, loadDetail: { _ in TranscriptDetail() })
        for n in [1, 2, 5] { session.apply(TranscriptEvent(job: "j", entry: tool(n, "\(n)"))) }
        await session.catchUp()
        #expect(asked == [2])
        #expect(session.entries.map(\.n) == [1, 2, 3, 5])
    }

    /// Each job's `transcript` events go to its own session, and only to one something holds.
    @Test func iosnew_12_events_go_to_their_jobs_session() throws {
        let phone = try newPhone()
        let mine = phone.hold(session: "j")
        let other = phone.hold(session: "k")
        phone.sample(.transcript(TranscriptEvent(job: "j", entry: tool(1, "x"))))
        #expect(mine.entries.map(\.n) == [1])
        #expect(other.entries.isEmpty)
        // Let go, it hears nothing more.
        phone.letGo(session: "j")
        phone.sample(.transcript(TranscriptEvent(job: "j", entry: tool(2, "y"))))
        #expect(mine.entries.map(\.n) == [1])
    }

    // MARK: Recovery

    @Test func iosnew_13_a_job_being_tried_again_stays_running() throws {
        let trying = try job(step: "Trying again in 5 minutes (2 of 3)")
        #expect(ThreadTaskLine.mark(trying) == .working)
        #expect(!trying.needsYou && trying.outcome == nil)
        #expect(trying.line == "Trying again in 5 minutes (2 of 3)")
        #expect(PhoneWork.items([trying], since: now, seen: [])[0].status == .running)
        // It isn't one of its steps, so it shows even with steps above it.
        let plan = TranscriptEntry(n: 2, kind: "plan", todos: [.init(text: "a", state: "running")])
        #expect(LiveTask.now(trying, entries: [tool(1, "Reading a.ts"), plan]) == "Trying again in 5 minutes (2 of 3)")
        // A restart's line, past the third try's error, the same way.
        #expect(LiveTask.now(try job(step: "Oby restarted while this ran: continuing it (1 of 3)"), entries: [tool(1, "Reading a.ts")]) != nil)
        // Past its tries: it needs you, with the real reason.
        let spent = try job(#", "reason": "API Error: 529""#, status: "interrupted", step: "")
        #expect(spent.needsYou)
        #expect(ThreadTaskLine.mark(spent) == .needs)
        #expect(LiveTask.why(spent) == "API Error: 529")
    }

    // MARK: Add-only

    @Test func iosnew_14_keys_it_doesnt_know_dont_fail_it() throws {
        let conversation = try Runner.decoder.decode(RunnerConversation.self, from: Data(#"{"id": "c", "title": "T", "talkAccount": "a", "talkedOn": "b", "movedOff": "a", "someNewKey": {"x": 1}}"#.utf8))
        #expect(conversation.talkAccount == "a" && conversation.talkedOn == "b" && conversation.movedOff == "a")
        // A value of a shape it doesn't expect loses that key, never the conversation.
        let odd = try Runner.decoder.decode(RunnerConversation.self, from: Data(#"{"id": "c", "title": "T", "talkAccount": {"x": 1}}"#.utf8))
        #expect(odd.talkAccount == nil)
        let account = try Runner.decoder.decode(Account.self, from: Data(#"{"id": "a", "agent": "claude", "name": "n", "signedIn": true, "running": 0, "email": "e@x", "org": "o", "plan": "max", "someNewKey": 1}"#.utf8))
        #expect(account.email == "e@x" && account.org == "o" && account.plan == "max")
        #expect(try job(#", "someNewKey": [1, 2], "nothingDone": true"#, status: "done").nothingDone == true)
        let status = #"{"runner": {"version": "1", "host": "h", "new": 1}, "accounts": [], "models": {"claude": {"model": "m", "effort": "e"}, "codex": {"model": "m", "effort": "e"}}, "talkAccount": "a", "talkNext": "b", "someNewKey": true}"#
        let read = try Runner.decoder.decode(RunnerStatus.self, from: Data(status.utf8))
        #expect(read.talkAccount == "a" && read.talkNext == "b")
    }

    // MARK: What Oby asks

    @Test func iosnew_15_the_stack_shows_each_ask_once() throws {
        let question = try item("question", id: "q")
        let failed = try item("failed", id: "f")
        let here = try item("start", id: "s1", offerIn: "c1")
        let elsewhere = try item("start", id: "s2", offerIn: "c2")
        let asks = PhoneAsks.over([question, failed, here, elsewhere], in: "c1").map(\.id)
        #expect(asks.contains("q"))
        #expect(!asks.contains("f"))
        #expect(!asks.contains("s1"))
        #expect(PhoneAsks.over([here], in: nil).map(\.id) == ["s1"])
    }
}
