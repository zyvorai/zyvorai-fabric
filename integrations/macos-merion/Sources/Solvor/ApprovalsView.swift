import MerionKit
import SwiftUI

/// Approvals waiting for this person. Each is a card that shows what the host read out of the exact request; with this Mac enrolled the decision
/// is signed here (Secure Enclave, Touch ID) over the exact text the host will check. Nothing here can be decided from a notification or the chat.
struct ApprovalsView: View {
    @EnvironmentObject var app: AppState
    @State private var message: String?
    @State private var enrolling = false

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: Space.m) {
                DeviceCard(enrolling: $enrolling)
                if let message {
                    Label(message, systemImage: "info.circle").font(.callout).foregroundStyle(.secondary).transition(.opacity)
                }
                if app.approvals.isEmpty {
                    EmptyState(symbol: "checkmark.seal", title: "Nothing is waiting for you", message: "When an agent wants to send, buy or change something, the details show up here and you decide.")
                        .frame(maxWidth: .infinity, minHeight: 260)
                } else {
                    ForEach(Array(app.approvals.enumerated()), id: \.element.id) { i, a in
                        ApprovalCardView(approval: a, message: $message).appear(delay: Motion.stagger(i))
                    }
                }
            }
            .padding(Space.l)
            .animation(Motion.spring, value: app.approvals.map(\.id))
        }
        .navigationTitle("Approvals")
        .toolbar { Button("Refresh") { Task { await app.refreshApprovals(); await app.refreshDevice() } } }
        .task { await app.refreshApprovals(); await app.refreshDevice() }
        .sheet(isPresented: $enrolling) { OperatorEnrolSheet(isPresented: $enrolling, message: $message) }
    }
}

/// Whether this Mac can approve, and how to fix it when it cannot.
struct DeviceCard: View {
    @EnvironmentObject var app: AppState
    @Binding var enrolling: Bool
    @State private var copied = false

    var body: some View {
        HStack(alignment: .top, spacing: Space.s) {
            AccentTile(symbol: symbol, size: 36)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(title).font(.headline)
                Text(detail).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                if case .notEnrolled = app.device { enrolActions }
            }
            Spacer(minLength: 0)
        }
        .padding(Space.m).card()
    }

    private var symbol: String {
        switch app.device { case .enrolled: return "touchid"; case .noKey: return "exclamationmark.shield"; default: return "person.badge.key" }
    }
    private var title: String {
        switch app.device {
        case .enrolled: return "This Mac can approve"
        case .noKey: return "No approval key on this Mac"
        case .notEnrolled: return "Enrol this Mac to approve"
        case .unknown: return "Checking this Mac"
        }
    }
    private var detail: String {
        switch app.device {
        case .enrolled(let id): return "Decisions are signed with a key in the Secure Enclave and Touch ID. Enrolled as \(id)."
        case .noKey: return "Solvor cannot use a Secure Enclave key here, so it cannot sign a decision. Approve from your phone instead."
        case .notEnrolled: return "A key lives in this Mac's Secure Enclave and only its public half leaves it. The host has to be told about it once, by whoever runs the host."
        case .unknown: return "Asking the host whether this Mac's key is enrolled."
        }
    }

    @ViewBuilder private var enrolActions: some View {
        HStack {
            Button { copyDetails() } label: { Label(copied ? "Copied" : "Copy details for your operator", systemImage: copied ? "checkmark" : "doc.on.doc").contentTransition(.symbolEffect(.replace)) }
                .primaryButton().controlSize(.small).disabled(app.userId.isEmpty)
            Button("I am the operator…") { enrolling = true }.secondaryButton().controlSize(.small).disabled(app.userId.isEmpty)
        }.padding(.top, Space.xxs)
        if app.userId.isEmpty { Text("Set the user you act for in Settings first.").font(.caption).foregroundStyle(.orange) }
    }

    private func copyDetails() {
        guard let key = app.myPublicKey else { return }
        let d = EnrolmentDetails(userId: app.userId, deviceName: Host.current().localizedName ?? "Mac", publicKeyBase64: key)
        NSPasteboard.general.clearContents(); NSPasteboard.general.setString(d.text, forType: .string); copied = true
    }
}

/// One approval, read the way a person reads a boarding pass: what, to whom, by when.
struct ApprovalCardView: View {
    let approval: Approval
    @Binding var message: String?
    @EnvironmentObject var app: AppState
    @State private var showPayload = false
    @State private var deciding = false

