import MerionKit
import SwiftUI

struct RootView: View {
    @EnvironmentObject var app: AppState

    var body: some View {
        NavigationSplitView {
            VStack(spacing: 0) {
                HStack(spacing: 10) {
                    LogoTile(size: 34)
                    VStack(alignment: .leading, spacing: 0) {
                        Text("Solvor").font(.system(size: 19, weight: .bold, design: .rounded))
                        Text("sealed-cell files").font(.caption2).foregroundStyle(.secondary)
                    }
                    Spacer()
                }.padding(.horizontal, 14).padding(.top, 14).padding(.bottom, 8)
                List(selection: Binding(get: { app.pane }, set: { app.pane = $0; if $0 != nil { app.selectedJob = nil } })) {
                    ForEach(Pane.allCases) { p in
                        Label { Text(p.rawValue) } icon: { Image(systemName: p.icon).foregroundStyle(p.tint) }
                            .badge(p == .approvals ? app.approvals.count : 0)
                            .tag(Optional(p))
                    }
                    if !app.jobs.isEmpty {
                        Section("This session") {
                            ForEach(app.jobs.prefix(8)) { job in
                                HStack {
                                    switch job.state {
                                    case .running: ProgressView().controlSize(.small)
                                    case .done: Image(systemName: "checkmark.circle.fill").foregroundStyle(Brand.good)
                                    case .failed: Image(systemName: "xmark.octagon.fill").foregroundStyle(.red)
                                    }
                                    Text(app.demo(job.demo)?.title ?? job.demo).lineLimit(1)
                                }
                                .contentShape(Rectangle())
                                .onTapGesture { app.selectedJob = job.id; app.pane = nil }
                            }
                        }
                    }
                }
                .listStyle(.sidebar).scrollContentBackground(.hidden)
                ConnectionStrip()
            }
            .navigationSplitViewColumnWidth(min: 210, ideal: 230)
        } detail: {
            Group {
                if let id = app.selectedJob, app.pane == nil, let job = app.jobs.first(where: { $0.id == id }) { ResultView(job: job) }
                else if !app.connected && app.pane != .settings { NotConnectedView(open: { app.pane = .settings }) }
                else {
                    switch app.pane ?? .home {
                    case .home: HomeView()
                    case .useCases: UseCasesView()
                    case .runs: RunsView()
                    case .approvals: ApprovalsView()
                    case .goals: GoalsPane()
                    case .memory: MemoryPane()
                    case .done: DonePane()
                    case .folders: WatchFoldersView()
                    case .settings: SettingsView()
                    }
                }
            }
            .toolbar {
                ToolbarItemGroup {
                    Button { app.sheet = .email } label: { Label("Read email from browser", systemImage: "envelope.open") }
                        .help("Read the email open in your browser (⌘⇧E)").keyboardShortcut("e", modifiers: [.command, .shift]).disabled(!app.connected)
                    Button { app.sheet = .voice } label: { Label("Talk to Solvor", systemImage: "mic.fill") }
                        .help("Give a spoken or typed command (⌘⇧V)").keyboardShortcut("v", modifiers: [.command, .shift]).disabled(!app.connected)
                }
            }
        }
        .frame(minWidth: 980, minHeight: 620)
        .task { if !app.connected { await app.connect() } }
        .onChange(of: app.selectedJob) { _, new in if new != nil { app.pane = nil } }
        .sheet(item: $app.sheet) { s in
            switch s {
            case .email: EmailReaderView().environmentObject(app)
            case .voice: VoiceView().environmentObject(app)
            case .welcome: WelcomeView().environmentObject(app)
            case .about: AboutView()
            case .menuBarPreview: MenuBarView().environmentObject(app)   // debug only: screenshotting the popover's own view
            }
        }
        .onAppear {
            if !app.welcomed { app.sheet = .welcome }
            #if DEBUG
            DebugLaunch.apply(app)
            #endif
        }
        .alert("Check before uploading", isPresented: Binding(get: { app.pendingSecretWarning != nil }, set: { if !$0 { app.pendingSecretWarning = nil } })) {
            Button("Upload anyway", role: .destructive) { app.confirmPending() }
            Button("Cancel", role: .cancel) { app.pendingSecretWarning = nil }
        } message: { Text((app.pendingSecretWarning?.findings ?? []).joined(separator: "\n")) }
        .alert("Connect to \(app.pendingLink?.host.host ?? "this host")?", isPresented: Binding(get: { app.pendingLink != nil }, set: { if !$0 { app.pendingLink = nil } })) {
            Button("Connect") { Task { await app.acceptPendingLink() } }
            Button("Cancel", role: .cancel) { app.pendingLink = nil }
        } message: {
            Text("A link asked Solvor to connect to \(app.pendingLink?.host.absoluteString ?? "") and store its token in your Keychain. Only continue if you started this.\(app.pendingLink?.isPlainRemote == true ? " This address uses plain http to another machine, so the token could be read on the network." : "")")
        }
        .alert("Solvor", isPresented: Binding(get: { app.notice != nil }, set: { if !$0 { app.notice = nil } })) { Button("OK") { app.notice = nil } } message: { Text(app.notice ?? "") }
        .confirmationDialog(app.choice?.title ?? "", isPresented: Binding(get: { app.choice != nil }, set: { if !$0 { app.choice = nil } }), titleVisibility: .visible) {
            ForEach(app.choice?.options ?? []) { d in
                Button(d.title) { if let c = app.choice { app.run(demo: d.id, files: c.files, source: c.source) } }
            }
            Button("Cancel", role: .cancel) {}
        }
    }
}

