import XCTest
@testable import KeepKit

final class CatalogTests: XCTestCase {
    func testGroupsAndHints() {
        XCTAssertEqual(Catalog.group(for: "github-prs"), .developer)
        XCTAssertEqual(Catalog.group(for: "windows-hotfixes"), .windows)
        XCTAssertEqual(Catalog.group(for: "my-own-usecase"), .other)
        XCTAssertTrue(Catalog.entry(for: "github-prs")?.howToGetFile?.hasPrefix("gh pr list") == true)
    }

    func testSuggestionsFollowTheExtensionThenTheName() {
        let demos = [
            Demo(id: "csv-clean", title: "", description: "", accepts: ["csv"], builtin: true, hasSample: nil, maxBytes: nil, egress: nil),
            Demo(id: "card-statement", title: "", description: "", accepts: ["csv"], builtin: false, hasSample: nil, maxBytes: nil, egress: nil),
            Demo(id: "pdf-brief", title: "", description: "", accepts: ["pdf"], builtin: true, hasSample: nil, maxBytes: nil, egress: nil),
        ]
        XCTAssertEqual(Catalog.suggestions(forFileNamed: "Card statement.CSV", among: demos).map(\.id), ["card-statement", "csv-clean"])
        XCTAssertEqual(Catalog.suggestions(forFileNamed: "x.pdf", among: demos).map(\.id), ["pdf-brief"])
        XCTAssertTrue(Catalog.suggestions(forFileNamed: "noext", among: demos).isEmpty)
    }

    /// Every pack in the repository has a catalogue entry (skipped when the tests run outside the repository).
    func testEveryExamplePackIsInTheCatalogue() throws {
        var dir = URL(fileURLWithPath: #filePath)
        for _ in 0..<5 { dir.deleteLastPathComponent() }   // CatalogTests.swift, KeepKitTests, Tests, macos-keep, integrations -> the repository root
        let examples = dir.appendingPathComponent("examples/merion-agents")
        guard let packs = try? FileManager.default.contentsOfDirectory(atPath: examples.path) else { throw XCTSkip("examples/merion-agents not found") }
        let known = Set(Catalog.entries.map(\.id))
        for p in packs {
            guard let data = try? Data(contentsOf: examples.appendingPathComponent("\(p)/pack.json")),
                  let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any], json["kind"] as? String == "usecase" else { continue }
            XCTAssertTrue(known.contains(p), "\(p) is missing from the catalogue; run tools/gen_catalog.py")
        }
    }
}
