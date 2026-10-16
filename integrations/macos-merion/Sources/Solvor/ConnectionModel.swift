import Foundation
import MerionKit
import SwiftUI

/// The state behind "connect": what the person has typed, the hosts found on this Mac, and the link they pasted.
/// It only prepares values; `AppState.saveConnection` is what stores a token and contacts the host.
@MainActor
final class ConnectionModel: ObservableObject {
    @Published var hostText = ""
    @Published var tokenText = ""
    @Published private(set) var found: [URL] = []
    @Published private(set) var probing = false
    /// Set when a pasted link points a token at a plain-http host on another machine.
    @Published private(set) var warning: String?

    private let session: URLSession
    init(session: URLSession = .shared) { self.session = session }

    var canConnect: Bool { !hostText.trimmingCharacters(in: .whitespaces).isEmpty && !tokenText.trimmingCharacters(in: .whitespaces).isEmpty }

    /// Looks for a host on this Mac. The first one found fills the address if the person has not typed one.
    func probe() async {
        probing = true; defer { probing = false }
        found = await HostProbe.local(session: session)
        if hostText.isEmpty, let first = found.first { hostText = first.absoluteString }
    }

    /// Fills the fields from pasted text: a `keep://connect` link, or a bare token. Returns whether anything was recognised.
    @discardableResult
    func paste(_ text: String) -> Bool {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if let link = ConnectionLink(t) {
            hostText = link.host.absoluteString; tokenText = link.token
            warning = link.isPlainRemote ? "This link sends the token over plain http to \(link.host.host ?? "another machine"). Anyone on the network could read it." : nil
            return true
        }
        if !t.isEmpty, !t.contains(" "), !t.contains("\n"), t.count >= 8, !t.hasPrefix("http") { tokenText = t; warning = nil; return true }
        return false
    }

    func use(_ host: URL) { hostText = host.absoluteString }

    func connect(_ app: AppState) async {
        await app.saveConnection(host: hostText.trimmingCharacters(in: .whitespaces), token: tokenText.trimmingCharacters(in: .whitespaces))
        if app.connected { tokenText = "" }
    }
}
