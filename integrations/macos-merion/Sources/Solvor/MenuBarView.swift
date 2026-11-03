import AppKit
import MerionKit
import SwiftUI

/// The menu-bar popover: a branded header, a one-line ask box, a drop target, waiting approvals as compact cards
/// (never decided from here — that needs the full window, Touch ID and screen space), and this session's recent runs.
struct MenuBarView: View {
    @EnvironmentObject var app: AppState
    @Environment(\.openWindow) private var openWindow
    @State private var targeted = false
    @State private var ask = ""

    var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            header
            askBox
            dropTarget
            if !app.approvals.isEmpty {
                Divider()
                ForEach(app.approvals.prefix(2)) { a in MenuBarApprovalRow(approval: a, open: openMain) }
                if app.approvals.count > 2 {
                    Button("\(app.approvals.count - 2) more waiting…", action: openMain).font(.caption).buttonStyle(.plain).foregroundStyle(.secondary)
                }
            }
            if !app.jobs.isEmpty {
                Divider()
                Text("Recent").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                ForEach(app.jobs.prefix(3)) { job in MenuBarJobRow(job: job, open: openMain) }
            }
            Divider()
            HStack {
                Button("Read email…") { openMain(); app.sheet = .email }.buttonStyle(.plain).disabled(!app.connected)
                Text("·").foregroundStyle(.tertiary)
                Button("Talk…") { openMain(); app.sheet = .voice }.buttonStyle(.plain).disabled(!app.connected)
                Spacer()
                Menu {
                    ForEach(app.demos.filter { $0.accepts.contains("txt") }) { d in Button(d.title) { runClipboard(d) } }
                } label: { Text("Run the clipboard as…") }
                .menuStyle(.borderlessButton).fixedSize().disabled(!app.connected || NSPasteboard.general.string(forType: .string) == nil)
            }.font(.caption)
            HStack {
                Button("Open Solvor", action: openMain).secondaryButton().controlSize(.small)
                Spacer()
                Text("⌥Space to ask anywhere").font(.caption2).foregroundStyle(.tertiary)
                Spacer()
                Button("Quit") { NSApp.terminate(nil) }.buttonStyle(.plain).font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(Space.m).frame(width: 320)
    }

    private var header: some View {
        HStack(spacing: Space.s) {
            LogoTile(size: 26)
            VStack(alignment: .leading, spacing: 0) {
                Text("Solvor").font(.system(size: 14, weight: .bold, design: .rounded))
                Text(app.connected ? (URL(string: app.host)?.host ?? "connected") : "not connected").font(.caption2).foregroundStyle(.secondary)
            }
            Spacer()
            Circle().fill(app.connected ? Brand.good : .orange).frame(width: 7, height: 7)
        }
    }

    private var askBox: some View {
        HStack(spacing: Space.xs) {
            Image(systemName: "sparkle").foregroundStyle(Brand.blue).font(.caption)
            TextField("Ask or drop a file…", text: $ask).textFieldStyle(.plain).font(.callout)
                .onSubmit(sendAsk).disabled(!app.connected)
        }
        .padding(.horizontal, Space.s).padding(.vertical, 6)
        .background(.quaternary, in: RoundedRectangle(cornerRadius: Radius.chip, style: .continuous))
    }

    private var dropTarget: some View {
        RoundedRectangle(cornerRadius: Radius.chip, style: .continuous)
            .strokeBorder(targeted ? Brand.blue : Color.gray.opacity(0.4), style: StrokeStyle(lineWidth: targeted ? 2.5 : 1.5, dash: [6]))
            .background(RoundedRectangle(cornerRadius: Radius.chip, style: .continuous).fill(targeted ? Brand.blue.opacity(0.08) : .clear))
            .frame(height: 48)
            .overlay {
                Label("Drop a file to summarise it", systemImage: "arrow.down.doc")
                    .font(.caption).foregroundStyle(targeted ? Brand.blue : .secondary)
                    .symbolEffect(.bounce, value: targeted)
            }
            .scaleEffect(targeted ? 1.03 : 1)
            .animation(Motion.spring, value: targeted)
            .dropDestination(for: URL.self) { urls, _ in DropRouter.route(urls, app: app, source: "menu bar"); return true } isTargeted: { targeted = $0 }
    }

    private func openMain() { NSApp.activate(ignoringOtherApps: true); openWindow(id: "main") }

    /// Sent through the same `ChatModel` Agent Home uses (`AppState.pendingHomeMessage`, consumed by `HomeView`), not a
    /// separate path, so this behaves exactly like typing it in Home would.
    private func sendAsk() {
        let text = ask.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return }
        ask = ""
        openMain()
        app.pane = .home
        app.pendingHomeMessage = text
    }

    private func runClipboard(_ demo: Demo) {
        guard let text = NSPasteboard.general.string(forType: .string) else { return }
        let url = FileManager.default.temporaryDirectory.appendingPathComponent("clipboard.txt")
        do { try text.write(to: url, atomically: true, encoding: .utf8); app.run(demo: demo.id, files: [url], source: "clipboard") } catch { app.notice = error.localizedDescription }
    }
}

/// A waiting approval, compact: what it is and how long it can still be signed. Opens the real window to decide it —
/// a popover has no room for Touch ID, and a decision belongs where the person can read the whole preview.
private struct MenuBarApprovalRow: View {
    let approval: Approval
    let open: () -> Void
    private var card: ApprovalCard { ApprovalCard(approval) }

    var body: some View {
        Button(action: open) {
            HStack(spacing: Space.xs) {
                Image(systemName: card.symbol).foregroundStyle(Brand.orange)
                VStack(alignment: .leading, spacing: 0) {
                    Text(card.title).font(.callout).lineLimit(1)
                    if let field = card.fields.first { Text(field.value).font(.caption2).foregroundStyle(.secondary).lineLimit(1) }
                }
                Spacer()
                Image(systemName: "chevron.right").font(.caption2).foregroundStyle(.tertiary)
            }
        }.buttonStyle(.plain)
    }
}

private struct MenuBarJobRow: View {
    let job: Job
    let open: () -> Void
    @EnvironmentObject var app: AppState

    private var status: StatusKind {
        switch job.state { case .running: return .running; case .done: return .done; case .failed: return .failed }
    }

    var body: some View {
        Button(action: open) {
            HStack(spacing: Space.xs) {
                Image(systemName: status.symbol).foregroundStyle(status.color).symbolEffect(.rotate, isActive: status == .running)
                Text(app.demo(job.demo)?.title ?? job.demo).font(.callout).lineLimit(1)
                Spacer()
            }
        }.buttonStyle(.plain)
    }
}
