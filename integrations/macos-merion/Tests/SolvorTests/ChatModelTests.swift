import MerionKit
import XCTest
@testable import Solvor

/// Answers from canned events, so a run streams deterministically without a network.
private final class StubChatAPI: ChatAPI, @unchecked Sendable {
    var threadsResult: [ChatThread] = []
    var messagesByThread: [String: [ChatMessage]] = [:]
    var events: [AGUIEvent] = []
    var streamError: Error?
    var deleted: [String] = []
    private(set) var sentAgent: String?
    private(set) var sentThreadId: String?
    private(set) var sentTexts: [String] = []

    func threads(agent: String?) async throws -> [ChatThread] { threadsResult }
    func messages(thread id: String) async throws -> [ChatMessage] { messagesByThread[id] ?? [] }
    func deleteThread(_ id: String) async throws { deleted.append(id) }

    func chat(agent: String, threadId: String, runId: String, text: String) -> AsyncThrowingStream<AGUIEvent, Error> {
        sentAgent = agent; sentThreadId = threadId; sentTexts.append(text)
        return AsyncThrowingStream { cont in
            if let streamError { cont.finish(throwing: streamError); return }
            for e in events { cont.yield(e) }
            cont.finish()
        }
    }
}

private func thread(_ id: String, agent: String = "echo-agent", clientThreadId: String? = nil) -> ChatThread {
    ChatThread(id: id, agent: agent, title: id, clientThreadId: clientThreadId, updatedAt: nil, messageCount: nil)
}

@MainActor
final class ChatModelTests: XCTestCase {
    private var api = StubChatAPI()
    private func makeModel(agent: String = "echo-agent") -> ChatModel {
        api = StubChatAPI()
        return ChatModel(agent: agent, api: { [unowned self] in self.api })
    }

    func testSendingAppendsTheUserBubbleThenStreamsTheReplyDeltaByDelta() async {
        let m = makeModel()
        api.events = [.runStarted, .textStart(id: "r1"), .textDelta(id: "r1", text: "Hel"), .textDelta(id: "r1", text: "lo"), .textEnd(id: "r1"), .runFinished(result: "Hello")]
        m.send("hi there")
        for _ in 0..<50 where m.sending { try? await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertEqual(m.bubbles.map(\.text), ["hi there", "Hello"])
        XCTAssertEqual(m.bubbles.map(\.role), [.user, .assistant])
        XCTAssertFalse(m.bubbles[1].streaming, "textEnd stops the streaming flag")
        XCTAssertFalse(m.sending)
        XCTAssertEqual(api.sentAgent, "echo-agent"); XCTAssertEqual(api.sentTexts, ["hi there"])
    }

    func testAnEmptyOrBlankMessageSendsNothing() {
        let m = makeModel()
        m.send(""); m.send("   \n")
        XCTAssertTrue(m.bubbles.isEmpty); XCTAssertTrue(api.sentTexts.isEmpty)
    }

    func testAPendingApprovalShowsAsACardAndClearsOnlyOnItsOwnDecision() async {
        let m = makeModel()
        api.events = [.approvalRequested(ApprovalNotice(id: "a1", kind: "send", prompt: "send this?", preview: nil)), .approvalDecided(id: "other", decision: "approved")]
        m.send("send it")
        for _ in 0..<50 where m.sending { try? await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertEqual(m.pendingApproval?.id, "a1", "a decision for a different approval does not clear this one")
        guard case .approval(let n) = m.bubbles.last(where: { if case .approval = $0.kind { return true } else { return false } })?.kind else { return XCTFail() }
        XCTAssertEqual(n.id, "a1")
        let m2 = makeModel()
        api.events = [.approvalRequested(ApprovalNotice(id: "a2", kind: "send", prompt: "ok?", preview: nil)), .approvalDecided(id: "a2", decision: "denied")]
        m2.send("y")
        for _ in 0..<50 where m2.sending { try? await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertNil(m2.pendingApproval, "its own decision clears it")
    }

    func testARunErrorIsShownAndDoesNotCrashTheStream() async {
        let m = makeModel()
        api.events = [.textStart(id: "r1"), .runError(message: "the sandbox failed", code: "boom")]
        m.send("do it")
        for _ in 0..<50 where m.sending { try? await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertEqual(m.bubbles.last?.text, "the sandbox failed")
        XCTAssertEqual(m.bubbles.last?.kind, .error)
    }

    func testATransportFailureIsShownAsABubbleNotACrash() async {
        let m = makeModel()
        api.streamError = MerionError.transport("offline")
        m.send("hi")
        for _ in 0..<50 where m.sending { try? await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertTrue(m.bubbles.last?.text.contains("offline") == true)
    }

    func testOpeningAThreadUsesItsClientThreadIdAndLoadsItsMessages() async {
        let m = makeModel()
        api.messagesByThread["t1"] = [ChatMessage(id: "1", role: .user, text: "hey"), ChatMessage(id: "2", role: .assistant, text: "hi")]
        await m.open(thread("t1", clientThreadId: "client-1"))
        XCTAssertEqual(m.threadId, "client-1"); XCTAssertEqual(m.bubbles.map(\.text), ["hey", "hi"])
        m.send("more")
        for _ in 0..<50 where m.sending { try? await Task.sleep(nanoseconds: 5_000_000) }
        XCTAssertEqual(api.sentThreadId, "client-1", "a reply to an opened thread keeps the host's own id")
    }

    func testANewThreadGetsAFreshIdAndForgetsThePreviousBubbles() async {
        let m = makeModel()
        await m.open(thread("t1", clientThreadId: "client-1"))
        let old = m.threadId
        m.newThread()
        XCTAssertNotEqual(m.threadId, old); XCTAssertTrue(m.bubbles.isEmpty); XCTAssertNil(m.pendingApproval)
    }

    func testLoadThreadsFiltersByAgentThroughTheAPI() async {
        let m = makeModel(agent: "memory-agent")
        api.threadsResult = [thread("t1", agent: "memory-agent")]
        await m.loadThreads()
        XCTAssertEqual(m.threads.map(\.id), ["t1"])
    }

    /// `ChatModel.swift` must never import the two things that can reach a decision or a secret: `ApprovalSigner` (which signs a decision)
    /// and `TokenStore` (which holds the token). The chat can show that an approval is waiting; deciding it stays in Approvals.
    func testChatModelDependsOnNeitherTheApprovalSignerNorATokenStore() throws {
        var dir = URL(fileURLWithPath: #filePath)
        for _ in 0..<3 { dir.deleteLastPathComponent() }   // ChatModelTests.swift, SolvorTests, Tests -> macos-merion
        let full = try String(contentsOf: dir.appendingPathComponent("Sources/Solvor/ChatModel.swift"), encoding: .utf8)
        // Only the code, not this file's own doc comments about what it must not do.
        let code = full.split(separator: "\n").filter { !$0.trimmingCharacters(in: .whitespaces).hasPrefix("//") }.joined(separator: "\n")
        XCTAssertFalse(code.contains("ApprovalSigner"), "ChatModel must not be able to sign a decision")
        XCTAssertFalse(code.contains("TokenStore"), "ChatModel must not be able to reach a token")
        XCTAssertFalse(code.contains(".decide("), "deciding is Approvals' job, not the chat's")
    }
}
