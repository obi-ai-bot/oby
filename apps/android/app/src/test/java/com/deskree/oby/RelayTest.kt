package com.deskree.oby

import kotlinx.coroutines.flow.take
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okio.ByteString
import okio.ByteString.Companion.toByteString
import org.junit.After
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test
import java.net.InetAddress
import java.security.MessageDigest
import java.util.Base64
import java.util.concurrent.CopyOnWriteArrayList
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

/** The relay (Relay.kt) as apps/ios/tests/RelayTests.swift proves the iPhone's: the docs' known answer, then a fake Sphere and Mac. */
class RelayTest {
    private fun hex(text: String) = text.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
    private fun bytes(from: Int) = ByteArray(32) { (from + it).toByte() }
    private fun b64(bytes: ByteArray) = Base64.getUrlEncoder().withoutPadding().encodeToString(bytes)

    @Test fun theEnvelopeGivesTheDocsKnownAnswer() {
        val mac = bytes(0)
        val s = RelayEnvelope.publicOf(mac)
        assertArrayEquals(hex("8f40c5adb68f25624ae5b214ea767a6ec94d829d3d7b5e1ad1ba6f3e2138285f"), s)
        val hello = RelayEnvelope.Hello(s, bytes(32))
        assertArrayEquals(hex("0101358072d6365880d1aeea329adf9121383851ed21a28e3b75e965d0d2cd166254a2b39ce6bc8467ffdd094cfa4ea2258b59"), hello.bytes)
        val (welcome, macSession) = FakeMac.welcome(mac, hello.bytes, bytes(64))
        assertArrayEquals(hex("010279a631eede1bf9c98f12032cdeadd0e7a079398fc786b88cc846ec89af85a51a0944396b30ca3b9834e5d232b1f803e05b"), welcome)
        val session = hello.welcomed(welcome)
        val head = """{"method":"GET","path":"/v1/status","headers":{"authorization":"Bearer test-device-token"}}""".toByteArray()
        val request = session.seal(RelayFrame(1, RelayFrame.Kind.Open, head).encoded)
        assertArrayEquals(hex("01030000000000000000f5727141453120bc5930b45fc6041dd63ff38849669a6597b32e0126a0a47e5b8fe24aa92a12c8cb0b0d802d00b9b406da400b17143ce5eb2c2ec1d382aea99104e3927783310d114581bd85e058d1ebfe3852a2b3daf14baac186d5bb8a9459923c8d5f80fc14bc380ab04ace824f10"), request)
        assertTrue(macSession.open(request) is RelayEnvelope.Opened.Frame)
        val answer = listOf(
            "010300000000000000000a35fcb1ed7cde7ec716ca66cb4bfe2f07395d39e87ad3465de5430d568548a6ff91920033e8ba5f0286bafa8e7381285da738f3788c0b3889dd22cf8b188f542334f708ec77c3267f435ac2e4c58b4d64",
            "0103000000000000000104872ef5d6738d2ec970fec24a408375a3ad546947fe63f62b4aac3868fe9137",
            "010300000000000000020715d08c652dc9cad1f99f67006dbe715414875ed5",
        ).map { (session.open(hex(it)) as RelayEnvelope.Opened.Frame).plain.let(RelayFrame::of)!! }
        assertEquals(listOf(RelayFrame.Kind.Open, RelayFrame.Kind.Data, RelayFrame.Kind.End), answer.map { it.kind })
        assertEquals("""{"status":200,"headers":{"content-type":"application/json"}}""", String(answer[0].payload))
        assertEquals("""{"ok":true}""", String(answer[1].payload))
    }

    @Test fun aChangedReplayedOrMissingEnvelopeIsntTaken() {
        val mac = bytes(0)
        val hello = RelayEnvelope.Hello(RelayEnvelope.publicOf(mac))
        val (welcome, macSide) = FakeMac.welcome(mac, hello.bytes, RelayEnvelope.newKey())
        val phone = hello.welcomed(welcome)
        val first = macSide.seal(byteArrayOf(0, 0, 0, 1, 1, 42))
        val second = macSide.seal(byteArrayOf(0, 0, 0, 1, 1, 43))
        val third = macSide.seal(byteArrayOf(0, 0, 0, 1, 1, 44))
        val changed = first.copyOf().also { it[12] = (it[12] + 1).toByte() }
        assertSame(RelayEnvelope.Opened.Dropped, phone.open(changed))
        assertTrue(phone.open(first) is RelayEnvelope.Opened.Frame)
        assertSame(RelayEnvelope.Opened.Dropped, phone.open(first))
        assertSame(RelayEnvelope.Opened.Gap, phone.open(third))
        assertTrue(phone.open(second) is RelayEnvelope.Opened.Frame)
        // Only the pinned Mac answers: a welcome from another key doesn't start a session.
        val (forged, _) = FakeMac.welcome(bytes(100), RelayEnvelope.Hello(RelayEnvelope.publicOf(bytes(100))).bytes, RelayEnvelope.newKey())
        try { hello.welcomed(forged); fail("a welcome from another key opened") } catch (e: RelayEnvelope.Refused) { assertTrue(e.notTheMac) }
        // A version this phone didn't offer is refused.
        val downgraded = welcome.copyOf().also { it[0] = 2 }
        try { hello.welcomed(downgraded); fail("a downgrade opened") } catch (_: RelayEnvelope.Refused) {}
    }

