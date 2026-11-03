import AppKit
import MerionKit

/// The "Send to Keep" entry in Finder's right-click Services menu. macOS finds it through NSServices in Info.plist
/// and calls `sendToMerion` on the services provider.
final class ServicesProvider: NSObject {
    @objc func sendToMerion(_ pboard: NSPasteboard, userData: String?, error: AutoreleasingUnsafeMutablePointer<NSString>) {
        guard let urls = pboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL], !urls.isEmpty else {
            error.pointee = "Keep needs at least one file." as NSString; return
        }
        Task { @MainActor in
            NSApp.activate(ignoringOtherApps: true)
            DropRouter.route(urls, app: AppState.shared, source: "service")
        }
    }
}
