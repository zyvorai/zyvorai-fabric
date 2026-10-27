// Copyright 2026 Zyvor AI Labs · https://zyvor.dev
// SPDX-License-Identifier: Apache-2.0

package dev.zyvor.merion.phone

import android.os.Bundle
import androidx.activity.compose.setContent
import androidx.biometric.BiometricPrompt
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.fragment.app.FragmentActivity
import dev.zyvor.merion.kit.Approval
import dev.zyvor.merion.kit.ApprovalPayload
import dev.zyvor.merion.kit.Decision
import dev.zyvor.merion.kit.MerionClient
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Approvals only: the host address, the user token and the device id are typed in (enrolment is
 * `POST /v1/users/<you>/devices` by an operator, with the key shown below). The text that will be
 * signed is shown before the biometric prompt. Not built or run in this repository's CI; see README.
 */
class MainActivity : FragmentActivity() {
    private val key by lazy { KeystoreApprovalKey.loadOrCreate() }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { MaterialTheme { Screen() } }
    }

    @Composable
    private fun Screen() {
        var host by remember { mutableStateOf("") }
        var token by remember { mutableStateOf("") }
        var deviceId by remember { mutableStateOf("") }
        var pending by remember { mutableStateOf<List<Approval>>(emptyList()) }
        var message by remember { mutableStateOf("") }
        val scope = rememberCoroutineScope()

        Column(Modifier.padding(16.dp)) {
            Text("This device's public key (enrol it):")
            Text(key.publicKeyBase64)
            OutlinedTextField(host, { host = it }, label = { Text("Host (https)") })
            OutlinedTextField(token, { token = it }, label = { Text("User token") })
            OutlinedTextField(deviceId, { deviceId = it }, label = { Text("Device id") })
            Button(onClick = {
                scope.launch {
                    message = runCatching {
                        pending = withContext(Dispatchers.IO) { MerionClient(host, token).inbox().pending }
                        "${pending.size} waiting"
                    }.getOrElse { it.message ?: "Could not load" }
                }
            }) { Text("Refresh") }
            Text(message)
            pending.forEach { a ->
                Column(Modifier.padding(vertical = 8.dp)) {
                    Text("${a.kind}: ${a.subject ?: ""}")
                    Row {
                        Button(onClick = { decide(a, Decision.APPROVED, host, token, deviceId) { message = it } }) { Text("Allow") }
                        Button(onClick = { decide(a, Decision.DENIED, host, token, deviceId) { message = it } }) { Text("Deny") }
                    }
                }
            }
        }
    }

    private fun decide(a: Approval, decision: Decision, host: String, token: String, deviceId: String, report: (String) -> Unit) {
        val sign = a.sign ?: return report("This approval has no signing challenge.")
        if (System.currentTimeMillis() / 1000 >= sign.expiresAt) return report("The signing window has passed.")
        val payload = ApprovalPayload.text(a, decision)
        val prompt = BiometricPrompt(
            this,
            ContextCompat.getMainExecutor(this),
            object : BiometricPrompt.AuthenticationCallback() {
                override fun onAuthenticationSucceeded(result: BiometricPrompt.AuthenticationResult) {
                    val signature = key.finish(result.cryptoObject!!.signature!!, payload)
                    Thread {
                        val outcome = runCatching { MerionClient(host, token).decide(a.id, decision, deviceId, signature) }
                        runOnUiThread { report(outcome.fold({ "Sent: ${decision.wire}" }, { it.message ?: "Not sent" })) }
                    }.start()
                }

                override fun onAuthenticationError(errorCode: Int, errString: CharSequence) = report(errString.toString())
            },
        )
        prompt.authenticate(
            BiometricPrompt.PromptInfo.Builder()
                .setTitle("${decision.wire} ${a.kind}")
                .setSubtitle(payload)
                .setNegativeButtonText("Cancel")
                .build(),
            BiometricPrompt.CryptoObject(key.signatureForPrompt()),
        )
    }
}