    private val sphere = MockWebServer().apply { start(InetAddress.getByName("127.0.0.1"), 0) }
    @After fun stop() = sphere.shutdown()

    @Test fun aRequestGoesSealedThroughTheSphere() = runBlocking {
        val mac = FakeMac(sphere)
        mac.answer = { _, _ -> 200 to """{"ok":true}""" }
        val runner = Runner(RemoteRoute.RELAY_BASE, "test-device-token", relay = PhoneRelay(mac.link))
        assertEquals("""{"ok":true}""", runner.send("GET", "v1/jobs?limit=50"))
        val (head, _) = mac.asked.single()
        assertEquals("GET", head["method"]); assertEquals("/v1/jobs?limit=50", head["path"])
        assertEquals("Bearer test-device-token", head["authorization"])
        // The socket opened with the relay token, at the relay's path.
        val upgrade = sphere.takeRequest()
        assertEquals("/v0.1/relay/${mac.link.id}", upgrade.path)
        assertEquals("Bearer ${mac.link.token}", upgrade.getHeader("Authorization"))
        // A second request rides the same session, on the next stream.
        assertEquals("""{"ok":true}""", runner.send("POST", "v1/clear"))
        assertEquals(listOf(1, 2), mac.streams)
    }

    @Test fun pairingGoesThroughTheRelay() = runBlocking {
        val mac = FakeMac(sphere)
        mac.answer = { _, _ -> 200 to """{"token":"tok-relay","expiresAt":"2026-11-22T10:00:00Z"}""" }
        val answer = Runner.redeem(RemoteRoute.RELAY_BASE, "one-time", "Pixel", PhoneRelay(mac.link))
        assertEquals("tok-relay", answer.token)
        val (head, body) = mac.asked.single()
        assertEquals("POST", head["method"]); assertEquals("/v1/pairing", head["path"]); assertNull(head["authorization"])
        assertEquals("application/json; charset=utf-8", head["content-type"])
        val sent = wire.parseToJsonElement(body).jsonObject
        assertEquals("one-time", sent["credential"]!!.jsonPrimitive.content); assertEquals("Pixel", sent["name"]!!.jsonPrimitive.content)
    }

    @Test fun anErrorAnswerReadsAsTheRunners() = runBlocking {
        val mac = FakeMac(sphere)
        mac.answer = { _, _ -> 422 to """{"error":"That title has a line break."}""" }
        val runner = Runner(RemoteRoute.RELAY_BASE, "t", relay = PhoneRelay(mac.link))
        try { runner.send("PATCH", "v1/jobs/a1", "{}"); fail("422 passed") } catch (e: RunnerError) {
            assertEquals(422, e.status); assertEquals("That title has a line break.", e.message)
        }
    }

    @Test fun aForgottenPhoneIsRemoved() = runBlocking {
        sphere.enqueue(MockResponse().setResponseCode(401))
        val link = FakeMac.link(sphere, RelayEnvelope.publicOf(bytes(0)))
        val runner = Runner(RemoteRoute.RELAY_BASE, "t", "Removed from your Mac. Pair again.", PhoneRelay(link))
        try { runner.send("GET", "v1/status"); fail("a forgotten relay answered") } catch (e: RunnerError) { assertEquals(401, e.status) }
    }

    @Test fun aHungUpRelayOpensNothingUntilResumed() = runBlocking {
        val mac = FakeMac(sphere)
        val relay = PhoneRelay(mac.link)
        relay.hangUp()
        val runner = Runner(RemoteRoute.RELAY_BASE, "t", relay = relay)
        try { runner.send("GET", "v1/status"); fail("a hung-up relay opened") } catch (_: RunnerError) {}
        relay.resume()
        assertEquals("{}", runner.send("GET", "v1/status"))
    }

