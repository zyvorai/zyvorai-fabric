import XCTest
@testable import MerionKit

final class ClientTests: XCTestCase {
    func client(_ base: String = "https://keep.example/api") throws -> MerionClient {
        try MerionClient(baseURL: URL(string: base)!, token: "kut1.secret", session: StubProtocol.session())
    }

    func testTheTokenIsSentInTheHeaderAndNeverInTheURL() async throws {
        let c = try client()
        StubProtocol.handler = { _, _ in (200, (try? Fixture.data("status.json")) ?? Data()) }
        _ = try await c.status()
        let req = try XCTUnwrap(StubProtocol.seen.last)
        XCTAssertEqual(req.value(forHTTPHeaderField: "Authorization"), "Bearer kut1.secret")
        XCTAssertFalse(req.url!.absoluteString.contains("kut1"))
        XCTAssertEqual(req.url?.path, "/api/v1/keep/status", "a base path is kept")
    }

    func testBadHostsAreRefusedUpFront() {
        XCTAssertThrowsError(try MerionClient(baseURL: URL(string: "ftp://x")!, token: "t")) { XCTAssertEqual($0 as? MerionError, .badURL) }
        XCTAssertThrowsError(try MerionClient(baseURL: URL(string: "not a url")!, token: "t"))
    }

    func testUnauthorizedAndErrorMessages() async throws {
        let c = try client()
        StubProtocol.handler = { _, _ in (401, Data(#"{"error":"missing or invalid bearer token"}"#.utf8)) }
        do { _ = try await c.demos(); XCTFail("expected 401") } catch { XCTAssertEqual(error as? MerionError, .unauthorized) }
        StubProtocol.handler = { _, _ in (404, Data(#"{"error":"no demo named \"nope\""}"#.utf8)) }
        do { _ = try await c.artifact(id: "x"); XCTFail("expected 404") } catch {
            XCTAssertEqual(error as? MerionError, .http(status: 404, message: #"no demo named "nope""#))
        }
    }

    func testRunPostsMultipartAndDecodesTheResult() async throws {
        let c = try client()
        let file = try Fixture.tempFile("a b.csv", "name,qty\nAnn,1\n")
        StubProtocol.handler = { _, _ in (201, (try? Fixture.data("run_single.json")) ?? Data()) }
        let out = try await c.run(demo: "csv-clean", files: [file])
        let req = try XCTUnwrap(StubProtocol.seen.last)
        XCTAssertEqual(req.httpMethod, "POST")
        XCTAssertEqual(req.url?.path, "/api/v1/demos/csv-clean")
        XCTAssertTrue(req.value(forHTTPHeaderField: "content-type")?.hasPrefix("multipart/form-data; boundary=") == true)
        let body = String(decoding: try XCTUnwrap(StubProtocol.bodies.last), as: UTF8.self)
        XCTAssertTrue(body.contains(#"name="file"; filename="a b.csv""#), body)
        XCTAssertTrue(body.contains("Ann,1"))
        XCTAssertEqual(out.egressConnects, 0)
    }

    func testAPartialBatchIs207AndNotAnError() async throws {
        let c = try client()
        let f1 = try Fixture.tempFile("one.csv", "a\n1\n"), f2 = try Fixture.tempFile("two.csv", "a\n2\n")
        StubProtocol.handler = { _, _ in (207, (try? Fixture.data("run_batch_partial.json")) ?? Data()) }
        let out = try await c.run(demo: "csv-clean", files: [f1, f2])
        XCTAssertEqual(out.items.filter { !$0.ok }.count, 1)
    }

    func testAFileOverTheUseCaseLimitIsRefusedBeforeUpload() async throws {
        let c = try client()
        let big = try Fixture.tempFile("big.txt", String(repeating: "x", count: 5000))
        StubProtocol.handler = { _, _ in XCTFail("nothing should be uploaded"); return (500, Data()) }
        do { _ = try await c.run(demo: "csv-clean", files: [big], maxBytes: 1000); XCTFail("expected tooLarge") } catch {
            guard case MerionError.tooLarge(_, _, let limit) = error else { return XCTFail("\(error)") }
            XCTAssertEqual(limit, 1000)
        }
    }

    func testDecidePostsTheSignedDecision() async throws {
        let c = try client()
        StubProtocol.handler = { _, _ in (200, Data("{}".utf8)) }
        try await c.decide(approval: "abc", .approved, deviceId: "mac", signature: "c2ln")
        let sent = try XCTUnwrap(try JSONSerialization.jsonObject(with: XCTUnwrap(StubProtocol.bodies.last)) as? [String: String])
        XCTAssertEqual(sent, ["decision": "approved", "device_id": "mac", "signature": "c2ln"])
        XCTAssertEqual(StubProtocol.seen.last?.url?.path, "/api/v1/approvals/abc")
    }

    func testMultipartQuotesAndBreaksInNamesAreNeutralised() throws {
        let file = try Fixture.tempFile("x.txt", "hello")
        let body = try MultipartBody.write(parts: [.init(filename: "we\"ird\r\nname.txt", source: file)])
        defer { body.remove() }
        let text = String(decoding: try Data(contentsOf: body.fileURL), as: UTF8.self)
        XCTAssertTrue(text.contains(#"filename="we'ird  name.txt""#), text)
        XCTAssertTrue(text.hasSuffix("--\(body.boundary)--\r\n"))
    }
}
