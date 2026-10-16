import AppKit
import KeepKit
import SwiftUI

/// The one place a person connects: found hosts, an address, a token (or a pasted `keep://connect` link), and what went wrong in plain words.
struct ConnectForm: View {
    @EnvironmentObject var app: AppState
    @StateObject private var model = ConnectionModel()
    @State private var busy = false

    var body: some View {
        VStack(alignment: .leading, spacing: Space.m) {
            if let host = model.found.first(where: { $0.absoluteString != model.hostText }) ?? (model.hostText.isEmpty ? model.found.first : nil) {
                HStack(spacing: Space.s) {
                    Image(systemName: "dot.radiowaves.left.and.right").symbolEffect(.variableColor.iterative, options: .repeating).foregroundStyle(Brand.orange)
                    Text("Found a Keep host on this Mac").font(.callout)
                    Button(host.host.map { "\($0):\(host.port ?? 0)" } ?? host.absoluteString) { model.use(host) }.secondaryButton().controlSize(.small)
                }
            }
            TextField("Address", text: $model.hostText, prompt: Text("http://127.0.0.1:9096 or https://keep.example.com")).textFieldStyle(.roundedBorder)
            SecureField("Token", text: $model.tokenText, prompt: Text(app.connected ? "•••••• stored in the Keychain" : "kut1…")).textFieldStyle(.roundedBorder)
            HStack {
                Button { if let t = NSPasteboard.general.string(forType: .string) { model.paste(t) } } label: { Label("Paste link or token", systemImage: "doc.on.clipboard") }
                    .secondaryButton()
                Spacer()
                Button {
                    busy = true
                    Task { await model.connect(app); busy = false }
                } label: {
                    HStack(spacing: 6) { if busy { ProgressView().controlSize(.small) }; Text(app.connected ? "Reconnect" : "Connect") }
                }
                .primaryButton().keyboardShortcut(.defaultAction).disabled(!model.canConnect || busy)
            }
            if let w = model.warning { Label(w, systemImage: "exclamationmark.triangle.fill").font(.callout).foregroundStyle(.orange) }
            if let p = app.problem, p.kind != .missing {
                VStack(alignment: .leading, spacing: 2) {
                    Label(p.title, systemImage: "xmark.octagon.fill").font(.callout.weight(.semibold)).foregroundStyle(.red)
                    Text(p.hint).font(.callout).foregroundStyle(.secondary)
                }
                .transition(.opacity.combined(with: .move(edge: .top)))
            }
            if app.connected, let s = app.status { Text("Connected. Use cases: \(s.demos.map { $0.builtin + $0.custom } ?? app.demos.count).").font(.callout).foregroundStyle(.secondary) }
            DisclosureGroup("No host yet? Try the demo") { TryTheDemo().padding(.top, Space.xs) }.font(.callout)
        }
        .animation(Motion.spring, value: app.problem)
        .task {
            // Look for a host on this Mac now, then every few seconds until connected, so starting the demo is enough.
            while !Task.isCancelled && !app.connected { await model.probe(); try? await Task.sleep(nanoseconds: 3_000_000_000) }
        }
    }
}

struct TryTheDemo: View {
    private let command = "scripts/merion-demo-local.sh"
    @State private var copied = false
    var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            Text("From a checkout of the repository, run this in a terminal. It starts a local host with sample use cases, and prints a token and a connect link. Solvor notices the host by itself.")
                .font(.callout).foregroundStyle(.secondary)
            HStack {
                Text(command).font(.system(.callout, design: .monospaced)).textSelection(.enabled)
                    .padding(.horizontal, 10).padding(.vertical, 6).background(.quaternary, in: RoundedRectangle(cornerRadius: Radius.chip))
                Button { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(command, forType: .string); copied = true } label: {
                    Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc").contentTransition(.symbolEffect(.replace))
                }.secondaryButton().controlSize(.small)
            }
            Text("The demo is simulated: it shows an amber notice instead of the proof pill, because it does not run a sealed cell.").font(.caption).foregroundStyle(.tertiary)
        }
    }
}
