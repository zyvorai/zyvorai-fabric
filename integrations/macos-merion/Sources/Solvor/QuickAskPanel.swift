import AppKit
import MerionKit
import SwiftUI

/// A small floating panel opened by the global hotkey (⌥Space by default): one line to ask an agent something without
/// first opening the main window. It is a thin shell around `ChatModel` — the same one Agent Home uses — so sending,
/// streaming and a pending approval notice all behave identically; there is no separate chat logic to keep in sync.
@MainActor
final class QuickAskController {
    private var panel: NSPanel?
    private let app: AppState

    init(app: AppState) { self.app = app }

    func toggle() {
        if let panel { close(panel); return }
        show()
    }

    private func show() {
        let chat = ChatModel(agent: UserDefaults.standard.string(forKey: "homeAgent") ?? "echo-agent", api: { [weak app] in app?.client })
        let content = QuickAskView(chat: chat, app: app, onDismiss: { [weak self] in self?.close(self?.panel) })
        let hosting = NSHostingView(rootView: content)
        let panel = NSPanel(contentRect: NSRect(x: 0, y: 0, width: 520, height: 76), styleMask: [.nonactivatingPanel, .fullSizeContentView], backing: .buffered, defer: false)
        panel.contentView = hosting
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.level = .floating
        panel.hasShadow = true
        panel.hidesOnDeactivate = false
        panel.isMovableByWindowBackground = true
        panel.collectionBehavior = [.canJoinAllSpaces, .transient]
        if let screen = NSScreen.main {
            let x = screen.frame.midX - panel.frame.width / 2
            let y = screen.frame.midY + screen.frame.height * 0.12
            panel.setFrameOrigin(NSPoint(x: x, y: y))
        }
        NSApp.activate(ignoringOtherApps: true)
        panel.makeKeyAndOrderFront(nil)
        self.panel = panel
    }

    private func close(_ panel: NSPanel?) {
        panel?.orderOut(nil)
        self.panel = nil
    }
}

private struct QuickAskView: View {
    @ObservedObject var chat: ChatModel
    let app: AppState
    let onDismiss: () -> Void
    @State private var input = ""
    @FocusState private var focused: Bool
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            HStack(spacing: Space.s) {
                LogoTile(size: 22)
                TextField("Ask \(chat.agent)…", text: $input)
                    .textFieldStyle(.plain).font(Typo.body).focused($focused)
                    .onSubmit(send)
                if chat.sending { ProgressView().controlSize(.small) }
            }
            if let last = chat.bubbles.last, last.role != .user {
                Divider()
                Group {
                    switch last.kind {
                    case .approval: Label("Waiting for your Mac — open Approvals to decide.", systemImage: "touchid").foregroundStyle(.secondary)
                    case .error: Label(last.text, systemImage: "exclamationmark.triangle.fill").foregroundStyle(.orange)
                    default: Text(last.text.isEmpty ? " " : last.text).textSelection(.enabled)
                    }
                }
                .font(.callout).lineLimit(4).fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(Space.m)
        .frame(width: 520)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: Radius.panel, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radius.panel, style: .continuous).strokeBorder(.white.opacity(0.12), lineWidth: 1))
        .onAppear { focused = true }
        .onExitCommand(perform: onDismiss)
    }

    private func send() {
        let text = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { onDismiss(); return }
        input = ""
        chat.send(text)
    }
}
