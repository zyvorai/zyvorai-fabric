import XCTest
@testable import Solvor

final class MarkdownTests: XCTestCase {
    typealias B = MarkdownView.Block

    func testTheSummaryShapeParsesIntoBlocks() {
        let md = "# deck.md\n\nSummary of `x`.\n\n## Amounts\n- 1× 50,000 EUR\n- (no matches)\n\n## Size\n| Measure | Value |\n|---|---|\n| Lines | 9 |\n"
        let blocks = MarkdownView.parse(md)
        XCTAssertEqual(blocks.count, 7)
        XCTAssertEqual(blocks[0], .heading(1, "deck.md"))
        XCTAssertEqual(blocks[3], .bullet(text: "1× 50,000 EUR", indent: 0, marker: "•"))
        guard case .table(let header, let align, let rows) = blocks[6] else { return XCTFail("table: \(blocks[6])") }
        XCTAssertEqual(header, ["Measure", "Value"])
        XCTAssertEqual(rows, [["Lines", "9"]])
        XCTAssertEqual(align, [.leading, .trailing], "the Value column is all numbers, so it is right-aligned")
    }

    func testListsCanBeNumberedAndNested() {
        let blocks = MarkdownView.parse("1. first\n2. second\n   - nested a\n   - nested b\n     * deeper\n- top\n")
        XCTAssertEqual(blocks, [
            .bullet(text: "first", indent: 0, marker: "1."), .bullet(text: "second", indent: 0, marker: "2."),
            .bullet(text: "nested a", indent: 1, marker: "•"), .bullet(text: "nested b", indent: 1, marker: "•"),
            .bullet(text: "deeper", indent: 2, marker: "•"), .bullet(text: "top", indent: 0, marker: "•"),
        ])
        XCTAssertEqual(MarkdownView.parse("2) paren style"), [.bullet(text: "paren style", indent: 0, marker: "2)")])
        XCTAssertEqual(MarkdownView.parse("+ plus"), [.bullet(text: "plus", indent: 0, marker: "•")])
    }

    func testQuotesRulesAndCodeWithALanguage() {
        let blocks = MarkdownView.parse("> a quoted\n> line\n\n---\n\n```swift\nlet x = 1\n\nlet y = 2\n```\nafter")
        XCTAssertEqual(blocks, [.quote("a quoted line"), .rule, .code(language: "swift", text: "let x = 1\n\nlet y = 2"), .paragraph("after")])
        XCTAssertEqual(MarkdownView.parse("```\nplain\n```"), [.code(language: nil, text: "plain")])
        XCTAssertEqual(MarkdownView.parse("***"), [.rule])
        XCTAssertEqual(MarkdownView.parse("- one"), [.bullet(text: "one", indent: 0, marker: "•")], "a dash and a space is a bullet, not a rule")
    }

    func testAnUnterminatedFenceTakesTheRestAndNothingCrashes() {
        XCTAssertEqual(MarkdownView.parse("```\nnever closed\nstill code"), [.code(language: nil, text: "never closed\nstill code")])
        XCTAssertEqual(MarkdownView.parse(""), [])
        XCTAssertEqual(MarkdownView.parse("\n\n  \n"), [])
        XCTAssertEqual(MarkdownView.parse("| not | a table\nplain"), [.paragraph("| not | a table plain")], "no separator row, so it is text")
        XCTAssertEqual(MarkdownView.parse("a\r\nb"), [.paragraph("a b")], "Windows line endings")
    }

    func testTableAlignmentComesFromTheSeparatorAndFromNumbers() {
        let md = "| Name | Qty | Price | Note |\n|:---|---:|:---:|---|\n| Pens | 12 | 3.50 | ok |\n| Ink | 1,200 | 40 | later |\n"
        guard case .table(_, let align, let rows) = MarkdownView.parse(md)[0] else { return XCTFail("table") }
        XCTAssertEqual(align, [.leading, .trailing, .center, .leading])
        XCTAssertEqual(rows.count, 2)
        // numbers in a column with no explicit alignment become right-aligned; a column with any text does not
        guard case .table(_, let auto, _) = MarkdownView.parse("| a | b | c |\n|---|---|---|\n| x | 1 | 5 |\n| y | 22 | n/a |\n")[0] else { return XCTFail("table") }
        XCTAssertEqual(auto, [.leading, .trailing, .leading])
        // ragged rows are kept as they are; the renderer pads them
        guard case .table(_, _, let ragged) = MarkdownView.parse("| a | b |\n|---|---|\n| only |\n| 1 | 2 | 3 |\n")[0] else { return XCTFail("table") }
        XCTAssertEqual(ragged, [["only"], ["1", "2", "3"]])
    }

