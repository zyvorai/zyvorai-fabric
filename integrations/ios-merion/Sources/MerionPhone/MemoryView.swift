import MerionKit
import SwiftUI

/// What Keep remembers about you, and what was done after you approved things. Memory is yours: off by default, an agent can only propose
/// an entry, and nothing is used until you accept it. The history is read-only: it shows what a send or a new event was and how it ended,
/// never the message itself.
struct MeView: View {
    enum Section: String, CaseIterable { case memory = "Memory", done = "Done" }
    @State private var section = Section.memory

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                Picker("", selection: $section) { ForEach(Section.allCases, id: \.self) { Text($0.rawValue).tag($0) } }
                    .pickerStyle(.segmented).padding([.horizontal, .top])
                switch section {
                case .memory: MemoryList()
                case .done: DoneList()
                }
            }
            .navigationTitle("You")
        }
    }
}

struct MemoryList: View {
    @EnvironmentObject var model: AppModel
    @State private var view = MemoryView(enabled: false, items: [], proposals: [])
    @State private var draft = ""
    @State private var kind = "note"
    @State private var message: String?
    @State private var forgetting = false

    var body: some View {
        List {
            if let message { Text(message).font(.footnote).foregroundStyle(.red) }
            Section {
                Toggle("Remember things about me", isOn: Binding(get: { view.enabled }, set: { on in run { try await $0.setMemory(enabled: on) } }))
            } footer: { Text("Off: your agents remember nothing about you. On: only what you add or accept below.") }
            if view.enabled {
                Section("Add a note") {
                    TextField("e.g. I prefer window seats", text: $draft, axis: .vertical)
                    Picker("Kind", selection: $kind) { Text("Preference").tag("preference"); Text("Fact").tag("fact"); Text("Note").tag("note") }
                    Button("Remember") { let t = draft, k = kind; draft = ""; run { try await $0.addMemory(text: t, kind: k) } }
                        .disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
                if !view.proposals.isEmpty {
                    Section("Suggested by your agent") {
                        ForEach(view.proposals) { p in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(p.text)
                                if p.tainted == true { Label("Made by an agent that had read untrusted content.", systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.orange) }
                                HStack {
                                    Button("Remember") { run { try await $0.decideMemoryProposal(p.id, accept: true) } }.buttonStyle(.borderedProminent)
                                    Button("No", role: .destructive) { run { try await $0.decideMemoryProposal(p.id, accept: false) } }.buttonStyle(.bordered)
                                }
                            }.padding(.vertical, 2)
                        }
                    }
                }
                Section("What it remembers") {
                    ForEach(view.items) { i in
                        HStack { if i.isPinned { Image(systemName: "pin.fill").foregroundStyle(.secondary) }; VStack(alignment: .leading) { Text(i.text); Text(i.kind).font(.caption).foregroundStyle(.secondary) } }
                    }
                    .onDelete { idx in let doomed = idx.map { view.items[$0].id }; run { c in for id in doomed { try await c.deleteMemory(id) } } }
                    if view.items.isEmpty { Text("Nothing yet.").foregroundStyle(.secondary) }
                }
                if !view.items.isEmpty || !view.proposals.isEmpty {
                    Section { Button("Forget everything", role: .destructive) { forgetting = true } }
                }
            }
        }
        .refreshable { await load() }
        .task { await load() }
        .confirmationDialog("Forget everything Keep remembers about you?", isPresented: $forgetting, titleVisibility: .visible) {
            Button("Forget everything", role: .destructive) { run { try await $0.forgetAllMemory() } }
        }
    }

    private func run(_ call: @escaping (MerionClient) async throws -> Void) {
        Task {
            guard let client = model.client else { return }
            do { try await call(client); message = nil } catch { message = error.localizedDescription }
            await load()
        }
    }

    private func load() async {
        guard let client = model.client else { return }
        do { view = try await client.memory(); message = nil } catch { message = error.localizedDescription }
    }
}

struct DoneList: View {
    @EnvironmentObject var model: AppModel
    @State private var receipts: [Receipt] = []
    @State private var loaded = false
    @State private var message: String?

    var body: some View {
        List {
            if let message { Text(message).font(.footnote).foregroundStyle(.red) }
            ForEach(receipts) { r in
                VStack(alignment: .leading, spacing: 3) {
                    HStack(alignment: .firstTextBaseline) {
                        Text("\(r.method) \(r.host)\(r.path)").font(.callout.weight(.semibold)).lineLimit(2)
                        Spacer()
                        Text(String(r.status)).font(.caption2.bold()).foregroundStyle(r.succeeded ? Color.green : Color.orange)
                    }
                    Text([r.agent, r.approved ? "approved by you" : nil, r.atDate?.formatted(.relative(presentation: .named))].compactMap { $0 }.joined(separator: " · "))
                        .font(.caption).foregroundStyle(.secondary)
                }
            }
            if loaded && receipts.isEmpty { Text("Nothing yet. A send or a new event you approve shows up here.").foregroundStyle(.secondary) }
        }
        .refreshable { await load() }
        .task { await load() }
    }

    private func load() async {
        guard let client = model.client else { return }
        do { receipts = try await client.receipts(); loaded = true; message = nil } catch { message = error.localizedDescription }
    }
}
