import MerionKit
import SwiftUI

struct RunsView: View {
    @EnvironmentObject var app: AppState
    @State private var selection = Set<String>()
    @State private var opened: Artifact?
    @State private var diffText: String?
    @State private var filter = ""

    private var rows: [Artifact] {
        app.runs.filter { filter.isEmpty || ($0.metadata?.demo ?? $0.agent ?? "").localizedCaseInsensitiveContains(filter) || $0.title.localizedCaseInsensitiveContains(filter) }
    }

    var body: some View {
        HSplitView {
            VStack(spacing: 0) {
                HStack {
                    TextField("Filter by use case", text: $filter).textFieldStyle(.roundedBorder)
                    Button("Refresh") { Task { await app.refreshRuns() } }
                }.padding(8)
                List(rows, selection: $selection) { a in
                    VStack(alignment: .leading) {
                        Text(a.title).font(.headline)
                        HStack {
                            Text(a.metadata?.demo ?? a.agent ?? "").foregroundStyle(.secondary)
                            Spacer()
                            if let d = a.createdDate { Text(d, style: .relative).foregroundStyle(.secondary) }
                        }.font(.caption)
                        if let f = a.metadata?.filename { Text(f).font(.caption2).foregroundStyle(.tertiary) }
                    }.tag(a.id)
                }
                .overlay { if rows.isEmpty { EmptyState(symbol: "clock.arrow.circlepath", title: filter.isEmpty ? "No runs yet" : "No runs match", message: filter.isEmpty ? "Drop a file on a use case and the result is kept here." : "Try a different filter.") } }
                HStack {
                    Button("Compare two runs") { compare() }.disabled(selection.count != 2)
                    Spacer()
                    Text(selection.count == 2 ? "" : "Select two runs of one use case to compare").font(.caption).foregroundStyle(.secondary)
                }.padding(8)
            }
            .frame(minWidth: 320, idealWidth: 380)
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    if let diffText { Text("Comparison").font(.title3.bold()); Text(diffText).font(.system(.body, design: .monospaced)).textSelection(.enabled) }
                    else if let opened { Text(opened.title).font(.title3.bold()); MarkdownView(markdown: opened.body) }
                    else { Text("Select a run to read it, or two to compare.").foregroundStyle(.secondary) }
                }.padding(20).frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .navigationTitle("Runs")
        .onChange(of: selection) { _, new in
            diffText = nil
            opened = new.count == 1 ? rows.first { $0.id == new.first } : nil
        }
        .task { await app.refreshRuns() }
    }

    private func compare() {
        let ids = rows.filter { selection.contains($0.id) }.sorted { ($0.createdDate ?? .distantPast) < ($1.createdDate ?? .distantPast) }.map(\.id)
        guard ids.count == 2, let c = app.client else { return }
        Task { do { diffText = try await c.diff(ids[0], ids[1]) } catch { diffText = error.localizedDescription } }
    }
}
