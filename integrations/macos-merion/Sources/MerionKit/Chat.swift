import Foundation

// Conversations with an agent over AG-UI (docs/merion/AGUI.md) and the stored threads (docs/merion/threads/README.md).
// A chat can show that an approval is waiting and what it would do, and can never decide one: that is `MerionClient.decide`, signed on the device.

public struct ChatThread: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var id: String
    public var agent: String
    public var title: String
    public var clientThreadId: String?
    public var updatedAt: String?
    public var messageCount: Int?
    public var updatedDate: Date? { updatedAt.flatMap(MerionDates.parse) }
    public init(id: String, agent: String, title: String, clientThreadId: String? = nil, updatedAt: String? = nil, messageCount: Int? = nil) {
        self.id = id; self.agent = agent; self.title = title; self.clientThreadId = clientThreadId; self.updatedAt = updatedAt; self.messageCount = messageCount
    }
}

struct ThreadsResponse: Codable { var items: [ChatThread] }

public struct ChatMessage: Equatable, Hashable, Identifiable, Sendable {
    public enum Role: String, Sendable { case user, assistant, other }
    public var id: String
    public var role: Role
    public var text: String
    public var createdAt: String?
    public init(id: String, role: Role, text: String, createdAt: String? = nil) { self.id = id; self.role = role; self.text = text; self.createdAt = createdAt }
    static func role(_ s: String?) -> Role { Role(rawValue: s ?? "") ?? .other }
}

extension ChatMessage: Decodable {
    private enum CodingKeys: String, CodingKey { case id, role, text, createdAt }
    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        role = ChatMessage.role(try c.decodeIfPresent(String.self, forKey: .role))
        text = try c.decodeIfPresent(String.self, forKey: .text) ?? ""
        createdAt = try c.decodeIfPresent(String.self, forKey: .createdAt)
    }
}

struct MessagesResponse: Decodable { var items: [ChatMessage] }

/// An approval the host is holding for the agent, as a chat is told about it.
public struct ApprovalNotice: Equatable, Hashable, Sendable {
    public var id: String
    public var kind: String
    public var prompt: String
    public var preview: ApprovalPreview?
    public init(id: String, kind: String, prompt: String, preview: ApprovalPreview? = nil) { self.id = id; self.kind = kind; self.prompt = prompt; self.preview = preview }
}

public enum AGUIEvent: Equatable, Sendable {
    case runStarted
    case snapshot([ChatMessage])
    case textStart(id: String)
    case textDelta(id: String, text: String)
    case textEnd(id: String)
    case approvalRequested(ApprovalNotice)
    case approvalDecided(id: String, decision: String)
    case status(String)
    case runFinished(result: String?)
    case runError(message: String, code: String?)
    case ignored
}

public enum AGUI {
    /// One `data:` line of the stream (the runtime writes each event on a single line) as an event. A malformed line is `nil`, not an error:
    /// one bad event must not end a conversation.
    public static func event(fromData json: String) -> AGUIEvent? {
        guard let data = json.data(using: .utf8), let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any], let type = o["type"] as? String else { return nil }
        switch type {
        case "RUN_STARTED": return .runStarted
        case "MESSAGES_SNAPSHOT":
            let list = (o["messages"] as? [[String: Any]] ?? []).compactMap { m -> ChatMessage? in
                guard let id = m["id"] as? String else { return nil }
                return ChatMessage(id: id, role: ChatMessage.role(m["role"] as? String), text: m["content"] as? String ?? "")
            }
            return .snapshot(list)
        case "TEXT_MESSAGE_START": return .textStart(id: o["messageId"] as? String ?? "")
        case "TEXT_MESSAGE_CONTENT": return .textDelta(id: o["messageId"] as? String ?? "", text: o["delta"] as? String ?? "")
        case "TEXT_MESSAGE_END": return .textEnd(id: o["messageId"] as? String ?? "")
        case "RUN_FINISHED":
            let r = o["result"]
            return .runFinished(result: r as? String ?? (r == nil || r is NSNull ? nil : String(data: (try? JSONSerialization.data(withJSONObject: r!, options: [.sortedKeys, .fragmentsAllowed])) ?? Data(), encoding: .utf8)))
        case "RUN_ERROR": return .runError(message: o["message"] as? String ?? "The run failed", code: o["code"] as? String)
        case "CUSTOM":
            let v = o["value"] as? [String: Any] ?? [:]
            // Custom events are `keep.<x>` from a runtime that has not been told otherwise and `merion.<x>` when the
            // client asks for them (forwardedProps.eventNames); handle both.
            let name = (o["name"] as? String).map { $0.hasPrefix("keep.") ? "merion." + $0.dropFirst(5) : $0 }
            switch name {
            case "merion.approval_requested":
                guard let id = v["approval_id"] as? String else { return .status("waiting for your approval") }   // an approval the agent itself asked for: no id to show
                var preview: ApprovalPreview?
                if let p = v["preview"], let d = try? JSONSerialization.data(withJSONObject: p) { preview = try? JSONDecoder().decode(ApprovalPreview.self, from: d) }
                return .approvalRequested(ApprovalNotice(id: id, kind: v["kind"] as? String ?? "send", prompt: v["prompt"] as? String ?? "", preview: preview))
            case "merion.approval_decided":
                return .approvalDecided(id: v["approval_id"] as? String ?? "", decision: v["decision"] as? String ?? "")
            case "merion.waiting": return .status("waiting…")
            case "merion.running": return .status("working…")
            default: return .ignored
            }
        default: return .ignored
        }
    }
}

