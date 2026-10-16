import Foundation

// A person's goals, the plans an agent proposes for them, and the suggestions agents make (docs/merion/goals, docs/merion/suggestions).
// Nothing here runs anything by itself: accepting a plan or a suggestion is a decision the host records, and a step that sends or spends
// still waits for an approval signed on the device.

public struct GoalStep: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var id: String
    public var title: String
    public var status: String
    public var detail: String?
    public var requiresApproval: Bool?
    public var isDone: Bool { status == "done" || status == "skipped" }
}

public struct ProposedStep: Codable, Equatable, Hashable, Sendable {
    public var title: String
    public var requiresApproval: Bool?
    /// Why the planner thinks this step needs a person's approval (`agent-runtime/src/goal_plan.rs`), shown next to the step. Optional and capped
    /// server-side; a step without one is unchanged from before this existed.
    public var approvalReason: String?
}

/// A plan an agent proposed for a goal, waiting for the person. `tainted`: the planner had read untrusted content while planning.
public struct ProposedPlan: Codable, Equatable, Hashable, Sendable {
    public var steps: [ProposedStep]
    public var tainted: Bool?
}

public struct Goal: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var id: String
    public var title: String
    public var description: String?
    public var agent: String
    public var status: String
    public var autorun: Bool?
    public var plan: [GoalStep]?
    public var proposedPlan: ProposedPlan?
    public var planningSessionId: String?
    public var updatedAt: String?

    public var steps: [GoalStep] { plan ?? [] }
    public var isOpen: Bool { status == "open" || status == "blocked" }
    /// Nothing to run yet: the person can ask the agent to propose steps.
    public var needsPlan: Bool { isOpen && steps.isEmpty && proposedPlan == nil }
    public var isPlanning: Bool { planningSessionId != nil }
    public var progress: Double { steps.isEmpty ? 0 : Double(steps.filter(\.isDone).count) / Double(steps.count) }
}

struct GoalsResponse: Decodable { var items: [Goal] }

public struct Suggestion: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var id: String
    public var title: String
    public var reason: String?
    public var agent: String
    public var tainted: Bool?
}

public struct SuggestionsView: Decodable, Equatable, Sendable {
    public var enabled: Bool
    public var pending: [Suggestion]
    public init(enabled: Bool, pending: [Suggestion]) { self.enabled = enabled; self.pending = pending }
}

extension KeepClient {
    /// The person's goals, newest first.
    public func goals() async throws -> [Goal] {
        (try await getJSON("/v1/goals") as GoalsResponse).items.sorted { ($0.updatedAt ?? "") > ($1.updatedAt ?? "") }
    }

    /// A goal with no steps yet (an agent can propose them) or with the steps given. A goal with no steps cannot run automatically.
    public func createGoal(title: String, agent: String, steps: [String] = [], autorun: Bool = false) async throws -> Goal {
        let body: [String: Any] = ["title": title, "agent": agent, "autorun": autorun, "plan": steps.map { ["title": $0] }]
        return try await post("/v1/goals", body: body, as: Goal.self)
    }

    public func cancelGoal(_ id: String) async throws { _ = try await patch("/v1/goals/\(pathEscape(id))", body: ["status": "cancelled"]) }
    public func setGoalAutorun(_ id: String, _ on: Bool) async throws { _ = try await patch("/v1/goals/\(pathEscape(id))", body: ["autorun": on]) }

    /// Ask the host's planner agent to propose steps. The plan appears on the goal (`proposedPlan`) when it is ready; nothing runs.
    public func requestPlan(goal id: String) async throws { _ = try await post("/v1/goals/\(pathEscape(id))/plan", body: nil, as: Ignored.self) }

    /// Accept the proposed plan. `confirmTainted` is needed when the planner had read untrusted content; `autorun` also lets the host run the steps.
    public func acceptPlan(goal id: String, confirmTainted: Bool = false, autorun: Bool = false) async throws {
        var body: [String: Any] = [:]
        if confirmTainted { body["confirm_tainted"] = true }
        if autorun { body["autorun"] = true }
        _ = try await post("/v1/goals/\(pathEscape(id))/plan/accept", body: body.isEmpty ? nil : body, as: Ignored.self)
    }

    public func rejectPlan(goal id: String) async throws { _ = try await post("/v1/goals/\(pathEscape(id))/plan/reject", body: nil, as: Ignored.self) }

    public func suggestions() async throws -> SuggestionsView { try await getJSON("/v1/suggestions") }

    public func setSuggestions(enabled: Bool) async throws {
        var req = request("/v1/suggestions/settings", method: "PUT")
        req.setValue("application/json", forHTTPHeaderField: "content-type")
        req.httpBody = try JSONSerialization.data(withJSONObject: ["enabled": enabled])
        _ = try await sendRequest(req)
    }

    /// Make the suggestion an ordinary goal (no plan, not running). `confirmTainted` is needed for one made after reading untrusted content.
    public func acceptSuggestion(_ id: String, confirmTainted: Bool = false) async throws {
        _ = try await post("/v1/suggestions/\(pathEscape(id))/accept", body: confirmTainted ? ["confirm_tainted": true] : nil, as: Ignored.self)
    }

    public func dismissSuggestion(_ id: String) async throws { _ = try await post("/v1/suggestions/\(pathEscape(id))/dismiss", body: nil, as: Ignored.self) }

    // MARK: plumbing

    struct Ignored: Decodable { init(from decoder: Decoder) throws {} }

    private func post<T: Decodable>(_ path: String, body: [String: Any]?, as: T.Type = T.self) async throws -> T {
        var req = request(path, method: "POST")
        // An empty body means "no options"; a body says its type.
        if let body { req.setValue("application/json", forHTTPHeaderField: "content-type"); req.httpBody = try JSONSerialization.data(withJSONObject: body) }
        let (data, _) = try await sendRequest(req)
        if T.self == Ignored.self { return try JSONDecoder().decode(T.self, from: Data("{}".utf8)) }
        do { return try JSONDecoder.keep.decode(T.self, from: data) } catch { throw KeepError.decoding("\(error)") }
    }

    private func patch(_ path: String, body: [String: Any]) async throws -> Data {
        var req = request(path, method: "PATCH")
        req.setValue("application/json", forHTTPHeaderField: "content-type")
        req.httpBody = try JSONSerialization.data(withJSONObject: body)
        return try await sendRequest(req).0
    }
}
