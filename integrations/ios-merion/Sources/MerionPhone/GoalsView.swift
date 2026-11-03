import MerionKit
import SwiftUI

/// The person's goals, the plans an agent proposed for them, and the suggestions agents made. Accepting one is a decision recorded on the host;
/// it starts nothing by itself, and a step that sends or spends still waits in the Approvals tab.
struct GoalsView: View {
    @EnvironmentObject var model: AppModel
    @State private var goals: [Goal] = []
    @State private var suggestions = SuggestionsView(enabled: false, pending: [])
    @State private var loaded = false
    @State private var message: String?
    @State private var adding = false

    var body: some View {
        NavigationStack {
            List {
                if let message { Text(message).font(.footnote).foregroundStyle(.red) }
                Section {
                    Toggle("Let agents suggest things", isOn: Binding(get: { suggestions.enabled }, set: { setSuggestions($0) }))
                    ForEach(suggestions.pending) { s in SuggestionRow(suggestion: s, act: act) }
                } header: { Text("Suggestions") } footer: {
                    Text("Off: agents suggest nothing. On: they may, and nothing happens until you make one a goal.")
                }
                Section("Goals") {
                    ForEach(goals) { g in NavigationLink(value: g) { GoalRow(goal: g) } }
                    if loaded && goals.isEmpty { Text("No goals yet.").foregroundStyle(.secondary) }
                }
            }
            .navigationTitle("Goals")
            .navigationDestination(for: Goal.self) { g in GoalDetail(goalId: g.id, goals: $goals, act: act) }
            .toolbar { Button { adding = true } label: { Image(systemName: "plus") }.disabled(!model.isConnected) }
            .sheet(isPresented: $adding) { NewGoalSheet { title, steps in
                await act { _ = try await $0.createGoal(title: title, agent: model.agent, steps: steps) }
            } }
            .overlay { if !model.isConnected { ContentUnavailableView("Not connected", systemImage: "network.slash", description: Text("Add your Keep host and token in Settings.")) } }
            .refreshable { await load() }
            .task { await load() }
        }
    }

    /// Runs one client call, shows a refusal in words, then reloads.
    func act(_ call: @escaping (MerionClient) async throws -> Void) async {
        guard let client = model.client else { return }
        do { try await call(client); message = nil } catch { message = error.localizedDescription }
        await load()
    }

    private func setSuggestions(_ on: Bool) { Task { await act { try await $0.setSuggestions(enabled: on) } } }

    private func load() async {
        guard let client = model.client else { return }
        do {
            async let g = client.goals(); async let s = client.suggestions()
            (goals, suggestions) = try await (g, s); loaded = true; message = nil
        } catch { message = error.localizedDescription }
    }
}

struct GoalRow: View {
    let goal: Goal
    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack { Text(goal.title).lineLimit(2); Spacer(); Text(goal.status).font(.caption2.bold()).foregroundStyle(.secondary) }
            if !goal.steps.isEmpty {
                ProgressView(value: goal.progress)
                Text("\(goal.steps.filter(\.isDone).count) of \(goal.steps.count) steps" + (goal.autorun == true ? " · runs automatically" : " · paused")).font(.caption).foregroundStyle(.secondary)
            } else if goal.proposedPlan != nil {
                Label("A plan is waiting for you", systemImage: "list.bullet.clipboard").font(.caption).foregroundStyle(.orange)
            } else if goal.isPlanning {
                Text("Your agent is planning this…").font(.caption).foregroundStyle(.secondary)
            } else if goal.needsPlan {
                Text("No steps yet").font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}

struct SuggestionRow: View {
    let suggestion: Suggestion
    let act: (@escaping (MerionClient) async throws -> Void) async -> Void
    @State private var confirming = false

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(suggestion.title).font(.headline)
            if let r = suggestion.reason, !r.isEmpty { Text(r).font(.callout).foregroundStyle(.secondary) }
            if suggestion.tainted == true { Label("Made by an agent that had read untrusted content.", systemImage: "exclamationmark.triangle").font(.caption).foregroundStyle(.orange) }
            HStack {
                Button("Make it a goal") { if suggestion.tainted == true { confirming = true } else { accept(false) } }.buttonStyle(.borderedProminent)
                Button("Dismiss", role: .destructive) { Task { await act { try await $0.dismissSuggestion(suggestion.id) } } }.buttonStyle(.bordered)
            }
        }
        .padding(.vertical, 4)
        .confirmationDialog("Read it first: an agent that had read untrusted content made this suggestion.", isPresented: $confirming, titleVisibility: .visible) {
            Button("Make it a goal anyway") { accept(true) }
        }
    }

    private func accept(_ confirmTainted: Bool) { Task { await act { try await $0.acceptSuggestion(suggestion.id, confirmTainted: confirmTainted) } } }
}

