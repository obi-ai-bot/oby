package com.deskree.oby

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.Instant

/** The rules the iPhone's tests hold (apps/ios/tests), on the Android port. */
class LogicTest {
    private fun job(json: String) = wire.decodeFromString(Job.serializer(), json)

    @Test fun anInvitationMustPointAtTheMacItNames() {
        val good = """{"url":"https://mac.tail0000.ts.net:8443","link":{"name":"Wellington's MacBook Pro","hostname":"mac.tail0000.ts.net:8443"},"secret":"s3"}"""
        assertEquals("s3", GatewayEnvelope.decode(good).secret)
        val elsewhere = """{"url":"https://evil.example.com","link":{"name":"Mac","hostname":"mac.tail0000.ts.net:8443"},"secret":"s3"}"""
        assertEquals("That invitation doesn't point at the Mac it names, so Oby won't pair with it.", runCatching { GatewayEnvelope.decode(elsewhere) }.exceptionOrNull()?.message)
        val home = """{"url":"http://192.168.1.20:8810","link":{"name":"Mac","hostname":"oby-abcdefghijklmnop.tetrix.pro"},"secret":"s3"}"""
        assertNotNull(GatewayEnvelope.decode(home))
        val plainAway = """{"url":"http://8.8.8.8:8810","link":{"name":"Mac","hostname":"oby-abcdefghijklmnop.tetrix.pro"},"secret":"s3"}"""
        assertTrue(runCatching { GatewayEnvelope.decode(plainAway) }.isFailure)
        assertTrue(runCatching { GatewayEnvelope.decode("not json") }.isFailure)
    }

    @Test fun hostnamesAreTheControlPlanesOrTailscales() {
        assertTrue(MacLink.isHostname("oby-abcdefghijklmnop.tetrix.pro"))
        assertTrue(MacLink.isHostname("mac.tail0000.ts.net:8443"))
        assertFalse(MacLink.isHostname("oby-short.tetrix.pro"))
        assertFalse(MacLink.isHostname("example.com"))
    }

    @Test fun theCodeIsSixDigits() {
        assertEquals("123456", PairingCode.normalized("123 456"))
        assertEquals("123456", PairingCode.normalized("123-456"))
        assertNull(PairingCode.normalized("12345"))
        assertNull(PairingCode.normalized("12a456"))
    }

    @Test fun eventsSplitOnBlankLinesAndKeepLineSeparatorsInJson() {
        val parser = EventParser()
        val lines = listOf("event: job", """data: {"id":"j1","created":"2026-10-09T21:43:30Z","status":"running","summary":"a b"}""", "")
        val event = lines.firstNotNullOfOrNull { parser.feed(it) } as RunnerEvent.JobChanged
        assertEquals("a b", event.job.summary)
        assertNull(parser.feed(": ping"))
        assertNull(parser.feed(""))
        assertEquals(RunnerEvent.Routine, listOf("event: routine", "data: {}", "").firstNotNullOfOrNull { parser.feed(it) })
    }

    @Test fun aFactThatCantBeReadNeverFailsTheJob() {
        val j = job("""{"id":"j1","created":"2026-10-09T21:43:30.123Z","status":"done","activity":"nonsense","usage":{"input":10,"output":5},"pipeline":{"stages":[{"id":"work","title":"Work","state":"newState"}],"number":1,"total":1}}""")
        assertNull(j.facts.activity)
        assertEquals(15, j.facts.usage?.tokens)
        assertEquals("upcoming", j.stages.single().stateName)
    }

    @Test fun whatNeedsYou() {
        val publish = job("""{"id":"p","created":"2026-10-09T21:43:30Z","status":"done","base":"dev","commits":2,"kind":"fix","checkedHead":"${"a".repeat(40)}"}""")
        assertTrue(publish.canPublish)
        // With no commit its check passed on, obid offers no Publish (job.rs can_publish, PUBLISHING-2): neither does the phone.
        val unchecked = job("""{"id":"q","created":"2026-10-09T21:43:30Z","status":"done","base":"dev","commits":2,"kind":"fix"}""")
        assertFalse(unchecked.canPublish)
        assertTrue(publish.needsYou)
        assertEquals(JobBadge("Ready to publish", Tone.Orange), publish.badge)
        val dismissed = job("""{"id":"p","created":"2026-10-09T21:43:30Z","status":"failed","dismissed":"2026-10-09T21:50:00Z"}""")
        assertFalse(dismissed.needsYou)
        assertEquals("Dismissed", dismissed.badge.text)
    }

    @Test fun homeHasEveryKindAndCountsNeedsOnce() {
        val since = Instant.parse("2026-10-09T00:00:00Z")
        val jobs = listOf(
            job("""{"id":"r","created":"2026-10-08T21:43:30Z","status":"running","kind":"review"}"""),
            job("""{"id":"f","created":"2026-10-08T21:43:30Z","status":"failed","kind":"fix","issue":3}"""),
        )
        val items = PhoneWork.items(jobs, emptyList(), since, emptySet())
        assertEquals(listOf("Review PR", "Issues", "Tasks", "Routines"), WorkList.kinds(items).map { it.label })
        assertEquals(2, WorkList.needsYou(items, loose = 1))
        assertEquals("1 running task · 2 need you", HomeLine.attention(1, 2))
    }

