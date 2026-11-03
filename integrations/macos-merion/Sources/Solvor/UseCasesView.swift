import AppKit
import MerionKit
import SwiftUI
import UniformTypeIdentifiers

struct UseCasesView: View {
    @EnvironmentObject var app: AppState
    @State private var search = ""
    @State private var group: CatalogGroup?
    @State private var targeted = false

    private var filtered: [Demo] {
        app.demos.filter { d in
            (group == nil || Catalog.group(for: d.id) == group) &&
            (search.isEmpty || d.id.localizedCaseInsensitiveContains(search) || d.title.localizedCaseInsensitiveContains(search) || d.description.localizedCaseInsensitiveContains(search))
        }
    }
    private var groups: [CatalogGroup] { CatalogGroup.allCases.filter { g in app.demos.contains { Catalog.group(for: $0.id) == g } } }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 22) {
                hero
                HStack(spacing: 8) {
                    chip("All", symbol: "square.grid.2x2", color: Color.accentColor, selected: group == nil) { group = nil }
                    ForEach(groups, id: \.self) { g in chip(g.title, symbol: Brand.symbol(g), color: Brand.color(g), selected: group == g) { group = g } }
                    Spacer()
                    TextField("Search", text: $search).textFieldStyle(.roundedBorder).frame(width: 200)
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 270), spacing: 14)], spacing: 14) {
                    ForEach(Array(filtered.enumerated()), id: \.element.id) { i, d in UseCaseCard(demo: d).appear(delay: Motion.stagger(i)) }
                }
                Text("\(filtered.count) of \(app.demos.count) use cases").font(.caption).foregroundStyle(.tertiary)
            }.padding(24)
        }
        .background { AuroraBackground(intensity: 0.8) }
        .dropDestination(for: URL.self) { urls, _ in DropRouter.route(urls, app: app); return true } isTargeted: { targeted = $0 }
        .overlay { if targeted { DropOverlay() } }
        .navigationTitle("Use cases")
    }

    private var hero: some View {
        HStack(alignment: .center, spacing: 26) {
            VStack(alignment: .leading, spacing: 10) {
                Text("Drop anything.\nGet answers.").font(Typo.hero).lineSpacing(-2).appear()
                Text("Every file is read inside a sealed cell that has no network. Nothing leaves it but the summary, and you see the proof.")
                    .font(.title3).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                HStack {
                    Button { app.sheet = .email } label: { Label("Read email from browser", systemImage: "envelope.open") }.primaryButton().controlSize(.large)
                    Button { app.sheet = .voice } label: { Label("Talk to Solvor", systemImage: "mic.fill") }.secondaryButton().controlSize(.large)
                }.padding(.top, 4)
            }
            Spacer(minLength: 20)
            VStack(spacing: 10) {
                Image(systemName: "arrow.down.doc.fill").font(.system(size: 44)).foregroundStyle(Color.accentColor.gradient)
                    .symbolEffect(.bounce.down, options: .repeat(.periodic(delay: 2.6)))
                Text("Drop a file here").font(.headline)
                Text("Solvor suggests the use case").font(.caption).foregroundStyle(.secondary)
            }
            .frame(width: 230, height: 170)
            .background(RoundedRectangle(cornerRadius: 20, style: .continuous).strokeBorder(Color.accentColor.opacity(0.55), style: StrokeStyle(lineWidth: 2, dash: [9, 7])))
            .background(RoundedRectangle(cornerRadius: 20, style: .continuous).fill(Color.accentColor.opacity(0.06)))
        }
        .padding(22).card()
    }

    private func chip(_ title: String, symbol: String, color: Color, selected: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: symbol).font(.callout.weight(.medium)).padding(.horizontal, 12).padding(.vertical, 6)
                .foregroundStyle(selected ? .white : color)
                .background(Capsule().fill(selected ? color : color.opacity(0.13)))
        }.buttonStyle(.plain)
    }
}

struct DropOverlay: View {
    var body: some View {
        RoundedRectangle(cornerRadius: 18, style: .continuous).strokeBorder(Color.accentColor, style: StrokeStyle(lineWidth: 3, dash: [10, 8])).padding(10)
            .background(Color.accentColor.opacity(0.06))
            .overlay(Label("Drop to read it in a sealed cell", systemImage: "lock.shield.fill").font(.title3.weight(.semibold)).padding(.horizontal, 20).padding(.vertical, 12).background(.regularMaterial, in: Capsule()))
            .allowsHitTesting(false).transition(.opacity)
    }
}

struct UseCaseCard: View {
    let demo: Demo
    @EnvironmentObject var app: AppState
    @State private var targeted = false
    @State private var hovered = false
    @State private var showHow = false

