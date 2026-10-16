import Foundation
import MerionKit

/// One conversation with an agent. Depends only on `ChatAPI` (threads, messages, the streamed run) — never on `ApprovalSigner` or a
/// `TokenStore`, so the chat cannot itself decide an approval or reach a secret; deciding stays in Approvals, signed on this Mac.
@MainActor
final class ChatModel: ObservableObject {
    struct Bubble: Identifiable, Equatable {
        enum Kind: Equatable { case text, status, approval(ApprovalNotice), decided(String), error }
        let id: String
        var role: ChatMessage.Role
        var text: String
        var kind: Kind = .text
        var streaming = false
    }

    @Published var agent: String
    @Published private(set) var threads: [ChatThread] = []
    @Published private(set) var bubbles: [Bubble] = []
    @Published private(set) var threadId = UUID().uuidString
    @Published private(set) var openThreadId: String?
    @Published private(set) var sending = false
    @Published private(set) var pendingApproval: ApprovalNotice?
    @Published var error: String?

    private let api: () -> (any ChatAPI)?
    private var run: Task<Void, Never>?

    init(agent: String, api: @escaping () -> (any ChatAPI)?) { self.agent = agent; self.api = api }

    func loadThreads() async {
        guard let c = api() else { return }
        threads = (try? await c.threads(agent: agent)) ?? threads
    }

    /// Opens a past conversation. Streaming into it uses the host's own thread id from then on.
    func open(_ t: ChatThread) async {
        run?.cancel(); sending = false; pendingApproval = nil; error = nil
        threadId = t.clientThreadId ?? t.id; openThreadId = t.id
        guard let c = api() else { return }
        let msgs = (try? await c.messages(thread: t.id)) ?? []
        bubbles = msgs.map { Bubble(id: $0.id, role: $0.role, text: $0.text) }
    }

    func newThread() {
        run?.cancel(); sending = false; pendingApproval = nil; error = nil
        threadId = UUID().uuidString; openThreadId = nil; bubbles = []
    }

    func cancel() { run?.cancel(); sending = false }

    /// A file dropped on the window: noted in the conversation. The run itself goes through the same path as everywhere else
    /// (`DropRouter`, `AppState.run`); the chat only says what happened, it does not start or approve anything.
    func receiveDroppedFiles(_ urls: [URL]) {
        let names = urls.map(\.lastPathComponent).joined(separator: ", ")
        bubbles.append(Bubble(id: UUID().uuidString, role: .other, text: "Added \(names). Solvor picked a use case for it — see Runs for progress.", kind: .status))
    }

    /// Sends a message and streams the reply. One run at a time; a new `send` while one is in flight replaces it.
    func send(_ text: String) {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, let c = api() else { return }
        run?.cancel()
        bubbles.append(Bubble(id: UUID().uuidString, role: .user, text: text))
        error = nil; sending = true
        let agent = self.agent, threadId = self.threadId
        run = Task { [weak self] in
            do {
                for try await ev in c.chat(agent: agent, threadId: threadId, runId: UUID().uuidString, text: text) {
                    guard let self, !Task.isCancelled else { return }
                    self.apply(ev)
                }
            } catch {
                guard let self, !Task.isCancelled else { return }
                self.bubbles.append(Bubble(id: UUID().uuidString, role: .other, text: error.localizedDescription, kind: .error))
            }
            guard let self else { return }
            self.sending = false
            await self.loadThreads()
        }
    }

    private func apply(_ ev: AGUIEvent) {
        switch ev {
        case .runStarted, .ignored: break
        case .snapshot(let msgs): bubbles = msgs.map { Bubble(id: $0.id, role: $0.role, text: $0.text) }
        case .textStart(let id): bubbles.append(Bubble(id: id, role: .assistant, text: "", streaming: true))
        case .textDelta(let id, let delta):
            if let i = bubbles.firstIndex(where: { $0.id == id }) { bubbles[i].text += delta }
            else { bubbles.append(Bubble(id: id, role: .assistant, text: delta, streaming: true)) }
        case .textEnd(let id):
            if let i = bubbles.firstIndex(where: { $0.id == id }) { bubbles[i].streaming = false }
        case .approvalRequested(let notice):
            pendingApproval = notice
            bubbles.append(Bubble(id: "approval-\(notice.id.isEmpty ? UUID().uuidString : notice.id)", role: .other, text: notice.prompt, kind: .approval(notice)))
        case .approvalDecided(let id, let decision):
            if pendingApproval?.id == id { pendingApproval = nil }
            bubbles.append(Bubble(id: UUID().uuidString, role: .other, text: decision, kind: .decided(decision)))
        case .status(let s):
            bubbles.append(Bubble(id: UUID().uuidString, role: .other, text: s, kind: .status))
        case .runFinished:
            break
        case .runError(let message, _):
            bubbles.append(Bubble(id: UUID().uuidString, role: .other, text: message, kind: .error))
        }
    }
}
