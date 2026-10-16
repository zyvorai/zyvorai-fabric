import MerionKit
import SwiftUI

/// The front door: ask an agent something, see what is waiting for you. Deciding an approval never happens here — a pending one
/// shows as a card that opens Approvals; the chat can only say that something is waiting.
struct HomeView: View {
    @EnvironmentObject var app: AppState
    @StateObject private var chat: ChatModel
    @State private var input = ""
    @State private var showThreads = false
    @FocusState private var focused: Bool
    @Namespace private var glass

    private static let exampleAgents = ["echo-agent", "memory-agent"]
    private static let starters = [
        "Summarise the file I just dropped",
        "What's waiting for my approval?",
        "Remember that I prefer window seats",
        "Plan a weekend trip",
    ]

    init() { _chat = StateObject(wrappedValue: ChatModel(agent: UserDefaults.standard.string(forKey: "homeAgent") ?? "echo-agent", api: { AppState.shared.client })) }

    var body: some View {
        HStack(spacing: 0) {
            if showThreads { threadList.frame(width: 240).transition(.move(edge: .leading).combined(with: .opacity)) }
            VStack(spacing: 0) {
                header
                Divider()
                conversation
                Divider()
                composer
            }
        }
        .background(AuroraBackground(intensity: 0.35, palette: AuroraBackground.blue))
        .animation(Motion.spring, value: showThreads)
        .navigationTitle("Solvor")
        .task { await chat.loadThreads() }
        .onChange(of: chat.agent) { _, a in UserDefaults.standard.set(a, forKey: "homeAgent"); Task { await chat.loadThreads() } }
        .task(id: app.pendingHomeMessage) {
            guard let text = app.pendingHomeMessage else { return }
            app.pendingHomeMessage = nil
            chat.send(text)
        }
        .onDrop(of: [.fileURL], isTargeted: nil) { providers in
            Task { @MainActor in
                var urls: [URL] = []
                for p in providers { if let u = try? await p.loadItem(forTypeIdentifier: "public.file-url") as? Data, let url = URL(dataRepresentation: u, relativeTo: nil) { urls.append(url) } }
                guard !urls.isEmpty else { return }
                DropRouter.route(urls, app: app)
                chat.receiveDroppedFiles(urls)
            }
            return true
        }
    }

    private var header: some View {
        HStack(spacing: Space.s) {
            Button { showThreads.toggle() } label: { Image(systemName: "sidebar.left") }.buttonStyle(.plain).help("Chats")
            Picker("Agent", selection: $chat.agent) {
                ForEach(Self.exampleAgents, id: \.self) { Text($0).tag($0) }
                if !Self.exampleAgents.contains(chat.agent) { Text(chat.agent).tag(chat.agent) }
            }.pickerStyle(.menu).labelsHidden().fixedSize()
            Spacer()
            if !app.approvals.isEmpty {
                Button { app.pane = .approvals } label: {
                    Label("\(app.approvals.count) waiting", systemImage: "checkmark.seal.fill").font(.callout.weight(.semibold))
                }.secondaryButton()
            }
            Button { chat.newThread() } label: { Image(systemName: "square.and.pencil") }.buttonStyle(.plain).help("New chat")
        }.padding(Space.m)
    }