struct ConnectionStrip: View {
    @EnvironmentObject var app: AppState
    var body: some View {
        HStack(spacing: 7) {
            Circle().fill(app.connected ? Brand.good : Color.orange).frame(width: 8, height: 8).shadow(color: (app.connected ? Brand.good : .orange).opacity(0.6), radius: 3)
            Text(app.connected ? (URL(string: app.host)?.host ?? "connected") : "not connected").font(.caption).lineLimit(1)
            Spacer()
            Button { app.sheet = .about } label: { Image(systemName: "info.circle") }.buttonStyle(.plain).foregroundStyle(.secondary).help("About Solvor")
        }.padding(.horizontal, 14).padding(.vertical, 10).background(.bar)
    }
}

struct NotConnectedView: View {
    let open: () -> Void
    @EnvironmentObject var app: AppState
    var body: some View {
        VStack(spacing: Space.m) {
            LogoTile(size: 72)
            Text("Connect Solvor to a Keep host").font(Typo.title)
            Text("Solvor reads each file in a sealed cell on a host you run.").foregroundStyle(.secondary).multilineTextAlignment(.center)
            ConnectForm().padding(Space.m).card()
        }.frame(maxWidth: 480).frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

#if DEBUG
/// Development only: `-SolvorOpen email|voice|runs|approvals|folders|settings|about|welcome`, `-SolvorDemoRun YES` and `-SolvorDemoEmail YES`
/// (launch arguments) put the app in a known state so its screens can be captured without clicking through them.
enum DebugLaunch {
    @MainActor static func apply(_ app: AppState) {
        let d = UserDefaults.standard
        if let s = d.string(forKey: "SolvorOpen") {
            switch s {
            case "email": app.sheet = .email
            case "voice": app.sheet = .voice
            case "about": app.sheet = .about
            case "welcome": app.sheet = .welcome
            case "runs": app.pane = .runs
            case "approvals": app.pane = .approvals
            case "folders": app.pane = .folders
            case "settings": app.pane = .settings
            case "menubar": app.sheet = .menuBarPreview
            default: break
            }
        }
        if d.bool(forKey: "SolvorDemoApproval") {
            let exp = Int(Date().timeIntervalSince1970) + 272
            let json = #"{"id":"demo-1","kind":"http.request","status":"pending","sign":{"format":"merion-approval-v1","challenge":"c","expires_at":\#(exp),"action_sha256":"ab"},"preview":{"kind":"gmail-message","fields":[{"label":"To","value":"ana@example.com"},{"label":"Subject","value":"Lunch on Friday?"},{"label":"Text","value":"Hi Ana, are you free for lunch on Friday at 12:30? I found a place near the office."}]}}"#
            Task { @MainActor in
                for _ in 0..<40 { if app.connected { break }; try? await Task.sleep(nanoseconds: 250_000_000) }
                try? await Task.sleep(nanoseconds: 1_500_000_000)
                app.debugHoldApprovals = true
                app.approvals = [try! JSONDecoder.keep.decode(Approval.self, from: Data(json.utf8))]
                app.device = .enrolled(deviceId: "work-mac")
            }
        }
        if d.bool(forKey: "SolvorDemoRun") {
            let url = FileManager.default.temporaryDirectory.appendingPathComponent("demo-statement.csv")
            try? "date,description,amount,category\n2026-09-01,Luma Cafe,12.40,Dining\n2026-09-02,Metro Card,30.00,Transport\n2026-09-03,Streamco,9.99,Subscriptions\n".write(to: url, atomically: true, encoding: .utf8)
            Task { @MainActor in
                for _ in 0..<40 { if app.connected { break }; try? await Task.sleep(nanoseconds: 250_000_000) }
                app.run(demo: "csv-clean", files: [url], source: "demo")
            }
        }
    }
}
#endif
