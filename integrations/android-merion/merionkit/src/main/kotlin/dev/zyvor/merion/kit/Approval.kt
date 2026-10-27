// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

package dev.zyvor.merion.kit

import org.json.JSONObject

enum class Decision(val wire: String) {
    APPROVED("approved"),
    DENIED("denied"),
}

/** The `sign` block of a pending approval (docs/merion/mobile/README.md). */
data class SignInfo(
    val format: String,
    val challenge: String,
    val expiresAt: Long,
    val actionSha256: String,
    val algorithms: List<String>,
)

data class Approval(
    val id: String,
    val kind: String,
    val subject: String?,
    val prompt: String,
    val status: String,
    val sessionId: String?,
    val sign: SignInfo?,
) {
    companion object {
        fun fromJson(o: JSONObject): Approval {
            val sign = o.optJSONObject("sign")?.let {
                val algs = it.optJSONArray("algorithms")
                SignInfo(
                    format = it.getString("format"),
                    challenge = it.getString("challenge"),
                    expiresAt = it.getLong("expires_at"),
                    actionSha256 = it.getString("action_sha256"),
                    algorithms = (0 until (algs?.length() ?: 0)).map { i -> algs!!.getString(i) },
                )
            }
            return Approval(
                id = o.getString("id"),
                kind = o.getString("kind"),
                subject = o.optString("subject").takeIf { o.has("subject") && !o.isNull("subject") },
                prompt = o.optString("prompt"),
                status = o.optString("status"),
                sessionId = o.optString("session_id").takeIf { o.has("session_id") && !o.isNull("session_id") },
                sign = sign,
            )
        }
    }
}

data class Inbox(val userId: String?, val pending: List<Approval>)
