import CryptoKit
import MerionKit
import UserNotifications
import XCTest
@testable import Solvor

private final class MemoryTokens: TokenStore, @unchecked Sendable {
    var value: String?
    func token() throws -> String? { value }
    func save(_ token: String) throws { value = token }
    func clear() throws { value = nil }
}

/// A host that answers from properties, so the state machine can be tested without a network.
private final class StubHost: KeepAPI, @unchecked Sendable {
    var whoamiResult: Result<WhoAmI, Error> = .success(WhoAmI(role: "user", userId: "ana", scopes: ["read"]))
    var statusResult: Result<MerionStatus, Error> = .failure(MerionError.http(status: 403, message: "this route is not available to user tokens"))
    var demosResult: Result<[Demo], Error> = .success([])
    var pending: [Approval] = []
    var deviceList: [DeviceInfo] = []
    var decisions: [(id: String, decision: Decision, deviceId: String?, signature: String?)] = []

    func whoami() async throws -> WhoAmI { try whoamiResult.get() }
    func status() async throws -> MerionStatus { try statusResult.get() }
    func demos() async throws -> [Demo] { try demosResult.get() }
    func artifacts(limit: Int?) async throws -> [Artifact] { [] }
    func artifact(id: String) async throws -> Artifact { throw MerionError.http(status: 404, message: "none") }
    func inbox(userId: String?) async throws -> Inbox {
        let ids = pending.map { #"{"id":"\#($0.id)","kind":"\#($0.kind)","status":"pending"}"# }.joined(separator: ",")
        return try JSONDecoder.keep.decode(Inbox.self, from: Data(#"{"user_id":"\#(userId ?? "")","pending_approvals":[\#(ids)]}"#.utf8))
    }
    func approvals() async throws -> [Approval] { pending }
    func usage(userId: String?) async throws -> UsageReport { throw MerionError.http(status: 404, message: "none") }
    func devices(userId: String) async throws -> [DeviceInfo] { deviceList }
    func decide(approval id: String, _ decision: Decision, deviceId: String?, signature: String?) async throws {
        decisions.append((id, decision, deviceId, signature)); pending.removeAll { $0.id == id }
    }
    var runDelaysUntilCancelled = false
    func run(demo: String, files: [URL], maxBytes: Int?) async throws -> RunOutcome {
        if runDelaysUntilCancelled {
            while !Task.isCancelled { try? await Task.sleep(nanoseconds: 5_000_000) }
            throw CancellationError()
        }
        throw MerionError.http(status: 500, message: "not in this test")
    }
}

private final class Counter: @unchecked Sendable {
    private let lock = NSLock(); private var n = 0
    func bump() { lock.lock(); n += 1; lock.unlock() }
    var value: Int { lock.lock(); defer { lock.unlock() }; return n }
}

private func approval(_ id: String) -> Approval {
    try! JSONDecoder.keep.decode(Approval.self, from: Data(#"{"id":"\#(id)","kind":"gmail.send","subject":"Hi","status":"pending"}"#.utf8))
}
private func signed(_ id: String, expiresIn seconds: Int = 300) -> Approval {
    let exp = Int(Date().timeIntervalSince1970) + seconds
    return try! JSONDecoder.keep.decode(Approval.self, from: Data(#"{"id":"\#(id)","kind":"gmail.send","subject":"Hi","status":"pending","sign":{"format":"keep-approval-v1","challenge":"ch","expires_at":\#(exp),"action_sha256":"ab"}}"#.utf8))
}
private func tempFile(_ name: String) -> URL {
    let dir = FileManager.default.temporaryDirectory.appendingPathComponent("solvor-test-\(UUID().uuidString)")
    try! FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    let url = dir.appendingPathComponent(name)
    try! Data("x".utf8).write(to: url)
    return url
}
private func demo(_ id: String) -> Demo {
    try! JSONDecoder.keep.decode(Demo.self, from: Data(#"{"id":"\#(id)","title":"T","description":"d","accepts":[],"builtin":true}"#.utf8))
}

@MainActor
final class AppStateTests: XCTestCase {
    private var host = StubHost()
    private var tokens = MemoryTokens()
    private var built = 0
    private var told: [String] = []
    private var keyReads = Counter()

    private func makeApp(host h: String = "http://127.0.0.1:9096", token: String? = "kut1abc") -> AppState {
        host = StubHost(); tokens = MemoryTokens(); tokens.value = token; built = 0; told = []; keyReads = Counter()
        let suite = "solvor-test-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suite)!
        addTeardownBlock { defaults.removePersistentDomain(forName: suite) }
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("solvor-test-\(UUID().uuidString)")
        addTeardownBlock { try? FileManager.default.removeItem(at: dir) }
        let app = AppState(defaults: defaults, tokens: tokens, supportDir: dir, makeClient: { [unowned self] _, _ in built += 1; return host })
        app.host = h
        app.notifier = { [unowned self] title, body in told.append("\(title): \(body)") }
        let reads = keyReads
        app.keyProvider = { reads.bump(); return nil }   // never the real Secure Enclave from a test: it can prompt and block
        return app
    }

    func testAUserTokenConnectsEvenThoughStatusIsOperatorOnlyAndTheUserIdComesFromWhoami() async {
        let app = makeApp()
        host.demosResult = .success([demo("csv-clean")])
        await app.connect()
        XCTAssertTrue(app.connected); XCTAssertNil(app.problem)
        XCTAssertEqual(app.userId, "ana", "the user id is read from whoami, not typed")
        XCTAssertEqual(keyReads.value, 0, "connecting does not touch the approval key (reading it can wait on a Keychain prompt)")
        XCTAssertEqual(app.demos.map(\.id), ["csv-clean"])
        XCTAssertNil(app.status)
    }

    func testAnOperatorTokenKeepsTheUserIdThePersonTyped() async {
        let app = makeApp(); app.userId = "bo"
        host.whoamiResult = .success(WhoAmI(role: "operator"))
        await app.connect()
        XCTAssertTrue(app.connected); XCTAssertEqual(app.userId, "bo")
    }

    func testEachFailureIsClassifiedAndLeavesTheAppDisconnected() async {
        let app = makeApp()
        host.demosResult = .failure(MerionError.unauthorized)
        await app.connect()
        XCTAssertFalse(app.connected); XCTAssertEqual(app.problem?.kind, .refusedToken)
        host.demosResult = .failure(MerionError.transport("The Internet connection appears to be offline."))
        await app.connect()
        XCTAssertEqual(app.problem?.kind, .offline)
        host.demosResult = .success([])
        await app.connect()
        XCTAssertTrue(app.connected); XCTAssertNil(app.problem, "a retry clears the problem")
    }

    func testMissingDetailsSayWhatToAdd() async {
        let noToken = makeApp(token: nil)
        await noToken.connect()
        XCTAssertEqual(noToken.problem?.kind, .missing); XCTAssertEqual(built, 0, "nothing is contacted without a token")
        let noHost = makeApp(host: "")
        await noHost.connect()
        XCTAssertEqual(noHost.problem?.kind, .missing)
    }

    func testOnlyApprovalsThatArriveAfterConnectingAreAnnouncedAndNotAgainAfterAReconnect() async {
        let app = makeApp()
        host.pending = [approval("a1")]
        await app.connect()
        await app.pollApprovals()
        XCTAssertEqual(told, [], "what was already waiting is on screen when the person opens the app")
        host.pending = [approval("a1"), approval("a2")]
        await app.pollApprovals()
        XCTAssertEqual(told.count, 1); XCTAssertTrue(told[0].hasPrefix("Approval needed"))
        await app.pollApprovals()
        XCTAssertEqual(told.count, 1, "the same approval is not announced twice")
        await app.connect()
        await app.pollApprovals()
        XCTAssertEqual(told.count, 1, "reconnecting does not announce it again")
        host.pending.append(approval("a3"))
        await app.pollApprovals()
        XCTAssertEqual(told.count, 2)
    }

    func testTurningNotificationsOffSilencesThem() async {
        let app = makeApp(); app.notify = false
        await app.connect()
        host.pending = [approval("a1")]
        await app.pollApprovals()
        XCTAssertEqual(told, [])
        app.notify = true
    }

    func testALinkFromOutsideNeedsTheirYesBeforeAnythingIsStoredOrContacted() async {
        let app = makeApp(host: "", token: nil)
        let link = ConnectionLink("keep://connect?host=http://127.0.0.1:19096&token=kut1new")!
        app.offer(link: link)
        XCTAssertEqual(app.pendingLink, link)
        XCTAssertNil(tokens.value); XCTAssertEqual(built, 0); XCTAssertFalse(app.connected)
        await app.acceptPendingLink()
        XCTAssertEqual(tokens.value, "kut1new"); XCTAssertEqual(app.host, "http://127.0.0.1:19096")
        XCTAssertTrue(app.connected); XCTAssertNil(app.pendingLink)
    }

    func testDisconnectForgetsTheTokenAndWhatWasAnnounced() async {
        let app = makeApp()
        host.pending = [approval("a1")]
        await app.connect()
        XCTAssertFalse(app.announcedApprovals.isEmpty)
        app.disconnect()
        XCTAssertNil(tokens.value); XCTAssertFalse(app.connected); XCTAssertTrue(app.announcedApprovals.isEmpty)
    }

    func testConnectionModelFillsFromAPastedLinkOrToken() {
        let m = ConnectionModel()
        XCTAssertFalse(m.canConnect)
        XCTAssertTrue(m.paste("keep://connect?host=http://keep.example.com&token=kut1zzzz"))
        XCTAssertEqual(m.hostText, "http://keep.example.com"); XCTAssertEqual(m.tokenText, "kut1zzzz")
        XCTAssertNotNil(m.warning, "plain http to another machine is flagged"); XCTAssertTrue(m.canConnect)
        XCTAssertTrue(m.paste("kut1othertoken")); XCTAssertEqual(m.tokenText, "kut1othertoken"); XCTAssertNil(m.warning)
        XCTAssertFalse(m.paste("hello there")); XCTAssertFalse(m.paste("")); XCTAssertFalse(m.paste("https://example.com"))
        XCTAssertEqual(m.tokenText, "kut1othertoken", "text that is neither a link nor a token changes nothing")
    }

    // MARK: deciding

    private func appWithKey(_ key: SoftwareP256Key, enrolled: Bool) async -> AppState {
        let app = makeApp()
        app.keyProvider = { key }
        if enrolled { host.deviceList = [DeviceInfo(deviceId: "work-mac", publicKey: key.publicKeyBase64)] }
        await app.connect()
        await app.refreshDevice()
        XCTAssertEqual(app.myPublicKey, key.publicKeyBase64)
        return app
    }

    func testASignedDecisionIsSignedOverTheExactTextAndSentWithTheEnrolledDeviceId() async throws {
        let key = SoftwareP256Key()
        let app = await appWithKey(key, enrolled: true)
        XCTAssertEqual(app.device, .enrolled(deviceId: "work-mac"))
        let a = signed("a1"); host.pending = [a]; await app.refreshApprovals()
        let r = await app.decide(a, .approved)
        XCTAssertEqual(r, .decided("Approved: Tool Use Thing".replacingOccurrences(of: "Tool Use Thing", with: "Gmail Send")))
        let sent = try XCTUnwrap(host.decisions.first)
        XCTAssertEqual(sent.deviceId, "work-mac")
        let payload = try ApprovalPayload.text(for: a, decision: .approved)
        let pub = try P256.Signing.PublicKey(derRepresentation: Data(base64Encoded: key.publicKeyBase64)!)
        let sig = try P256.Signing.ECDSASignature(derRepresentation: Data(base64Encoded: try XCTUnwrap(sent.signature))!)
        XCTAssertTrue(pub.isValidSignature(sig, for: Data(payload.utf8)), "the host can check this signature against the enrolled key")
        XCTAssertFalse(pub.isValidSignature(sig, for: Data(try ApprovalPayload.text(for: a, decision: .denied).utf8)), "an approval is not a denial")
        XCTAssertTrue(app.approvals.isEmpty)
    }

    func testAnUnsignedDecisionIsNeverSentWhenTheHostAskedForASignature() async {
        let app = await appWithKey(SoftwareP256Key(), enrolled: false)
        XCTAssertEqual(app.device, .notEnrolled)
        let a = signed("a1"); host.pending = [a]
        let r = await app.decide(a, .approved)
        guard case .refused(let why) = r else { return XCTFail("\(r)") }
        XCTAssertTrue(why.contains("not enrolled"))
        XCTAssertTrue(host.decisions.isEmpty, "the host was not contacted")
    }

    func testAnExpiredWindowAndAMissingKeyAreRefusedWithoutContactingTheHost() async {
        let key = SoftwareP256Key()
        let app = await appWithKey(key, enrolled: true)
        let old = signed("a1", expiresIn: -5)
        guard case .refused = await app.decide(old, .approved) else { return XCTFail("expired") }
        app.keyProvider = { nil }; await app.refreshDevice()
        XCTAssertEqual(app.device, .noKey)
        guard case .refused = await app.decide(signed("a2"), .denied) else { return XCTFail("no key") }
        XCTAssertTrue(host.decisions.isEmpty)
    }

    func testAHostThatIssuesNoChallengeAcceptsAnUnsignedDecision() async throws {
        let app = await appWithKey(SoftwareP256Key(), enrolled: false)
        let plain = approval("p1"); host.pending = [plain]
        let r = await app.decide(plain, .denied)
        guard case .decided = r else { return XCTFail("\(r)") }
        let sent = try XCTUnwrap(host.decisions.first)
        XCTAssertNil(sent.signature); XCTAssertNil(sent.deviceId)
    }

    func testANotificationOnlyOffersToOpenTheApproval() {
        let c = Notifier.approvalCategory
        XCTAssertEqual(c.actions.map(\.identifier), ["open"], "there is no approve or deny button on a notification")
        XCTAssertTrue(c.actions.allSatisfy { $0.options.contains(.foreground) })
        XCTAssertEqual(Notifier.approvalCategoryId, "keep.approval")
    }

    // MARK: cancel and retry

    func testCancellingARunningJobStopsItAndTheLateErrorDoesNotOverwriteThat() async {
        let app = makeApp()
        host.demosResult = .success([demo("pdf-brief")])
        await app.connect()
        host.runDelaysUntilCancelled = true
        let file = tempFile("r.pdf")
        app.run(demo: "pdf-brief", files: [file])
        let job = app.jobs[0]
        guard case .running = job.state else { return XCTFail("should still be running") }
        app.cancelJob(job.id)
        guard case .failed(let m) = app.jobs[0].state else { return XCTFail("should be failed") }
        XCTAssertEqual(m, "Cancelled")
        try? await Task.sleep(nanoseconds: 60_000_000)   // give the cancelled task a chance to race in and overwrite it
        guard case .failed(let m2) = app.jobs[0].state else { return XCTFail("should still be failed") }
        XCTAssertEqual(m2, "Cancelled", "the task's own CancellationError must not replace the clean 'Cancelled' state")
    }

    func testCancellingAJobThatAlreadyFinishedDoesNothing() async {
        let app = makeApp()
        host.demosResult = .success([demo("pdf-brief")])
        await app.connect()
        let file = tempFile("r.pdf")
        app.run(demo: "pdf-brief", files: [file])
        let id = app.jobs[0].id
        for _ in 0..<50 { if case .failed = app.jobs[0].state { break }; try? await Task.sleep(nanoseconds: 5_000_000) }
        app.cancelJob(id)
        guard case .failed(let m) = app.jobs[0].state else { return XCTFail() }
        XCTAssertNotEqual(m, "Cancelled", "the run had already failed on its own; cancelling afterwards changes nothing")
    }

    func testRetryRunsTheSameFilesAndSourceAgainAsANewJob() async {
        let app = makeApp()
        host.demosResult = .success([demo("pdf-brief")])
        await app.connect()
        let file = tempFile("r.pdf")
        app.run(demo: "pdf-brief", files: [file], source: "drop")
        let first = app.jobs[0]
        app.retry(first)
        XCTAssertEqual(app.jobs.count, 2)
        XCTAssertEqual(app.jobs[0].demo, "pdf-brief"); XCTAssertEqual(app.jobs[0].files, [file]); XCTAssertEqual(app.jobs[0].source, "drop")
        XCTAssertNotEqual(app.jobs[0].id, first.id)
    }
}
