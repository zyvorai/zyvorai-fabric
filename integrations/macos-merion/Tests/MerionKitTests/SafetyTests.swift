import XCTest
@testable import MerionKit

final class SafetyTests: XCTestCase {
    func testSecretsAreFoundButNeverEchoed() throws {
        let f = try Fixture.tempFile("notes.txt", "deploy with \(fakePrivateKeyHeader)\nabc\nand key \(fakeAWSKey) and token=\(fakeToken)")
        let found = SecretScan.scan(fileURL: f)
        XCTAssertTrue(found.contains { $0.kind == "a private key" })
        XCTAssertTrue(found.contains { $0.kind == "an AWS access key id" })
        for finding in found { XCTAssertFalse(finding.kind.contains("AKIA") || finding.where_.contains("AKIA")) }
    }

    func testCleanFilesAndSecretLookingNames() throws {
        XCTAssertTrue(SecretScan.scan(fileURL: try Fixture.tempFile("report.csv", "name,qty\nAnn,1\n")).isEmpty)
        XCTAssertEqual(SecretScan.scan(fileURL: try Fixture.tempFile(".env", "A=1")).map(\.kind), ["an environment file"])
        XCTAssertTrue(SecretScan.scan(fileURL: try Fixture.tempFile("id_ed25519", "x")).contains { $0.kind == "an SSH private key" })
    }

    func testBinaryFilesAreOnlyCheckedByName() throws {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent("photo.png")
        try Data([0x89, 0x50, 0, 0x47] + Array((fakeAWSKey).utf8)).write(to: url)
        XCTAssertTrue(SecretScan.scan(fileURL: url).isEmpty)
    }

    func testFolderRulesMatchCaseInsensitivelyAndSkipDotfiles() {
        let rule = FolderRule(folder: URL(fileURLWithPath: "/tmp/in"), patterns: ["*.pdf", "invoice*.xlsx"], demo: "pdf-brief")
        XCTAssertTrue(rule.matches(filename: "Contract.PDF"))
        XCTAssertTrue(rule.matches(filename: "invoice-sep.xlsx"))
        XCTAssertFalse(rule.matches(filename: "notes.txt"))
        XCTAssertFalse(rule.matches(filename: ".hidden.pdf"))
        var off = rule; off.enabled = false
        XCTAssertFalse(off.matches(filename: "a.pdf"))
    }

    func testAFileIsSummarisedOnceEvenIfRenamed() throws {
        let a = try Fixture.tempFile("a.pdf", "same bytes"), b = try Fixture.tempFile("renamed.pdf", "same bytes")
        let rule = UUID()
        var seen = SeenFiles()
        let h1 = try ContentHash.sha256(of: a), h2 = try ContentHash.sha256(of: b)
        XCTAssertEqual(h1, h2)
        XCTAssertFalse(seen.contains(rule: rule, hash: h1))
        seen.insert(rule: rule, hash: h1)
        XCTAssertTrue(seen.contains(rule: rule, hash: h2))
        XCTAssertFalse(seen.contains(rule: UUID(), hash: h2), "another rule may run the same content")
        XCTAssertEqual(h1.count, 64)
    }

    func testTheInMemoryAndKeychainStoresRoundTrip() throws {
        let mem = InMemoryTokenStore()
        XCTAssertNil(try mem.token()); try mem.save("t1"); XCTAssertEqual(try mem.token(), "t1"); try mem.clear(); XCTAssertNil(try mem.token())
        let kc = KeychainTokenStore(service: "dev.zyvor.merion.test-\(UUID().uuidString)")
        do {
            try kc.save("abc"); XCTAssertEqual(try kc.token(), "abc")
            try kc.save("def"); XCTAssertEqual(try kc.token(), "def")
            try kc.clear(); XCTAssertNil(try kc.token())
        } catch { throw XCTSkip("Keychain not usable from this test process: \(error)") }
    }

    func testATokenStoredUnderThePreRenameServiceMovesToTheNewOne() throws {
        let tag = UUID().uuidString
        let old = KeychainTokenStore(service: "dev.zyvor.keep.test-\(tag)", legacyService: nil)
        let new = KeychainTokenStore(service: "dev.zyvor.merion.test-\(tag)", legacyService: "dev.zyvor.keep.test-\(tag)")
        do {
            try old.save("carried-over")
            XCTAssertEqual(try new.token(), "carried-over")      // found under the old name and moved
            XCTAssertNil(try old.token())                         // the old item is gone
            XCTAssertEqual(try new.token(), "carried-over")      // and it now lives under the new name
            try new.clear()
            XCTAssertNil(try new.token())
        } catch { throw XCTSkip("Keychain not usable from this test process: \(error)") }
    }
}
