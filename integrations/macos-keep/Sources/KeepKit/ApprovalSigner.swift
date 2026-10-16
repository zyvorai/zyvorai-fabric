import CryptoKit
import Foundation
import Security

/// The exact text an approval is signed over (`keep-approval-v1`, docs/merion/mobile/README.md). It is readable on purpose:
/// show it, or the parts a person understands, before they confirm.
public enum ApprovalPayload {
    public static func text(approvalId: String, decision: Decision, kind: String, subject: String?, sign: SignInfo) -> String {
        "keep-approval-v1\napproval: \(approvalId)\ndecision: \(decision.rawValue)\nkind: \(kind)\nsubject: \(subject ?? "")\n"
            + "action-sha256: \(sign.actionSha256)\nchallenge: \(sign.challenge)\nexpires: \(sign.expiresAt)\n"
    }

    public static func text(for approval: Approval, decision: Decision) throws -> String {
        guard let sign = approval.sign else { throw SignerError.noChallenge }
        return text(approvalId: approval.id, decision: decision, kind: approval.kind, subject: approval.subject, sign: sign)
    }
}

public enum SignerError: Error, Equatable, LocalizedError {
    case noChallenge
    case expired
    case secureEnclaveUnavailable
    case keychain(OSStatus)
    public var errorDescription: String? {
        switch self {
        case .noChallenge: return "This approval carries no signing challenge (the host is not set up for device signatures)."
        case .expired: return "The signing window for this approval has passed."
        case .secureEnclaveUnavailable: return "This device has no Secure Enclave available to the app."
        case .keychain(let s): return "Keychain error \(s)."
        }
    }
}

/// A P-256 key that can sign approvals. `publicKeyBase64` is the X.509 SubjectPublicKeyInfo the runtime expects when enrolling.
public protocol ApprovalKey: Sendable {
    var publicKeyBase64: String { get }
    func sign(_ payload: String) throws -> String  // base64 of the DER ECDSA signature over SHA-256
}

/// A key held in memory; for tests and for Macs without a Secure Enclave.
public struct SoftwareP256Key: ApprovalKey {
    private let key: P256.Signing.PrivateKey
    public init(_ key: P256.Signing.PrivateKey = P256.Signing.PrivateKey()) { self.key = key }
    public var publicKeyBase64: String { key.publicKey.derRepresentation.base64EncodedString() }
    public func sign(_ payload: String) throws -> String {
        try key.signature(for: Data(payload.utf8)).derRepresentation.base64EncodedString()
    }
}

/// A key that never leaves the Secure Enclave and needs the current fingerprints (or the login password) to sign.
public struct SecureEnclaveKey: ApprovalKey {
    private let key: SecureEnclave.P256.Signing.PrivateKey
    let reference: Data
    public var publicKeyBase64: String { key.publicKey.derRepresentation.base64EncodedString() }

    public func sign(_ payload: String) throws -> String {
        try key.signature(for: Data(payload.utf8)).derRepresentation.base64EncodedString()
    }

    /// Creates the key on first use and stores its opaque handle in the Keychain; loads it afterwards.
    public static func loadOrCreate(service: String = "dev.zyvor.keep.approval-key", account: String = "default", requireBiometry: Bool = true) throws -> SecureEnclaveKey {
        guard SecureEnclave.isAvailable else { throw SignerError.secureEnclaveUnavailable }
        let base: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: account]
        var q = base; q[kSecReturnData as String] = true; q[kSecMatchLimit as String] = kSecMatchLimitOne
        var out: CFTypeRef?
        if SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess, let data = out as? Data {
            return SecureEnclaveKey(key: try SecureEnclave.P256.Signing.PrivateKey(dataRepresentation: data), reference: data)
        }
        var flags: SecAccessControlCreateFlags = [.privateKeyUsage]
        if requireBiometry { flags.insert(.biometryCurrentSet) }
        var error: Unmanaged<CFError>?
        guard let access = SecAccessControlCreateWithFlags(nil, kSecAttrAccessibleWhenUnlockedThisDeviceOnly, flags, &error) else {
            throw SignerError.keychain(errSecParam)
        }
        let key = try SecureEnclave.P256.Signing.PrivateKey(accessControl: access)
        var add = base
        add[kSecValueData as String] = key.dataRepresentation
        add[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        let status = SecItemAdd(add as CFDictionary, nil)
        guard status == errSecSuccess else { throw SignerError.keychain(status) }
        return SecureEnclaveKey(key: key, reference: key.dataRepresentation)
    }

    public static func delete(service: String = "dev.zyvor.keep.approval-key", account: String = "default") {
        SecItemDelete([kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service, kSecAttrAccount as String: account] as CFDictionary)
    }
}

public struct ApprovalSigner: Sendable {
    public let key: ApprovalKey
    public let deviceId: String
    public init(key: ApprovalKey, deviceId: String) { self.key = key; self.deviceId = deviceId }

    /// Signs one decision. Refuses when the window has passed, so no signature is made that the host would reject.
    public func signature(for approval: Approval, decision: Decision, now: Date = Date()) throws -> (payload: String, signature: String) {
        guard let sign = approval.sign else { throw SignerError.noChallenge }
        if Int(now.timeIntervalSince1970) > sign.expiresAt { throw SignerError.expired }
        let payload = ApprovalPayload.text(approvalId: approval.id, decision: decision, kind: approval.kind, subject: approval.subject, sign: sign)
        return (payload, try key.sign(payload))
    }
}
