import Foundation

public enum CatalogGroup: String, CaseIterable, Codable, Sendable {
    case documents, phone, mac, windows, developer, browser, office, other

    public var title: String {
        switch self {
        case .documents: return "Documents"
        case .phone: return "Phone"
        case .mac: return "Mac"
        case .windows: return "Windows"
        case .developer: return "Developer"
        case .browser: return "Browser"
        case .office: return "Office"
        case .other: return "Other"
        }
    }
}

/// What the app knows about a use case beyond what the host lists: its group, and the command a person runs to get the file.
/// Nothing here is ever executed by the app.
public struct CatalogEntry: Equatable, Hashable, Sendable {
    public let id: String
    public let group: CatalogGroup
    public let howToGetFile: String?
}

public enum Catalog {
    public static let entries: [CatalogEntry] = generated
    private static let byId: [String: CatalogEntry] = Dictionary(uniqueKeysWithValues: generated.map { ($0.id, $0) })

    public static func entry(for id: String) -> CatalogEntry? { byId[id] }
    public static func group(for id: String) -> CatalogGroup { byId[id]?.group ?? .other }

    /// Use cases that read this file type, best guess first (the extension decides; the name breaks ties).
    public static func suggestions(forFileNamed name: String, among demos: [Demo]) -> [Demo] {
        let ext = (name as NSString).pathExtension.lowercased()
        guard !ext.isEmpty else { return [] }
        let lower = name.lowercased()
        return demos.filter { $0.accepts.map { $0.lowercased() }.contains(ext) }
            .sorted { a, b in
                let sa = lower.contains(a.id.split(separator: "-").first.map(String.init) ?? a.id) ? 0 : 1
                let sb = lower.contains(b.id.split(separator: "-").first.map(String.init) ?? b.id) ? 0 : 1
                return sa == sb ? a.id < b.id : sa < sb
            }
    }
}
