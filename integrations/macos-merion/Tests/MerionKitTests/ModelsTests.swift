import XCTest
@testable import MerionKit

final class ModelsTests: XCTestCase {
    func testStatusAndDemosDecodeFromTheLiveHost() throws {
        let s = try Fixture.decode(MerionStatus.self, "status.json")
        XCTAssertEqual(s.merionMode, true)
        XCTAssertEqual(s.demoTemplate, "node22-agent")
        XCTAssertEqual(s.fluxvm?.ready, true)
        let d = try Fixture.decode(DemosResponse.self, "demos.json").demos
        XCTAssertEqual(Set(d.map(\.id)), ["pdf-brief", "csv-clean"])
        XCTAssertEqual(d.first { $0.id == "pdf-brief" }?.accepts, ["pdf"])
        XCTAssertEqual(d.first { $0.id == "pdf-brief" }?.builtin, true)
    }

    func testASingleRunKeepsTheEgressCountAndBadge() throws {
        let r = try Fixture.decode(RunResult.self, "run_single.json")
        XCTAssertEqual(r.demo, "csv-clean")
        XCTAssertEqual(r.egressConnects, 0)
        XCTAssertEqual(r.badge?.evidence, "software-test")
        XCTAssertEqual(r.badge?.operatorCanRead, true)
        XCTAssertEqual(r.artifacts.map(\.title), ["clean.csv", "report.md"])
    }

    func testASimulatedRunIsNeverPresentedAsProof() throws {
        let sim = Data(#"""
        {"demo":"csv-clean","session_id":"s1","filename":"a.csv","bytes":10,"artifacts":[{"id":"a1","kind":"csv","title":"clean.csv"}],
         "egress_connects":0,"badge":{"evidence":"simulated","sealed":false,"operator_can_read":true,"proxy":"strict"}}
        """#.utf8)
        let outcome = try MerionClient.decodeRun(sim)
        XCTAssertTrue(outcome.isSimulated)
        XCTAssertEqual(outcome.egressConnects, 0, "the count is still shown, but never as evidence")
        // a real cell (and an older host with no `sealed` field) is not simulated
        let real = try MerionClient.decodeRun(Fixture.data("run_single.json"))
        XCTAssertFalse(real.isSimulated)
        XCTAssertNil((try Fixture.decode(RunResult.self, "run_single.json")).badge?.sealed)
    }

    func testABatchWithOneFailureIsDecodedNotThrown() throws {
        let outcome = try MerionClient.decodeRun(Fixture.data("run_batch_partial.json"))
        guard case .batch(let b) = outcome else { return XCTFail("expected a batch") }
        XCTAssertEqual(b.count, 2); XCTAssertEqual(b.ok, 1); XCTAssertEqual(b.failed, 1)
        XCTAssertEqual(outcome.items.map(\.ok), [true, false])
        XCTAssertEqual(outcome.items[1].error, "the file is empty")
        XCTAssertEqual(outcome.egressConnects, 0)
    }

    func testASingleResponseBecomesOneItem() throws {
        let outcome = try MerionClient.decodeRun(Fixture.data("run_single.json"))
        guard case .single = outcome else { return XCTFail("expected a single result") }
        XCTAssertEqual(outcome.items.count, 1)
        XCTAssertTrue(outcome.items[0].ok)
    }

    func testArtifactsDecodeAndParseNanosecondDates() throws {
        let a = try Fixture.decode(ArtifactsResponse.self, "artifacts.json").items
        XCTAssertEqual(a.count, 1)
        XCTAssertTrue(a[0].body.hasPrefix("# "))
        XCTAssertNotNil(a[0].createdDate, "the runtime writes nanoseconds: \(a[0].createdAt ?? "nil")")
        XCTAssertNotNil(a[0].metadata?.demo)
    }

    func testApprovalsAndTheInboxCarryTheSigningChallenge() throws {
        let list = try Fixture.decode(ApprovalsResponse.self, "approvals.json").items
        XCTAssertFalse(list.isEmpty)
        XCTAssertNil(list[0].sign, "a decided approval has no challenge")
        let inbox = try Fixture.decode(Inbox.self, "inbox.json")
        let p = try XCTUnwrap(inbox.pendingApprovals.first)
        XCTAssertTrue(p.isPending)
        XCTAssertEqual(p.sign?.expiresAt, 1_790_003_600)
        XCTAssertEqual(p.sign?.algorithms, ["p256", "ed25519"])
        XCTAssertNotNil(MerionDates.parse(p.createdAt ?? ""))
    }

    func testUsageDecodes() throws {
        let u = try Fixture.decode(UsageReport.self, "usage.json")
        XCTAssertEqual(u.usage.runs, 0)
        XCTAssertNil(u.limits.maxRunsPerDay)
    }
}