    private var card: ApprovalCard { ApprovalCard(approval) }

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { ctx in
            let readiness = ApprovalGate.readiness(for: approval, device: app.device, now: ctx.date)
            VStack(alignment: .leading, spacing: 0) {
                header(now: ctx.date)
                Divider()
                VStack(alignment: .leading, spacing: Space.s) {
                    if card.hasPreview {
                        Grid(alignment: .topLeading, horizontalSpacing: Space.m, verticalSpacing: Space.xs) {
                            ForEach(Array(card.fields.enumerated()), id: \.offset) { _, f in
                                GridRow {
                                    Text(f.label).font(.callout).foregroundStyle(.secondary).gridColumnAlignment(.trailing)
                                    Text(f.value).font(.body).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                                }
                            }
                        }
                    } else if let p = approval.prompt ?? approval.subject {
                        Text(p).textSelection(.enabled)
                    }
                    if let payload = try? ApprovalPayload.text(for: approval, decision: .approved) {
                        DisclosureGroup("The exact text your Mac will sign", isExpanded: $showPayload) {
                            Text(payload).font(.system(.caption, design: .monospaced)).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                        }.font(.callout)
                    }
                    if let why = blocked(readiness) { Label(why, systemImage: "exclamationmark.triangle.fill").font(.callout).foregroundStyle(.orange) }
                    HStack {
                        Button { decide(.approved) } label: {
                            HStack(spacing: 6) { Image(systemName: "touchid"); Text("Approve") }
                        }.primaryButton().controlSize(.large).disabled(!readiness.canDecide || deciding)
                        Button("Deny") { decide(.denied) }.secondaryButton().controlSize(.large).disabled(!readiness.canDecide || deciding)
                        if deciding { ProgressView().controlSize(.small) }
                    }
                }.padding(Space.m)
            }
        }
        .card()
    }

    private func header(now: Date) -> some View {
        HStack(spacing: Space.s) {
            Image(systemName: card.symbol).font(.title2).foregroundStyle(Brand.orange).symbolRenderingMode(.hierarchical)
            VStack(alignment: .leading, spacing: 0) {
                Text(card.title).font(.headline)
                if let s = approval.subject, !s.isEmpty, !card.hasPreview { Text(s).font(.callout).foregroundStyle(.secondary) }
            }
            Spacer()
            if let left = card.secondsLeft(now: now) {
                Label(ApprovalCard.countdown(left), systemImage: "clock")
                    .font(.callout.monospacedDigit())
                    .foregroundStyle(left <= 0 ? Color.red : left < 60 ? Color.orange : Color.secondary)
                    .contentTransition(.numericText(countsDown: true))
                    .help("How long this approval can still be signed")
            }
        }.padding(Space.m)
    }

    private func blocked(_ r: ApprovalReadiness) -> String? {
        switch r {
        case .ready: return nil
        case .expired: return "The signing window has passed. Ask the agent to try again."
        case .needsEnrolment: return app.device == .unknown ? "Checking whether this Mac is enrolled…" : "Enrol this Mac first (see above). The host would refuse an unsigned decision."
        case .noKey: return "This Mac cannot sign. Approve from your phone."
        }
    }

    private func decide(_ d: Decision) {
        deciding = true
        Task {
            let r = await app.decide(approval, d)
            deciding = false
            switch r {
            case .decided(let m): message = m; if d == .approved { Haptics.success() }
            case .refused(let m), .failed(let m): message = m
            }
        }
    }
}

/// Developer mode: enrol this Mac with the operator token, used once and not stored.
struct OperatorEnrolSheet: View {
    @EnvironmentObject var app: AppState
    @Binding var isPresented: Bool
    @Binding var message: String?
    @State private var operatorToken = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Enrol this Mac").font(.headline)
            Text("Enrolling needs the operator token, because a user token must not be able to add its own key. It is used once and is not stored.").font(.callout)
            SecureField("Operator token", text: $operatorToken).textFieldStyle(.roundedBorder)
            HStack { Spacer(); Button("Cancel") { close() }; Button("Enrol") { enrol() }.keyboardShortcut(.defaultAction).disabled(operatorToken.isEmpty) }
        }.padding().frame(width: 460)
    }

    private func close() { operatorToken = ""; isPresented = false }

    private func enrol() {
        let token = operatorToken; close()
        guard let url = URL(string: app.host), let key = app.myPublicKey, let c = try? MerionClient(baseURL: url, token: token) else { return }
        let details = EnrolmentDetails(userId: app.userId, deviceName: Host.current().localizedName ?? "Mac", publicKeyBase64: key)
        Task {
            do {
                try await c.enrolDevice(user: details.userId, deviceId: details.deviceId, publicKeyBase64: details.publicKeyBase64, name: details.name)
                message = "Enrolled \(details.deviceId) for \(details.userId)."
                await app.refreshDevice()
            } catch { message = error.localizedDescription }
        }
    }
}