    var body: some View {
        let group = Catalog.group(for: demo.id)
        let entry = Catalog.entry(for: demo.id)
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 10) {
                Image(systemName: Brand.symbol(group)).font(.system(size: 16, weight: .semibold)).foregroundStyle(.white).frame(width: 34, height: 34)
                    .background(LinearGradient(colors: [Brand.color(group).opacity(0.85), Brand.color(group)], startPoint: .top, endPoint: .bottom), in: RoundedRectangle(cornerRadius: 9, style: .continuous))
                VStack(alignment: .leading, spacing: 0) { Text(demo.title).font(.headline).lineLimit(1); Text(group.title).font(.caption2).foregroundStyle(Brand.color(group)) }
                Spacer()
            }
            Text(demo.description).font(.callout).foregroundStyle(.secondary).lineLimit(3).frame(minHeight: 54, alignment: .top)
            HStack(spacing: 4) {
                ForEach(demo.accepts.prefix(4), id: \.self) { Text(".\($0)").font(.caption.monospaced()).padding(.horizontal, 6).padding(.vertical, 1).background(.quaternary, in: Capsule()) }
            }
            HStack {
                Button("Choose file…") { pick() }.primaryButton().controlSize(.small)
                if let how = entry?.howToGetFile {
                    Button("How to get the file") { showHow.toggle() }.controlSize(.small)
                        .popover(isPresented: $showHow) {
                            VStack(alignment: .leading, spacing: 8) {
                                Text("Run this yourself, then choose the file. Solvor never runs it for you.").font(.callout)
                                Text(how).font(.system(.callout, design: .monospaced)).textSelection(.enabled)
                                Button("Copy command") { NSPasteboard.general.clearContents(); NSPasteboard.general.setString(how, forType: .string) }
                            }.padding().frame(maxWidth: 420)
                        }
                }
            }
        }
        .padding(14).card(hover: hovered || targeted)
        .onHover { hovered = $0 }
        .dropDestination(for: URL.self) { urls, _ in
            let files = FileExpander.files(from: urls); guard !files.isEmpty else { return false }
            app.run(demo: demo.id, files: files); return true
        } isTargeted: { targeted = $0 }
    }

    private func pick() {
        let p = NSOpenPanel(); p.allowsMultipleSelection = true; p.canChooseDirectories = false
        let types = demo.accepts.compactMap { UTType(filenameExtension: $0) }
        if !types.isEmpty { p.allowedContentTypes = types }
        if p.runModal() == .OK { app.run(demo: demo.id, files: p.urls) }
    }
}

enum FileExpander {
    /// Folders become their files (one level deep); hidden files are skipped.
    static func files(from urls: [URL]) -> [URL] {
        urls.flatMap { url -> [URL] in
            var isDir: ObjCBool = false
            guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) else { return [] }
            if isDir.boolValue {
                let inner = (try? FileManager.default.contentsOfDirectory(at: url, includingPropertiesForKeys: nil, options: [.skipsHiddenFiles])) ?? []
                return inner.filter { var d: ObjCBool = false; return FileManager.default.fileExists(atPath: $0.path, isDirectory: &d) && !d.boolValue }
            }
            return [url]
        }
    }
}

enum DropRouter {
    /// Dropped files with no chosen use case, from anywhere (the window, the Dock icon, a Service, a Shortcut, a watched
    /// folder): asks `IntakeCoordinator` what to do, then either runs it, asks the person to choose, or says why nothing
    /// happened. A mixed drop that leaves files out says so — `IntakeCoordinator.leftOutNotice` — instead of going quiet.
    @MainActor static func route(_ urls: [URL], app: AppState, source: String = "drop") {
        let plan = IntakeCoordinator.plan(urls: urls, demos: app.demos)
        switch plan.outcome {
        case .none:
            let files = FileExpander.files(from: urls)
            let ext = files.first.map { ".\($0.pathExtension)" } ?? "that"
            app.notice = files.isEmpty ? "Nothing there to read." : "No use case reads \(ext) files. Pick a card and drop the file on it."
        case .choose(let files, let optionIds):
            let options = optionIds.compactMap { id in app.demos.first { $0.id == id } }
            let title = "Which use case should read \(files.first?.lastPathComponent ?? "this file")?"
            app.choice = AppState.ChoiceRequest(title: title, files: files, options: options, source: source)
        case .run(let demoId, let files):
            app.run(demo: demoId, files: files, source: source)
        }
        if let notice = IntakeCoordinator.leftOutNotice(plan) { app.notice = notice }
    }
}
