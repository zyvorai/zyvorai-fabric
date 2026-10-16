import MerionKit
import SwiftUI
import UIKit

struct SettingsView: View {
    @EnvironmentObject var model: AppModel
    @State private var token = ""
    @State private var result: String?
    @State private var checking = false

    var body: some View {
        NavigationStack {
            Form {
                Section("Keep host") {
                    TextField("https://keep.example.com", text: $model.host).textContentType(.URL).keyboardType(.URL).textInputAutocapitalization(.never).autocorrectionDisabled()
                    SecureField(model.isConnected ? "Token saved (enter a new one to replace it)" : "Your user token", text: $token).textInputAutocapitalization(.never).autocorrectionDisabled()
                    TextField("Agent to chat with", text: $model.agent).textInputAutocapitalization(.never).autocorrectionDisabled()
                    Button(checking ? "Checking…" : "Save and check") { Task { await saveAndCheck() } }.disabled(checking || model.host.isEmpty)
                    if let result { Text(result).font(.footnote) }
                    Text("The token is kept in this device's Keychain. It is sent only to the host above, in a header.").font(.footnote).foregroundStyle(.secondary)
                    if model.isConnected { Button("Forget this host", role: .destructive) { model.forget(); token = ""; result = nil } }
                }
                Section("This device as an approver") { DeviceKeyView() }
            }.navigationTitle("Settings")
        }
    }

    private func saveAndCheck() async {
        checking = true; defer { checking = false }
        if !token.isEmpty { model.saveToken(token); token = "" }
        guard let client = model.client else { result = "Enter the host address and your token."; return }
        do {
            let s = try await client.status()
            result = "Connected. \(s.merionMode == true ? "Keep mode is on." : "Keep mode is off.")"
            await model.refreshApprovals()
        } catch { result = error.localizedDescription }
    }
}

/// The approval key lives in the Secure Enclave and never leaves it; only its public half is shown. Enrolling it for a user is the operator's
/// step (a user token must not be able to add its own key), so this shows exactly what the operator needs.
struct DeviceKeyView: View {
    @EnvironmentObject var model: AppModel
    @State private var publicKey: String?
    @State private var error: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            TextField("Name this device (e.g. sus-iphone)", text: $model.deviceId).textInputAutocapitalization(.never).autocorrectionDisabled()
            if let publicKey {
                Text("Public key").font(.caption).foregroundStyle(.secondary)
                Text(publicKey).font(.system(.caption2, design: .monospaced)).textSelection(.enabled)
                Button("Copy the enrolment details") { UIPasteboard.general.string = enrolment(publicKey) }.disabled(model.deviceId.isEmpty)
                Text("Give these to your Keep operator: they enrol this key for your user with POST /v1/users/<you>/devices. Until then the host will refuse your signed decisions.").font(.footnote).foregroundStyle(.secondary)
            } else {
                Button("Create the key on this device") { load() }.disabled(model.deviceId.isEmpty)
            }
            if let error { Text(error).font(.footnote).foregroundStyle(.red) }
        }.onAppear { if !model.deviceId.isEmpty, (try? SecureEnclaveKey.loadOrCreate()) != nil { load() } }
    }

    private func load() {
        do { publicKey = try SecureEnclaveKey.loadOrCreate().publicKeyBase64; error = nil } catch { self.error = error.localizedDescription }
    }

    private func enrolment(_ key: String) -> String {
        #"{"device_id":"\#(model.deviceId)","alg":"p256","public_key":"\#(key)","name":"\#(model.deviceId)"}"#
    }
}
