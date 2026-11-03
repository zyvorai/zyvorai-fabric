import MerionKit
import SwiftUI

/// The connection to one Keep host: its address (a preference), the person's token (the Keychain, never a preference or a file), the agent to chat
/// with, and the approvals waiting. Everything else is asked of the host when a screen needs it.
@MainActor
final class AppModel: ObservableObject {
    @AppStorage("host") var host = ""
    @AppStorage("agent") var agent = "echo-agent"
    @AppStorage("deviceId") var deviceId = ""
    @Published private(set) var token: String?
    @Published private(set) var approvals: [Approval] = []
    @Published var problem: String?

    private let store: TokenStore

    init(store: TokenStore = KeychainTokenStore(service: "dev.zyvor.merion.phone.token", legacyService: "dev.zyvor.keep.phone.token")) {
        self.store = store
        token = (try? store.token()) ?? nil
    }

    var isConnected: Bool { client != nil }
    var pendingCount: Int { approvals.filter(\.isPending).count }

    var client: MerionClient? {
        guard let token, !token.isEmpty, let url = URL(string: host.trimmingCharacters(in: .whitespaces)) else { return nil }
        return try? MerionClient(baseURL: url, token: token)
    }

    func saveToken(_ value: String) {
        let t = value.trimmingCharacters(in: .whitespacesAndNewlines)
        do {
            if t.isEmpty { try store.clear(); token = nil } else { try store.save(t); token = t }
        } catch { problem = error.localizedDescription }
    }

    func forget() { saveToken(""); approvals = [] }

    /// The approvals waiting for this person (their inbox), with what the host would do and what to sign.
    func refreshApprovals() async {
        guard let client else { approvals = []; return }
        do { approvals = try await client.inbox().pendingApprovals.filter(\.isPending); problem = nil }
        catch { problem = error.localizedDescription }
    }

    /// Signs the decision on this device (Secure Enclave, Face ID or Touch ID) over the exact text the host checks, then sends it.
    func decide(_ approval: Approval, _ decision: Decision) async throws {
        guard let client else { throw MerionError.badURL }
        var signature: String?
        if approval.sign != nil {
            guard !deviceId.isEmpty else { throw SignerError.noChallenge }
            let signer = ApprovalSigner(key: try SecureEnclaveKey.loadOrCreate(), deviceId: deviceId)
            signature = try signer.signature(for: approval, decision: decision).signature
        }
        try await client.decide(approval: approval.id, decision, deviceId: signature == nil ? nil : deviceId, signature: signature)
        await refreshApprovals()
    }
}
