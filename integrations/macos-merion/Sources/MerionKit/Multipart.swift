import Foundation

/// A multipart/form-data body written to a temporary file, so a large upload is streamed from disk
/// (`URLSession.upload(for:fromFile:)`) instead of held in memory.
public struct MultipartBody {
    public let boundary: String
    public let fileURL: URL
    public var contentType: String { "multipart/form-data; boundary=\(boundary)" }

    public struct Part {
        public var field: String
        public var filename: String
        public var source: URL
        public init(field: String = "file", filename: String, source: URL) {
            self.field = field; self.filename = filename; self.source = source
        }
    }

    /// Fixed-size text parts (e.g. `note=none`) are allowed too.
    public static func write(parts: [Part], fields: [String: String] = [:], boundary: String = "keep-\(UUID().uuidString)") throws -> MultipartBody {
        let out = FileManager.default.temporaryDirectory.appendingPathComponent("keep-upload-\(UUID().uuidString)")
        FileManager.default.createFile(atPath: out.path, contents: nil)
        let handle = try FileHandle(forWritingTo: out)
        defer { try? handle.close() }
        func put(_ s: String) throws { try handle.write(contentsOf: Data(s.utf8)) }
        for (name, value) in fields.sorted(by: { $0.key < $1.key }) {
            try put("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(clean(name))\"\r\n\r\n\(value)\r\n")
        }
        for part in parts {
            try put("--\(boundary)\r\nContent-Disposition: form-data; name=\"\(clean(part.field))\"; filename=\"\(clean(part.filename))\"\r\nContent-Type: application/octet-stream\r\n\r\n")
            let input = try FileHandle(forReadingFrom: part.source)
            defer { try? input.close() }
            while let chunk = try input.read(upToCount: 1 << 20), !chunk.isEmpty { try handle.write(contentsOf: chunk) }
            try put("\r\n")
        }
        try put("--\(boundary)--\r\n")
        return MultipartBody(boundary: boundary, fileURL: out)
    }

    /// Header values cannot carry quotes or line breaks.
    static func clean(_ s: String) -> String {
        s.replacingOccurrences(of: "\"", with: "'").replacingOccurrences(of: "\r", with: " ").replacingOccurrences(of: "\n", with: " ")
    }

    public func remove() { try? FileManager.default.removeItem(at: fileURL) }
}
