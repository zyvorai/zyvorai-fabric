import MerionKit
import SwiftUI

/// What Keep remembers about you. Off by default; an agent can only propose an entry, and nothing is used until you accept it. Needs a user token.
struct MemoryPane: View {
    @EnvironmentObject var app: AppState
    @State private var view = MemoryView(enabled: false, items: [], proposals: [])
    @State private var draft = ""
    @State private var kind = "note"
    @State private var message: String?
    @State private var forgetting = false

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: Space.m) {
                if let message { Label(message, systemImage: "exclamationmark.triangle.fill").font(.callout).foregroundStyle(.red) }
                VStack(alignment: .leading, spacing: Space.xs) {
                    Toggle("Remember things about me", isOn: Binding(get: { view.enabled }, set: { on in run { try await $0.setMemory(enabled: on) } }))
                        .toggleStyle(.switch)
                    Text("Off: your agents remember nothing about you. On: only what you add or accept below.").font(.caption).foregroundStyle(.secondary)
                }.padding(Space.m).card()
                if view.enabled {
                    VStack(alignment: .leading, spacing: Space.s) {
                        SectionHeader("Add a note")
                        HStack {
                            TextField("e.g. I prefer window seats", text: $draft).textFieldStyle(.plain)
                                .padding(Space.s).background(.quaternary, in: RoundedRectangle(cornerRadius: Radius.chip, style: .continuous))
                            Picker("", selection: $kind) { Text("Preference").tag("preference"); Text("Fact").tag("fact"); Text("Note").tag("note") }.labelsHidden().frame(width: 120)
                            Button("Remember") { let t = draft, k = kind; draft = ""; run { try await $0.addMemory(text: t, kind: k) } }
                                .primaryButton().disabled(draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                        }
                    }.padding(Space.m).card()
                    if !view.proposals.isEmpty {
                        VStack(alignment: .leading, spacing: Space.s) {
                            SectionHeader("Suggested by your agent")
                            ForEach(view.proposals) { p in
                                VStack(alignment: .leading, spacing: 4) {
                                    Text(p.text)
                                    if p.tainted == true { Label("Made by an agent that had read untrusted content.", systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.orange) }
                                    HStack {
                                        Button("Remember") { run { try await $0.decideMemoryProposal(p.id, accept: true) } }.secondaryButton()
                                        Button("No", role: .destructive) { run { try await $0.decideMemoryProposal(p.id, accept: false) } }.secondaryButton()
                                    }.controlSize(.small)
                                }.padding(.vertical, 4)
                                if p.id != view.proposals.last?.id { Divider() }
                            }
                        }.padding(Space.m).card()
                    }
                    VStack(alignment: .leading, spacing: Space.s) {
                        SectionHeader("What it remembers", subtitle: view.items.isEmpty ? nil : "\(view.items.count) item\(view.items.count == 1 ? "" : "s")")
                        if view.items.isEmpty {
                            EmptyState(symbol: "brain.head.profile", title: "Nothing yet", message: "What you add or accept above shows up here.").frame(minHeight: 160)
                        } else {
                            ForEach(view.items) { i in
                                HStack {
                                    if i.isPinned { Image(systemName: "pin.fill").foregroundStyle(.secondary) }
                                    VStack(alignment: .leading) { Text(i.text); Text(i.kind).font(.caption).foregroundStyle(.secondary) }
                                    Spacer()
                                    Button { run { try await $0.deleteMemory(i.id) } } label: { Image(systemName: "trash") }.buttonStyle(.borderless).help("Forget this")
                                }.padding(.vertical, 4)
                                if i.id != view.items.last?.id { Divider() }
                            }
                        }
                    }.padding(Space.m).card()
                    if !view.items.isEmpty || !view.proposals.isEmpty {
                        Button("Forget everything", role: .destructive) { forgetting = true }.secondaryButton()
                    }
                }
            }.padding(Space.l)
        }
        .navigationTitle("Memory")
        .toolbar { Button { Task { await load() } } label: { Image(systemName: "arrow.clockwise") } }
        .task { await load() }
        .confirmationDialog("Forget everything Keep remembers about you?", isPresented: $forgetting, titleVisibility: .visible) {
            Button("Forget everything", role: .destructive) { run { try await $0.forgetAllMemory() } }
        }
    }

    private func run(_ call: @escaping (MerionClient) async throws -> Void) {
        Task {
            guard let c = app.client else { return }
            do { try await call(c); message = nil } catch { message = error.localizedDescription }
            await load()
        }
    }

    private func load() async {
        guard let c = app.client else { return }
        do { view = try await c.memory(); message = nil } catch { message = error.localizedDescription }
    }
}

/// What was done after you approved things: what it was and how it ended, never the message. Read-only.
struct DonePane: View {
    @EnvironmentObject var app: AppState
    @State private var receipts: [Receipt] = []
    @State private var loaded = false
    @State private var message: String?

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: Space.s) {
                if let message { Label(message, systemImage: "exclamationmark.triangle.fill").font(.callout).foregroundStyle(.red) }
                if loaded && receipts.isEmpty {
                    EmptyState(symbol: "checkmark.circle", title: "Nothing yet", message: "A send or a new event you approve shows up here.").frame(minHeight: 260)
                } else {
                    ForEach(Array(receipts.enumerated()), id: \.element.id) { i, r in
                        HStack(alignment: .top, spacing: Space.s) {
                            Image(systemName: r.succeeded ? "checkmark.circle.fill" : "xmark.octagon.fill").foregroundStyle(r.succeeded ? Brand.good : .orange).font(.title3)
                            VStack(alignment: .leading, spacing: 3) {
                                Text("\(r.method) \(r.host)\(r.path)").font(.callout.weight(.semibold)).lineLimit(2)
                                Text([r.agent, r.approved ? "approved by you" : nil, r.atDate?.formatted(.relative(presentation: .named))].compactMap { $0 }.joined(separator: " · ")).font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer()
                            StatusChip(kind: r.succeeded ? .done : .failed, label: String(r.status))
                        }.padding(Space.m).card().appear(delay: Motion.stagger(i))
                    }
                }
            }.padding(Space.l)
        }
        .navigationTitle("Done")
        .toolbar { Button { Task { await load() } } label: { Image(systemName: "arrow.clockwise") } }
        .task { await load() }
    }

    private func load() async {
        guard let c = app.client else { return }
        do { receipts = try await c.receipts(); loaded = true; message = nil } catch { message = error.localizedDescription }
    }
}
