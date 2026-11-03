// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

package dev.zyvor.merion.kit

import java.math.BigInteger
import java.security.KeyPairGenerator
import java.security.KeyFactory
import java.security.PrivateKey
import java.security.Signature
import java.security.spec.ECGenParameterSpec
import java.security.spec.X509EncodedKeySpec
import java.util.Base64

/**
 * The exact text an approval is signed over (`merion-approval-v1`). Readable on purpose: show it, or
 * the parts a person understands, before they confirm. Every line ends in `\n`.
 */
object ApprovalPayload {
    const val FORMAT = "merion-approval-v1"

    fun text(approvalId: String, decision: Decision, kind: String, subject: String?, sign: SignInfo): String =
        "$FORMAT\napproval: $approvalId\ndecision: ${decision.wire}\nkind: $kind\nsubject: ${subject ?: ""}\n" +
            "action-sha256: ${sign.actionSha256}\nchallenge: ${sign.challenge}\nexpires: ${sign.expiresAt}\n"

    fun text(approval: Approval, decision: Decision): String {
        val sign = approval.sign ?: throw SignerException.NoChallenge()
        return text(approval.id, decision, approval.kind, approval.subject, sign)
    }
}

sealed class SignerException(message: String) : Exception(message) {
    class NoChallenge : SignerException("This approval carries no signing challenge (the host is not set up for device signatures).")
    class Expired : SignerException("The signing window for this approval has passed.")
}

/** A P-256 key that can sign approvals. */
interface ApprovalKey {
    /** Base64 of the X.509 SubjectPublicKeyInfo, what the runtime expects when enrolling. */
    val publicKeyBase64: String

    /** Base64 of the DER ECDSA signature over SHA-256 of the UTF-8 payload. */
    fun sign(payload: String): String
}

/** A key in memory, for tests and for devices with no hardware keystore. */
class SoftwareP256Key private constructor(
    private val privateKey: PrivateKey,
    override val publicKeyBase64: String,
) : ApprovalKey {
    override fun sign(payload: String): String = signWith(privateKey, payload)

    companion object {
        fun generate(): SoftwareP256Key {
            val pair = KeyPairGenerator.getInstance("EC").apply { initialize(ECGenParameterSpec("secp256r1")) }.generateKeyPair()
            return SoftwareP256Key(pair.private, Base64.getEncoder().encodeToString(pair.public.encoded))
        }
    }
}

/** The signature step shared by every key: SHA256withECDSA over the UTF-8 text, DER, base64. */
fun signWith(key: PrivateKey, payload: String): String {
    val der = Signature.getInstance("SHA256withECDSA").apply {
        initSign(key)
        update(payload.toByteArray(Charsets.UTF_8))
    }.sign()
    return Base64.getEncoder().encodeToString(der)
}

data class SignedDecision(val decision: Decision, val signature: String)

object ApprovalSigner {
    /** Build the text, check the window, and sign it. Nothing is sent. */
    fun sign(approval: Approval, decision: Decision, key: ApprovalKey, nowEpochSeconds: Long): SignedDecision {
        val sign = approval.sign ?: throw SignerException.NoChallenge()
        if (nowEpochSeconds >= sign.expiresAt) throw SignerException.Expired()
        return SignedDecision(decision, key.sign(ApprovalPayload.text(approval, decision)))
    }
}

object ApprovalVerifier {
    /** Verify a P-256 signature given as base64 of DER, or of the raw 64-byte r||s. */
    fun verifyP256(publicKeyBase64: String, payload: String, signatureBase64: String): Boolean {
        val key = KeyFactory.getInstance("EC").generatePublic(X509EncodedKeySpec(Base64.getDecoder().decode(publicKeyBase64)))
        val raw = Base64.getDecoder().decode(signatureBase64)
        val der = if (raw.size == 64) rawToDer(raw) else raw
        return Signature.getInstance("SHA256withECDSA").apply {
            initVerify(key)
            update(payload.toByteArray(Charsets.UTF_8))
        }.verify(der)
    }

    private fun rawToDer(raw: ByteArray): ByteArray {
        fun int(bytes: ByteArray): ByteArray {
            val b = BigInteger(1, bytes).toByteArray()
            return byteArrayOf(0x02, b.size.toByte()) + b
        }
        val body = int(raw.copyOfRange(0, 32)) + int(raw.copyOfRange(32, 64))
        return byteArrayOf(0x30, body.size.toByte()) + body
    }
}