    @Test fun aRelayLessRelayRouteSaysToPairAgain() = runBlocking {
        try { Runner(RemoteRoute.RELAY_BASE, "t").send("GET", "v1/status"); fail("it went somewhere") } catch (e: RunnerError) {
            assertEquals(401, e.status); assertTrue(e.message!!.contains("Pair again"))
        }
    }

    @Test fun theEventStreamComesSealedAndStopsWhenLeft() = runBlocking {
        val mac = FakeMac(sphere)
        mac.stream = "event: clear\ndata: {\"clearedAt\":\"2026-11-01T10:00:00Z\"}\n\n"
        val runner = Runner(RemoteRoute.RELAY_BASE, "t", relay = PhoneRelay(mac.link))
        val events = kotlinx.coroutines.withTimeout(10_000) { runner.events().take(2).toList() }
        assertTrue(events.first() == RunnerEvent.Connected)
        assertTrue(events[1] is RunnerEvent.Cleared)
        assertTrue("the Mac heard the reset", mac.resets.await(5, TimeUnit.SECONDS))
    }

    @Test fun theQRCodeCarriesTheRelay() {
        val link = FakeMac.link(sphere, RelayEnvelope.publicOf(bytes(0))).copy(sphere = "https://sphere.example")
        val relay = wire.encodeToString(RelayLink.serializer(), link)
        val envelope = GatewayEnvelope.decode("""{"url":"https://sphere.example","link":{"hostname":"studio","name":"Studio","relay":$relay},"secret":"s"}""")
        assertEquals(link, envelope.relay)
        assertEquals(MacLink("Studio", "studio"), envelope.link)
        for (wrong in listOf(link.copy(sphere = "http://sphere.example"), link.copy(token = "short"), link.copy(key = link.key.dropLast(2)),
                             link.copy(id = "not/base64url/at/all/but/long/enough"))) {
            val text = wire.encodeToString(RelayLink.serializer(), wrong)
            try { GatewayEnvelope.decode("""{"url":"https://sphere.example","link":{"hostname":"studio","name":"Studio","relay":$text},"secret":"s"}"""); fail(text) } catch (_: RunnerError) {}
        }
        for ((hostname, name) in listOf("" to "Studio", "studio/evil" to "Studio", "studio:" to "Studio", "studio:123456" to "Studio", "studio" to "", "studio" to "Studio\nPay here")) {
            val named = wire.encodeToString(MacLink.serializer(), MacLink(name, hostname)).dropLast(1)
            try { GatewayEnvelope.decode("""{"url":"https://sphere.example","link":$named,"relay":$relay},"secret":"s"}"""); fail("$hostname $name") } catch (_: RunnerError) {}
        }
        // A relay that isn't one is refused, not taken for none; a Sphere with a path or a query too.
        for (bad in listOf(""""relay":{"sphere":"https://s.example"}""", """"relay":${wire.encodeToString(RelayLink.serializer(), link.copy(sphere = "https://s.example/?x"))}""")) {
            try { GatewayEnvelope.decode("""{"url":"https://s.example","link":{"hostname":"studio","name":"Studio",$bad},"secret":"s"}"""); fail(bad) } catch (e: RunnerError) {
                assertTrue(e.message!!.contains("Obysphere"))
            }
        }
        // A name with a right-to-left override isn't shown.
        try { GatewayEnvelope.decode("""{"url":"https://s.example","link":{"hostname":"studio","name":"Studio\u202e","relay":$relay},"secret":"s"}"""); fail("RLO") } catch (_: RunnerError) {}
        // A link can't arrive already relayed: only this phone's pairing sets that.
        try { GatewayEnvelope.decode("""{"url":"https://studio.tail0000.ts.net:8443","link":{"hostname":"studio.tail0000.ts.net:8443","name":"Studio","relayed":"a.b"},"secret":"s"}"""); fail() } catch (_: RunnerError) {}
        // A Mac paired through its Obysphere is reached through it alone, at home too.
        val lan = "http://192.168.1.20:8810"
        assertSame(RemoteRoute.Relay, RemoteRoute.choose(lan, envelope.link.copy(relayed = link.pinned), true))
        assertEquals(RemoteRoute.Home(lan), RemoteRoute.choose(lan, envelope.link, true))
        assertEquals("relay:${link.pinned}", envelope.link.copy(relayed = link.pinned).identity)
    }
}

