import MerionKit
import SwiftUI

struct ChatsView: View {
    @EnvironmentObject var model: AppModel
    @State private var threads: [ChatThread] = []
    @State private var loaded = false
    @State private var newChatId = UUID().uuidString

    var body: some View {
        NavigationStack {
            List {
                ForEach(threads) { t in
                    NavigationLink(value: ChatTarget.existing(t)) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(t.title.isEmpty ? "(untitled)" : t.title).lineLimit(1)
                            HStack {
                                Text(t.agent)
                                if let d = t.updatedDate { Text("· " + d.formatted(.relative(presentation: .named))) }
                            }.font(.caption).foregroundStyle(.secondary)
                        }
                    }
                }
                .onDelete { idx in
                    let doomed = idx.map { threads[$0] }
                    threads.remove(atOffsets: idx)
                    Task { for t in doomed { try? await model.client?.deleteThread(t.id) } }
                }
            }
            .overlay {
                if loaded && threads.isEmpty { ContentUnavailableView("No chats yet", systemImage: "bubble", description: Text("Start one with the pencil.")) }
                if !model.isConnected { ContentUnavailableView("Not connected", systemImage: "network.slash", description: Text("Add your Keep host and token in Settings.")) }
            }
            .navigationTitle("Chats")
            .toolbar {
                NavigationLink(value: ChatTarget.new(newChatId)) { Image(systemName: "square.and.pencil") }.disabled(!model.isConnected)
            }
            .navigationDestination(for: ChatTarget.self) { ChatView(target: $0) }
            .refreshable { await load() }
            .task { await load() }
        }
    }

    private func load() async {
        guard let client = model.client else { return }
        do { threads = try await client.threads(); loaded = true; model.problem = nil } catch { model.problem = error.localizedDescription }
    }
}

/// A conversation to open: one the host already holds, or a new one under a fresh client id.
enum ChatTarget: Hashable {
    case existing(ChatThread)
    case new(String)
}
