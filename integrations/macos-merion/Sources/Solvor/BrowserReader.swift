import AppKit
import Foundation
import MerionKit

/// Reads the front tab of a supported browser through macOS scripting, when the person asks. It runs a fixed script (`BrowserScript`)
/// and never anything from a page.
@MainActor
final class BrowserReader: ObservableObject {
    @Published private(set) var running: [BrowserKind] = []
    private(set) var lastActive: BrowserKind?
    private var observer: NSObjectProtocol?

    init() {
        refresh()
        observer = NSWorkspace.shared.notificationCenter.addObserver(forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main) { [weak self] note in
            guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication, let id = app.bundleIdentifier,
                  let kind = BrowserKind.allCases.first(where: { $0.bundleID == id }) else { return }
            Task { @MainActor in self?.lastActive = kind; self?.refresh() }
        }
    }
    deinit { if let observer { NSWorkspace.shared.notificationCenter.removeObserver(observer) } }

    func refresh() {
        running = BrowserKind.allCases.filter { !NSRunningApplication.runningApplications(withBundleIdentifier: $0.bundleID).isEmpty }
    }

    /// The browser to read: the one most recently in front, else the only one running.
    var defaultBrowser: BrowserKind? {
        if let l = lastActive, running.contains(l) { return l }
        return running.first
    }

    struct Failure: Error, LocalizedError { let problem: BrowserScript.Problem; var errorDescription: String? { BrowserScript.help(problem) } }

    func read(_ browser: BrowserKind) async throws -> CapturedPage {
        let source = BrowserScript.appleScript(for: browser)
        let result: (String?, Int, String) = await withCheckedContinuation { cont in
            DispatchQueue.global(qos: .userInitiated).async {
                var err: NSDictionary?
                let script = NSAppleScript(source: source)
                let out = script?.executeAndReturnError(&err)
                if let err {
                    cont.resume(returning: (nil, err[NSAppleScript.errorNumber] as? Int ?? 0, err[NSAppleScript.errorMessage] as? String ?? "The script failed."))
                } else { cont.resume(returning: (out?.stringValue, 0, "")) }
            }
        }
        guard let text = result.0 else { throw Failure(problem: BrowserScript.explain(errorNumber: result.1, message: result.2, browser: browser)) }
        return try BrowserScript.parse(text)
    }
}
