import Foundation

public struct SecretFinding: Equatable, Hashable, Sendable {
    public let kind: String
    /// Where it was seen, never the value itself.
    public let where_: String
}

/// A cheap check before a file is uploaded: warns on private keys, cloud and token strings and secret-looking file names.
/// It is a safety net, not a scanner: it reads the first 512 KB of text files and never returns or logs a secret.
public enum SecretScan {
    static let namePatterns: [(String, String)] = [
        (#"^\.env(\..+)?$"#, "an environment file"), (#"^id_(rsa|dsa|ecdsa|ed25519)$"#, "an SSH private key"),
        (#"\.(pem|p12|pfx|key|keystore|jks)$"#, "a key or certificate store"), (#"^\.(npmrc|netrc|pypirc)$"#, "a credentials file"),
        (#"^(credentials|secrets?)(\.(json|ya?ml|txt))?$"#, "a credentials file"),
    ]
    static let contentPatterns: [(String, String)] = [
        (#"-----BEGIN ([A-Z]+ )?PRIVATE KEY-----"#, "a private key"), (#"\bAKIA[0-9A-Z]{16}\b"#, "an AWS access key id"),
        (#"\bgh[pousr]_[A-Za-z0-9]{36,}\b"#, "a GitHub token"), (#"\bxox[baprs]-[A-Za-z0-9-]{10,}\b"#, "a Slack token"),
        (#"\bsk-[A-Za-z0-9]{32,}\b"#, "an API key"),
        (#"(?i)\b(api[_-]?key|secret|passw(or)?d|token)\b\s*[:=]\s*['"]?[A-Za-z0-9/+_\-]{16,}"#, "a key or password assignment"),
    ]

    public static func scan(fileURL: URL, maxBytes: Int = 512 * 1024) -> [SecretFinding] {
        var found: [SecretFinding] = []
        let name = fileURL.lastPathComponent
        for (pattern, kind) in namePatterns where name.range(of: pattern, options: [.regularExpression, .caseInsensitive]) != nil {
            found.append(SecretFinding(kind: kind, where_: "file name"))
        }
        guard let handle = try? FileHandle(forReadingFrom: fileURL), let data = try? handle.read(upToCount: maxBytes) else { return found }
        try? handle.close()
        if data.contains(0) { return found }  // binary: names only
        return found + scan(text: String(decoding: data, as: UTF8.self))
    }

    /// The same content checks on text that is not in a file yet (for example an email read from a browser).
    public static func scan(text: String) -> [SecretFinding] {
        contentPatterns.compactMap { pattern, kind in
            text.range(of: pattern, options: .regularExpression) != nil ? SecretFinding(kind: kind, where_: "contents") : nil
        }
    }
}
