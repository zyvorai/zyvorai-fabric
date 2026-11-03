// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

package dev.zyvor.merion.phone

import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.security.keystore.StrongBoxUnavailableException
import android.util.Base64
import dev.zyvor.merion.kit.ApprovalKey
import dev.zyvor.merion.kit.signWith
import java.security.KeyPairGenerator
import java.security.KeyStore
import java.security.PrivateKey
import java.security.Signature
import java.security.spec.ECGenParameterSpec

/**
 * A P-256 key that never leaves the Android Keystore (StrongBox when the phone has it) and needs a
 * fresh biometric or screen-lock check for each signature.
 *
 * Signing goes through [signatureForPrompt]: the caller hands that `Signature` to a
 * `BiometricPrompt.CryptoObject`, and after the person confirms calls [finish] with the same object.
 */
class KeystoreApprovalKey private constructor(private val privateKey: PrivateKey, private val publicKeyEncoded: ByteArray) : ApprovalKey {
    override val publicKeyBase64: String get() = Base64.encodeToString(publicKeyEncoded, Base64.NO_WRAP)

    /** An initialised `Signature` to wrap in a `BiometricPrompt.CryptoObject`. */
    fun signatureForPrompt(): Signature = Signature.getInstance("SHA256withECDSA").apply { initSign(privateKey) }

    /** Sign after the biometric check passed, using the prompt's authenticated `Signature`. */
    fun finish(authenticated: Signature, payload: String): String {
        authenticated.update(payload.toByteArray(Charsets.UTF_8))
        return Base64.encodeToString(authenticated.sign(), Base64.NO_WRAP)
    }

    /** Only works when the key does not require a fresh authentication (not the case for this one). */
    override fun sign(payload: String): String = signWith(privateKey, payload)

    companion object {
        private const val ALIAS = "merion-approval"

        fun loadOrCreate(): KeystoreApprovalKey {
            val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
            val existing = store.getKey(ALIAS, null) as? PrivateKey
            if (existing != null) return KeystoreApprovalKey(existing, store.getCertificate(ALIAS).publicKey.encoded)
            val pair = try {
                generate(strongBox = true)
            } catch (_: StrongBoxUnavailableException) {
                generate(strongBox = false)
            }
            return KeystoreApprovalKey(pair.private, pair.public.encoded)
        }

        private fun generate(strongBox: Boolean) = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, "AndroidKeyStore").apply {
            initialize(
                KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_SIGN)
                    .setAlgorithmParameterSpec(ECGenParameterSpec("secp256r1"))
                    .setDigests(KeyProperties.DIGEST_SHA256)
                    .setUserAuthenticationRequired(true)
                    .setInvalidatedByBiometricEnrollment(true)
                    .setIsStrongBoxBacked(strongBox)
                    .build(),
            )
        }.generateKeyPair()
    }
}
