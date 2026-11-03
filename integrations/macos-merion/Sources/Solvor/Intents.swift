import AppIntents
import Foundation
import MerionKit

/// Lets Shortcuts and Siri run a Keep use case on a file. The intent only starts the run and returns the summary;
/// it has the same limits as the app (a file you pass, a sealed cell, no commands on your Mac).
struct RunUseCaseIntent: AppIntent {
    static var title: LocalizedStringResource = "Summarise a file with Keep"
    static var description = IntentDescription("Runs a Keep use case on a file in a sealed cell and returns the summary.")

    @Parameter(title: "Use case", description: "The use case id, for example pdf-brief or csv-clean.", default: "pdf-brief")
    var useCase: String

    @Parameter(title: "File")
    var file: IntentFile

    static var parameterSummary: some ParameterSummary { Summary("Summarise \(\.$file) with \(\.$useCase)") }

    func perform() async throws -> some IntentResult & ReturnsValue<String> {
        let (client, demo) = try await MainActor.run { () -> (MerionClient, Demo?) in
            guard let c = AppState.shared.client else { throw IntentError.notConnected }
            return (c, AppState.shared.demo(useCase))
        }
        let name = file.filename.isEmpty ? "file" : file.filename
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("intent-\(UUID().uuidString)-\(name)")
        try file.data.write(to: url)
        defer { try? FileManager.default.removeItem(at: url) }
        let outcome = try await client.run(demo: useCase, files: [url], maxBytes: demo?.maxBytes)
        guard let ref = outcome.items.first?.result?.artifacts.first(where: { $0.title.hasSuffix(".md") }) ?? outcome.items.first?.result?.artifacts.first else {
            throw IntentError.noResult(outcome.items.first?.error ?? "the run returned nothing")
        }
        return .result(value: try await client.artifact(id: ref.id).body)
    }
}

enum IntentError: Error, CustomLocalizedStringResourceConvertible {
    case notConnected, noResult(String)
    var localizedStringResource: LocalizedStringResource {
        switch self {
        case .notConnected: return "Solvor is not connected to a host. Open the app and add the host and token in Settings."
        case .noResult(let m): return "Keep returned no summary: \(m)"
        }
    }
}

/// Opens Solvor on the email reader; the person reviews the page text before anything is sent.
struct ReadBrowserEmailIntent: AppIntent {
    static var title: LocalizedStringResource = "Read my browser email with Solvor"
    static var description = IntentDescription("Opens Solvor's email reader for the message open in your browser. You review the text before it is sent.")
    static var openAppWhenRun = true
    @MainActor func perform() async throws -> some IntentResult { AppState.shared.perform(.readBrowserEmail); return .result() }
}

struct SummariseLatestDownloadIntent: AppIntent {
    static var title: LocalizedStringResource = "Summarise my latest download with Solvor"
    static var description = IntentDescription("Picks your newest file in Downloads, shows you which one, and reads it with the matching use case.")
    static var openAppWhenRun = true
    @MainActor func perform() async throws -> some IntentResult { AppState.shared.perform(.summarise(.latestDownload, useCase: nil)); return .result() }
}

struct ShowApprovalsIntent: AppIntent {
    static var title: LocalizedStringResource = "Show my approvals in Solvor"
    static var openAppWhenRun = true
    @MainActor func perform() async throws -> some IntentResult { AppState.shared.perform(.show(.approvals)); return .result() }
}

/// Takes a sentence, for example dictated in a Shortcut and translated to English by the Translate action, and runs it through the same fixed command parser as the app.
struct TellSolvorIntent: AppIntent {
    static var title: LocalizedStringResource = "Tell Solvor"
    static var description = IntentDescription("Runs an English command such as \"read the email in my browser\". It can never approve or deny anything.")
    static var openAppWhenRun = true
    @Parameter(title: "Command", requestValueDialog: "What should Solvor do?") var command: String
    static var parameterSummary: some ParameterSummary { Summary("Tell Solvor \(\.$command)") }
    @MainActor func perform() async throws -> some IntentResult {
        let app = AppState.shared
        app.perform(VoiceCommandParser.parse(command, useCases: app.demos.map { ($0.id, $0.title) }))
        return .result()
    }
}

struct SolvorShortcuts: AppShortcutsProvider {
    static var appShortcuts: [AppShortcut] {
        AppShortcut(intent: ReadBrowserEmailIntent(), phrases: ["Read my email with \(.applicationName)", "Read my browser email with \(.applicationName)"], shortTitle: "Read browser email", systemImageName: "envelope.open")
        AppShortcut(intent: SummariseLatestDownloadIntent(), phrases: ["Summarise my latest download with \(.applicationName)", "Read my newest file with \(.applicationName)"], shortTitle: "Summarise latest download", systemImageName: "arrow.down.doc")
        AppShortcut(intent: ShowApprovalsIntent(), phrases: ["Show my approvals in \(.applicationName)"], shortTitle: "Show approvals", systemImageName: "checkmark.seal")
        AppShortcut(intent: TellSolvorIntent(), phrases: ["Tell \(.applicationName) something", "Give \(.applicationName) a command"], shortTitle: "Tell Solvor", systemImageName: "mic")
        AppShortcut(intent: RunUseCaseIntent(), phrases: ["Summarise a file with \(.applicationName)", "Run \(.applicationName) on a file"], shortTitle: "Summarise a file", systemImageName: "doc.text.magnifyingglass")
    }
}
