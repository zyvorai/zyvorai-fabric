import Foundation
import MerionKit

/// Watches folders with a DispatchSource on each directory and reports a matching file once it has stopped growing.
final class FolderWatcher: @unchecked Sendable {
    var onFile: ((FolderRule, URL) -> Void)?
    private var sources: [UUID: DispatchSourceFileSystemObject] = [:]
    private var rules: [UUID: FolderRule] = [:]
    private let queue = DispatchQueue(label: "dev.zyvor.solvor.watch")
    private var pending: Set<String> = []

    func update(rules newRules: [FolderRule]) {
        queue.async { [self] in
            for (_, s) in sources { s.cancel() }
            sources = [:]; rules = [:]
            for r in newRules where r.enabled {
                rules[r.id] = r
                let fd = open(r.folder.path, O_EVTONLY)
                guard fd >= 0 else { continue }
                let src = DispatchSource.makeFileSystemObjectSource(fileDescriptor: fd, eventMask: [.write, .extend, .rename], queue: queue)
                src.setEventHandler { [weak self] in self?.scan(r) }
                src.setCancelHandler { close(fd) }
                src.resume()
                sources[r.id] = src
                scan(r)   // files already there when the rule starts; the seen-set keeps each content to one run
            }
        }
    }

    private func scan(_ rule: FolderRule) {
        guard let names = try? FileManager.default.contentsOfDirectory(atPath: rule.folder.path) else { return }
        for name in names where rule.matches(filename: name) {
            let url = rule.folder.appendingPathComponent(name)
            var isDir: ObjCBool = false
            guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir), !isDir.boolValue else { continue }
            let key = "\(rule.id)/\(name)"
            guard pending.insert(key).inserted else { continue }
            settle(url) { [weak self] in
                self?.queue.async { self?.pending.remove(key) }
                self?.onFile?(rule, url)
            }
        }
    }

    /// A file that is still being copied changes size; wait until it holds still for a second.
    private func settle(_ url: URL, then done: @escaping () -> Void) {
        func size() -> Int { (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? Int) ?? -1 }
        var last = size()
        func tick(_ tries: Int) {
            queue.asyncAfter(deadline: .now() + 1) {
                let now = size()
                if now == last && now > 0 { done() } else if tries > 0 { last = now; tick(tries - 1) } else { done() }
            }
        }
        tick(30)
    }
}