    @Test fun permissionsOfferAlwaysAllowOnlyWithARule() {
        val withRule = CardOption.permission(RunnerPermission("p", title = "Run swift test", rule = "Bash(swift test:*)"))
        assertEquals(listOf("Allow once", "Always allow", "Deny"), withRule.map { it.label })
        assertTrue(withRule.last().destructive)
        assertFalse(withRule.last().safeOnReturn)
        assertEquals(listOf("Allow once", "Deny"), CardOption.permission(RunnerPermission("p", title = "Run")).map { it.label })
    }

    @Test fun theRunnersTurnListsTheEchoById() {
        val echo = Turn(id = "e1", who = Who.user, text = "Hi", conversation = "c1")
        assertTrue(ChatThread.lists(Turn(id = "e1", who = Who.user, text = "Hi", conversation = "c1"), echo))
        assertFalse(ChatThread.lists(Turn(id = "e2", who = Who.user, text = "Hi", conversation = "c1"), echo))
    }

    @Test fun theOwnerComesFromTheMacsName() {
        assertEquals("Wellington", HomeLine.owner("Wellington's MacBook Pro"))
        assertNull(HomeLine.owner("MacBook Pro"))
        assertEquals("What do you want me to work on, Wellington?", HomeLine.placeholder("Wellington"))
    }

    @Test fun wordsComeInOneAtATime() {
        val (shown, _) = WordReveal.shown("one two three", elapsed = 0.075)
        assertEquals("one two ", shown)
    }

    @Test fun plainHttpOnlyToHomeAddresses() {
        listOf("192.168.1.20", "10.0.0.2", "172.20.1.1", "169.254.3.4", "mac.local", "fd12:3456::1", "[fd00::2]", "fe80::1%wlan0")
            .forEach { assertTrue(it, GatewayEnvelope.isLocal(it)) }
        listOf("8.8.8.8", "172.32.0.1", "2001:db8::1", "fec0::1", "fd00::g", "example.com", "1.2.3")
            .forEach { assertFalse(it, GatewayEnvelope.isLocal(it)) }
    }

    @Test fun anApprovalSignsWhatTheRunnerChecks() {
        assertEquals("oby-publish\nj1\nPush it\n2026-10-20T17:51:00Z", String(Approval.message("j1", "Push it", "2026-10-20T17:51:00Z")))
    }

    /** Sangam's TaskMenu (#1714 on the iPhone): Rename, the state's own item, Move and Mode, then Delete task last. */
    @Test fun aTasksMenuIsSangams() {
        val running = job("""{"id":"a1","request":"r","agent":"claude","account":"x","repo":"o/r","status":"running","mode":"bypass","created":"2026-11-01T10:00:00Z"}""")
        assertEquals(listOf(TaskMenu.Item.OpenConversation, TaskMenu.Item.Rename, TaskMenu.Item.Stop, TaskMenu.Item.Move, TaskMenu.Item.Mode, TaskMenu.Item.Delete),
                     TaskMenu.items(running, true))
        val done = job("""{"id":"a2","request":"r","agent":"claude","account":"x","repo":"o/r","status":"done","created":"2026-11-01T10:00:00Z","archived":"2026-11-01T11:00:00Z"}""")
        assertEquals(TaskMenu.Item.Archive, TaskMenu.items(done, false)[1])
        // Archived on any app: off Home, into Archived.
        assertEquals(listOf("a2"), PhoneWork.archived(listOf(running, done), null).map { it.id })
    }

    /** Mode offers Sangam's four, and obid's old Sandbox only while it's the one set. */
    @Test fun modeOffersSangamsFour() {
        assertEquals(listOf(RunnerMode.ask, RunnerMode.acceptEdits, RunnerMode.plan, RunnerMode.bypass), RunnerMode.offered(RunnerMode.bypass))
        assertEquals(RunnerMode.sandbox, RunnerMode.offered(RunnerMode.sandbox).last())
    }

    /** obid's `job` event for a deleted task is a deletion, not a job that fails to decode. */
    @Test fun aDeletedTaskLeaves() {
        val parser = EventParser()
        assertNull(parser.feed("event: job"))
        assertNull(parser.feed("""data: {"id":"a1","deleted":true}"""))
        assertEquals(RunnerEvent.JobDeleted("a1"), parser.feed(""))
    }

