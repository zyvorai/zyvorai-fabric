import XCTest
@testable import MerionKit

final class EmailAndVoiceTests: XCTestCase {
    // MARK: EmailBuilder
    func testHeadersCannotBeInjectedFromThePage() {
        let eml = EmailBuilder.eml(subject: "Hi\r\nBcc: attacker@example.com\nX: y", from: "A <a@x>\r\nTo: z", body: "body", messageId: "id1")
        let headerBlock = eml.components(separatedBy: "\n\n")[0]
        XCTAssertFalse(headerBlock.contains("\nBcc:"))
        XCTAssertFalse(headerBlock.contains("\nTo:"))
        XCTAssertEqual(headerBlock.components(separatedBy: "\n").filter { $0.hasPrefix("Subject:") }.count, 1)
        XCTAssertTrue(headerBlock.contains("Subject: Hi Bcc: attacker@example.com X: y"))
    }

    func testABodyLineCannotLookLikeAnMboxSeparator() {
        let eml = EmailBuilder.eml(subject: "s", body: "hello\nFrom evil@x Mon Jan 1\nbye")
        XCTAssertTrue(eml.contains("\n>From evil@x Mon Jan 1\n"))
        XCTAssertFalse(eml.contains("\nFrom evil@x"))
    }

    func testTheMessageHasTheHeadersTheReaderNeeds() {
        let eml = EmailBuilder.eml(subject: "Invoice INV-1 overdue", body: "Pay Rs 5,000", sourceHost: "mail.example.com", messageId: "abc")
        for h in ["From: ", "Subject: Invoice INV-1 overdue", "Date: ", "Message-ID: <abc@solvor.local>", "X-Solvor-Source: mail.example.com", "Content-Type: text/plain; charset=utf-8"] {
            XCTAssertTrue(eml.contains(h), h)
        }
        XCTAssertTrue(eml.hasSuffix("Pay Rs 5,000\n"))
        XCTAssertEqual(EmailBuilder.eml(subject: "", body: "x").contains("Subject: (no subject)"), true)
    }

    func testAHugeBodyIsCapped() {
        let eml = EmailBuilder.eml(subject: "s", body: String(repeating: "a", count: 500_000))
        XCTAssertLessThan(eml.count, EmailBuilder.maxBody + 1000)
    }

    // MARK: Redactor
    func testCodesAreMaskedButAmountsAreNot() {
        let r = Redactor.redact("Your verification code is 482913. Invoice total Rs 1,25,000 due 15 Aug.", options: .codes)
        XCTAssertFalse(r.text.contains("482913"))
        XCTAssertTrue(r.text.contains("Rs 1,25,000"))
        XCTAssertEqual(r.codes, 1)
    }

    func testLongNumbersMerionOnlyTheLastFour() {
        let r = Redactor.redact("Card 4111 1111 1111 1111 and account 123456789012, ref INV-2026-0142.", options: .longNumbers)
        XCTAssertFalse(r.text.contains("4111 1111"))
        XCTAssertFalse(r.text.contains("123456789012"))
        XCTAssertTrue(r.text.contains("••••1111") && r.text.contains("••••9012"))
        XCTAssertTrue(r.text.contains("INV-2026-0142"), "short reference numbers are left alone")
        XCTAssertEqual(r.numbers, 2)
    }

    func testTrackingQueryStringsAreShortened() {
        let r = Redactor.redact("See https://shop.example.com/order/9?utm_source=mail&utm_campaign=abcdefghijklmnop&token=zzzzzzzz ok", options: .trackingLinks)
        XCTAssertTrue(r.text.contains("https://shop.example.com/order/9?…"))
        XCTAssertFalse(r.text.contains("utm_source"))
        XCTAssertEqual(r.links, 1)
    }

    func testEachSwitchWorksAlone() {
        let text = "code 123456 and 9876543210123"
        XCTAssertEqual(Redactor.redact(text, options: []).text, text)
        XCTAssertEqual(Redactor.redact(text, options: .all).total, 2)
    }

    // MARK: EmailRouter
    func testRoutingNamesTheWordsItMatched() {
        let avail: Set = ["receivables-ageing", "travel-itinerary", "subscription-finder", "reimbursement-claims", "mailbox-triage"]
        let inv = EmailRouter.route("Reminder: invoice INV-2026-0142 is overdue, amount due Rs 1,25,000", available: avail)
        XCTAssertEqual(inv.first?.useCase, "receivables-ageing")
        XCTAssertTrue(inv.first?.reason.contains("invoice") == true)
        XCTAssertEqual(inv.last?.useCase, "mailbox-triage")
        let trip = EmailRouter.route("Your flight departs at 07:45. Booking reference K7QP2M. Hotel check-in after 3pm.", available: avail)
        XCTAssertEqual(trip.first?.useCase, "travel-itinerary")
    }

    func testOnlyUseCasesTheHostListsAreOffered() {
        let r = EmailRouter.route("invoice overdue payment", available: ["mailbox-triage"])
        XCTAssertEqual(r.map(\.useCase), ["mailbox-triage"])
        XCTAssertTrue(EmailRouter.route("invoice overdue", available: []).isEmpty)
    }

    // MARK: VoiceCommandParser
    let useCases: [(id: String, title: String)] = [("card-statement", "Card statement"), ("csv-clean", "CSV cleanup"), ("pdf-brief", "PDF brief")]

