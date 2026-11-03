import MerionKit
import ServiceManagement
import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var app: AppState
    @State private var launchAtLogin = SMAppService.mainApp.status == .enabled

    var body: some View {
        Form {
            Section("Keep host") {
                HStack { Text("Status"); Spacer(); StatusChip(kind: app.connected ? .done : .waiting, label: app.connected ? "Connected" : "Not connected") }
                if app.connected { LabeledContent("Connected to", value: URL(string: app.host)?.host ?? app.host) }
                if app.connected, !app.userId.isEmpty { LabeledContent("Signed in as", value: app.userId) }
                ConnectForm()
                if app.connected { Button("Forget the token", role: .destructive) { app.disconnect() } }
                if let u = app.usage {
                    HStack(spacing: Space.l) {
                        statStack(u.usage.runs, "Runs"); statStack(u.usage.artifacts, "Artifacts"); statStack(u.usage.modelCalls, "Model calls")
                    }.padding(.top, Space.xxs)
                }
            }
            if app.connected, app.userId.isEmpty {
                Section("Operator token") {
                    TextField("User to act for", text: Binding(get: { app.userId }, set: { app.userId = $0 }), prompt: Text("needed for usage, approvals and the inbox"))
                    Text("This token belongs to the operator, so the host does not say which person it is for. A user token needs nothing here.").font(.callout).foregroundStyle(.secondary)
                }
            }
            Section("Behaviour") {
                Toggle("Notifications", isOn: app.$notify)
                Stepper("Warn before uploading files over \(app.uploadWarnMB) MB", value: app.$uploadWarnMB, in: 1...200)
                Toggle("Open at login", isOn: $launchAtLogin).onChange(of: launchAtLogin) { _, on in
                    do { if on { try SMAppService.mainApp.register() } else { try SMAppService.mainApp.unregister() } } catch { app.notice = error.localizedDescription }
                }
            }
            Section("What to know") {
                Text("Files you choose are uploaded to the host and read in a sealed cell that has no network. The evidence class is software-test: whoever operates the host could still read a cell's memory. The app never runs commands on your Mac and never drives other apps.")
                    .font(.callout).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped).navigationTitle("Settings")
    }

    private func statStack(_ value: Int, _ label: String) -> some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("\(value)").font(.title3.weight(.semibold))
            Text(label).font(.caption).foregroundStyle(.secondary)
        }
    }
}
