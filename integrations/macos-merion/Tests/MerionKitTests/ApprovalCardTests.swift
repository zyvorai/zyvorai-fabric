import XCTest
@testable import MerionKit

final class ApprovalCardTests: XCTestCase {
    private func approval(kind: String = "egress", previewKind: String? = nil, expires: Int? = nil) -> Approval {
        let sign = expires.map { #","sign":{"format":"merion-approval-v1","challenge":"c","expires_at":\#($0),"action_sha256":"a"}"# } ?? ""
        let preview = previewKind.map { #","preview":{"kind":"\#($0)","fields":[{"label":"To","value":"ana@example.com"},{"label":"Subject","value":"Lunch?"}]}"# } ?? ""
        return try! JSONDecoder.keep.decode(Approval.self, from: Data(#"{"id":"a1","kind":"\#(kind)","status":"pending"\#(sign)\#(preview)}"#.utf8))
    }
    private let now = Date(timeIntervalSince1970: 1_000_000)

    func testTheCardSaysWhatWillHappenInTheHostsWords() {
        let mail = ApprovalCard(approval(previewKind: "gmail-message"))
        XCTAssertEqual(mail.title, "Send an email"); XCTAssertEqual(mail.symbol, "envelope.fill")
        XCTAssertEqual(mail.fields.map(\.label), ["To", "Subject"]); XCTAssertTrue(mail.hasPreview)
        XCTAssertEqual(ApprovalCard(approval(previewKind: "graph-message")).title, "Send an email")
        XCTAssertEqual(ApprovalCard(approval(previewKind: "graph-event")).title, "Add a calendar event")
        XCTAssertEqual(ApprovalCard(approval(previewKind: "calendar-event")).symbol, "calendar.badge.plus")
        let plain = ApprovalCard(approval(kind: "egress"))
        XCTAssertEqual(plain.title, "Allow a network request"); XCTAssertFalse(plain.hasPreview)
        XCTAssertEqual(ApprovalCard(approval(kind: "tool.use_thing")).title, "Tool Use Thing")
    }

    func testTheCountdownFollowsTheSigningWindow() {
        let card = ApprovalCard(approval(expires: 1_000_272))
        XCTAssertEqual(card.secondsLeft(now: now), 272)
        XCTAssertEqual(ApprovalCard.countdown(272), "4:32"); XCTAssertEqual(ApprovalCard.countdown(9), "0:09")
        XCTAssertEqual(card.secondsLeft(now: Date(timeIntervalSince1970: 1_000_500)), 0)
        XCTAssertEqual(ApprovalCard.countdown(0), "expired")
        XCTAssertNil(ApprovalCard(approval()).secondsLeft(now: now), "no signing window, no countdown")
    }

    func testAnUnsignedDecisionIsRefusedWhenTheHostIssuedAChallenge() {
        let signed = approval(expires: 1_000_100)
        XCTAssertEqual(ApprovalGate.readiness(for: signed, device: .enrolled(deviceId: "mac"), now: now), .ready(signed: true))
        XCTAssertEqual(ApprovalGate.readiness(for: signed, device: .notEnrolled, now: now), .needsEnrolment)
        XCTAssertEqual(ApprovalGate.readiness(for: signed, device: .unknown, now: now), .needsEnrolment, "not knowing is not a licence to send unsigned")
        XCTAssertEqual(ApprovalGate.readiness(for: signed, device: .noKey, now: now), .noKey)
        XCTAssertEqual(ApprovalGate.readiness(for: signed, device: .enrolled(deviceId: "mac"), now: Date(timeIntervalSince1970: 1_000_101)), .expired)
        XCTAssertEqual(ApprovalGate.readiness(for: approval(), device: .notEnrolled, now: now), .ready(signed: false), "a host that issues no challenge does not need one")
        XCTAssertFalse(ApprovalReadiness.expired.canDecide); XCTAssertTrue(ApprovalReadiness.ready(signed: true).canDecide)
    }

    func testEnrolmentIsDecidedByThePublicKeyNotByAName() {
        let devices = [DeviceInfo(deviceId: "ana-phone", publicKey: "PHONEKEY"), DeviceInfo(deviceId: "work-mac", name: "Work", publicKey: "MACKEY")]
        XCTAssertEqual(DeviceState.resolve(devices: devices, publicKeyBase64: "MACKEY"), .enrolled(deviceId: "work-mac"))
        XCTAssertEqual(DeviceState.resolve(devices: devices, publicKeyBase64: "OTHER"), .notEnrolled)
        XCTAssertEqual(DeviceState.resolve(devices: [], publicKeyBase64: "MACKEY"), .notEnrolled)
        XCTAssertEqual(DeviceState.enrolled(deviceId: "x").deviceId, "x"); XCTAssertNil(DeviceState.notEnrolled.deviceId)
    }

    func testEnrolmentDetailsCarryOnlyPublicThings() {
        let d = EnrolmentDetails(userId: "ana", deviceName: "Ana's MacBook Pro", publicKeyBase64: "PUB")
        XCTAssertEqual(d.deviceId, "ana-s-macbook-pro")
        XCTAssertTrue(d.text.contains("/v1/users/ana/devices")); XCTAssertTrue(d.text.contains("\"public_key\": \"PUB\""))
        XCTAssertTrue(d.text.contains("\"alg\": \"p256\""))
        XCTAssertEqual(EnrolmentDetails(userId: "a", deviceName: "***", publicKeyBase64: "k").deviceId, "mac")
    }

    func testDevicesListDecodes() async throws {
        let session = StubProtocol.session()
        StubProtocol.handler = { req, _ in
            XCTAssertEqual(req.url?.path, "/v1/users/ana/devices")
            return (200, Data(#"{"items":[{"user_id":"ana","device_id":"m","alg":"p256","public_key":"K","created_at":"2026-01-01T00:00:00Z"}]}"#.utf8))
        }
        let c = try MerionClient(baseURL: URL(string: "http://h:1")!, token: "t", session: session)
        let list = try await c.devices(userId: "ana")
        XCTAssertEqual(list, [DeviceInfo(deviceId: "m", alg: "p256", publicKey: "K")])
    }
}
