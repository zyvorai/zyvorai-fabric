import XCTest
@testable import MerionKit

final class MemoryTests: XCTestCase {
    func client() throws -> MerionClient { try MerionClient(baseURL: URL(string: "https://keep.example")!, token: "kut1.secret", session: StubProtocol.session()) }
    func body() throws -> NSDictionary { try JSONSerialization.jsonObject(with: try XCTUnwrap(StubProtocol.bodies.last)) as! NSDictionary }

    func testMemoryDecodesItemsAndProposalsAndTheSwitch() async throws {
        StubProtocol.handler = { _, _ in (200, Data(#"{"enabled":true,"items":[{"id":"m1","text":"prefers window seats","kind":"preference","pinned":true,"origin":"user"}],"proposals":[{"id":"p1","text":"likes aisle","kind":"note","tainted":true,"origin":"agent"}]}"#.utf8)) }
        let v = try await client().memory()
        XCTAssertTrue(v.enabled)
        XCTAssertEqual(v.items.map(\.text), ["prefers window seats"])
        XCTAssertTrue(v.items[0].isPinned)
        XCTAssertEqual(v.proposals[0].tainted, true)
        XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/memory")
    }

    func testEveryMemoryChangeUsesItsOwnRouteAndBody() async throws {
        StubProtocol.handler = { _, _ in (200, Data("{}".utf8)) }
        let c = try client()
        try await c.setMemory(enabled: true)
        XCTAssertEqual(StubProtocol.seen.last?.httpMethod, "PUT"); XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/memory/settings")
        XCTAssertEqual(try body(), ["enabled": true] as NSDictionary)
        try await c.addMemory(text: "vegetarian", kind: "fact", pinned: true)
        XCTAssertEqual(StubProtocol.seen.last?.httpMethod, "POST"); XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/memory")
        XCTAssertEqual(try body(), ["text": "vegetarian", "kind": "fact", "pinned": true] as NSDictionary)
        try await c.decideMemoryProposal("p 1", accept: true)
        XCTAssertEqual(StubProtocol.seen.last?.url?.absoluteString, "https://keep.example/v1/memory/p%201/accept")
        try await c.decideMemoryProposal("p1", accept: false)
        XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/memory/p1/reject")
        StubProtocol.handler = { _, _ in (204, Data()) }
        try await c.deleteMemory("m1")
        XCTAssertEqual(StubProtocol.seen.last?.httpMethod, "DELETE"); XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/memory/m1")
        try await c.forgetAllMemory()
        XCTAssertEqual(StubProtocol.seen.last?.httpMethod, "DELETE"); XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/memory")
    }

    func testTheHostRefusingASecretIsSurfacedInWords() async throws {
        StubProtocol.handler = { _, _ in (400, Data(#"{"error":"the text looks like it contains a secret (github-token); memory never stores credentials"}"#.utf8)) }
        do { try await client().addMemory(text: "token ghp_x"); XCTFail("expected 400") } catch {
            XCTAssertEqual((error as? MerionError), .http(status: 400, message: "the text looks like it contains a secret (github-token); memory never stores credentials"))
        }
    }

    func testReceiptsDecodeWithoutAnythingSecretAndReadWhatWasDone() async throws {
        StubProtocol.handler = { _, _ in (200, Data(#"{"items":[{"id":"r1","at":"2026-09-27T10:00:00Z","agent":"mail-compose","credential":"gmail-send","method":"POST","url":"https://gmail.googleapis.com/gmail/v1/users/me/messages/send","body_bytes":812,"body_sha256":"digest","approval_id":"a1","idempotency_key":"k","status":200},{"id":"r2","method":"POST","url":"https://x.example/y","status":502}]}"#.utf8)) }
        let list = try await client().receipts(limit: 20)
        XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/v1/receipts")
        XCTAssertEqual(URLComponents(url: StubProtocol.seen.last!.url!, resolvingAgainstBaseURL: false)?.queryItems, [URLQueryItem(name: "limit", value: "20")])
        XCTAssertEqual(list.count, 2)
        XCTAssertEqual(list[0].host, "gmail.googleapis.com"); XCTAssertEqual(list[0].path, "/gmail/v1/users/me/messages/send")
        XCTAssertTrue(list[0].approved); XCTAssertTrue(list[0].succeeded)
        XCTAssertFalse(list[1].approved); XCTAssertFalse(list[1].succeeded); XCTAssertNil(list[1].atDate)
        XCTAssertNotNil(list[0].atDate)
        let mirror = Mirror(reflecting: list[0]).children.compactMap(\.label)
        XCTAssertFalse(mirror.contains { $0.lowercased().contains("sha") || $0.lowercased().contains("key") }, "the app model holds no body digest or key: \(mirror)")
    }
}
