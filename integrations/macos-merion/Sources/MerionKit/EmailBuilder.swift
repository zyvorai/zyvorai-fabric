import Foundation

/// What was read from a browser tab.
public struct CapturedPage: Equatable, Sendable {
    public var url: URL?
    public var title: String
    public var text: String
    /// True when the text is what the person had selected on the page, not the whole page.
    public var isSelection: Bool
    public init(url: URL?, title: String, text: String, isSelection: Bool) { self.url = url; self.title = title; self.text = text; self.isSelection = isSelection }
    public var host: String? { url?.host }
}

/// Turns text read from a webmail page into an RFC 822 message the `eml` extractor reads. The page is untrusted, so every value that goes
/// into a header is stripped of line breaks (no header injection), and body lines that would look like an mbox separator are escaped.
public enum EmailBuilder {
    public static let maxBody = 200_000

    public static func eml(subject: String, from: String? = nil, date: Date = Date(), body: String, sourceHost: String? = nil, messageId: String = UUID().uuidString) -> String {
        let headers = [
            "From: \(header(from ?? "Webmail <webmail@solvor.invalid>"))",
            "Subject: \(header(subject.isEmpty ? "(no subject)" : subject))",
            "Date: \(rfc2822(date))",
            "Message-ID: <\(header(messageId))@solvor.local>",
            "X-Solvor-Source: \(header(sourceHost ?? "browser"))",
            "MIME-Version: 1.0",
            "Content-Type: text/plain; charset=utf-8",
        ]
        return headers.joined(separator: "\n") + "\n\n" + safeBody(body) + "\n"
    }

    public static func eml(from page: CapturedPage, body: String? = nil) -> String {
        eml(subject: page.title, body: body ?? page.text, sourceHost: page.host)
    }

    /// One line, no control characters, bounded.
    static func header(_ s: String) -> String {
        let scalars = s.unicodeScalars.map { CharacterSet.controlCharacters.contains($0) || $0 == "\u{2028}" || $0 == "\u{2029}" ? " " : Character($0) }
        let line = String(scalars).replacingOccurrences(of: #"\s+"#, with: " ", options: .regularExpression).trimmingCharacters(in: .whitespaces)
        return String(line.prefix(200))
    }

    static func safeBody(_ s: String) -> String {
        let normalised = s.replacingOccurrences(of: "\r\n", with: "\n").replacingOccurrences(of: "\r", with: "\n")
        return String(normalised.prefix(maxBody))
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map { $0.hasPrefix("From ") ? ">" + $0 : String($0) }
            .joined(separator: "\n")
    }

    static func rfc2822(_ d: Date) -> String {
        let f = DateFormatter(); f.locale = Locale(identifier: "en_US_POSIX"); f.dateFormat = "EEE, dd MMM yyyy HH:mm:ss Z"
        return f.string(from: d)
    }
}
