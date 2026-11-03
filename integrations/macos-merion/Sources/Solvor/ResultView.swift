import AppKit
import MerionKit
import SwiftUI

/// The outcome of a job: one card per file, each with its summary and the honesty line.
struct ResultView: View {
    let job: Job
    @EnvironmentObject var app: AppState

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    Text(app.demo(job.demo)?.title ?? job.demo).font(.system(size: 26, weight: .bold, design: .rounded))
                    Spacer()
                    Text(job.source).font(.caption).foregroundStyle(.secondary)
                }
                switch job.state {
                case .running:
                    VStack(spacing: 10) {
                        SealedCellView(fileName: job.files.map(\.lastPathComponent).joined(separator: ", "))
                        // whether the cell is sealed is only known from the result (a local simulator is not), so this claims nothing yet
                        Text("Reading it in a cell on your Keep host").font(.headline)
                        Text("A cold cell takes about 15 to 25 seconds.").font(.callout).foregroundStyle(.secondary)
                        Button("Cancel") { app.cancelJob(job.id) }.secondaryButton()
                    }.frame(maxWidth: .infinity)
                case .failed(let message):
                    VStack(alignment: .leading, spacing: 10) {
                        Label(message, systemImage: "exclamationmark.triangle").foregroundStyle(.red).textSelection(.enabled)
                        Button("Try again") { app.retry(job) }.secondaryButton()
                    }
                case .done(let outcome):
                    ProofPill(egress: outcome.egressConnects, evidence: outcome.items.first?.result?.badge?.evidence, simulated: outcome.isSimulated)
                        .stamp()
                        .background { SparkBurst(trigger: outcome.isSimulated ? 0 : 1).frame(width: 300, height: 220) }
                        .task { if !outcome.isSimulated { try? await Task.sleep(nanoseconds: 350_000_000); Haptics.success() } }
                    ForEach(Array(outcome.items.enumerated()), id: \.offset) { i, item in FileResultCard(item: item).appear(delay: 0.25 + Motion.stagger(i)) }
                    Button("Run again") { app.retry(job) }.secondaryButton()
                }
            }
            .padding(20)
        }
    }
}

struct FileResultCard: View {
    let item: BatchItem
    @EnvironmentObject var app: AppState
    @State private var artifacts: [Artifact] = []
    @State private var selected: String?
    @State private var error: String?
    @State private var savedURL: URL?
    @State private var sharing = false

    private var current: Artifact? { artifacts.first { $0.id == selected } }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Image(systemName: item.ok ? "checkmark.circle.fill" : "xmark.octagon.fill").foregroundStyle(item.ok ? .green : .red)
                Text(item.filename).font(.headline)
                Spacer()
                if let text = current?.body {
                    Button("Copy") { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(text, forType: .string) }
                    if let saved = savedURL {
                        Button("Reveal in Finder") { NSWorkspace.shared.activateFileViewerSelecting([saved]) }
                    } else {
                        Button("Save…") { save(text) }
                    }
                    Button("Share") { sharing = true }
                        .background(SharePicker(isPresented: $sharing, items: [text]))
                }
            }
            if artifacts.count > 1 {
                Picker("Artifact", selection: Binding(get: { selected ?? "" }, set: { selected = $0 })) {
                    ForEach(artifacts) { a in Text(a.title).tag(a.id) }
                }.pickerStyle(.segmented).labelsHidden()
            }
            if let error = item.error ?? error { Text(error).foregroundStyle(.red) }
            if let text = current?.body { MarkdownView(markdown: text) } else if item.ok && artifacts.isEmpty { ProgressView() }
        }
        .padding(16).card()
        .task(id: item.result?.sessionId) { await load() }
    }

    private func load() async {
        guard item.ok, let c = app.client, let refs = item.result?.artifacts, !refs.isEmpty else { return }
        do {
            // The Markdown summary opens first; the rest (a chart, the raw extract, a diff) are one tab away, not lost.
            let loaded = try await withThrowingTaskGroup(of: Artifact.self) { group in
                for ref in refs { group.addTask { try await c.artifact(id: ref.id) } }
                var out: [Artifact] = []
                for try await a in group { out.append(a) }
                return out
            }
            artifacts = loaded.sorted { ($0.title.hasSuffix(".md") ? 0 : 1, $0.title) < ($1.title.hasSuffix(".md") ? 0 : 1, $1.title) }
            selected = artifacts.first?.id
        } catch { self.error = error.localizedDescription }
    }

    private func save(_ text: String) {
        let name = current?.title ?? "summary.md"
        let p = NSSavePanel(); p.nameFieldStringValue = name; p.allowedContentTypes = []
        if p.runModal() == .OK, let url = p.url { try? text.write(to: url, atomically: true, encoding: .utf8); savedURL = url }
    }
}

/// Wraps `NSSharingServicePicker` (Mail, Messages, AirDrop, …) so a SwiftUI `Button` can present it, since SwiftUI has no
/// native share sheet on macOS.
struct SharePicker: NSViewRepresentable {
    @Binding var isPresented: Bool
    let items: [Any]

    func makeNSView(context: Context) -> NSView { NSView() }
    func updateNSView(_ view: NSView, context: Context) {
        guard isPresented else { return }
        DispatchQueue.main.async {
            let picker = NSSharingServicePicker(items: items)
            picker.show(relativeTo: .zero, of: view, preferredEdge: .minY)
            isPresented = false
        }
    }
}