    @ViewBuilder private var conversation: some View {
        if chat.bubbles.isEmpty { starterState }
        else {
            ScrollViewReader { proxy in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: Space.m) {
                        ForEach(chat.bubbles) { b in BubbleView(bubble: b).id(b.id) }
                        if chat.sending, chat.bubbles.last?.streaming != true { TypingDots().padding(.leading, Space.s) }
                    }.padding(Space.l)
                }
                .onChange(of: chat.bubbles.count) { _, _ in if let last = chat.bubbles.last?.id { withAnimation(Motion.spring) { proxy.scrollTo(last, anchor: .bottom) } } }
            }
        }
    }

    private var starterState: some View {
        VStack(spacing: Space.l) {
            Spacer()
            AnimatedLogo(size: 56, accent: Brand.blue).frame(height: 80)
            Text("What can I do for you?").font(Typo.title)
            VStack(alignment: .leading, spacing: Space.xs) {
                ForEach(Self.starters, id: \.self) { s in
                    Button { input = s; focused = true } label: {
                        HStack { Image(systemName: "sparkle").foregroundStyle(Brand.blue); Text(s); Spacer() }
                            .padding(.horizontal, Space.m).padding(.vertical, Space.s)
                            .glassEffect(.regular, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
                    }.buttonStyle(.plain)
                }
            }.frame(maxWidth: 420)
            Text("Drop a file anywhere on this window to attach it.").font(.caption).foregroundStyle(.secondary)
            Spacer()
        }.frame(maxWidth: .infinity)
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: Space.s) {
            TextField("Message \(chat.agent)", text: $input, axis: .vertical)
                .textFieldStyle(.plain).lineLimit(1...6).focused($focused)
                .padding(Space.s).glassEffect(.regular, in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
                .onSubmit(send)
            Button { chat.sending ? chat.cancel() : send() } label: {
                Image(systemName: chat.sending ? "stop.fill" : "arrow.up").font(.body.weight(.bold))
            }.primaryButton().buttonBorderShape(.circle).controlSize(.large).disabled(!chat.sending && input.trimmingCharacters(in: .whitespaces).isEmpty)
        }.padding(Space.m)
    }

    private func send() {
        let text = input; input = ""
        guard !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        chat.send(text)
    }

    private var threadList: some View {
        List {
            ForEach(chat.threads) { t in
                Button { Task { await chat.open(t) } } label: {
                    VStack(alignment: .leading, spacing: 2) {
                        Text(t.title).lineLimit(1)
                        if let n = t.messageCount { Text("\(n) messages").font(.caption).foregroundStyle(.secondary) }
                    }
                }.buttonStyle(.plain)
            }
        }
        .listStyle(.sidebar).scrollContentBackground(.hidden)
        .overlay { if chat.threads.isEmpty { EmptyState(symbol: "bubble.left.and.text.bubble.right", title: "No chats yet", message: "Say hello to start one.") } }
        .task { await chat.loadThreads() }
    }
}

private struct BubbleView: View {
    let bubble: ChatModel.Bubble
    var body: some View {
        switch bubble.kind {
        case .approval(let notice): ApprovalNoticeCard(notice: notice)
        case .decided(let decision): Label(decision == "approved" ? "Approved" : "Denied", systemImage: decision == "approved" ? "checkmark.circle.fill" : "xmark.circle.fill")
                .font(.callout).foregroundStyle(decision == "approved" ? Brand.good : .secondary)
        case .status: Label(bubble.text, systemImage: "ellipsis.circle").font(.callout).foregroundStyle(.secondary)
        case .error: Label(bubble.text, systemImage: "exclamationmark.triangle.fill").font(.callout).foregroundStyle(.red)
                .padding(Space.s).background(.red.opacity(0.12), in: RoundedRectangle(cornerRadius: Radius.chip))
        case .text: textBubble
        }
    }

    private var textBubble: some View {
        HStack {
            if bubble.role == .user { Spacer(minLength: 60) }
            VStack(alignment: .leading, spacing: 4) {
                if bubble.text.isEmpty, bubble.streaming { TypingDots() }
                else { MarkdownView(markdown: bubble.text).textSelection(.enabled) }
            }
            .padding(Space.s)
            .frame(maxWidth: 560, alignment: .leading)
            .background(bubble.role == .user ? AnyShapeStyle(Color.accentColor.opacity(0.16)) : AnyShapeStyle(.quaternary), in: RoundedRectangle(cornerRadius: Radius.card, style: .continuous))
            if bubble.role != .user { Spacer(minLength: 60) }
        }
    }
}

/// "Waiting for your Mac": what the chat is allowed to show about a pending approval. The one button opens Approvals; nothing here decides.
private struct ApprovalNoticeCard: View {
    let notice: ApprovalNotice
    @EnvironmentObject var app: AppState
    var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: "touchid").font(.title3).foregroundStyle(Brand.blue)
            VStack(alignment: .leading, spacing: 2) {
                Text("Waiting for your Mac").font(.headline)
                Text(notice.preview?.fields.first?.value ?? notice.prompt).font(.callout).foregroundStyle(.secondary).lineLimit(2)
            }
            Spacer()
            Button("Review") { app.pane = .approvals }.secondaryButton().controlSize(.small)
        }
        .padding(Space.m).card()
    }
}
