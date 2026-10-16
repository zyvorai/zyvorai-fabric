import MerionKit
import SwiftUI

/// Your goals, the plans an agent proposed for them, and the suggestions agents made. Accepting one is a decision recorded on the host; it
/// starts nothing by itself, and a step that sends or spends still waits in Approvals. Needs a user token (Settings).
struct GoalsPane: View {
    @EnvironmentObject var app: AppState
    @State private var goals: [Goal] = []
    @State private var suggestions = SuggestionsView(enabled: false, pending: [])
    @State private var selected: String?
    @State private var message: String?
    @AppStorage("goalAgent") private var goalAgent = "echo-agent"
    @State private var newTitle = ""
    @State private var newSteps = ""
    @State private var confirmTaintedFor: ConfirmTainted?

    struct ConfirmTainted: Identifiable { let id = UUID(); let text: String; let action: () -> Void }

    var body: some View {
        HSplitView {
            List(selection: $selected) {
                Section("Suggestions") {
                    Toggle("Let agents suggest things", isOn: Binding(get: { suggestions.enabled }, set: { on in act { try await $0.setSuggestions(enabled: on) } }))
                    ForEach(suggestions.pending) { s in suggestionRow(s) }
                }
                Section("New goal") {
                    TextField("A goal, e.g. Plan the weekend trip", text: $newTitle)
                    TextField("Agent that does it", text: $goalAgent)
                    TextField("One step per line (or leave empty and ask the agent to plan)", text: $newSteps, axis: .vertical).lineLimit(1...4)
                    Button("Add goal") {
                        let title = newTitle.trimmingCharacters(in: .whitespaces)
                        let steps = newSteps.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
                        newTitle = ""; newSteps = ""
                        act { _ = try await $0.createGoal(title: title, agent: goalAgent, steps: steps) }
                    }.disabled(newTitle.trimmingCharacters(in: .whitespaces).isEmpty)
                }
                Section("Goals") {
                    ForEach(goals) { g in
                        VStack(alignment: .leading, spacing: 4) {
                            HStack { Text(g.title).lineLimit(2); Spacer(); StatusChip(kind: g.statusKind, label: g.status.capitalized) }
                            if !g.steps.isEmpty { ProgressView(value: g.progress) }
                            else if g.proposedPlan != nil { Label("A plan is waiting for you", systemImage: "list.bullet.clipboard").font(.caption).foregroundStyle(.orange) }
                        }.tag(g.id).padding(.vertical, 2)
                    }
                    if goals.isEmpty {
                        EmptyState(symbol: "checklist", title: "No goals yet", message: "Add one above, or make a suggestion into one.")
                            .scaleEffect(0.85).frame(minHeight: 180)
                    }
                }
            }
            .frame(minWidth: 320, idealWidth: 360)
            detail
        }
        .navigationTitle("Goals")
        .toolbar { Button { Task { await load() } } label: { Image(systemName: "arrow.clockwise") } }
        .task { await load() }
        .confirmationDialog(confirmTaintedFor?.text ?? "", isPresented: Binding(get: { confirmTaintedFor != nil }, set: { if !$0 { confirmTaintedFor = nil } }), titleVisibility: .visible) {
            Button("Continue anyway") { confirmTaintedFor?.action(); confirmTaintedFor = nil }
        }
    }

