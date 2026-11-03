import MerionKit
import SwiftUI

/// Approvals waiting for this person. Each one shows what the host read out of the request; a decision is signed on this device
/// (Secure Enclave, Face ID or Touch ID) over the exact text the host checks, so nothing an agent or a chat says can stand in for it.
struct ApprovalsView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        NavigationStack {
            List {
                if let problem = model.problem { Text(problem).font(.callout).foregroundStyle(.red) }
                ForEach(model.approvals) { a in ApprovalRow(approval: a) }
            }
            .overlay { if model.approvals.isEmpty { ContentUnavailableView("Nothing waiting", systemImage: "checkmark.circle", description: Text("An action that needs you shows up here.")) } }
            .navigationTitle("Approvals")
            .refreshable { await model.refreshApprovals() }
            .toolbar { Button { Task { await model.refreshApprovals() } } label: { Image(systemName: "arrow.clockwise") } }
        }
    }
}

struct ApprovalRow: View {
    @EnvironmentObject var model: AppModel
    let approval: Approval
    @State private var busy = false
    @State private var message: String?
    @State private var showPayload = false

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack { Text(approval.kind.capitalized).font(.headline); Text(approval.subject ?? "").foregroundStyle(.secondary).lineLimit(1) }
            if let p = approval.preview {
                Grid(alignment: .topLeading, horizontalSpacing: 10, verticalSpacing: 3) {
                    ForEach(Array(p.fields.prefix(24).enumerated()), id: \.offset) { _, f in
                        GridRow { Text(f.label).foregroundStyle(.secondary); Text(f.value) }.font(.callout)
                    }
                }
            } else if let prompt = approval.prompt { Text(prompt).font(.callout) }
            if let payload = try? ApprovalPayload.text(for: approval, decision: .approved) {
                DisclosureGroup("The exact text this device signs", isExpanded: $showPayload) {
                    Text(payload).font(.system(.caption2, design: .monospaced)).textSelection(.enabled)
                }.font(.footnote)
            } else if approval.sign == nil {
                Text("The host is not set up for device signatures; it will take an unsigned decision only from an operator.").font(.footnote).foregroundStyle(.orange)
            }
            if let message { Text(message).font(.footnote).foregroundStyle(.secondary) }
            HStack {
                Button("Approve") { decide(.approved) }.buttonStyle(.borderedProminent).disabled(busy)
                Button("Deny", role: .destructive) { decide(.denied) }.buttonStyle(.bordered).disabled(busy)
            }
        }.padding(.vertical, 4)
    }

    private func decide(_ d: Decision) {
        busy = true; message = nil
        Task {
            defer { busy = false }
            do { try await model.decide(approval, d) } catch { message = error.localizedDescription }
        }
    }
}