    func testWhatCountsAsANumber() {
        for yes in ["9", "1,234", "-3.5", "12%", "€50,000", "4 KB", "3×", "~40", "<5", "**7**", "`8`", "$1.20", "1 234"] { XCTAssertTrue(MarkdownView.isNumeric(yes), yes) }
        for no in ["", "n/a", "abc", "1.2.3.x.y.z", "two", "12 apples and pears", "-"] { XCTAssertFalse(MarkdownView.isNumeric(no), no) }
    }

    func testHeadingsMerionTheirLevelAndOnlyFourAreHeadings() {
        XCTAssertEqual(MarkdownView.parse("#### four"), [.heading(4, "four")])
        XCTAssertEqual(MarkdownView.parse("##### five"), [.paragraph("##### five")])
        XCTAssertEqual(MarkdownView.parse("#nospace"), [.paragraph("#nospace")])
    }
}

final class ComponentTests: XCTestCase {
    func testEveryStatusHasASymbolAWordAndItsOwnMeaning() {
        for kind in StatusKind.allCases { XCTAssertFalse(kind.symbol.isEmpty); XCTAssertFalse(kind.defaultLabel.isEmpty) }
        XCTAssertEqual(Set(StatusKind.allCases.map(\.symbol)).count, StatusKind.allCases.count, "no two states share a symbol, so colour is never the only cue")
    }

    func testEveryPaneHasATint() {
        XCTAssertEqual(Set(Pane.allCases.map(\.tint.description)).count > 4, true)
        for p in Pane.allCases { XCTAssertFalse(p.icon.isEmpty) }
    }

    /// Sidebar order is `Pane.allCases`, i.e. its declaration order: Home first (the default landing pane), Approvals
    /// right after it (it carries the live waiting-count badge), Settings last. A reorder here should be deliberate.
    func testSidebarOrderPutsHomeFirstApprovalsSecondAndSettingsLast() {
        XCTAssertEqual(Pane.allCases.first, .home)
        XCTAssertEqual(Pane.allCases.dropFirst().first, .approvals)
        XCTAssertEqual(Pane.allCases.last, .settings)
    }

    func testSpacingIsOnAFourPointGrid() {
        for v in [Space.xxs, Space.xs, Space.s, Space.m, Space.l, Space.xl, Space.xxl] { XCTAssertEqual(v.truncatingRemainder(dividingBy: 4), 0) }
    }

    func testHapticsGoThroughTheSwappablePerformer() {
        final class Spy: HapticPerformer { var seen: [NSHapticFeedbackManager.FeedbackPattern] = []; func perform(_ p: NSHapticFeedbackManager.FeedbackPattern) { seen.append(p) } }
        let spy = Spy(); let saved = Haptics.performer; Haptics.performer = spy; defer { Haptics.performer = saved }
        Haptics.success(); Haptics.tick()
        XCTAssertEqual(spy.seen, [.levelChange, .alignment])
    }
}

final class MotionTests: XCTestCase {
    func testStaggerGrowsThenStopsSoALongListDoesNotWait() {
        XCTAssertEqual(Motion.stagger(0), 0)
        XCTAssertEqual(Motion.stagger(3, step: 0.1), 0.3, accuracy: 1e-9)
        XCTAssertEqual(Motion.stagger(500), Motion.stagger(14), "capped")
        XCTAssertEqual(Motion.stagger(-4), 0, "never negative")
    }

    func testTheAuroraBlobsStayInsideTheFrameForever() {
        for i in 0..<4 { for t in stride(from: 0.0, through: 4000, by: 37.3) {
            let p = Motion.blob(i, at: t)
            XCTAssertTrue((0.1...0.9).contains(p.x) && (0.1...0.9).contains(p.y), "blob \(i) at \(t): \(p)")
        } }
    }

    func testSparksAreDeterministicSpreadAroundTheRingAndAllFadeOut() {
        let a = SparkField.make(seed: 7, count: 26), b = SparkField.make(seed: 7, count: 26), c = SparkField.make(seed: 8, count: 26)
        XCTAssertEqual(a, b, "the same seed gives the same burst")
        XCTAssertNotEqual(a, c)
        XCTAssertEqual(a.count, 26)
        XCTAssertGreaterThan(Set(a.map { Int(($0.angle / (2 * .pi)) * 8) % 8 }).count, 4, "spread around the ring, not clumped")
        for p in a {
            XCTAssertEqual(SparkField.state(p, t: 0).opacity, 0, "not visible before its delay ends")
            XCTAssertGreaterThan(SparkField.state(p, t: 0.5).opacity, 0.2)
            XCTAssertEqual(SparkField.state(p, t: 1.2).opacity, 0, accuracy: 1e-9, "gone by 1.2 s")
            XCTAssertTrue((3...8).contains(p.size) && p.hue < 3)
        }
        XCTAssertEqual(SparkField.make(seed: 1, count: 0), [])
    }
}

