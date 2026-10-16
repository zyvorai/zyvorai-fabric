import Foundation

public struct RedactionOptions: OptionSet, Sendable {
    public let rawValue: Int
    public init(rawValue: Int) { self.rawValue = rawValue }
    public static let codes = RedactionOptions(rawValue: 1)          // one-time and verification codes
    public static let longNumbers = RedactionOptions(rawValue: 2)    // card and account numbers, long digit runs
    public static let trackingLinks = RedactionOptions(rawValue: 4)  // long query strings in links
    public static let all: RedactionOptions = [.codes, .longNumbers, .trackingLinks]
}

public struct RedactionResult: Equatable, Sendable {
    public var text: String
    public var codes = 0, numbers = 0, links = 0
    public var total: Int { codes + numbers + links }
}

/// Masks the things in a message that a summary does not need and that are risky to send anywhere: one-time codes, long digit runs (cards,
/// accounts, phone numbers) and tracking-laden links. It works on the text the person is about to upload, never on the page.
public enum Redactor {
    static let codeWords = #"(?i)(one[- ]?time|otp|verification|security|login|sign[- ]?in|passcode|pin|code)"#

    public static func redact(_ input: String, options: RedactionOptions = .all) -> RedactionResult {
        var r = RedactionResult(text: input)
        if options.contains(.codes) {
            // a code word followed, within 40 characters, by 4 to 8 digits
            let pattern = codeWords + #"([^\n\d]{0,40}?)\b(\d{4,8})\b"#
            r.text = replace(r.text, pattern) { m, s in
                r.codes += 1
                return String(s[Range(m.range(at: 1), in: s)!]) + String(s[Range(m.range(at: 2), in: s)!]) + "••••••"
            }
        }
        if options.contains(.longNumbers) {
            // 9 or more digits, allowing single spaces or dashes between groups (13-19 for cards): keep the last 4
            r.text = replace(r.text, #"\b\d(?:[ -]?\d){8,18}\b"#) { m, s in
                let digits = String(s[Range(m.range, in: s)!].filter(\.isNumber))
                r.numbers += 1
                return "••••" + digits.suffix(4)
            }
        }
        if options.contains(.trackingLinks) {
            r.text = replace(r.text, #"(https?://[^\s?<>"]+)\?[^\s<>"]{30,}"#) { m, s in
                r.links += 1
                return String(s[Range(m.range(at: 1), in: s)!]) + "?…"
            }
        }
        return r
    }

    /// Replaces every match with what `transform` returns. Matches are processed back to front so ranges stay valid.
    private static func replace(_ s: String, _ pattern: String, _ transform: (NSTextCheckingResult, String) -> String) -> String {
        guard let re = try? NSRegularExpression(pattern: pattern) else { return s }
        var out = s
        for m in re.matches(in: s, range: NSRange(s.startIndex..., in: s)).reversed() {
            guard let range = Range(m.range, in: out) else { continue }
            out.replaceSubrange(range, with: transform(m, out))
        }
        return out
    }
}