    func testReadingTheEmailInTheBrowser() {
        for phrase in ["Read the email in my browser", "read this email", "Please check my Gmail inbox", "open and summarise this message in the browser tab"] {
            XCTAssertEqual(VoiceCommandParser.parse(phrase, useCases: useCases), .readBrowserEmail, phrase)
        }
    }

    func testSummarisingTheLatestDownloadWithANamedUseCase() {
        XCTAssertEqual(VoiceCommandParser.parse("Summarise my latest download with card statement", useCases: useCases), .summarise(.latestDownload, useCase: "card-statement"))
        XCTAssertEqual(VoiceCommandParser.parse("summarize the newest file", useCases: useCases), .summarise(.latestDownload, useCase: nil))
        XCTAssertEqual(VoiceCommandParser.parse("run csv cleanup on what I copied", useCases: useCases), .summarise(.clipboard, useCase: "csv-clean"))
    }

    func testNavigationAndLastRun() {
        XCTAssertEqual(VoiceCommandParser.parse("show my approvals"), .show(.approvals))
        XCTAssertEqual(VoiceCommandParser.parse("Open the run history"), .show(.runs))
        XCTAssertEqual(VoiceCommandParser.parse("go to watch folders"), .show(.watchFolders))
        XCTAssertEqual(VoiceCommandParser.parse("What did I run last?"), .lastRun)
    }

    func testVoiceCanNeverApproveOrDeny() {
        for phrase in ["approve it", "Approve the payment", "deny that request", "reject the approval", "yes accept"] {
            XCTAssertEqual(VoiceCommandParser.parse(phrase, useCases: useCases), .approvalNeedsTouchID, phrase)
        }
        XCTAssertEqual(VoiceCommandParser.parse("show my approvals"), .show(.approvals), "asking to see them is fine")
    }

    func testUnknownAndDescriptions() {
        XCTAssertEqual(VoiceCommandParser.parse("make me a sandwich"), .unknown("make me a sandwich"))
        XCTAssertEqual(VoiceCommandParser.parse("   "), .unknown("   "))
        XCTAssertTrue(VoiceCommand.readBrowserEmail.describe().contains("preview"))
        XCTAssertTrue(VoiceCommand.approvalNeedsTouchID.describe().contains("Touch ID"))
    }
}

final class BrowserScriptTests: XCTestCase {
    func testTheScriptIsFixedTextWithNoRoomForPageData() {
        for b in BrowserKind.allCases {
            let s = BrowserScript.appleScript(for: b)
            XCTAssertTrue(s.contains(b.appName))
            XCTAssertTrue(s.contains(b == .safari ? "do JavaScript" : "execute (active tab of front window) javascript"))
            XCTAssertTrue(s.contains("getSelection"))
            XCTAssertTrue(s.contains("slice(0,\(BrowserScript.maxChars))"))
            // the JavaScript's quotes and backslashes are escaped so it stays inside the AppleScript string
            XCTAssertFalse(s.replacingOccurrences(of: "\\\"", with: "").dropFirst(20).dropLast(20).contains("\"\"\""))
        }
    }

    /// Writes the scripts out when KEEP_DUMP_SCRIPTS is a directory, so `osacompile` can check their syntax against the installed browsers.
    func testDumpScriptsForACompileCheck() throws {
        guard let dir = ProcessInfo.processInfo.environment["KEEP_DUMP_SCRIPTS"] else { throw XCTSkip("KEEP_DUMP_SCRIPTS not set") }
        for b in BrowserKind.allCases { try BrowserScript.appleScript(for: b).write(toFile: "\(dir)/\(b.rawValue).applescript", atomically: true, encoding: .utf8) }
    }

    func testAPageResultBecomesACapturedPage() throws {
        let json = #"{"url":"https://mail.example.com/x","title":"Invoice","text":"Hello  \n\nPay now","selection":true}"#
        let page = try BrowserScript.parse(json)
        XCTAssertEqual(page.host, "mail.example.com"); XCTAssertTrue(page.isSelection); XCTAssertEqual(page.title, "Invoice")
    }

    func testOnlyWebPagesWithTextAreAccepted() {
        XCTAssertThrowsError(try BrowserScript.parse(#"{"url":"file:///etc/passwd","title":"","text":"x","selection":false}"#)) { XCTAssertEqual($0 as? BrowserScript.ReadError, .notHTTP) }
        XCTAssertThrowsError(try BrowserScript.parse(#"{"url":"https://a.b","title":"","text":"  ","selection":false}"#)) { XCTAssertEqual($0 as? BrowserScript.ReadError, .empty) }
        XCTAssertThrowsError(try BrowserScript.parse("not json")) { XCTAssertEqual($0 as? BrowserScript.ReadError, .notJSON) }
    }

    func testFailuresAreExplainedAndSecretsInTextAreFound() {
        XCTAssertEqual(BrowserScript.explain(errorNumber: -1743, message: "Not authorized to send Apple events to Google Chrome.", browser: .chrome), .automationDenied(.chrome))
        XCTAssertEqual(BrowserScript.explain(errorNumber: -1708, message: "Executing JavaScript through AppleScript is turned off. To turn it on, choose View > Developer > Allow JavaScript from Apple Events", browser: .chrome), .scriptingOff(.chrome))
        XCTAssertTrue(BrowserScript.help(.scriptingOff(.safari)).contains("Develop"))
        XCTAssertEqual(SecretScan.scan(text: "here is \(fakeAWSKey)").map(\.kind), ["an AWS access key id"])
    }
}
