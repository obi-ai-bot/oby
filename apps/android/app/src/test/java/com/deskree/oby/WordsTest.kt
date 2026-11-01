package com.deskree.oby

import kotlinx.coroutines.runBlocking
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.net.InetAddress

/** The owner's words (docs/runner-api.md, "Words the owner typed"): every request but a read carries `X-Oby-Words`, signed over exactly it. */
class WordsTest {
    @Test fun everyWriteIsSignedAndNoRead() = runBlocking {
        val server = MockWebServer().apply { start(InetAddress.getByName("127.0.0.1"), 0) }
        repeat(2) { server.enqueue(MockResponse().setBody("{}")) }
        val signed = mutableListOf<Triple<String, String, String>>()
        val runner = Runner("http://127.0.0.1:${server.port}", "t") { method, path, body -> signed += Triple(method, path, String(body)); "dev 2026-11-01T10:00:00.123Z sig" }
        runner.send("GET", "v1/jobs?limit=50")
        runner.send("PATCH", "v1/jobs/a1?x=1", """{"title":"New"}""")
        assertNull(server.takeRequest().getHeader("X-Oby-Words"))
        assertEquals("dev 2026-11-01T10:00:00.123Z sig", server.takeRequest().getHeader("X-Oby-Words"))
        // The path from /v1/ on, without its query, and the body as it goes.
        assertEquals(listOf(Triple("PATCH", "/v1/jobs/a1", """{"title":"New"}""")), signed)
        server.shutdown()
    }
}
