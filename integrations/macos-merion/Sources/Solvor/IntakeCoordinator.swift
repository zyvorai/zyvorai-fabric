import Foundation
import MerionKit

/// What to do with a set of files a person handed to Solvor — dropped on the window or the Dock icon, opened from Finder,
/// sent by a Service or a Shortcut, or matched by a watched folder. One place decides this, so every entry point behaves
/// the same way and no file is silently left out without a word.
enum IntakeCoordinator {
    struct Plan: Equatable {
        enum Outcome: Equatable {
            case run(demoId: String, files: [URL])
            /// Several use cases read the one file dropped; the person picks.
            case choose(files: [URL], optionIds: [String])
            case none
        }
        var outcome: Outcome
        /// Files that were part of the drop but not this run, because they do not match the use case the other files chose
        /// (a mixed drop of a PDF and a spreadsheet, say). Never silently dropped: `AppState` turns this into a notice.
        var leftOut: [URL] = []
    }

    /// `urls` is expanded (folders become their files, one level deep) before deciding.
    static func plan(urls: [URL], demos: [Demo]) -> Plan {
        let files = FileExpander.files(from: urls)
        guard let first = files.first else { return Plan(outcome: .none) }
        let suggestions = Catalog.suggestions(forFileNamed: first.lastPathComponent, among: demos)
        guard let best = suggestions.first else {
            // Nothing at all reads the first file; the rest are left out too rather than guessed at individually.
            return Plan(outcome: .none, leftOut: files)
        }
        let accepted = Set(best.accepts.map { $0.lowercased() })
        let matching = files.filter { accepted.contains($0.pathExtension.lowercased()) }
        let leftOut = files.filter { !accepted.contains($0.pathExtension.lowercased()) }
        if suggestions.count > 1 && files.count == 1 {
            return Plan(outcome: .choose(files: matching, optionIds: suggestions.prefix(6).map(\.id)), leftOut: leftOut)
        }
        return Plan(outcome: .run(demoId: best.id, files: matching), leftOut: leftOut)
    }

    /// Words for the files a plan left out, or nil when it left nothing out.
    static func leftOutNotice(_ plan: Plan) -> String? {
        guard !plan.leftOut.isEmpty else { return nil }
        if case .none = plan.outcome { return nil }   // nothing ran; the "no use case reads it" message already covers this
        let names = plan.leftOut.map(\.lastPathComponent).joined(separator: ", ")
        return plan.leftOut.count == 1
            ? "\(names) wasn't included: no use case reads it together with the rest. Drop it on its own."
            : "\(names) weren't included: no use case reads them together with the rest. Drop them on their own."
    }
}