    @ViewBuilder private var detail: some View {
        if let id = selected, let g = goals.first(where: { $0.id == id }) {
            ScrollView {
                VStack(alignment: .leading, spacing: Space.m) {
                    HStack { Text(g.title).font(Typo.title); Spacer(); StatusChip(kind: g.statusKind, label: g.status.capitalized) }
                    if let message { Label(message, systemImage: "exclamationmark.triangle.fill").font(.callout).foregroundStyle(.red) }
                    if let d = g.description, !d.isEmpty { Text(d).foregroundStyle(.secondary) }
                    if let p = g.proposedPlan {
                        VStack(alignment: .leading, spacing: Space.s) {
                            SectionHeader("Proposed plan", subtitle: "Nothing runs until you accept it")
                            PlanTimeline(rows: p.steps.enumerated().map { i, s in
                                PlanTimeline.Row(id: i, title: s.title, state: .pending, subtitle: s.approvalReason, waitsForApproval: s.requiresApproval == true)
                            })
                            if p.tainted == true { Label("The agent had read untrusted content while planning. Read every step first.", systemImage: "exclamationmark.triangle").font(.callout).foregroundStyle(.orange) }
                            HStack {
                                Button("Accept") { accept(g, p, autorun: false) }.primaryButton()
                                Button("Accept and run") { accept(g, p, autorun: true) }.secondaryButton()
                                Button("Discard", role: .destructive) { act { try await $0.rejectPlan(goal: g.id) } }.secondaryButton()
                            }
                        }.padding(Space.m).card()
                    }
                    if !g.steps.isEmpty {
                        VStack(alignment: .leading, spacing: Space.s) {
                            SectionHeader("Steps") { ProgressView(value: g.progress).frame(width: 90) }
                            PlanTimeline(rows: g.steps.enumerated().map { i, s in
                                let state: PlanTimeline.State = s.isDone ? .done : s.status == "blocked" ? .blocked : (g.steps.firstIndex { !$0.isDone } == i ? .current : .pending)
                                return PlanTimeline.Row(id: i, title: s.title, state: state, subtitle: s.status == "blocked" ? s.detail : nil, waitsForApproval: s.requiresApproval == true)
                            })
                        }.padding(Space.m).card()
                    }
                    if g.needsPlan {
                        EmptyState(symbol: "list.bullet.clipboard", title: "No plan yet",
                                   message: "Ask the agent to propose steps for this goal. Nothing runs until you accept them.",
                                   actionTitle: g.isPlanning ? "Planning…" : "Ask the agent for a plan") { act { try await $0.requestPlan(goal: g.id) } }
                            .frame(minHeight: 220)
                    }
                    if g.isOpen {
                        HStack {
                            if !g.steps.isEmpty { Toggle("Run automatically", isOn: Binding(get: { g.autorun == true }, set: { on in act { try await $0.setGoalAutorun(g.id, on) } })) }
                            Spacer()
                            Button("Cancel goal", role: .destructive) { act { try await $0.cancelGoal(g.id) } }.secondaryButton()
                        }
                    }
                }.padding(Space.l)
            }
        } else {
            EmptyState(symbol: "checklist", title: "Select a goal", message: "Or make a suggestion into one from the list.")
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func suggestionRow(_ s: Suggestion) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(s.title).font(.headline)
            if let r = s.reason, !r.isEmpty { Text(r).font(.callout).foregroundStyle(.secondary) }
            if s.tainted == true { Label("Made by an agent that had read untrusted content.", systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.orange) }
            HStack {
                Button("Make it a goal") {
                    if s.tainted == true { confirmTaintedFor = ConfirmTainted(text: "An agent that had read untrusted content made this suggestion. Make it a goal anyway?") { act { try await $0.acceptSuggestion(s.id, confirmTainted: true) } } }
                    else { act { try await $0.acceptSuggestion(s.id) } }
                }
                Button("Dismiss", role: .destructive) { act { try await $0.dismissSuggestion(s.id) } }
            }.controlSize(.small)
        }.padding(.vertical, 3)
    }

    private func accept(_ g: Goal, _ p: ProposedPlan, autorun: Bool) {
        let go = { act { try await $0.acceptPlan(goal: g.id, confirmTainted: p.tainted == true, autorun: autorun) } }
        if p.tainted == true { confirmTaintedFor = ConfirmTainted(text: "The planner had read untrusted content. Accept this plan anyway?", action: go) } else { go() }
    }

    private func act(_ call: @escaping (MerionClient) async throws -> Void) {
        Task {
            guard let c = app.client else { return }
            do { try await call(c); message = nil } catch { message = error.localizedDescription }
            await load()
        }
    }

    private func load() async {
        guard let c = app.client else { return }
        do { async let g = c.goals(); async let s = c.suggestions(); (goals, suggestions) = try await (g, s); message = nil }
        catch { message = error.localizedDescription }
    }
}

private extension Goal {
    var statusKind: StatusKind {
        switch status {
        case "done": return .done
        case "cancelled", "failed": return .failed
        case "blocked": return .waiting
        default: return proposedPlan != nil ? .waiting : .running
        }
    }
}

/// A goal's plan, drawn as a line of dots joined by a rail — done steps behind you, one current step, the rest still ahead.
/// Read-only: it never decides anything, it only shows where a plan (proposed or accepted) currently stands.
struct PlanTimeline: View {
    enum State { case done, current, pending, blocked }
    struct Row: Identifiable { let id: Int; let title: String; let state: State; var subtitle: String?; var waitsForApproval: Bool = false }
    let rows: [Row]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(rows) { r in
                HStack(alignment: .top, spacing: Space.s) {
                    VStack(spacing: 0) {
                        dot(for: r.state)
                        if r.id != rows.count - 1 { Rectangle().fill(r.state == .done ? Brand.good.opacity(0.5) : Color.secondary.opacity(0.25)).frame(width: 2).frame(minHeight: 22) }
                    }.frame(width: 16)
                    VStack(alignment: .leading, spacing: 2) {
                        HStack(spacing: Space.xxs) {
                            Text(r.title).font(.callout).strikethrough(r.state == .done).foregroundStyle(r.state == .done ? .secondary : .primary)
                            if r.waitsForApproval { StatusChip(kind: .waiting, label: "Asks you first") }
                        }
                        if let subtitle = r.subtitle, !subtitle.isEmpty { Text(subtitle).font(.caption).foregroundStyle(.secondary) }
                    }.padding(.bottom, r.id == rows.count - 1 ? 0 : Space.s)
                }
            }
        }
    }

    @ViewBuilder private func dot(for state: State) -> some View {
        switch state {
        case .done: Image(systemName: "checkmark.circle.fill").foregroundStyle(Brand.good)
        case .current: Image(systemName: "circle.inset.filled").foregroundStyle(Brand.blue).symbolEffect(.pulse, options: .repeating)
        case .blocked: Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.orange)
        case .pending: Image(systemName: "circle").foregroundStyle(.secondary)
        }
    }
}
