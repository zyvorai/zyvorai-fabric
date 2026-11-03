import Foundation
import XCTest
@testable import MerionKit

enum Fixture {
    static func data(_ name: String) throws -> Data {
        let url = Bundle.module.url(forResource: name, withExtension: nil, subdirectory: "Fixtures")
            ?? Bundle.module.resourceURL!.appendingPathComponent("Fixtures/\(name)")
        return try Data(contentsOf: url)
    }
    static func decode<T: Decodable>(_ type: T.Type, _ name: String) throws -> T {
        try JSONDecoder.keep.decode(T.self, from: data(name))
    }
    static func tempFile(_ name: String, _ contents: String) throws -> URL {
        let dir = FileManager.default.temporaryDirectory.appendingPathComponent("keepkit-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let url = dir.appendingPathComponent(name)
        try contents.write(to: url, atomically: true, encoding: .utf8)
        return url
    }
}

/// Answers every request from a closure, so no network is used.
final class StubProtocol: URLProtocol {
    typealias Handler = (URLRequest, Data?) -> (Int, Data)
    nonisolated(unsafe) static var handler: Handler?
    nonisolated(unsafe) static var seen: [URLRequest] = []
    nonisolated(unsafe) static var bodies: [Data] = []

    override class func canInit(with request: URLRequest) -> Bool { true }
    override class func canonicalRequest(for request: URLRequest) -> URLRequest { request }
    override func startLoading() {
        var body: Data?
        if let stream = request.httpBodyStream {
            stream.open(); defer { stream.close() }
            var data = Data(); var buf = [UInt8](repeating: 0, count: 65536)
            while stream.hasBytesAvailable { let n = stream.read(&buf, maxLength: buf.count); if n <= 0 { break }; data.append(buf, count: n) }
            body = data
        } else { body = request.httpBody }
        Self.seen.append(request); Self.bodies.append(body ?? Data())
        let (status, data) = Self.handler?(request, body) ?? (500, Data())
        let resp = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: ["content-type": "application/json"])!
        client?.urlProtocol(self, didReceive: resp, cacheStoragePolicy: .notAllowed)
        client?.urlProtocol(self, didLoad: data)
        client?.urlProtocolDidFinishLoading(self)
    }
    override func stopLoading() {}

    static func session() -> URLSession {
        let c = URLSessionConfiguration.ephemeral; c.protocolClasses = [StubProtocol.self]
        seen = []; bodies = []
        return URLSession(configuration: c)
    }
}


// Fake secrets for testing the scanner, built at run time so no literal that looks like a credential sits in the source.
let fakeAWSKey = "AKIA" + "ABCDEFGHIJKLMNOP"
let fakeToken = "abcdEFGH" + "12345678abcdEFGH"
let fakePrivateKeyHeader = "-----BEGIN OPENSSH " + "PRIVATE KEY-----"
