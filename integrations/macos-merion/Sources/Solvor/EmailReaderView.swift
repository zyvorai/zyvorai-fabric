import MerionKit
import SwiftUI

/// Read the email open in a browser tab, review it, and let Solvor pick and run the matching use case.
struct EmailReaderView: View {
    @EnvironmentObject var app: AppState
    @Environment(\.dismiss) private var dismiss
    @StateObject private var reader = BrowserReader()

    @State private var browser: BrowserKind?
    @State private var page: CapturedPage?
    @State private var subject = ""
    @State private var text = ""
    @State private var reading = false
    @State private var problem: String?
    @State private var redactCodes = true
    @State private var redactNumbers = true
    @State private var redactLinks = true
    @State private var chosen: String?

    private var options: RedactionOptions {
        var o: RedactionOptions = []
        if redactCodes { o.insert(.codes) }; if redactNumbers { o.insert(.longNumbers) }; if redactLinks { o.insert(.trackingLinks) }
        return o
    }
    private var redaction: RedactionResult { Redactor.redact(text, options: options) }
    /// The person's pick, else the best suggestion, so the button is never dead when a match exists.
    private var effective: String? { chosen ?? suggestions.first?.useCase }
    private var suggestions: [RouteSuggestion] { EmailRouter.route(redaction.text, available: Set(app.demos.map(\.id))) }
    private var secrets: [SecretFinding] { SecretScan.scan(text: redaction.text) }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 12) {
                AccentTile(symbol: "envelope.open.fill")
                VStack(alignment: .leading) { Text("Read an email from your browser").font(.title3.bold()); Text("Only when you click. You review everything before it is sent.").font(.callout).foregroundStyle(.secondary) }
                Spacer()
                Button("Close") { dismiss() }
            }
            if page == nil { pickStep } else { previewStep }
        }
        .padding(24).frame(width: 720, height: page == nil ? 420 : 600)
        .onAppear {
            browser = reader.defaultBrowser
            #if DEBUG
            if UserDefaults.standard.bool(forKey: "SolvorDemoEmail") {
                let p = CapturedPage(url: URL(string: "https://mail.example.com/u/0/#inbox/1"), title: "Reminder: INV-2026-0142 is overdue",
                                     text: "Hello,\n\nThis is a reminder that invoice INV-2026-0142 for Rs 1,25,000.00 is overdue since 15 Aug 2026. Your verification code is 482913.\nPlease pay by 12 Sep 2026 to account 123456789012.\nDetails: https://pay.example.com/i/142?utm_source=mail&utm_campaign=abcdefghijklmnop&token=zzzzzzzzzz\n\nThanks,\nAccounts", isSelection: false)
                page = p; subject = p.title; text = p.text
                chosen = EmailRouter.route(Redactor.redact(p.text).text, available: Set(app.demos.map(\.id))).first?.useCase
            }
            #endif
        }
    }

    // MARK: step 1

    private var pickStep: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Open the message in Safari, Chrome, Brave, Edge or Arc. To read only part of it, select that text first.").foregroundStyle(.secondary)
            if reader.running.isEmpty {
                Label("No supported browser is open.", systemImage: "exclamationmark.triangle").foregroundStyle(.orange)
            } else {
                HStack {
                    Picker("Browser", selection: $browser) { ForEach(reader.running, id: \.self) { Text($0.appName).tag(Optional($0)) } }.frame(maxWidth: 260)
                    Button { Task { await read() } } label: { Label(reading ? "Reading…" : "Read the front tab", systemImage: "arrow.down.doc") }
                        .primaryButton().disabled(browser == nil || reading)
                }
            }
            if let problem { Text(problem).font(.callout).padding(12).frame(maxWidth: .infinity, alignment: .leading).background(Color.orange.opacity(0.14), in: RoundedRectangle(cornerRadius: 10)) }
            GroupBox("What you need to allow, once") {
                VStack(alignment: .leading, spacing: 6) {
                    Text("1. macOS asks whether Solvor may control the browser. Choose OK.")
                    Text("2. The browser must allow scripts to read a page: Safari, Develop, Allow JavaScript from Apple Events. In Chrome, Brave, Edge and Arc: View, Developer, Allow JavaScript from Apple Events.")
                    Text("Firefox has no scripting interface, so it cannot be read this way.").foregroundStyle(.secondary)
                }.font(.callout).frame(maxWidth: .infinity, alignment: .leading).padding(4)
            }
            Spacer()
            Text("Solvor reads the text of the page you are looking at. It never sends, replies to, deletes or clicks anything, and never touches your mail password.").font(.caption).foregroundStyle(.tertiary)
        }
    }

    private func read() async {
        guard let browser else { return }
        reading = true; problem = nil
        defer { reading = false }
        do {
            let p = try await reader.read(browser)
            page = p; subject = p.title; text = p.text
            chosen = EmailRouter.route(Redactor.redact(p.text).text, available: Set(app.demos.map(\.id))).first?.useCase
        } catch { problem = error.localizedDescription }
    }

    // MARK: step 2

    private var previewStep: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Label(page?.host ?? "page", systemImage: "globe").font(.callout.weight(.medium))
                if page?.isSelection == true { Text("selected text only").font(.caption).padding(.horizontal, 8).padding(.vertical, 2).background(Color.accentColor.opacity(0.15), in: Capsule()) }
                Spacer()
                Button("Read again") { page = nil; problem = nil }.controlSize(.small)
            }
            TextField("Subject", text: $subject).textFieldStyle(.roundedBorder)
            TextEditor(text: $text).font(.system(.callout, design: .monospaced)).frame(minHeight: 170).overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(.quaternary))
            HStack(spacing: 14) {
                Toggle("Codes (\(redaction.codes))", isOn: $redactCodes)
                Toggle("Long numbers (\(redaction.numbers))", isOn: $redactNumbers)
                Toggle("Tracking links (\(redaction.links))", isOn: $redactLinks)
                Spacer()
                Text("\(redaction.text.count) characters will be sent").font(.caption).foregroundStyle(.secondary)
            }.toggleStyle(.checkbox).font(.callout)
            if !secrets.isEmpty {
                Label("This text looks like it contains \(secrets.map(\.kind).joined(separator: ", ")). Edit it before sending.", systemImage: "exclamationmark.triangle.fill").font(.callout).foregroundStyle(.orange)
            }
            Divider()
            Text("Which use case should read it?").font(.headline)
            if suggestions.isEmpty {
                Text("This host lists no email use case. Deploy mailbox-triage to read messages.").foregroundStyle(.secondary)
            } else {
                ForEach(suggestions, id: \.useCase) { s in
                    Button { chosen = s.useCase } label: {
                        HStack {
                            Image(systemName: effective == s.useCase ? "largecircle.fill.circle" : "circle").foregroundStyle(effective == s.useCase ? Color.accentColor : Color.secondary)
                            VStack(alignment: .leading) { Text(app.demo(s.useCase)?.title ?? s.useCase).fontWeight(.medium); Text(s.reason).font(.caption).foregroundStyle(.secondary) }
                            Spacer()
                        }.contentShape(Rectangle())
                    }.buttonStyle(.plain)
                }
            }
            HStack {
                Text("The text goes to \(URL(string: app.host)?.host ?? "your host"), not to the mail site.").font(.caption).foregroundStyle(.secondary)
                Spacer()
                Button("Cancel") { dismiss() }
                Button { send() } label: { Label("Send to a sealed cell", systemImage: "lock.shield") }
                    .primaryButton().disabled(effective == nil || text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty).keyboardShortcut(.defaultAction)
            }
        }
    }

    private func send() {
        guard let chosen = effective, var page else { return }
        page.title = subject
        let eml = EmailBuilder.eml(from: page, body: redaction.text)
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("email-\(UUID().uuidString.prefix(8)).eml")
        do { try eml.write(to: url, atomically: true, encoding: .utf8) } catch { problem = error.localizedDescription; return }
        // The text was already reviewed and checked in the sheet, so the file-level upload checks are not repeated.
        app.run(demo: chosen, files: [url], source: "browser email", skipChecks: true)
        dismiss()
    }
}
