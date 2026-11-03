import AppKit
import SwiftUI

/// A Markdown renderer for Keep's summaries, built to read like a designed document rather than a text blob: headings with rhythm, real lists (nested
/// and numbered), quotes, code with a language label and a copy button, rules, and tables with alignment, zebra rows and right-aligned numbers.
/// `AttributedString` reads inline Markdown but not blocks or tables, so the blocks are parsed here (`parse`, unit-tested).
struct MarkdownView: View {
    let markdown: String

    enum Align: Equatable { case leading, center, trailing }

    enum Block: Equatable {
        case heading(Int, String)
        /// `marker` is "•" for a bullet or "1." for a numbered item; `indent` is the nesting level (0 is the top).
        case bullet(text: String, indent: Int, marker: String)
        case paragraph(String)
        case quote(String)
        case code(language: String?, text: String)
        case rule
        case table(header: [String], align: [Align], rows: [[String]])
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.xs) {
            ForEach(Array(Self.parse(markdown).enumerated()), id: \.offset) { _, block in blockView(block) }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    @ViewBuilder private func blockView(_ block: Block) -> some View {
        switch block {
        case .heading(let level, let text):
            Text(inline(text))
                .font(level <= 1 ? Typo.title : level == 2 ? Typo.heading : .subheadline.weight(.semibold))
                .foregroundStyle(level >= 3 ? .secondary : .primary)
                .padding(.top, level <= 2 ? Space.s : Space.xxs)
        case .bullet(let text, let indent, let marker):
            HStack(alignment: .firstTextBaseline, spacing: Space.xs) {
                Text(marker).foregroundStyle(marker == "•" ? Brand.orange : .secondary).frame(minWidth: 14, alignment: .trailing)
                Text(inline(text)).textSelection(.enabled)
            }
            .padding(.leading, CGFloat(indent) * Space.l)
        case .paragraph(let text):
            Text(inline(text)).lineSpacing(3).textSelection(.enabled)
        case .quote(let text):
            HStack(spacing: Space.s) {
                RoundedRectangle(cornerRadius: 2).fill(Brand.orange.opacity(0.7)).frame(width: 3)
                Text(inline(text)).foregroundStyle(.secondary).textSelection(.enabled)
            }.fixedSize(horizontal: false, vertical: true)
        case .code(let language, let text):
            CodeBlock(language: language, text: text)
        case .rule:
            Divider().padding(.vertical, Space.xs)
        case .table(let header, let align, let rows):
            TableBlock(header: header, align: align, rows: rows)
        }
    }

    private func inline(_ s: String) -> AttributedString {
        (try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(s)
    }

    // MARK: parsing

    /// Whether a table cell reads as a number: `1,234`, `-3.5`, `12%`, `€50,000`, `4 KB`, `3×`. Such columns are right-aligned so digits line up.
    static func isNumeric(_ cell: String) -> Bool {
        var t = cell.trimmingCharacters(in: .whitespaces)
        for wrap in ["**", "`", "_"] where t.hasPrefix(wrap) && t.hasSuffix(wrap) && t.count > 2 * wrap.count { t = String(t.dropFirst(wrap.count).dropLast(wrap.count)) }
        return t.range(of: #"^[~≈<>]?\s?[-+−]?\s?[$€£¥₹]?\s?\d[\d,._ ]*(\s?(%|×|x|[A-Za-z]{1,5}|[$€£¥₹]))?$"#, options: .regularExpression) != nil
    }

    static func parse(_ text: String) -> [Block] {
        var blocks: [Block] = []
        var lines = text.replacingOccurrences(of: "\r\n", with: "\n").components(separatedBy: "\n")[...]
        var paragraph: [String] = []
        func flush() { if !paragraph.isEmpty { blocks.append(.paragraph(paragraph.joined(separator: " "))); paragraph = [] } }
        func cells(_ line: String) -> [String] {
            var t = line.trimmingCharacters(in: .whitespaces)
            if t.hasPrefix("|") { t.removeFirst() }
            if t.hasSuffix("|") { t.removeLast() }
            return t.split(separator: "|", omittingEmptySubsequences: false).map { $0.trimmingCharacters(in: .whitespaces) }
        }
        func alignment(_ cell: String) -> Align {
            let c = cell.trimmingCharacters(in: .whitespaces)
            switch (c.hasPrefix(":"), c.hasSuffix(":")) {
            case (true, true): return .center
            case (false, true): return .trailing
            default: return .leading
            }
        }
        func isSeparator(_ line: String) -> Bool {
            let cs = cells(line)
            return !cs.isEmpty && cs.allSatisfy { $0.range(of: #"^:?-{1,}:?$"#, options: .regularExpression) != nil }
        }
        while let line = lines.popFirst() {
            let t = line.trimmingCharacters(in: .whitespaces)
            if t.isEmpty { flush(); continue }
            if t.hasPrefix("```") {
                flush()
                let lang = String(t.dropFirst(3)).trimmingCharacters(in: .whitespaces)
                var code: [String] = []
                while let l = lines.popFirst(), !l.trimmingCharacters(in: .whitespaces).hasPrefix("```") { code.append(l) }
                blocks.append(.code(language: lang.isEmpty ? nil : lang, text: code.joined(separator: "\n"))); continue
            }
            if t.range(of: #"^(-{3,}|\*{3,}|_{3,})$"#, options: .regularExpression) != nil { flush(); blocks.append(.rule); continue }
            if let m = t.range(of: #"^#{1,4} "#, options: .regularExpression) {
                flush(); blocks.append(.heading(t[m].count - 1, String(t[m.upperBound...]))); continue
            }
            if t.hasPrefix(">") {
                flush()
                var quote = [String(t.dropFirst().drop(while: { $0 == " " }))]
                while let n = lines.first?.trimmingCharacters(in: .whitespaces), n.hasPrefix(">") { quote.append(String(n.dropFirst().drop(while: { $0 == " " }))); _ = lines.popFirst() }
                blocks.append(.quote(quote.joined(separator: " "))); continue
            }
            if t.hasPrefix("|"), let next = lines.first, next.trimmingCharacters(in: .whitespaces).hasPrefix("|"), isSeparator(next) {
                flush()
                let sep = cells(lines.popFirst()!)
                var rows: [[String]] = []
                while let r = lines.first, r.trimmingCharacters(in: .whitespaces).hasPrefix("|") { rows.append(cells(r)); _ = lines.popFirst() }
                let header = cells(t)
                var align = (0..<header.count).map { $0 < sep.count ? alignment(sep[$0]) : .leading }
                // a column with no explicit alignment whose every value is a number reads better right-aligned
                for i in align.indices where align[i] == .leading && !(sep.indices.contains(i) && sep[i].hasPrefix(":")) {
                    let col = rows.compactMap { $0.indices.contains(i) ? $0[i] : nil }.filter { !$0.isEmpty }
                    if !col.isEmpty && col.allSatisfy(isNumeric) { align[i] = .trailing }
                }
                blocks.append(.table(header: header, align: align, rows: rows)); continue
            }
            if let m = line.range(of: #"^(\s*)([-*+]|\d+[.)])\s+"#, options: .regularExpression) {
                flush()
                let head = line[m]
                let indent = head.prefix(while: { $0 == " " || $0 == "\t" }).reduce(0) { $0 + ($1 == "\t" ? 2 : 1) } / 2
                let token = head.trimmingCharacters(in: .whitespaces)
                let marker = token.first.map { $0.isNumber } == true ? token : "•"
                blocks.append(.bullet(text: String(line[m.upperBound...]).trimmingCharacters(in: .whitespaces), indent: indent, marker: marker)); continue
            }
            paragraph.append(t)
        }
        flush()
        return blocks
    }
}

private struct CodeBlock: View {
    let language: String?
    let text: String
    @State private var copied = false
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(language ?? "code").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                Spacer()
                Button { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(text, forType: .string); copied = true; Task { try? await Task.sleep(nanoseconds: 1_500_000_000); copied = false } } label: {
                    Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc").font(.caption)
                }.buttonStyle(.borderless).help("Copy the code")
            }
            .padding(.horizontal, Space.s).padding(.vertical, Space.xs)
            Divider()
            Text(text).font(Typo.mono).textSelection(.enabled).padding(Space.s).frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(.quaternary.opacity(0.5), in: RoundedRectangle(cornerRadius: Radius.chip, style: .continuous))
    }
}

private struct TableBlock: View {
    let header: [String]
    let align: [MarkdownView.Align]
    let rows: [[String]]

    private func cellAlignment(_ i: Int) -> Alignment {
        switch align.indices.contains(i) ? align[i] : .leading { case .leading: return .leading; case .center: return .center; case .trailing: return .trailing }
    }
    private func text(_ s: String) -> AttributedString {
        (try? AttributedString(markdown: s, options: .init(interpretedSyntax: .inlineOnlyPreservingWhitespace))) ?? AttributedString(s)
    }

    var body: some View {
        Grid(alignment: .leading, horizontalSpacing: 0, verticalSpacing: 0) {
            GridRow {
                ForEach(Array(header.enumerated()), id: \.offset) { i, h in
                    Text(text(h)).font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: cellAlignment(i)).padding(.horizontal, Space.s).padding(.vertical, Space.xs)
                }
            }
            Divider().gridCellColumns(max(header.count, 1))
            ForEach(Array(rows.enumerated()), id: \.offset) { r, row in
                GridRow {
                    ForEach(Array(header.indices), id: \.self) { i in
                        Text(text(row.indices.contains(i) ? row[i] : "")).monospacedDigit().textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: cellAlignment(i)).padding(.horizontal, Space.s).padding(.vertical, Space.xs)
                    }
                }
                .background(r % 2 == 1 ? Color.primary.opacity(0.04) : Color.clear)
            }
        }
        .background(.quaternary.opacity(0.35), in: RoundedRectangle(cornerRadius: Radius.chip, style: .continuous))
        .clipShape(RoundedRectangle(cornerRadius: Radius.chip, style: .continuous))
    }
}
