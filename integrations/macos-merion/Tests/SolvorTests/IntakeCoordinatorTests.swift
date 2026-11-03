import MerionKit
import XCTest
@testable import Solvor

final class IntakeCoordinatorTests: XCTestCase {
    private func demo(_ id: String, accepts: [String]) -> Demo {
        Demo(id: id, title: id, description: "", accepts: accepts, builtin: true, hasSample: nil, maxBytes: nil, egress: nil)
    }
    private lazy var dir: URL = {
        let d = FileManager.default.temporaryDirectory.appendingPathComponent("intake-\(UUID().uuidString)").resolvingSymlinksInPath()
        try? FileManager.default.createDirectory(at: d, withIntermediateDirectories: true)
        addTeardownBlock { try? FileManager.default.removeItem(at: d) }
        return d
    }()
    /// `FileExpander` (and so `IntakeCoordinator`) only looks at files that exist, so every fixture is a real, empty file.
    private func touch(_ name: String, in base: URL? = nil) -> URL {
        let url = (base ?? dir).appendingPathComponent(name)
        FileManager.default.createFile(atPath: url.path, contents: Data())
        return url
    }

    func testOneFileWithOneMatchingUseCaseJustRuns() {
        let demos = [demo("pdf-brief", accepts: ["pdf"])]
        let f = touch("report.pdf")
        let plan = IntakeCoordinator.plan(urls: [f], demos: demos)
        XCTAssertEqual(plan.outcome, .run(demoId: "pdf-brief", files: [f]))
        XCTAssertTrue(plan.leftOut.isEmpty)
    }

    func testOneFileWithSeveralMatchingUseCasesAsksWhichOne() {
        let demos = [demo("csv-clean", accepts: ["csv"]), demo("card-statement", accepts: ["csv"])]
        let f = touch("Card statement.csv")
        let plan = IntakeCoordinator.plan(urls: [f], demos: demos)
        guard case .choose(let files, let options) = plan.outcome else { return XCTFail("\(plan.outcome)") }
        XCTAssertEqual(files, [f])
        XCTAssertEqual(Set(options), Set(["card-statement", "csv-clean"]), "the name-matched one and the plain one, in Catalog's order")
    }

    func testNoUseCaseReadsItLeavesEverythingOutRatherThanGuessing() {
        let f = touch("notes.docx")
        let plan = IntakeCoordinator.plan(urls: [f], demos: [demo("csv-clean", accepts: ["csv"])])
        XCTAssertEqual(plan.outcome, .none)
        XCTAssertEqual(plan.leftOut, [f])
        XCTAssertNil(IntakeCoordinator.leftOutNotice(plan), "no run happened, so there is nothing to explain as 'left out'")
    }

    func testAMixedDropRunsTheMatchingFilesAndNamesWhatWasLeftOut() {
        let demos = [demo("pdf-brief", accepts: ["pdf"])]
        let pdf1 = touch("a.pdf"), pdf2 = touch("b.pdf"), other = touch("c.docx")
        let plan = IntakeCoordinator.plan(urls: [pdf1, other, pdf2], demos: demos)
        guard case .run(let id, let files) = plan.outcome else { return XCTFail("\(plan.outcome)") }
        XCTAssertEqual(id, "pdf-brief"); XCTAssertEqual(Set(files), [pdf1, pdf2])
        XCTAssertEqual(plan.leftOut, [other])
        XCTAssertEqual(IntakeCoordinator.leftOutNotice(plan), "c.docx wasn't included: no use case reads it together with the rest. Drop it on its own.")
    }

    func testTheNoticeIsPluralForMoreThanOneLeftOutFile() {
        let demos = [demo("pdf-brief", accepts: ["pdf"])]
        let plan = IntakeCoordinator.plan(urls: [touch("a.pdf"), touch("x.docx"), touch("y.png")], demos: demos)
        XCTAssertEqual(IntakeCoordinator.leftOutNotice(plan), "x.docx, y.png weren't included: no use case reads them together with the rest. Drop them on their own.")
    }

    func testAFolderIsExpandedOneLevelBeforePlanning() {
        let sub = dir.appendingPathComponent("statements"); try? FileManager.default.createDirectory(at: sub, withIntermediateDirectories: true)
        let f1 = touch("jan.csv", in: sub), f2 = touch("feb.csv", in: sub)
        let plan = IntakeCoordinator.plan(urls: [sub], demos: [demo("csv-clean", accepts: ["csv"])])
        guard case .run(_, let files) = plan.outcome else { return XCTFail("\(plan.outcome)") }
        // Compared by name, not the full URL: resolving `/var` to `/private/var` differs between a constructed URL and
        // one the filesystem hands back, and that difference is not what this test is about.
        XCTAssertEqual(Set(files.map(\.lastPathComponent)), Set([f1, f2].map(\.lastPathComponent)))
    }

    func testNothingDroppedIsNone() {
        XCTAssertEqual(IntakeCoordinator.plan(urls: [], demos: []).outcome, .none)
    }
}