struct GoalDetail: View {
    let goalId: String
    @Binding var goals: [Goal]
    let act: (@escaping (MerionClient) async throws -> Void) async -> Void
    @State private var confirming: Bool?   // run automatically?

    private var goal: Goal? { goals.first { $0.id == goalId } }

    var body: some View {
        if let goal {
            List {
                if let d = goal.description, !d.isEmpty { Section { Text(d) } }
                if let p = goal.proposedPlan {
                    Section {
                        ForEach(Array(p.steps.enumerated()), id: \.offset) { _, s in
                            Text(s.title + (s.requiresApproval == true ? "  (asks you first)" : ""))
                        }
                        if p.tainted == true { Label("The agent had read untrusted content while planning. Read every step first.", systemImage: "exclamationmark.triangle").font(.footnote).foregroundStyle(.orange) }
                        Button("Accept") { accept(p, autorun: false) }
                        Button("Accept and run") { accept(p, autorun: true) }
                        Button("Discard", role: .destructive) { Task { await act { try await $0.rejectPlan(goal: goal.id) } } }
                    } header: { Text("Proposed plan") } footer: { Text("Nothing runs until you accept it. A step that sends or spends still asks you.") }
                }
                if !goal.steps.isEmpty {
                    Section("Steps") {
                        ForEach(goal.steps) { s in
                            HStack {
                                Image(systemName: s.isDone ? "checkmark.circle.fill" : s.status == "blocked" ? "exclamationmark.circle.fill" : "circle").foregroundStyle(s.isDone ? .green : s.status == "blocked" ? .orange : .secondary)
                                VStack(alignment: .leading) { Text(s.title); if s.status == "blocked", let d = s.detail { Text(d).font(.caption).foregroundStyle(.secondary) } }
                            }
                        }
                    }
                }
                if goal.needsPlan {
                    Section { Button(goal.isPlanning ? "Planning…" : "Ask the agent for a plan") { Task { await act { try await $0.requestPlan(goal: goal.id) } } }.disabled(goal.isPlanning) }
                }
                if goal.isOpen {
                    Section {
                        if !goal.steps.isEmpty { Toggle("Run automatically", isOn: Binding(get: { goal.autorun == true }, set: { on in Task { await act { try await $0.setGoalAutorun(goal.id, on) } } })) }
                        Button("Cancel goal", role: .destructive) { Task { await act { try await $0.cancelGoal(goal.id) } } }
                    }
                }
            }
            .navigationTitle(goal.title).navigationBarTitleDisplayMode(.inline)
            .confirmationDialog("The planner had read untrusted content. Accept this plan anyway?", isPresented: Binding(get: { confirming != nil }, set: { if !$0 { confirming = nil } }), titleVisibility: .visible) {
                Button("Accept anyway") { let run = confirming ?? false; confirming = nil; send(goal, confirmTainted: true, autorun: run) }
            }
        } else {
            ContentUnavailableView("This goal is gone", systemImage: "questionmark.folder")
        }
    }

    private func accept(_ p: ProposedPlan, autorun: Bool) {
        if p.tainted == true { confirming = autorun } else if let goal { send(goal, confirmTainted: false, autorun: autorun) }
    }

    private func send(_ goal: Goal, confirmTainted: Bool, autorun: Bool) {
        Task { await act { try await $0.acceptPlan(goal: goal.id, confirmTainted: confirmTainted, autorun: autorun) } }
    }
}

struct NewGoalSheet: View {
    @Environment(\.dismiss) private var dismiss
    let create: (String, [String]) async -> Void
    @State private var title = ""
    @State private var steps = ""

    var body: some View {
        NavigationStack {
            Form {
                Section { TextField("A goal, e.g. Plan the weekend trip", text: $title, axis: .vertical) }
                Section {
                    TextField("One step per line", text: $steps, axis: .vertical).lineLimit(3...8)
                } footer: { Text("Leave the steps empty and ask your agent to plan it once the goal exists.") }
            }
            .navigationTitle("New goal").navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Add") {
                        let list = steps.split(separator: "\n").map { $0.trimmingCharacters(in: .whitespaces) }.filter { !$0.isEmpty }
                        Task { await create(title.trimmingCharacters(in: .whitespaces), list); dismiss() }
                    }.disabled(title.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            }
        }
    }
}
