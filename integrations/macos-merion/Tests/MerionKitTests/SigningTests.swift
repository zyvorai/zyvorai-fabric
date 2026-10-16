import CryptoKit
import XCTest
@testable import MerionKit

/// The signed text and the signatures are checked against docs/merion/mobile/test-vectors.json, which the runtime's own
/// test suite generates and re-verifies, so this app and the runtime cannot drift apart.
final class SigningTests: XCTestCase {
    struct Vectors: Decodable {
        struct Approval: Decodable { var id: String; var kind: String; var subject: String? }
        struct Sign: Decodable { var actionSha256: String; var challenge: String; var expiresAt: Int; var format: String }
        struct Case: Decodable { var alg: String; var decision: String; var payload: String; var publicKey: String; var signature: String }
        var approval: Approval; var sign: Sign; var cases: [Case]
    }
    func vectors() throws -> Vectors { try Fixture.decode(Vectors.self, "test-vectors.json") }

    func testThePayloadMatchesEveryVector() throws {
        let v = try vectors()
        for c in v.cases {
            let sign = SignInfo(format: v.sign.format, challenge: v.sign.challenge, expiresAt: v.sign.expiresAt, actionSha256: v.sign.actionSha256, algorithms: nil)
            let text = ApprovalPayload.text(approvalId: v.approval.id, decision: try XCTUnwrap(Decision(rawValue: c.decision)), kind: v.approval.kind, subject: v.approval.subject, sign: sign)
            XCTAssertEqual(text, c.payload, "\(c.alg) \(c.decision)")
        }
    }

    func testTheRuntimeSignaturesVerifyWithCryptoKit() throws {
        for c in try vectors().cases {
            let payload = Data(c.payload.utf8)
            let sig = try XCTUnwrap(Data(base64Encoded: c.signature)), pub = try XCTUnwrap(Data(base64Encoded: c.publicKey))
            switch c.alg {
            case "p256":
                let key = try P256.Signing.PublicKey(derRepresentation: pub)
                XCTAssertTrue(key.isValidSignature(try P256.Signing.ECDSASignature(derRepresentation: sig), for: payload), "p256 \(c.decision)")
            case "ed25519":
                let key = try Curve25519.Signing.PublicKey(rawRepresentation: pub)
                XCTAssertTrue(key.isValidSignature(sig, for: payload), "ed25519 \(c.decision)")
            default: XCTFail("unknown algorithm \(c.alg)")
            }
        }
    }

    func testAnApprovalFromTheInboxSignsAndVerifies() throws {
        let approval = try XCTUnwrap(Fixture.decode(Inbox.self, "inbox.json").pendingApprovals.first)
        let key = SoftwareP256Key()
        let signer = ApprovalSigner(key: key, deviceId: "test-mac")
        let now = Date(timeIntervalSince1970: 1_790_000_000)
        let (payload, signature) = try signer.signature(for: approval, decision: .approved, now: now)
        XCTAssertTrue(payload.hasPrefix("keep-approval-v1\napproval: 11111111"))
        XCTAssertTrue(payload.contains("decision: approved\n"))
        let pub = try P256.Signing.PublicKey(derRepresentation: XCTUnwrap(Data(base64Encoded: key.publicKeyBase64)))
        let sig = try P256.Signing.ECDSASignature(derRepresentation: XCTUnwrap(Data(base64Encoded: signature)))
        XCTAssertTrue(pub.isValidSignature(sig, for: Data(payload.utf8)))
        // A signature for one decision does not verify for the other.
        let (denied, _) = try signer.signature(for: approval, decision: .denied, now: now)
        XCTAssertFalse(pub.isValidSignature(sig, for: Data(denied.utf8)))
    }

    func testNothingIsSignedAfterTheWindowOrWithoutAChallenge() throws {
        let approval = try XCTUnwrap(Fixture.decode(Inbox.self, "inbox.json").pendingApprovals.first)
        let signer = ApprovalSigner(key: SoftwareP256Key(), deviceId: "d")
        XCTAssertThrowsError(try signer.signature(for: approval, decision: .approved, now: Date(timeIntervalSince1970: 1_790_003_601))) {
            XCTAssertEqual($0 as? SignerError, .expired)
        }
        var bare = approval; bare.sign = nil
        XCTAssertThrowsError(try signer.signature(for: bare, decision: .approved)) { XCTAssertEqual($0 as? SignerError, .noChallenge) }
    }

    func testTheSecureEnclaveKeyIsUsedWhenTheSystemAllowsIt() throws {
        // A test bundle has no keychain entitlement, so creating an Enclave key can be refused; that is not a failure here.
        do {
            let key = try SecureEnclaveKey.loadOrCreate(service: "dev.zyvor.merion.test-\(UUID().uuidString)", requireBiometry: false)
            let sig = try key.sign("keep-approval-v1\n")
            XCTAssertFalse(sig.isEmpty); XCTAssertFalse(key.publicKeyBase64.isEmpty)
        } catch { throw XCTSkip("Secure Enclave key not available to this test process: \(error)") }
    }
}
