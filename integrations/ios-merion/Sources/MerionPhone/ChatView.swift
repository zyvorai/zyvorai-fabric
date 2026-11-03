import MerionKit
import SwiftUI

/// One conversation. It shows the agent's answers as they stream, and shows an approval the host is holding (with what the host read out of the request)
/// as a read-only card: it can say "waiting for your phone" and can never decide it. Deciding is the Approvals tab, signed on this device.
struct ChatView: View {
    @EnvironmentObject var model: AppModel
    let target: ChatTarget

    @State private var messages: [ChatMessage] = []
    @State private var cards: [ApprovalNotice] = []
    @State private var closed: [String: String] = [:]
    @State private var draft = ""
    @State private var status: String?
    @State private var error: String?
    @State private var running = false
    @State private var streaming: String?
    @State private var sawText = false

    private var clientThreadId: String? {
        switch target { case .new(let id): return id; case .existing(let t): return t.clientThreadId }
    }
    private var agentName: String {
        if case .existing(let t) = target { return t.agent }
        return model.agent
    }
    private var title: String { if case .existing(let t) = target { return t.title } else { return agentName } }

    var body: some View {
        VStack(spacing: 0) {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(spacing: 6) {
                        ForEach(messages) { m in Bubble(message: m).id(m.id) }
                        ForEach(cards, id: \.id) { c in ApprovalCard(notice: c, outcome: closed[c.id]).id(c.id) }
                        if let status { Text(status).font(.caption).foregroundStyle(.secondary).id("status") }
                        if let error { Text(error).font(.callout).foregroundStyle(.red).padding(8).id("error") }
                    }.padding(12)
                }
                .onChange(of: messages) { _, _ in withAnimation { proxy.scrollTo(messages.last?.id, anchor: .bottom) } }
                .onChange(of: cards.count) { _, _ in withAnimation { proxy.scrollTo(cards.last?.id, anchor: .bottom) } }
            }
            Divider()
            if clientThreadId == nil {
                Text("This conversation was made outside the app, so it can be read here but not continued.").font(.footnote).foregroundStyle(.secondary).padding()
            } else {
                HStack(alignment: .bottom, spacing: 8) {
                    TextField("Message", text: $draft, axis: .vertical).lineLimit(1...5).textFieldStyle(.roundedBorder).submitLabel(.send).onSubmit(send)
                    Button(action: send) { Image(systemName: "arrow.up.circle.fill").font(.title) }
                        .disabled(running || draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }.padding(10)
                Text("Approvals are decided on this device, in the Approvals tab. A chat cannot approve or deny anything.").font(.caption2).foregroundStyle(.secondary).padding(.bottom, 6)
            }
        }
        .navigationTitle(title).navigationBarTitleDisplayMode(.inline)
        .task { await loadHistory() }
    }

    private func loadHistory() async {
        guard case .existing(let t) = target, let client = model.client else { return }
        do { messages = try await client.messages(thread: t.id) } catch { self.error = error.localizedDescription }
    }

    private func send() {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, !running, let client = model.client, let tid = clientThreadId else { return }
        draft = ""; error = nil; running = true; sawText = false; status = "sending…"
        messages.append(ChatMessage(id: "me-\(UUID().uuidString)", role: .user, text: text))
        Task {
            defer { running = false; status = nil; streaming = nil; Task { await model.refreshApprovals() } }
            do {
                for try await ev in client.chat(agent: agentName, threadId: tid, text: text) { handle(ev) }
            } catch { self.error = error.localizedDescription }
        }
    }

    private func handle(_ ev: AGUIEvent) {
        switch ev {
        case .runStarted: status = "working…"
        case .snapshot: break   // this screen already shows the conversation
        case .textStart(let id): streaming = id; sawText = true; messages.append(ChatMessage(id: id, role: .assistant, text: "")); status = nil
        case .textDelta(let id, let t):
            sawText = true
            if let i = messages.lastIndex(where: { $0.id == id }) { messages[i].text += t }
            else { messages.append(ChatMessage(id: id, role: .assistant, text: t)); status = nil }
        case .textEnd: streaming = nil
        case .approvalRequested(let n): cards.append(n); status = "waiting for your approval"
        case .approvalDecided(let id, let decision): closed[id] = decision; status = "working…"
        case .status(let s): status = s
        case .runFinished(let result):
            // an agent that only returns a value (no printed text) still answers
            if !sawText, let r = result { messages.append(ChatMessage(id: "result-\(UUID().uuidString)", role: .assistant, text: r)) }
            status = nil
        case .runError(let message, let code): error = message + (code.map { " (\($0))" } ?? ""); status = nil
        case .ignored: break
        }
    }
}

struct Bubble: View {
    let message: ChatMessage
    var body: some View {
        HStack {
            if message.role == .user { Spacer(minLength: 40) }
            Text(message.text)
                .padding(.horizontal, 12).padding(.vertical, 8)
                .background(message.role == .user ? Color.accentColor : Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 16))
                .foregroundStyle(message.role == .user ? Color.white : Color.primary)
                .textSelection(.enabled)
            if message.role != .user { Spacer(minLength: 40) }
        }
    }
}

/// An approval the host holds for the agent: what would be sent (as the host read it), and where to decide it.
struct ApprovalCard: View {
    let notice: ApprovalNotice
    let outcome: String?
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Label("Waiting for you", systemImage: "hourglass").font(.subheadline.bold()).foregroundStyle(.orange)
            if let p = notice.preview {
                Grid(alignment: .topLeading, horizontalSpacing: 10, verticalSpacing: 3) {
                    ForEach(Array(p.fields.prefix(24).enumerated()), id: \.offset) { _, f in
                        GridRow { Text(f.label).foregroundStyle(.secondary); Text(f.value).textSelection(.enabled) }.font(.callout)
                    }
                }
            } else { Text(notice.prompt).font(.callout) }
            Text(outcomeText).font(.caption).foregroundStyle(.secondary)
        }
        .padding(12).frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 14))
        .overlay(RoundedRectangle(cornerRadius: 14).stroke(outcome == "approved" ? Color.green : outcome == nil ? Color.orange : Color.red, lineWidth: 1))
    }
    private var outcomeText: String {
        switch outcome {
        case nil: return "Decide it in the Approvals tab. This chat cannot."
        case "approved": return "Approved on your device."
        case "denied": return "Denied on your device."
        default: return "Not decided in time, so it was not done."
        }
    }
}
