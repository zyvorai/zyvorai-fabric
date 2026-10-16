import Foundation

public enum BrowserKind: String, CaseIterable, Sendable {
    case safari, chrome, brave, edge, arc

    public var appName: String {
        switch self {
        case .safari: return "Safari"
        case .chrome: return "Google Chrome"
        case .brave: return "Brave Browser"
        case .edge: return "Microsoft Edge"
        case .arc: return "Arc"
        }
    }
    public var bundleID: String {
        switch self {
        case .safari: return "com.apple.Safari"
        case .chrome: return "com.google.Chrome"
        case .brave: return "com.brave.Browser"
        case .edge: return "com.microsoft.edgemac"
        case .arc: return "company.thebrowser.Browser"
        }
    }
    /// What the person has to switch on once in the browser for scripting to read a page.
    public var scriptingSwitch: String {
        switch self {
        case .safari: return "In Safari's menu bar choose Develop, then Allow JavaScript from Apple Events. (If there is no Develop menu: Safari, Settings, Advanced, Show features for web developers.)"
        default: return "In \(appName)'s menu bar choose View, then Developer, then Allow JavaScript from Apple Events."
        }
    }
}

/// The fixed AppleScript and JavaScript used to read the front tab. Nothing from a page or the person is ever placed in the script text.
public enum BrowserScript {
    public static let maxChars = 200_000

    /// Runs in the page. Returns the selected text if there is a selection, otherwise the main content area, as one JSON string.
    public static let javascript = """
    (function(){var sel=String(window.getSelection?window.getSelection():'').trim();var t='';var isSel=false;\
    if(sel.length>20){t=sel;isSel=true}else{var el=document.querySelector('[role=main]')||document.querySelector('main')||document.querySelector('article')||document.body;t=(el&&el.innerText)||''}\
    t=t.replace(/\\r/g,'').replace(/\\n{3,}/g,'\\n\\n').slice(0,\(maxChars));\
    return JSON.stringify({url:location.href,title:document.title||'',text:t,selection:isSel});})()
    """

    public static func appleScript(for browser: BrowserKind) -> String {
        let js = javascript.replacingOccurrences(of: "\\", with: "\\\\").replacingOccurrences(of: "\"", with: "\\\"")
        switch browser {
        case .safari:
            return "tell application \"Safari\"\nreturn do JavaScript \"\(js)\" in current tab of front window\nend tell"
        case .chrome:
            return "tell application \"Google Chrome\"\nreturn execute (active tab of front window) javascript \"\(js)\"\nend tell"
        default:
            // Brave, Edge and Arc share Chrome's scripting terms.
            return "tell application \"\(browser.appName)\"\nusing terms from application \"Google Chrome\"\nreturn execute (active tab of front window) javascript \"\(js)\"\nend using terms from\nend tell"
        }
    }

    public enum ReadError: Error, Equatable, LocalizedError {
        case notHTTP, empty, notJSON
        public var errorDescription: String? {
            switch self {
            case .notHTTP: return "Only web pages (http or https) can be read."
            case .empty: return "The page had no text to read. Open the message, or select its text, and try again."
            case .notJSON: return "The browser's answer was not what Solvor expected."
            }
        }
    }

    /// Reads the JSON the page script returned. Non-web pages are refused, and the text is capped.
    public static func parse(_ result: String) throws -> CapturedPage {
        guard let data = result.data(using: .utf8), let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { throw ReadError.notJSON }
        guard let urlString = o["url"] as? String, let url = URL(string: urlString), let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https" else { throw ReadError.notHTTP }
        let text = String((o["text"] as? String ?? "").prefix(maxChars)).trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { throw ReadError.empty }
        return CapturedPage(url: url, title: String((o["title"] as? String ?? "").prefix(200)), text: text, isSelection: o["selection"] as? Bool ?? false)
    }

    public enum Problem: Equatable, Sendable {
        case automationDenied(BrowserKind)
        case scriptingOff(BrowserKind)
        case noWindow(BrowserKind)
        case other(String)
    }

    /// Turns an AppleScript failure into something the person can act on.
    public static func explain(errorNumber: Int, message: String, browser: BrowserKind) -> Problem {
        let m = message.lowercased()
        if errorNumber == -1743 || m.contains("not authorized") || m.contains("not allowed") { return .automationDenied(browser) }
        if m.contains("javascript") && (m.contains("apple events") || m.contains("turned off") || m.contains("enable")) { return .scriptingOff(browser) }
        if errorNumber == -1719 || m.contains("invalid index") || m.contains("can't get window") || m.contains("can’t get window") { return .noWindow(browser) }
        return .other(message)
    }

    public static func help(_ p: Problem) -> String {
        switch p {
        case .automationDenied(let b): return "macOS has not allowed Solvor to control \(b.appName). Open System Settings, Privacy & Security, Automation, and switch on \(b.appName) under Solvor, then try again."
        case .scriptingOff(let b): return "\(b.appName) is not allowing scripts to read pages yet. \(b.scriptingSwitch) Then try again."
        case .noWindow(let b): return "\(b.appName) has no open window with a page to read."
        case .other(let m): return m
        }
    }
}
