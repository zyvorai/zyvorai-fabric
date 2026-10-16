import XCTest
@testable import MerionKit

final class ChatTests: XCTestCase {
    func client() throws -> MerionClient { try MerionClient(baseURL: URL(string: "https://keep.example")!, token: "kut1.secret", session: StubProtocol.session()) }

    // MARK: the event stream

    func testTextRunAndFinishAreMapped() {
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"RUN_STARTED","threadId":"t","runId":"r"}"#), .runStarted)
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"TEXT_MESSAGE_START","messageId":"m1","role":"assistant"}"#), .textStart(id: "m1"))
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"TEXT_MESSAGE_CONTENT","messageId":"m1","delta":"hi ☕"}"#), .textDelta(id: "m1", text: "hi ☕"))
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"TEXT_MESSAGE_END","messageId":"m1"}"#), .textEnd(id: "m1"))
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"RUN_FINISHED","result":"done"}"#), .runFinished(result: "done"))
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"RUN_FINISHED","result":null}"#), .runFinished(result: nil))
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"RUN_ERROR","message":"boom","code":"failed"}"#), .runError(message: "boom", code: "failed"))
        if case .runFinished(let r)? = AGUI.event(fromData: #"{"type":"RUN_FINISHED","result":{"a":1}}"#) { XCTAssertEqual(r, #"{"a":1}"#) } else { XCTFail() }
    }

    func testAHeldApprovalCarriesTheHostsPreviewAndTheDecisionClosesIt() throws {
        let line = #"{"type":"CUSTOM","name":"keep.approval_requested","value":{"approval_id":"a-1","kind":"send","prompt":"Agent wants to POST","decide":"x","preview":{"kind":"gmail-message","fields":[{"label":"To","value":"ana@example.com"},{"label":"Subject","value":"Hi"}]}}}"#
        guard case .approvalRequested(let n)? = AGUI.event(fromData: line) else { return XCTFail("not an approval") }
        XCTAssertEqual(n.id, "a-1")
        XCTAssertEqual(n.preview?.fields, [PreviewField(label: "To", value: "ana@example.com"), PreviewField(label: "Subject", value: "Hi")])
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"CUSTOM","name":"keep.approval_decided","value":{"approval_id":"a-1","decision":"denied"}}"#), .approvalDecided(id: "a-1", decision: "denied"))
        // an approval the agent asked for itself has no id: a status, never something a chat could act on
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"CUSTOM","name":"keep.approval_requested","value":{"prompt":"ok?"}}"#), .status("waiting for your approval"))
    }

    func testTheSnapshotAndOddEventsAreHandledWithoutFailing() {
        guard case .snapshot(let list)? = AGUI.event(fromData: #"{"type":"MESSAGES_SNAPSHOT","messages":[{"id":"msg-1","role":"user","content":"hello"},{"id":"msg-2","role":"assistant","content":"hi"},{"role":"user"}]}"#) else { return XCTFail() }
        XCTAssertEqual(list, [ChatMessage(id: "msg-1", role: .user, text: "hello"), ChatMessage(id: "msg-2", role: .assistant, text: "hi")])
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"CUSTOM","name":"keep.log","value":{}}"#), .ignored)
        XCTAssertEqual(AGUI.event(fromData: #"{"type":"STATE_DELTA"}"#), .ignored)
        XCTAssertNil(AGUI.event(fromData: "not json"))
        XCTAssertNil(AGUI.event(fromData: #"{"no":"type"}"#))
    }

    // MARK: the client

    func testAChatRunPostsTheMessageWithTheAgentAndStreamsUntilItFinishes() async throws {
        let sse = """
        data: {"type":"RUN_STARTED","threadId":"t","runId":"r"}

        data: {"type":"TEXT_MESSAGE_START","messageId":"m","role":"assistant"}

        data: {"type":"TEXT_MESSAGE_CONTENT","messageId":"m","delta":"Hello"}

        data: {"type":"CUSTOM","name":"keep.log","value":{}}

        data: {"type":"RUN_FINISHED","threadId":"t","runId":"r","result":"Hello"}

        data: {"type":"TEXT_MESSAGE_CONTENT","messageId":"m","delta":"never seen"}

        """
        StubProtocol.handler = { _, _ in (200, Data(sse.utf8)) }
        var got: [AGUIEvent] = []
        for try await ev in try client().chat(agent: "echo-agent", threadId: "thread-1", runId: "run-1", text: "hi there") { got.append(ev) }
        XCTAssertEqual(got, [.runStarted, .textStart(id: "m"), .textDelta(id: "m", text: "Hello"), .runFinished(result: "Hello")], "noise dropped, and nothing after the end")
        let req = try XCTUnwrap(StubProtocol.seen.last)
        XCTAssertEqual(req.url?.path, "/v1/agui")
        XCTAssertEqual(req.httpMethod, "POST")
        XCTAssertEqual(req.value(forHTTPHeaderField: "Authorization"), "Bearer kut1.secret")
        XCTAssertFalse(req.url!.absoluteString.contains("kut1"))
        let body = try JSONSerialization.jsonObject(with: try XCTUnwrap(StubProtocol.bodies.last)) as! [String: Any]
        XCTAssertEqual(body["threadId"] as? String, "thread-1")
        XCTAssertEqual(body["runId"] as? String, "run-1")
        XCTAssertEqual((body["forwardedProps"] as? [String: String])?["agent"], "echo-agent")
        XCTAssertEqual(((body["messages"] as? [[String: String]])?.first)?["content"], "hi there")
    }

    func testAChatRunReportsRefusals() async throws {
        StubProtocol.handler = { _, _ in (409, Data(#"{"error":"this thread belongs to agent 'other'"}"#.utf8)) }
        do { for try await _ in try client().chat(agent: "a", threadId: "t", text: "x") {}; XCTFail("expected 409") } catch {
            XCTAssertEqual(error as? MerionError, .http(status: 409, message: "this thread belongs to agent 'other'"))
        }
        StubProtocol.handler = { _, _ in (401, Data()) }
        do { for try await _ in try client().chat(agent: "a", threadId: "t", text: "x") {}; XCTFail("expected 401") } catch { XCTAssertEqual(error as? MerionError, .unauthorized) }
    }

    func testThreadsAreFilteredByAgentNewestFirstAndMessagesRead() async throws {
        let c = try client()
        StubProtocol.handler = { req, _ in
            if req.url?.path == "/v1/threads" {
                return (200, Data(#"{"items":[{"id":"t1","agent":"echo-agent","title":"old","updated_at":"2026-09-01T10:00:00Z","message_count":2},{"id":"t2","agent":"echo-agent","title":"new","updated_at":"2026-09-20T10:00:00Z","message_count":4,"client_thread_id":"c-2"},{"id":"t3","agent":"other","title":"x","updated_at":"2026-09-25T10:00:00Z"}]}"#.utf8))
            }
            return (200, Data(#"{"items":[{"id":"msg-1","role":"user","text":"hello","created_at":"2026-09-20T10:00:00Z"},{"id":"msg-2","role":"assistant","text":"hi"}]}"#.utf8))
        }
        let threads = try await c.threads(agent: "echo-agent")
        XCTAssertEqual(threads.map(\.id), ["t2", "t1"])
        XCTAssertEqual(threads.first?.clientThreadId, "c-2")
        let msgs = try await c.messages(thread: "t2")
        XCTAssertEqual(msgs.map(\.text), ["hello", "hi"])
        XCTAssertEqual(msgs.map(\.role), [.user, .assistant])
        XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/threads/t2/messages")
        StubProtocol.handler = { _, _ in (204, Data()) }
        try await c.deleteThread("t2")
        XCTAssertEqual(StubProtocol.seen.last?.httpMethod, "DELETE")
        XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/threads/t2")
    }

    func testAnApprovalDecodesItsPreviewAndOldHostsWithoutOneStillDecode() throws {
        let with = #"{"items":[{"id":"a","kind":"send","subject":"gmail.googleapis.com","prompt":"p","status":"pending","preview":{"kind":"calendar-event","fields":[{"label":"Title","value":"Dinner"}]}}]}"#
        let a = try JSONDecoder.keep.decode(ApprovalsResponse.self, from: Data(with.utf8)).items[0]
        XCTAssertEqual(a.preview?.fields.first, PreviewField(label: "Title", value: "Dinner"))
        let without = try Fixture.decode(ApprovalsResponse.self, "approvals.json").items[0]
        XCTAssertNil(without.preview)
    }
}
