import Foundation

public enum AppScreen: String, Equatable, Sendable { case useCases, runs, approvals, watchFolders, settings }

public enum VoiceTarget: Equatable, Sendable { case latestDownload, clipboard }

/// What a spoken command asks for. Deliberately small: nothing here can approve, deny, send or delete.
public enum VoiceCommand: Equatable, Sendable {
    case readBrowserEmail
    case summarise(VoiceTarget, useCase: String?)
    case show(AppScreen)
    case lastRun
    /// The person asked to approve or deny by voice; that is only ever done with Touch ID, so the app opens the approvals screen instead.
    case approvalNeedsTouchID
    case unknown(String)

    /// The plain-English "I will ..." line shown before anything happens.
    public func describe(useCaseTitle: (String) -> String? = { _ in nil }) -> String {
        switch self {
        case .readBrowserEmail: return "Read the email open in your browser, then show you a preview before anything is sent."
        case .summarise(let t, let uc):
            let what = t == .latestDownload ? "your newest file in Downloads" : "the text on your clipboard"
            return "Summarise \(what)" + (uc.map { " with \(useCaseTitle($0) ?? $0)" } ?? " with the best-matching use case") + ", after showing you what will be sent."
        case .show(let s): return "Open \(s.rawValue == "watchFolders" ? "Watch folders" : s.rawValue == "useCases" ? "Use cases" : s.rawValue.capitalized)."
        case .lastRun: return "Show your most recent run."
        case .approvalNeedsTouchID: return "Open Approvals. Approving or denying is only done with Touch ID, never by voice."
        case .unknown(let t): return "I did not understand \"\(t)\"."
        }
    }
}

/// English text to a `VoiceCommand`, by fixed rules (no model). The text may have come from speech recognition and translation, so it is lenient
/// about wording and strict about what it will trigger.
public enum VoiceCommandParser {
    public static func parse(_ raw: String, useCases: [(id: String, title: String)] = []) -> VoiceCommand {
        let t = normalise(raw)
        guard !t.isEmpty else { return .unknown(raw) }
        func has(_ words: String...) -> Bool { words.contains { t.contains($0) } }

        if has("approve", "approval", "deny", "reject", "accept") && !has("show", "open", "go to") { return .approvalNeedsTouchID }
        if has("email", "mail", "message", "inbox", "gmail", "outlook") && has("read", "open", "check", "summar", "process", "look at", "handle")
            && has("browser", "tab", "page", "gmail", "outlook", "webmail", "this email", "this mail", "this message", "inbox", "open") { return .readBrowserEmail }
        if has("last run", "what did i run", "latest run", "recent run", "last result") { return .lastRun }
        if has("show", "open", "go to", "take me to") {
            if has("approval") { return .show(.approvals) }
            if has("run", "history") { return .show(.runs) }
            if has("watch", "folder") { return .show(.watchFolders) }
            if has("setting") { return .show(.settings) }
            if has("use case", "usecase") { return .show(.useCases) }
        }
        if has("summar", "analys", "analyz", "run", "process", "read", "check") {
            let use = matchUseCase(t, useCases)
            if has("clipboard", "copied", "what i copied") { return .summarise(.clipboard, useCase: use) }
            if has("download", "newest file", "latest file", "last file", "recent file") { return .summarise(.latestDownload, useCase: use) }
        }
        return .unknown(raw)
    }

    static func normalise(_ s: String) -> String {
        s.lowercased().folding(options: .diacriticInsensitive, locale: nil)
            .replacingOccurrences(of: #"[^a-z0-9 ]"#, with: " ", options: .regularExpression)
            .replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression).trimmingCharacters(in: .whitespaces)
    }

    /// A use case is named when every word of its title (or its id split on dashes) appears in the command; the longest match wins.
    static func matchUseCase(_ t: String, _ useCases: [(id: String, title: String)]) -> String? {
        var best: (String, Int)?
        for u in useCases {
            for name in [normalise(u.title), normalise(u.id.replacingOccurrences(of: "-", with: " "))] where !name.isEmpty {
                let words = name.split(separator: " ")
                if words.allSatisfy({ t.contains($0) }), words.count > (best?.1 ?? 0) { best = (u.id, words.count) }
            }
        }
        return best?.0
    }
}