/// What Agent Home needs from a host: threads, messages and the streamed run. Deliberately smaller than `KeepAPI` (no approvals,
/// no devices, no run/upload), so a chat view built against it cannot decide an approval or touch a file upload by construction.
public protocol ChatAPI: Sendable {
    func threads(agent: String?) async throws -> [ChatThread]
    func messages(thread id: String) async throws -> [ChatMessage]
    func deleteThread(_ id: String) async throws
    func chat(agent: String, threadId: String, runId: String, text: String) -> AsyncThrowingStream<AGUIEvent, Error>
}

extension MerionClient: ChatAPI {}

extension MerionClient {
    /// The conversations the host holds for this person, newest first, optionally only those with one agent.
    public func threads(agent: String? = nil) async throws -> [ChatThread] {
        let all = (try await getJSON("/v1/threads") as ThreadsResponse).items
        return all.filter { agent == nil || $0.agent == agent }.sorted { ($0.updatedDate ?? .distantPast) > ($1.updatedDate ?? .distantPast) }
    }

    public func messages(thread id: String) async throws -> [ChatMessage] {
        (try await getJSON("/v1/threads/\(pathEscape(id))/messages") as MessagesResponse).items
    }

    /// Forget a conversation on the host.
    public func deleteThread(_ id: String) async throws {
        _ = try await sendRequest(request("/v1/threads/\(pathEscape(id))", method: "DELETE"))
    }

    /// Say something to an agent and stream what it does. `threadId` is the client's own conversation id (the host keeps the conversation under it),
    /// `runId` is unique per message. The stream ends after `runFinished` or `runError`.
    public func chat(agent: String, threadId: String, runId: String = UUID().uuidString, text: String) -> AsyncThrowingStream<AGUIEvent, Error> {
        AsyncThrowingStream { continuation in
            let task = Task {
                do {
                    var req = request("/v1/agui", method: "POST")
                    req.setValue("application/json", forHTTPHeaderField: "content-type")
                    req.setValue("text/event-stream", forHTTPHeaderField: "Accept")
                    req.timeoutInterval = 300
                    let body: [String: Any] = [
                        "threadId": threadId, "runId": runId, "state": [String: String](),
                        "messages": [["id": UUID().uuidString, "role": "user", "content": text]],
                        "forwardedProps": ["agent": agent],
                    ]
                    req.httpBody = try JSONSerialization.data(withJSONObject: body)
                    let (bytes, resp) = try await streamSession.bytes(for: req)
                    guard let http = resp as? HTTPURLResponse else { throw MerionError.transport("not an HTTP response") }
                    if http.statusCode == 401 { throw MerionError.unauthorized }
                    guard (200..<300).contains(http.statusCode) else {
                        var data = Data()
                        for try await b in bytes { data.append(b); if data.count > 2000 { break } }
                        throw MerionError.http(status: http.statusCode, message: Self.errorMessage(data))
                    }
                    for try await line in bytes.lines where line.hasPrefix("data:") {
                        if let ev = AGUI.event(fromData: String(line.dropFirst(5)).trimmingCharacters(in: .whitespaces)), ev != .ignored {
                            continuation.yield(ev)
                            if case .runFinished = ev { break }
                            if case .runError = ev { break }
                        }
                    }
                    continuation.finish()
                } catch let e as MerionError { continuation.finish(throwing: e)
                } catch is CancellationError { continuation.finish()
                } catch let e as URLError { continuation.finish(throwing: e.code == .cancelled ? nil : MerionError.transport(e.localizedDescription))
                } catch { continuation.finish(throwing: MerionError.transport("\(error)")) }
            }
            continuation.onTermination = { _ in task.cancel() }
        }
    }
}