/** obid's side of the relay, as obid's `relay::envelope` does it, behind a MockWebServer playing the Sphere. */
class FakeMac(sphere: MockWebServer, private val key: ByteArray = RelayEnvelope.newKey()) {
    val link = link(sphere, RelayEnvelope.publicOf(key))
    var answer: (Map<String, String>, String) -> Pair<Int, String> = { _, _ -> 200 to "{}" }
    /** An answer that never ends: the event stream's first event, then nothing, until the phone resets it. */
    var stream: String? = null
    val asked = CopyOnWriteArrayList<Pair<Map<String, String>, String>>()
    val streams = CopyOnWriteArrayList<Int>()
    val resets = CountDownLatch(1)
    private var session: RelayEnvelope.Session? = null
    private val heads = mutableMapOf<Int, Map<String, String>>()
    private val bodies = mutableMapOf<Int, ByteArray>()

    init {
        sphere.enqueue(MockResponse().withWebSocketUpgrade(object : WebSocketListener() {
            override fun onMessage(webSocket: WebSocket, bytes: ByteString) = synchronized(this@FakeMac) { heard(webSocket, bytes.toByteArray()) }
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) { webSocket.close(code, null) }
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {}
        }))
    }

    private fun heard(socket: WebSocket, envelope: ByteArray) {
        if (envelope[1] == 1.toByte()) {
            val (welcome, started) = welcome(key, envelope, RelayEnvelope.newKey())
            session = started
            socket.send(welcome.toByteString())
            return
        }
        val opened = session!!.open(envelope) as? RelayEnvelope.Opened.Frame ?: return
        val frame = RelayFrame.of(opened.plain)!!
        fun send(kind: RelayFrame.Kind, payload: ByteArray = ByteArray(0)) = socket.send(session!!.seal(RelayFrame(frame.stream, kind, payload).encoded).toByteString())
        when (frame.kind) {
            RelayFrame.Kind.Open -> {
                val o = wire.parseToJsonElement(String(frame.payload)).jsonObject
                heads[frame.stream] = mapOf("method" to o["method"]!!.jsonPrimitive.content, "path" to o["path"]!!.jsonPrimitive.content) +
                    o["headers"]!!.jsonObject.mapValues { it.value.jsonPrimitive.content }
                streams += frame.stream
            }
            RelayFrame.Kind.Data -> bodies[frame.stream] = (bodies[frame.stream] ?: ByteArray(0)) + frame.payload
            RelayFrame.Kind.End -> {
                val head = heads.getValue(frame.stream)
                val body = String(bodies[frame.stream] ?: ByteArray(0))
                asked += head to body
                stream?.takeIf { head["path"] == "/v1/events" }?.let {
                    send(RelayFrame.Kind.Open, """{"status":200,"headers":{"content-type":"text/event-stream"}}""".toByteArray())
                    send(RelayFrame.Kind.Data, it.toByteArray())
                    return
                }
                val (status, text) = answer(head, body)
                send(RelayFrame.Kind.Open, """{"status":$status,"headers":{"content-type":"application/json"}}""".toByteArray())
                send(RelayFrame.Kind.Data, text.toByteArray())
                send(RelayFrame.Kind.End)
            }
            RelayFrame.Kind.Reset -> resets.countDown()
        }
    }

    companion object {
        fun link(sphere: MockWebServer, mac: ByteArray) = RelayLink(
            "http://127.0.0.1:${sphere.port}", "r".repeat(22), "t".repeat(43), Base64.getUrlEncoder().withoutPadding().encodeToString(mac))

        /** The Mac's welcome to `hello`, with `ephemeral` as M's private key, and its side of the session. */
        fun welcome(mac: ByteArray, hello: ByteArray, ephemeral: ByteArray): Pair<ByteArray, RelayEnvelope.Session> {
            val s = RelayEnvelope.publicOf(mac)
            val e = hello.copyOfRange(2, 34)
            val es = RelayEnvelope.agree(mac, e)
            val versions = RelayEnvelope.open(RelayEnvelope.derive(s + e, es, "oby relay v1 hello"), 0, hello.copyOfRange(34, hello.size), hello.copyOfRange(0, 34))!!
            val v = versions.max()
            val m = RelayEnvelope.publicOf(ephemeral)
            val ee = RelayEnvelope.agree(ephemeral, e)
            val salt = MessageDigest.getInstance("SHA-256").digest(hello + m)
            fun key(info: String) = RelayEnvelope.derive(salt, es + ee, info)
            val head = byteArrayOf(v, 2) + m
            val welcome = head + RelayEnvelope.seal(key("oby relay v1 welcome"), 0, byteArrayOf(v), head)
            return welcome to RelayEnvelope.Session(v, key("oby relay v1 mac>phone"), key("oby relay v1 phone>mac"))
        }
    }
}
