import Foundation

/// `GET /v1/whoami`: who the token belongs to, so the app never asks the person to type a user id.
public struct WhoAmI: Codable, Equatable, Sendable {
    public var role: String
    public var userId: String?
    public var scopes: [String]?
    public init(role: String, userId: String? = nil, scopes: [String]? = nil) { self.role = role; self.userId = userId; self.scopes = scopes }
    public var isOperator: Bool { role == "operator" }
}

extension MerionClient {
    public func whoami() async throws -> WhoAmI { try await getJSON("/v1/whoami") }
}

/// What the app state needs from a host, so a test can supply a stub instead of a network.
public protocol KeepAPI: Sendable {
    func whoami() async throws -> WhoAmI
    func status() async throws -> MerionStatus
    func demos() async throws -> [Demo]
    func artifacts(limit: Int?) async throws -> [Artifact]
    func artifact(id: String) async throws -> Artifact
    func inbox(userId: String?) async throws -> Inbox
    func approvals() async throws -> [Approval]
    func usage(userId: String?) async throws -> UsageReport
    func run(demo: String, files: [URL], maxBytes: Int?) async throws -> RunOutcome
    func devices(userId: String) async throws -> [DeviceInfo]
    func decide(approval id: String, _ decision: Decision, deviceId: String?, signature: String?) async throws
}

extension MerionClient: KeepAPI {}

/// A `keep://connect?host=…&token=…` link, as a person might paste it. Parsing never contacts anything: what the link
/// says is shown to the person, who decides (a link a web page can open must not be able to point the app somewhere).
public struct ConnectionLink: Equatable, Sendable {
    public var host: URL
    public var token: String

    public init?(_ string: String) {
        guard let url = URL(string: string.trimmingCharacters(in: .whitespacesAndNewlines)) else { return nil }
        self.init(url: url)
    }

    public init?(url: URL) {
        guard url.scheme?.lowercased() == "keep", url.host?.lowercased() == "connect",
              let items = URLComponents(url: url, resolvingAgainstBaseURL: false)?.queryItems,
              let hostString = items.first(where: { $0.name == "host" })?.value,
              let token = items.first(where: { $0.name == "token" })?.value, !token.isEmpty,
              let host = URL(string: hostString), let scheme = host.scheme?.lowercased(), scheme == "http" || scheme == "https", host.host != nil
        else { return nil }
        self.host = host; self.token = token
    }

    /// Loopback and `.local` hosts are the person's own machine or network; anything else is worth a second look.
    public var isLocal: Bool {
        guard let h = host.host?.lowercased() else { return false }
        return h == "localhost" || h == "127.0.0.1" || h == "::1" || h.hasSuffix(".localhost")
    }
    /// Sending a token over plain http to another machine exposes it.
    public var isPlainRemote: Bool { host.scheme?.lowercased() == "http" && !isLocal }
}

/// Finds a Keep host on this Mac (the demo script and the default port). It does not scan a network.
public enum HostProbe {
    public static let localPorts = [19096, 9096]

    /// The base URLs among `ports` on 127.0.0.1 whose `/healthz` answers `{"ok": true}`.
    public static func local(ports: [Int] = localPorts, session: URLSession = .shared, timeout: TimeInterval = 1.0) async -> [URL] {
        await withTaskGroup(of: (Int, URL?).self) { group in
            for (i, port) in ports.enumerated() {
                group.addTask { (i, await healthy(URL(string: "http://127.0.0.1:\(port)")!, session: session, timeout: timeout)) }
            }
            var found: [(Int, URL)] = []
            for await (i, url) in group { if let url { found.append((i, url)) } }
            return found.sorted { $0.0 < $1.0 }.map(\.1)
        }
    }

    static func healthy(_ base: URL, session: URLSession, timeout: TimeInterval) async -> URL? {
        var req = URLRequest(url: base.appendingPathComponent("healthz")); req.timeoutInterval = timeout
        guard let (data, resp) = try? await session.data(for: req), (resp as? HTTPURLResponse)?.statusCode == 200,
              let o = try? JSONSerialization.jsonObject(with: data) as? [String: Any], o["ok"] as? Bool == true else { return nil }
        return base
    }
}

/// A connection failure in words a person can act on.
public struct ConnectionProblem: Equatable, Sendable {
    public enum Kind: Equatable, Sendable { case missing, badAddress, offline, refusedToken, forbidden, notMerion, other }
    public var kind: Kind
    public var title: String
    public var hint: String

    public init(_ error: Error) {
        switch error as? MerionError {
        case .badURL:
            self = .init(.badAddress, "That address does not look right", "Use the full address, for example http://127.0.0.1:9096 or https://keep.example.com.")
        case .unauthorized:
            self = .init(.refusedToken, "The host does not accept this token", "The token may be mistyped, expired or revoked. Ask whoever runs the host for a new one.")
        case .transport(let m):
            self = .init(.offline, "Solvor could not reach the host", "Check the address and that the host is running. (\(m))")
        case .http(let status, _) where status == 403:
            self = .init(.forbidden, "This token is not allowed to do that", "The host recognised the token but its scopes do not allow this.")
        case .decoding:
            self = .init(.notMerion, "That does not look like a Keep host", "The address answered, but not with what a Keep host sends. Check the port.")
        case .http(let status, let message):
            self = .init(.other, "The host answered \(status)", message)
        default:
            self = .init(.other, "Could not connect", error.localizedDescription)
        }
    }

    public static let missing = ConnectionProblem(.missing, "Add the host address and a token", "Or try the demo, which needs nothing.")

    init(_ kind: Kind, _ title: String, _ hint: String) { self.kind = kind; self.title = title; self.hint = hint }
}
