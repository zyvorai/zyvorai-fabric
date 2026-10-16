import XCTest
@testable import MerionKit

final class GoalsTests: XCTestCase {
    func client() throws -> MerionClient { try MerionClient(baseURL: URL(string: "https://keep.example")!, token: "kut1.secret", session: StubProtocol.session()) }
    func json(_ req: URLRequest) throws -> [String: Any] {
        try JSONSerialization.jsonObject(with: try XCTUnwrap(StubProtocol.bodies.last)) as! [String: Any]
    }

    let goalsJSON = #"""
    {"items":[
      {"id":"g1","title":"Trip","agent":"a","status":"open","autorun":true,"updated_at":"2026-09-20T10:00:00Z","plan":[{"id":"s1","title":"Find","status":"done"},{"id":"s2","title":"Book","status":"pending","requires_approval":true}]},
      {"id":"g2","title":"Lisbon","agent":"a","status":"open","autorun":false,"updated_at":"2026-09-26T10:00:00Z","plan":[],"proposed_plan":{"tainted":true,"steps":[{"title":"Flights","requires_approval":false},{"title":"Hotel","requires_approval":true,"approval_reason":"Books a non-refundable room over $200"}]}},
      {"id":"g3","title":"Fresh","agent":"a","status":"open","plan":[],"updated_at":"2026-09-25T10:00:00Z"},
      {"id":"g4","title":"Asked","agent":"a","status":"open","plan":[],"planning_session_id":"sess"},
      {"id":"g5","title":"Done","agent":"a","status":"done","plan":[]}
    ]}
    """#

    func testGoalsDecodeNewestFirstWithPlanStates() async throws {
        StubProtocol.handler = { _, _ in (200, Data(self.goalsJSON.utf8)) }
        let goals = try await client().goals()
        XCTAssertEqual(goals.map(\.id).prefix(3), ["g2", "g3", "g1"], "newest first")
        let g = Dictionary(uniqueKeysWithValues: goals.map { ($0.id, $0) })
        XCTAssertEqual(g["g1"]?.progress, 0.5)
        XCTAssertEqual(g["g1"]?.steps.last?.requiresApproval, true)
        XCTAssertEqual(g["g2"]?.proposedPlan?.tainted, true)
        XCTAssertEqual(g["g2"]?.proposedPlan?.steps.map(\.title), ["Flights", "Hotel"])
        XCTAssertNil(g["g2"]?.proposedPlan?.steps.first?.approvalReason, "a step with no reason decodes to nil, not a crash")
        XCTAssertEqual(g["g2"]?.proposedPlan?.steps.last?.approvalReason, "Books a non-refundable room over $200")
        XCTAssertEqual(g["g2"]?.needsPlan, false, "it has a proposal to read")
        XCTAssertEqual(g["g3"]?.needsPlan, true)
        XCTAssertEqual(g["g4"]?.isPlanning, true)
        XCTAssertEqual(g["g5"]?.needsPlan, false, "a finished goal is not planned")
        XCTAssertEqual(g["g3"]?.progress, 0)
    }

    func testCreatingAGoalWithNoStepsSendsAnEmptyPlanAndNoAutorun() async throws {
        StubProtocol.handler = { _, _ in (201, Data(#"{"id":"g9","title":"T","agent":"echo","status":"open","plan":[]}"#.utf8)) }
        let g = try await client().createGoal(title: "T", agent: "echo")
        XCTAssertEqual(g.id, "g9")
        let sent = try json(StubProtocol.seen.last!)
        XCTAssertEqual(sent["title"] as? String, "T"); XCTAssertEqual(sent["agent"] as? String, "echo"); XCTAssertEqual(sent["autorun"] as? Bool, false)
        XCTAssertEqual((sent["plan"] as? [Any])?.count, 0)
        XCTAssertNil(sent["user_id"], "a person's token names no user")
        _ = try await client().createGoal(title: "T", agent: "echo", steps: ["a", "b"], autorun: true)
        XCTAssertEqual(((try json(StubProtocol.seen.last!))["plan"] as? [[String: String]])?.map { $0["title"] }, ["a", "b"])
    }

    func testPlanRequestsUseTheRightRoutesAndOnlyRealSwitchesAreSent() async throws {
        StubProtocol.handler = { _, _ in (200, Data("{}".utf8)) }
        let c = try client()
        try await c.requestPlan(goal: "g 1")
        XCTAssertEqual(StubProtocol.seen.last?.url?.absoluteString, "https://keep.example/v1/goals/g%201/plan", "an id is escaped once")
        XCTAssertEqual(StubProtocol.seen.last?.httpMethod, "POST")
        XCTAssertTrue(StubProtocol.bodies.last?.isEmpty ?? false, "no body means no options")
        XCTAssertNil(StubProtocol.seen.last?.value(forHTTPHeaderField: "content-type"))
        try await c.acceptPlan(goal: "g1")
        XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/goals/g1/plan/accept")
        XCTAssertTrue(StubProtocol.bodies.last?.isEmpty ?? false)
        try await c.acceptPlan(goal: "g1", confirmTainted: true, autorun: true)
        XCTAssertEqual(try json(StubProtocol.seen.last!) as NSDictionary, ["confirm_tainted": true, "autorun": true] as NSDictionary)
        try await c.rejectPlan(goal: "g1")
        XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/goals/g1/plan/reject")
    }

    func testCancelAndPauseArePatches() async throws {
        StubProtocol.handler = { _, _ in (200, Data("{}".utf8)) }
        let c = try client()
        try await c.cancelGoal("g1")
        XCTAssertEqual(StubProtocol.seen.last?.httpMethod, "PATCH")
        XCTAssertEqual(try json(StubProtocol.seen.last!) as NSDictionary, ["status": "cancelled"] as NSDictionary)
        try await c.setGoalAutorun("g1", false)
        XCTAssertEqual(try json(StubProtocol.seen.last!) as NSDictionary, ["autorun": false] as NSDictionary)
    }

    func testSuggestionsAreReadSwitchedAndDecided() async throws {
        StubProtocol.handler = { req, _ in
            req.httpMethod == "GET"
                ? (200, Data(#"{"enabled":true,"pending":[{"id":"s1","title":"Book the dentist","reason":"a gap on Friday","agent":"a","tainted":false},{"id":"s2","title":"Wire","agent":"a","tainted":true}],"decided":[]}"#.utf8))
                : (200, Data(#"{"ok":true}"#.utf8))
        }
        let c = try client()
        let v = try await c.suggestions()
        XCTAssertTrue(v.enabled)
        XCTAssertEqual(v.pending.map(\.title), ["Book the dentist", "Wire"])
        XCTAssertEqual(v.pending[1].tainted, true)
        try await c.setSuggestions(enabled: false)
        XCTAssertEqual(StubProtocol.seen.last?.httpMethod, "PUT")
        XCTAssertEqual(try json(StubProtocol.seen.last!) as NSDictionary, ["enabled": false] as NSDictionary)
        try await c.acceptSuggestion("s1")
        XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/suggestions/s1/accept")
        XCTAssertTrue(StubProtocol.bodies.last?.isEmpty ?? false)
        try await c.acceptSuggestion("s2", confirmTainted: true)
        XCTAssertEqual(try json(StubProtocol.seen.last!) as NSDictionary, ["confirm_tainted": true] as NSDictionary)
        try await c.dismissSuggestion("s1")
        XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/suggestions/s1/dismiss")
    }

    func testAHostRefusalIsSurfacedWithItsMessage() async throws {
        StubProtocol.handler = { _, _ in (409, Data(#"{"error":"the planner had read untrusted content; accept with confirm_tainted: true"}"#.utf8)) }
        do { try await client().acceptPlan(goal: "g1"); XCTFail("expected 409") } catch {
            XCTAssertEqual((error as? MerionError), .http(status: 409, message: "the planner had read untrusted content; accept with confirm_tainted: true"))
        }
    }
}
