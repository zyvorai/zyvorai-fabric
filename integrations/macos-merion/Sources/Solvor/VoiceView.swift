import AppKit
import MerionKit
import Speech
import SwiftUI
import Translation

/// Give Solvor a spoken (or typed) command in your language. It is turned into English, shown back to you as "I will …", and only acts when you confirm.
struct VoiceView: View {
    @EnvironmentObject var app: AppState
    @Environment(\.dismiss) private var dismiss
    @StateObject private var listener = VoiceListener()

    @State private var localeID = ""
    @State private var typed = ""
    @State private var heard = ""
    @State private var english: String?
    @State private var translating = false
    @State private var translateProblem: String?
    @State private var translateRequest: TranslateRequest?

    struct TranslateRequest: Equatable { let id = UUID(); let text: String; let source: Locale.Language }

    private var locale: Locale { Locale(identifier: localeID.isEmpty ? (Locale.current.identifier) : localeID) }
    private var useCases: [(id: String, title: String)] { app.demos.map { ($0.id, $0.title) } }
    private var command: VoiceCommand? { english.map { VoiceCommandParser.parse($0, useCases: useCases) } }

    var body: some View {
        VStack(spacing: 18) {
            HStack {
                AccentTile(symbol: "mic.fill")
                VStack(alignment: .leading) { Text("Talk to Solvor").font(.title3.bold()); Text("Speak in your language. Solvor turns it into English and asks before it acts.").font(.callout).foregroundStyle(.secondary) }
                Spacer()
                Button("Close") { listener.finish(); dismiss() }
            }
            HStack {
                Picker("Language", selection: $localeID) {
                    ForEach(VoiceListener.locales(), id: \.identifier) { Text(Locale.current.localizedString(forIdentifier: $0.identifier) ?? $0.identifier).tag($0.identifier) }
                }.frame(maxWidth: 320)
                Spacer()
            }
            micButton
            VStack(alignment: .leading, spacing: 8) {
                if listener.listening || !listener.transcript.isEmpty { line("I hear", listener.transcript.isEmpty ? "…" : listener.transcript) }
                if translating { HStack { ProgressView().controlSize(.small); Text("Translating to English…").foregroundStyle(.secondary) } }
                if let english, !heard.isEmpty, heard != english { line("In English", english) }
                if let command { line("I will", command.describe(useCaseTitle: { id in app.demo(id)?.title })).foregroundStyle(isUnknown ? .orange : .primary) }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            if let p = listener.problem ?? translateProblem { Text(p).font(.callout).padding(12).frame(maxWidth: .infinity, alignment: .leading).background(Color.orange.opacity(0.14), in: RoundedRectangle(cornerRadius: 10)) }
            HStack {
                TextField("Or type a command", text: $typed).textFieldStyle(.roundedBorder).onSubmit { submit(typed) }
                Button("Use") { submit(typed) }.disabled(typed.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            Divider()
            HStack {
                Text("Voice can open screens, read your browser email, and summarise a file it shows you first. It can never approve, deny, send or delete.").font(.caption).foregroundStyle(.tertiary)
                Spacer()
                Button("Cancel") { listener.finish(); dismiss() }
                Button { if let command { listener.finish(); app.perform(command); dismiss() } } label: { Label("Do it", systemImage: "checkmark") }
                    .primaryButton().disabled(command == nil || isUnknown).keyboardShortcut(.defaultAction)
            }
        }
        .padding(24).frame(width: 640, height: 560)
        .onAppear {
            localeID = VoiceListener.bestLocaleID(app.speechLanguage.isEmpty ? Locale.current.identifier : app.speechLanguage)
            listener.onFinished = { text in handle(text) }
        }
        .onChange(of: localeID) { _, new in app.speechLanguage = new }
        .background { TranslateBridge(request: translateRequest, onResult: { english = $0; translating = false }, onError: { translateProblem = $0; translating = false }) }
    }

    private var isUnknown: Bool { if case .unknown = command { return true }; return false }

    private var micButton: some View {
        Button {
            if listener.listening { listener.finish() } else { reset(); Task { await listener.start(locale: locale) } }
        } label: {
            ZStack {
                Circle().fill(Color.accentColor.opacity(0.18)).frame(width: 96 + CGFloat(listener.level) * 60, height: 96 + CGFloat(listener.level) * 60).animation(.easeOut(duration: 0.1), value: listener.level)
                Circle().fill(listener.listening ? AnyShapeStyle(Color.red) : AnyShapeStyle(Color.accentColor.gradient)).frame(width: 84, height: 84)
                Image(systemName: listener.listening ? "stop.fill" : "mic.fill").font(.system(size: 30)).foregroundStyle(.white)
            }.frame(height: 150)
        }.buttonStyle(.plain).help(listener.listening ? "Stop listening" : "Start listening")
    }

    private func line(_ label: String, _ text: String) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) { Text(label).font(.caption.weight(.semibold)).foregroundStyle(.secondary).frame(width: 64, alignment: .trailing); Text(text).textSelection(.enabled) }
    }

    private func reset() { heard = ""; english = nil; translateProblem = nil; translating = false; listener.problem = nil }

    private func submit(_ text: String) {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines); guard !t.isEmpty else { return }
        reset(); listener.transcript = t; handle(t)
    }

    /// Heard text: English goes straight to the parser; anything else is translated on the device first.
    private func handle(_ text: String) {
        heard = text
        let lang = locale.language
        if lang.languageCode?.identifier == "en" { english = text; return }
        translating = true
        translateRequest = TranslateRequest(text: text, source: lang)
    }
}

/// Apple's on-device Translation, driven from SwiftUI. The first use of a language may ask to download it.
struct TranslateBridge: View {
    let request: VoiceView.TranslateRequest?
    let onResult: (String) -> Void
    let onError: (String) -> Void
    @State private var configuration: TranslationSession.Configuration?

    var body: some View {
        Color.clear
            .translationTask(configuration) { session in
                guard let text = request?.text else { return }
                do { onResult(try await session.translate(text).targetText) } catch { onError("Translation failed: \(error.localizedDescription). Download the language in System Settings, General, Language & Region, Translation Languages.") }
            }
            .onChange(of: request) { _, new in
                guard let new else { return }
                configuration = TranslationSession.Configuration(source: new.source, target: Locale.Language(identifier: "en"))
            }
    }
}
