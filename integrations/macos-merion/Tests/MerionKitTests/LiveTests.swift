import XCTest
@testable import MerionKit

/// Runs against a real Keep host. Skipped unless KEEP_API and KEEP_TOKEN are set (for example through an SSH tunnel).
final class LiveTests: XCTestCase {
    func liveClient() throws -> MerionClient {
        let env = ProcessInfo.processInfo.environment
        guard let api = env["KEEP_API"], let token = env["KEEP_TOKEN"], let url = URL(string: api) else { throw XCTSkip("KEEP_API / KEEP_TOKEN not set") }
        return try MerionClient(baseURL: url, token: token)
    }

    func testStatusAndTheUseCaseList() async throws {
        let c = try liveClient()
        let status = try await c.status()
        XCTAssertEqual(status.fluxvm?.ready, true)
        let demos = try await c.demos()
        XCTAssertTrue(demos.contains { $0.id == "csv-clean" })
    }

    func testRunACsvInARealCellAndReadTheArtifact() async throws {
        let c = try liveClient()
        let file = try Fixture.tempFile("live.csv", "name,qty\nAnn,1\nBob,2\n")
        let out = try await c.run(demo: "csv-clean", files: [file])
        XCTAssertEqual(out.egressConnects, 0, "a use-case cell makes no outbound connection")
        let ref = try XCTUnwrap(out.items.first?.result?.artifacts.first)
        let artifact = try await c.artifact(id: ref.id)
        XCTAssertFalse(artifact.body.isEmpty)
    }

    func testABatchRunsOneCellPerFile() async throws {
        let c = try liveClient()
        let a = try Fixture.tempFile("one.csv", "a,b\n1,2\n"), b = try Fixture.tempFile("two.csv", "a,b\n3,4\n")
        let out = try await c.run(demo: "csv-clean", files: [a, b])
        guard case .batch(let batch) = out else { return XCTFail("expected a batch") }
        XCTAssertEqual(batch.ok, 2)
        XCTAssertEqual(batch.egressConnects, 0)
    }

    func testTheRunAppearsInHistoryAndTwoRunsCanBeCompared() async throws {
        let c = try liveClient()
        let items = try await c.artifacts(limit: 5)
        XCTAssertFalse(items.isEmpty)
        if items.count >= 2 {
            let text = try await c.diff(items[1].id, items[0].id)
            XCTAssertFalse(text.isEmpty)
        }
    }

    /// A message built from page text, redacted and routed, runs through the real eml reader in a real cell.
    func testAnEmailBuiltFromPageTextIsReadByTheRealEmlExtractor() async throws {
        let c = try liveClient()
        let available = Set(try await c.demos().map(\.id))
        let page = CapturedPage(url: URL(string: "https://mail.example.com/u/0/#inbox/abc"), title: "Reminder: INV-2026-0142 is overdue",
                                text: "Hello,\nReminder: invoice INV-2026-0142 (Rs 1,25,000.00) is overdue since 15 Aug 2026. Your verification code is 482913.\nFrom evil@x Mon\nPlease pay by 12 Sep 2026.", isSelection: false)
        let redacted = Redactor.redact(page.text)
        XCTAssertFalse(redacted.text.contains("482913"))
        let route = try XCTUnwrap(EmailRouter.route(redacted.text, available: available).first, "the host lists no email use case; deploy mailbox-triage")
        let file = try Fixture.tempFile("page.eml", EmailBuilder.eml(from: page, body: redacted.text))
        let out = try await c.run(demo: route.useCase, files: [file])
        XCTAssertEqual(out.egressConnects, 0)
        let ref = try XCTUnwrap(out.items.first?.result?.artifacts.first)
        let body = try await c.artifact(id: ref.id).body
        XCTAssertTrue(body.contains("INV-2026-0142"), body)
        XCTAssertFalse(body.contains("482913"), "the code was redacted before upload")
    }
}
