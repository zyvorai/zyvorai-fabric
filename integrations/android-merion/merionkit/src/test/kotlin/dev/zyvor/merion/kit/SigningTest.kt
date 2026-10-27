// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

package dev.zyvor.merion.kit

import com.sun.net.httpserver.HttpServer
import org.json.JSONObject
import java.io.File
import java.net.InetSocketAddress
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue

class SigningTest {
    private val vectors = JSONObject(File(System.getProperty("merion.vectors")).readText())
    private val approvalJson = vectors.getJSONObject("approval")
    private val signJson = vectors.getJSONObject("sign")

    private fun approval(withSign: Boolean = true): Approval = Approval(
        id = approvalJson.getString("id"),
        kind = approvalJson.getString("kind"),
        subject = approvalJson.getString("subject"),
        prompt = "p",
        status = "pending",
        sessionId = null,
        sign = if (withSign) SignInfo(
            format = signJson.getString("format"),
            challenge = signJson.getString("challenge"),
            expiresAt = signJson.getLong("expires_at"),
            actionSha256 = signJson.getString("action_sha256"),
            algorithms = listOf("p256"),
        ) else null,
    )

    private fun cases() = (0 until vectors.getJSONArray("cases").length()).map { vectors.getJSONArray("cases").getJSONObject(it) }

    @Test
    fun `the payload is byte for byte the one the runtime signed in its vectors`() {
        for (c in cases()) {
            val decision = Decision.entries.first { it.wire == c.getString("decision") }
            assertEquals(c.getString("payload"), ApprovalPayload.text(approval(), decision), "${c.getString("alg")} ${decision.wire}")
        }
    }

    @Test
    fun `the runtime's p256 signatures verify, as DER and as raw r and s`() {
        val p256 = cases().filter { it.getString("alg") == "p256" }
        assertTrue(p256.isNotEmpty())
        for (c in p256) {
            val key = c.getString("public_key")
            val payload = c.getString("payload")
            assertTrue(ApprovalVerifier.verifyP256(key, payload, c.getString("signature")), "DER")
            assertTrue(ApprovalVerifier.verifyP256(key, payload, c.getString("signature_raw")), "raw")
            // a signature does not carry over to the other decision
            val other = payload.replace("decision: approved", "decision: denied").replace("decision: denied\nkind", "decision: denied\nkind")
            if (other != payload) assertFalse(ApprovalVerifier.verifyP256(key, other, c.getString("signature")))
        }
    }

    @Test
    fun `a key made here signs a text the verifier accepts and refuses once it changes`() {
        val key = SoftwareP256Key.generate()
        val signed = ApprovalSigner.sign(approval(), Decision.APPROVED, key, nowEpochSeconds = 1_790_000_000)
        val text = ApprovalPayload.text(approval(), Decision.APPROVED)
        assertTrue(ApprovalVerifier.verifyP256(key.publicKeyBase64, text, signed.signature))
        assertFalse(ApprovalVerifier.verifyP256(key.publicKeyBase64, text.replace("approved", "denied"), signed.signature))
    }

    @Test
    fun `signing is refused without a challenge and after the window`() {
        val key = SoftwareP256Key.generate()
        assertFailsWith<SignerException.NoChallenge> { ApprovalSigner.sign(approval(withSign = false), Decision.APPROVED, key, 0) }
        assertFailsWith<SignerException.Expired> { ApprovalSigner.sign(approval(), Decision.APPROVED, key, 1_790_003_600) }
    }

    @Test
    fun `the client sends the token in a header, parses the inbox and posts a signed decision`() {
        val seen = mutableListOf<String>()
        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.createContext("/") { ex ->
            seen += "${ex.requestMethod} ${ex.requestURI} auth=${ex.requestHeaders.getFirst("Authorization")} body=${ex.requestBody.readBytes().decodeToString()}"
            val reply = if (ex.requestURI.path == "/v1/inbox") {
                """{"user_id":"ana","pending_approvals":[{"id":"a1","kind":"send","subject":null,"prompt":"Send it","status":"pending",
                  "sign":{"format":"merion-approval-v1","challenge":"c","expires_at":9,"action_sha256":"s","algorithms":["p256"]}}]}"""
            } else "{}"
            val bytes = reply.toByteArray()
            ex.sendResponseHeaders(200, bytes.size.toLong())
            ex.responseBody.use { it.write(bytes) }
        }
        server.start()
        try {
            val client = MerionClient("http://127.0.0.1:${server.address.port}", "tok")
            val inbox = client.inbox()
            assertEquals("ana", inbox.userId)
            assertEquals(null, inbox.pending.single().subject)
            assertEquals(9L, inbox.pending.single().sign?.expiresAt)
            client.decide("a1", Decision.DENIED, "ana-phone", "c2ln")
            assertTrue(seen[0].startsWith("GET /v1/inbox auth=Bearer tok"), seen[0])
            assertTrue(seen[1].startsWith("POST /v1/approvals/a1 auth=Bearer tok"), seen[1])
            val sent = JSONObject(seen[1].substringAfter("body="))
            assertEquals("denied", sent.getString("decision"))
            assertEquals("ana-phone", sent.getString("device_id"))
            assertFailsWith<IllegalArgumentException> { client.decide("a1", Decision.APPROVED, "ana-phone", null) }
        } finally {
            server.stop(0)
        }
    }

    @Test
    fun `the client refuses a plain http host that is not loopback and a token in the address`() {
        assertFailsWith<IllegalArgumentException> { MerionClient("http://merion.example", "t") }
        assertFailsWith<IllegalArgumentException> { MerionClient("https://user:pw@merion.example", "t") }
        MerionClient("https://merion.example", "t")
    }
}