    /** A decision's card as the Electron app words it (live.ts itemAsk, Sangam's ASK_LABEL), as the iPhone's #1719. */
    @Test fun cardsSayWhatElectronsSay() {
        fun item(json: String, text: String = "", options: String = "[]") =
            wire.decodeFromString(InboxItem.serializer(), """{"id": "i", "job": "j", "text": "$text", "options": $options, "created": "2026-10-09T21:42:53Z", $json}""")
        val merge = item(""""kind": "merge", "title": "Ready to merge: Fix login"""", text = """Not merged yet: the checks on your Mac failed. Fix them first.\nNobody approved its head.""")
        assertEquals("Ready to merge", PhoneAsk.label(merge))
        assertEquals("The checks on your Mac failed." to "Fix them first.\nNobody approved its head.", PhoneAsk.merge(merge))
        assertEquals("Failed", PhoneAsk.label(item(""""kind": "failure", "title": "x"""")))
        assertEquals("Question", PhoneAsk.label(item(""""kind": "agentQuestion", "title": "x"""")))
        assertFalse(AskKind.failed("agentQuestion"))
        val confirm = item(""""kind": "confirm", "title": "Delete Vault tests", "confirm": {"action": "delete_job", "target": "Vault tests"}""", options = """["Confirm", "Cancel"]""")
        assertEquals("Confirm?", PhoneAsk.label(confirm)); assertNull(PhoneAsk.check(confirm))
        val check = item(""""kind": "confirm", "title": "Use `pnpm test`", "confirm": {"action": "set_check", "target": "acme/app", "payload": {"check": "pnpm test", "was": null}}""", options = """["Confirm", "Cancel"]""")
        assertEquals("Check", PhoneAsk.label(check)); assertEquals("pnpm test", PhoneAsk.check(check))
        assertEquals("Use this as acme/app’s check before publishing?", PhoneAsk.checkTitle(check))
        assertEquals(listOf("Use it" to "Confirm", "No check" to "Cancel"), PhoneAsk.checkOptions(check).map { it.label to it.answer })
        assertEquals(listOf(false, false), PhoneAsk.checkOptions(check).map { it.recommended })
        // A payload with more than text (a snapshot, a number) keeps its text fields.
        val more = item(""""kind": "confirm", "title": "Continue Vault tests", "confirm": {"action": "continue_job", "target": "j", "payload": {"words": "Go on", "snapshot": {"round": 2}, "n": 3}}""")
        assertEquals("Go on", more.confirmValue?.words)
        assertEquals("Keep it", PhoneAsk.checkOptions(item(""""kind": "confirm", "title": "t", "confirm": {"action": "set_check", "target": "a/b", "payload": {"check": "make test", "was": "npm test"}}""")).last().label)
    }

    /** Settings › Agents words an account's usage as Electron's `accountRow` and `weekWords` (the iPhone's AgentUsage). */
    @Test fun agentUsageSaysWhatElectronSays() {
        val now = Instant.parse("2026-11-01T12:00:00Z")
        fun account(json: String) = wire.decodeFromString(Account.serializer(), """{"id":"a","name":"Personal",$json}""")
        val claude = AgentUsage(account(""""agent":"claude","signedIn":true,"fiveHour":0.42,"sevenDay":0.3,"fiveHourResets":"2026-11-01T14:00:00Z","sevenDayResets":"2026-11-05T09:00:00Z""""), now)
        assertEquals(42, claude.session)
        assertNotNull(claude.sessionResets)
        assertTrue(claude.week, claude.week.startsWith("This week 30% · resets "))
        val past = AgentUsage(account(""""agent":"claude","signedIn":true,"fiveHour":0.9,"fiveHourResets":"2026-11-01T11:00:00Z""""), now)
        assertEquals(0, past.session); assertNull(past.sessionResets); assertEquals("No weekly use reported yet", past.week)
        assertEquals("Cursor doesn’t report usage", AgentUsage(account(""""agent":"cursor","signedIn":true"""), now).week)
        assertEquals("Not signed in", AgentUsage(account(""""agent":"codex","signedIn":false"""), now).week)
        // A model the account chose that the tool's list doesn't have stays first.
        assertEquals(listOf("x" to "x", "a" to "A"), choices(listOf("a" to "A"), "x"))
        assertEquals(listOf("a" to "A"), choices(listOf("a" to "A"), "a"))
    }

    /** A push with no pull request is out, as obid's `Job::unpublished` says (#1763): nothing to publish, so Home doesn't say "Needs you · Failed". */
    @Test fun aPushWithNoPullRequestIsOut() {
        fun done(extra: String) = job("""{"id":"j","request":"r","agent":"claude","account":"a","repo":"o/r","base":"main","kind":"build","status":"done","created":"2026-11-01T10:00:00Z",$extra}""")
        // obid's shape for a push with no pull request: published null, publishError null, pushed equal to commits.
        val pushed = done(""""commits":1,"pushed":1""")
        assertEquals(0, pushed.unpublished)
        assertFalse(pushed.canPublish); assertFalse(pushed.needsYou)
        // Its publish failed: the commit isn't out, and it waits for Publish.
        val failed = done(""""commits":1,"pushed":1,"publishError":"push refused","checkedHead":"abc"""")
        assertEquals(1, failed.unpublished); assertTrue(failed.canPublish)
        // Published, and a commit since: that one is still to go.
        assertEquals(1, done(""""commits":2,"pushed":1,"published":"https://github.com/o/r/pull/1"""").unpublished)
        // Never pushed: everything is to go.
        assertEquals(1, done(""""commits":1""").unpublished)
    }
}
