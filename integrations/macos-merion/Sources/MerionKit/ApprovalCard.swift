import Foundation

/// One enrolled device, as `GET /v1/users/{id}/devices` lists it (a user may read their own).
public struct DeviceInfo: Codable, Equatable, Sendable {
    public var deviceId: String
    public var name: String?
    public var alg: String?
    public var publicKey: String
    public init(deviceId: String, name: String? = nil, alg: String? = nil, publicKey: String) { self.deviceId = deviceId; self.name = name; self.alg = alg; self.publicKey = publicKey }
}

struct DevicesResponse: Codable { var items: [DeviceInfo] }

extension MerionClient {
    public func devices(userId: String) async throws -> [DeviceInfo] {
        (try await getJSON("/v1/users/\(pathEscape(userId))/devices") as DevicesResponse).items
    }
}

/// Whether this Mac's key is one the host knows. Decided by comparing the public key, never by a name the person typed.
public enum DeviceState: Equatable, Sendable {
    case unknown                       // not asked yet, or the host could not say
    case noKey                         // this Mac has no Secure Enclave key available
    case notEnrolled
    case enrolled(deviceId: String)

    public static func resolve(devices: [DeviceInfo], publicKeyBase64: String) -> DeviceState {
        devices.first { $0.publicKey == publicKeyBase64 }.map { .enrolled(deviceId: $0.deviceId) } ?? .notEnrolled
    }
    public var deviceId: String? { if case .enrolled(let id) = self { return id } else { return nil } }
}

/// Can this decision be made from here, and how?
public enum ApprovalReadiness: Equatable, Sendable {
    case ready(signed: Bool)
    case expired
    /// The host issued a signing challenge, so an unsigned decision would be refused; this Mac has to be enrolled first.
    case needsEnrolment
    case noKey

    public var canDecide: Bool { if case .ready = self { return true } else { return false } }
}

public enum ApprovalGate {
    /// A host that issues a challenge (`sign` present) expects a signature. We refuse to send an unsigned decision in that case
    /// rather than let the host answer with an error after the person has touched the sensor.
    public static func readiness(for a: Approval, device: DeviceState, now: Date = Date()) -> ApprovalReadiness {
        guard let sign = a.sign else { return .ready(signed: false) }
        if Int(now.timeIntervalSince1970) > sign.expiresAt { return .expired }
        switch device {
        case .enrolled: return .ready(signed: true)
        case .noKey: return .noKey
        case .notEnrolled, .unknown: return .needsEnrolment
        }
    }
}

/// An approval as a person reads it: what will happen, in the host's words.
public struct ApprovalCard: Equatable, Sendable {
    public var title: String
    public var symbol: String
    /// The fields the host rendered from the exact request (recipients, subject, text; event and guests). Empty when the host rendered none.
    public var fields: [PreviewField]
    public var hasPreview: Bool { !fields.isEmpty }
    public var expiresAt: Date?

    public init(_ a: Approval) {
        (title, symbol) = Self.describe(previewKind: a.preview?.kind, approvalKind: a.kind)
        fields = a.preview?.fields ?? []
        expiresAt = a.sign.map { Date(timeIntervalSince1970: TimeInterval($0.expiresAt)) }
    }

    static func describe(previewKind: String?, approvalKind: String) -> (String, String) {
        switch previewKind {
        case "gmail-message", "graph-message": return ("Send an email", "envelope.fill")
        case "calendar-event", "graph-event": return ("Add a calendar event", "calendar.badge.plus")
        default: break
        }
        switch approvalKind {
        case "egress": return ("Allow a network request", "network")
        default: return (approvalKind.replacingOccurrences(of: "_", with: " ").replacingOccurrences(of: ".", with: " ").capitalized, "checkmark.seal.fill")
        }
    }

    /// Whole seconds left in the signing window; nil when there is none, 0 once it has passed.
    public func secondsLeft(now: Date = Date()) -> Int? { expiresAt.map { max(0, Int($0.timeIntervalSince(now).rounded(.up))) } }

    /// "4:32", "0:09", or "expired".
    public static func countdown(_ seconds: Int) -> String { seconds <= 0 ? "expired" : String(format: "%d:%02d", seconds / 60, seconds % 60) }
}

/// What an operator needs to enrol this Mac: the person's own user id, a device id, and the public key. The private key never leaves the Secure Enclave.
public struct EnrolmentDetails: Equatable, Sendable {
    public var userId: String
    public var deviceId: String
    public var publicKeyBase64: String
    public var name: String

    public init(userId: String, deviceName: String, publicKeyBase64: String) {
        self.userId = userId; self.name = deviceName; self.publicKeyBase64 = publicKeyBase64
        let slug = String(deviceName.lowercased().map { $0.isLetter || $0.isNumber ? $0 : "-" }).split(separator: "-").joined(separator: "-")
        deviceId = slug.isEmpty ? "mac" : slug
    }

    /// The request body the operator sends, and the route, as plain text a person can paste into a message.
    public var text: String {
        "Please enrol my Mac as an approval device.\n\nPOST /v1/users/\(userId)/devices   (operator token)\n"
            + "{\"device_id\": \"\(deviceId)\", \"alg\": \"p256\", \"name\": \"\(name)\", \"public_key\": \"\(publicKeyBase64)\"}\n"
    }
}
