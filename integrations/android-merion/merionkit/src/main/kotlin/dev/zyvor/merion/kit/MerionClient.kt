// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

package dev.zyvor.merion.kit

import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URI
import java.net.URLEncoder

class MerionHttpException(val status: Int, val body: String) : IOException("Merion answered $status")

/**
 * A client for the runtime API. Blocking; call it off the main thread. The token goes in the
 * Authorization header only, never in a URL, and only to an https host (or loopback, for a dev host).
 */
class MerionClient(baseUrl: String, private val token: String) {
    private val base: URI = URI.create(baseUrl.trimEnd('/'))

    init {
        val loopback = base.host in setOf("localhost", "127.0.0.1", "[::1]", "::1")
        require(base.scheme == "https" || (base.scheme == "http" && loopback)) {
            "the host must be https (http is allowed only for loopback)"
        }
        require(base.userInfo == null) { "the host address must not carry a user name or password" }
    }

    fun inbox(): Inbox {
        val o = JSONObject(request("GET", "/v1/inbox", null))
        val items = o.optJSONArray("pending_approvals")
        return Inbox(
            userId = o.optString("user_id").takeIf { o.has("user_id") && !o.isNull("user_id") },
            pending = (0 until (items?.length() ?: 0)).map { Approval.fromJson(items!!.getJSONObject(it)) },
        )
    }

    /** Send a decision. A signed one carries the enrolled device id and the signature. */
    fun decide(approvalId: String, decision: Decision, deviceId: String? = null, signature: String? = null) {
        require((deviceId == null) == (signature == null)) { "deviceId and signature go together" }
        val body = JSONObject().put("decision", decision.wire)
        if (deviceId != null) body.put("device_id", deviceId).put("signature", signature)
        request("POST", "/v1/approvals/${URLEncoder.encode(approvalId, "UTF-8")}", body.toString())
    }

    private fun request(method: String, path: String, body: String?): String {
        val conn = URI.create(base.toString() + path).toURL().openConnection() as HttpURLConnection
        try {
            conn.requestMethod = method
            conn.connectTimeout = 10_000
            conn.readTimeout = 15_000
            conn.instanceFollowRedirects = false
            conn.setRequestProperty("Authorization", "Bearer $token")
            conn.setRequestProperty("Accept", "application/json")
            if (body != null) {
                conn.doOutput = true
                conn.setRequestProperty("Content-Type", "application/json")
                conn.outputStream.use { it.write(body.toByteArray(Charsets.UTF_8)) }
            }
            val status = conn.responseCode
            val stream = if (status in 200..299) conn.inputStream else conn.errorStream
            val text = stream?.bufferedReader(Charsets.UTF_8)?.use { it.readText() }.orEmpty()
            if (status !in 200..299) throw MerionHttpException(status, text.take(500))
            return text
        } finally {
            conn.disconnect()
        }
    }
}
