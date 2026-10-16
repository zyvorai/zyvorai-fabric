import Foundation

// Shapes of the Keep runtime API (agent-runtime/src/app.rs). Decoded with `.convertFromSnakeCase`; every field the app
// does not need is left out, and fields that may be null or absent are optional, so a newer runtime does not break it.

public struct KeepStatus: Codable, Equatable, Sendable {
    public struct Demos: Codable, Equatable, Sendable { public var builtin: Int; public var custom: Int }
    public struct Fluxvm: Codable, Equatable, Sendable { public var ready: Bool; public var error: String? }
    public var demoTemplate: String?
    public var demos: Demos?
    public var fluxvm: Fluxvm?
    public var keepMode: Bool?
    public var signatureRequired: Bool?
    public var trustedSigners: Int?
}

public struct Demo: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var id: String
    public var title: String
    public var description: String
    public var accepts: [String]
    public var builtin: Bool
    public var hasSample: Bool?
    public var maxBytes: Int?
    public var egress: String?
    public init(id: String, title: String, description: String, accepts: [String], builtin: Bool, hasSample: Bool? = nil, maxBytes: Int? = nil, egress: String? = nil) {
        self.id = id; self.title = title; self.description = description; self.accepts = accepts; self.builtin = builtin
        self.hasSample = hasSample; self.maxBytes = maxBytes; self.egress = egress
    }
}

struct DemosResponse: Codable { var demos: [Demo] }

public struct Badge: Codable, Equatable, Sendable {
    public var evidence: String?
    /// `false` for a run in the local simulator (`scripts/merion-demo-local.sh`): no VM, no network policy. Absent on older hosts, which are real cells.
    public var sealed: Bool?
    public var operatorCanRead: Bool?
    public var proxy: String?

    /// A simulated run must never be presented as proof of anything.
    public var isSimulated: Bool { sealed == false || evidence == "simulated" }
}

public struct ArtifactRef: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var id: String
    public var kind: String
    public var title: String
}

/// What a use-case run returns for one file.
public struct RunResult: Codable, Equatable, Sendable {
    public var demo: String
    public var sessionId: String?
    public var filename: String?
    public var bytes: Int?
    public var artifacts: [ArtifactRef]
    public var egressConnects: Int
    public var badge: Badge?
    public var honesty: String?
    public var batchId: String?
}

public struct BatchItem: Codable, Equatable, Sendable {
    public var filename: String
    public var ok: Bool
    public var result: RunResult?
    public var error: String?
}

/// A batch: several files in one request, one cell each. HTTP 207 means some files failed.
public struct BatchResult: Codable, Equatable, Sendable {
    public var batchId: String?
    public var demo: String
    public var count: Int
    public var ok: Int
    public var failed: Int
    public var egressConnects: Int
    public var results: [BatchItem]
}

public enum RunOutcome: Equatable, Sendable {
    case single(RunResult)
    case batch(BatchResult)

    /// Every file that ran, with its result, whichever way it was sent.
    public var items: [BatchItem] {
        switch self {
        case .single(let r): return [BatchItem(filename: r.filename ?? "file", ok: true, result: r, error: nil)]
        case .batch(let b): return b.results
        }
    }
    public var egressConnects: Int {
        switch self {
        case .single(let r): return r.egressConnects
        case .batch(let b): return b.egressConnects
        }
    }
    /// True when any file ran in the local simulator, which must be shown as "not sealed" instead of the proof.
    public var isSimulated: Bool { items.contains { $0.result?.badge?.isSimulated == true } }
}

public struct Artifact: Codable, Equatable, Hashable, Identifiable, Sendable {
    public struct Metadata: Codable, Equatable, Hashable, Sendable {
        public var demo: String?
        public var filename: String?
        public var batchId: String?
    }
    public var id: String
    public var title: String
    public var kind: String
    public var body: String
    public var contentType: String?
    public var agent: String?
    public var sessionId: String?
    public var createdAt: String?
    public var metadata: Metadata?

    public var createdDate: Date? { createdAt.flatMap(KeepDates.parse) }
}

struct ArtifactsResponse: Codable { var items: [Artifact] }

public struct PlannedAction: Codable, Equatable, Hashable, Sendable {
    public var method: String?
    public var bodySha256: String?
}

public struct SignInfo: Codable, Equatable, Hashable, Sendable {
    public var format: String
    public var challenge: String
    public var expiresAt: Int
    public var actionSha256: String
    public var algorithms: [String]?
}

/// What the host read out of the request an approval is about (recipients, subject, text; docs/merion/connectors/README.md). Rendered by the host,
/// not by the agent, and covered by the signature through `planned_action.preview_sha256`. Present only while the approval is pending.
public struct PreviewField: Codable, Equatable, Hashable, Sendable {
    public var label: String
    public var value: String
    public init(label: String, value: String) { self.label = label; self.value = value }
}

public struct ApprovalPreview: Codable, Equatable, Hashable, Sendable {
    public var kind: String
    public var fields: [PreviewField]
    public init(kind: String, fields: [PreviewField]) { self.kind = kind; self.fields = fields }
}

public struct Approval: Codable, Equatable, Hashable, Identifiable, Sendable {
    public var id: String
    public var kind: String
    public var subject: String?
    public var prompt: String?
    public var status: String?
    public var sessionId: String?
    public var createdAt: String?
    public var decidedAt: String?
    public var sign: SignInfo?
    public var preview: ApprovalPreview?

    public var isPending: Bool { (status ?? "pending") == "pending" }
}

struct ApprovalsResponse: Codable { var items: [Approval] }

public struct Inbox: Codable, Equatable, Sendable {
    public var userId: String?
    public var pendingApprovals: [Approval]
}

public struct UsageReport: Codable, Equatable, Sendable {
    public struct Limits: Codable, Equatable, Sendable {
        public var maxArtifacts: Int?
        public var maxModelCallsPerDay: Int?
        public var maxRunsPerDay: Int?
    }
    public struct Usage: Codable, Equatable, Sendable {
        public var artifactBytes: Int
        public var artifacts: Int
        public var modelCalls: Int
        public var runs: Int
        public var sessionSeconds: Int?
    }
    public var limits: Limits
    public var usage: Usage
}

public enum Decision: String, Sendable { case approved, denied }

public enum KeepDates {
    private static let withFraction: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter(); f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]; return f
    }()
    private static let plain = ISO8601DateFormatter()
    /// The runtime writes nanosecond precision; ISO8601DateFormatter only reads up to milliseconds, so trim.
    public static func parse(_ s: String) -> Date? {
        let trimmed = s.replacingOccurrences(of: #"(\.\d{3})\d+"#, with: "$1", options: .regularExpression)
        return withFraction.date(from: trimmed) ?? plain.date(from: trimmed)
    }
}

extension JSONDecoder {
    public static var keep: JSONDecoder {
        let d = JSONDecoder(); d.keyDecodingStrategy = .convertFromSnakeCase; return d
    }
}
