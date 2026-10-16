import CryptoKit
import Darwin
import Foundation

/// "When a file that looks like this appears in this folder, run this use case."
public struct FolderRule: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var id: UUID
    public var folder: URL
    /// Shell-style patterns, case-insensitive, e.g. `*.pdf`, `invoice*.xlsx`.
    public var patterns: [String]
    public var demo: String
    public var saveBesideFile: Bool
    public var notify: Bool
    public var enabled: Bool

    public init(id: UUID = UUID(), folder: URL, patterns: [String], demo: String, saveBesideFile: Bool = true, notify: Bool = true, enabled: Bool = true) {
        self.id = id; self.folder = folder; self.patterns = patterns; self.demo = demo
        self.saveBesideFile = saveBesideFile; self.notify = notify; self.enabled = enabled
    }

    public func matches(filename: String) -> Bool {
        guard enabled, !filename.hasPrefix(".") else { return false }
        return patterns.contains { fnmatch($0, filename, FNM_CASEFOLD) == 0 }
    }
}

public enum ContentHash {
    /// SHA-256 of a file, streamed, as hex.
    public static func sha256(of url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }
        var hasher = SHA256()
        while let chunk = try handle.read(upToCount: 1 << 20), !chunk.isEmpty { hasher.update(data: chunk) }
        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

/// Remembers which file contents a rule already ran, so a file is summarised once even if it is touched again or renamed.
public struct SeenFiles: Codable, Equatable, Sendable {
    private var seen: Set<String> = []
    public init() {}
    private func key(_ rule: UUID, _ hash: String) -> String { "\(rule.uuidString):\(hash)" }
    public func contains(rule: UUID, hash: String) -> Bool { seen.contains(key(rule, hash)) }
    public mutating func insert(rule: UUID, hash: String) { seen.insert(key(rule, hash)) }
}
