import Foundation

// A person's opt-in memory (docs/merion/memory) and the receipts of what happened after they approved (docs/merion/receipts).
// Memory is data ABOUT the person that they control: an agent may only propose an entry, and it is used only once they accept it.

public struct MemoryEntry: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var id: String
    public var text: String
    public var kind: String
    public var pinned: Bool?
    /// Proposed by an agent that had read untrusted content.
    public var tainted: Bool?
    public var origin: String?
    public var isPinned: Bool { pinned == true }
}

public struct MemoryView: Decodable, Equatable, Sendable {
    public var enabled: Bool
    public var items: [MemoryEntry]
    /// Entries an agent proposed, waiting for the person. Not used until accepted.
    public var proposals: [MemoryEntry]
    public init(enabled: Bool, items: [MemoryEntry], proposals: [MemoryEntry]) { self.enabled = enabled; self.items = items; self.proposals = proposals }
}

/// What was done after an approval: when, what (method, host, path), how it ended, which agent. The host never sends the body or its digest here.
public struct Receipt: Decodable, Equatable, Hashable, Identifiable, Sendable {
    public var id: String
    public var at: String?
    public var agent: String?
    public var credential: String?
    public var method: String
    public var url: String
    public var status: Int
    public var approvalId: String?

    public var atDate: Date? { at.flatMap(KeepDates.parse) }
    public var approved: Bool { approvalId != nil }
    public var succeeded: Bool { status < 400 }
    public var host: String { URL(string: url)?.host ?? "" }
    public var path: String { URL(string: url)?.path ?? url }
}

struct ReceiptsResponse: Decodable { var items: [Receipt] }

extension KeepClient {
    public func memory() async throws -> MemoryView { try await getJSON("/v1/memory") }

    public func setMemory(enabled: Bool) async throws {
        var req = request("/v1/memory/settings", method: "PUT")
        req.setValue("application/json", forHTTPHeaderField: "content-type")
        req.httpBody = try JSONSerialization.data(withJSONObject: ["enabled": enabled])
        _ = try await sendRequest(req)
    }

    /// The person adds an entry themselves. Kind is `preference`, `fact` or `note`. The host refuses text that looks like a credential.
    public func addMemory(text: String, kind: String = "note", pinned: Bool = false) async throws {
        var req = request("/v1/memory", method: "POST")
        req.setValue("application/json", forHTTPHeaderField: "content-type")
        req.httpBody = try JSONSerialization.data(withJSONObject: ["text": text, "kind": kind, "pinned": pinned] as [String: Any])
        _ = try await sendRequest(req)
    }

    public func deleteMemory(_ id: String) async throws { _ = try await sendRequest(request("/v1/memory/\(pathEscape(id))", method: "DELETE")) }

    /// Accept (`true`) or reject (`false`) an entry an agent proposed.
    public func decideMemoryProposal(_ id: String, accept: Bool) async throws {
        _ = try await sendRequest(request("/v1/memory/\(pathEscape(id))/\(accept ? "accept" : "reject")", method: "POST"))
    }

    /// Forget everything (the on/off switch stays).
    public func forgetAllMemory() async throws { _ = try await sendRequest(request("/v1/memory", method: "DELETE")) }

    /// What was done after the person approved, newest first.
    public func receipts(limit: Int = 50) async throws -> [Receipt] {
        var req = request("/v1/receipts", query: [URLQueryItem(name: "limit", value: String(limit))])
        req.httpMethod = "GET"
        let (data, _) = try await sendRequest(req)
        do { return try JSONDecoder.keep.decode(ReceiptsResponse.self, from: data).items } catch { throw KeepError.decoding("\(error)") }
    }
}
