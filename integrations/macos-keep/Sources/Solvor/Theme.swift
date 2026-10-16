import KeepKit
import SwiftUI

/// The palette: an Apple-style blue carries the interface (the accent colour, the mark's fill); Zyvor's orange
/// (`--hs-accent-fill #ff5a15` at zyvor.dev) is kept as one deliberate drop of warmth — the rings closing around a run,
/// an approval waiting for you, a stamp landing. It is never the accent colour itself, so it keeps meaning "look here."
enum Brand {
    static let orange = Color(red: 1.0, green: 0.353, blue: 0.082)
    static let deep = Color(red: 0.8, green: 0.259, blue: 0.039)
    static let glow = Color(red: 1.0, green: 0.478, blue: 0.239)
    static let ink = Color(red: 0.078, green: 0.086, blue: 0.102)
    static let good = Color(red: 0.19, green: 0.78, blue: 0.45)

    /// Apple.com's blue (the `#0071e3` used for its buttons and links).
    static let blue = Color(red: 0.0, green: 0.443, blue: 0.851)
    static let blueDeep = Color(red: 0.0, green: 0.263, blue: 0.545)
    static let blueGlow = Color(red: 0.42, green: 0.70, blue: 1.0)

    static var gradient: LinearGradient { LinearGradient(colors: [blueGlow, blue, blueDeep], startPoint: .topLeading, endPoint: .bottomTrailing) }

    static func color(_ g: CatalogGroup) -> Color {
        switch g {
        case .documents: return Color(red: 0.36, green: 0.55, blue: 1.0)
        case .phone: return Color(red: 0.24, green: 0.78, blue: 0.62)
        case .mac: return Color(red: 0.62, green: 0.45, blue: 0.95)
        case .windows: return Color(red: 0.2, green: 0.62, blue: 0.95)
        case .developer: return Color(red: 0.95, green: 0.55, blue: 0.2)
        case .browser: return Color(red: 0.95, green: 0.4, blue: 0.55)
        case .office: return Color(red: 0.85, green: 0.7, blue: 0.2)
        case .other: return Color.gray
        }
    }

    static func symbol(_ g: CatalogGroup) -> String {
        switch g {
        case .documents: return "doc.text"
        case .phone: return "iphone"
        case .mac: return "macbook"
        case .windows: return "pc"
        case .developer: return "chevron.left.forwardslash.chevron.right"
        case .browser: return "safari"
        case .office: return "briefcase"
        case .other: return "square.grid.2x2"
        }
    }
}

/// The Zyvor Z (the path from the Zyvor favicon), scaled into any rect. Stroke it with round caps. Kept for anything
/// that still wants the wordmark (docs, the menu-bar glyph); the app mark itself now draws `FaceMark`.
struct ZMark: Shape {
    func path(in rect: CGRect) -> Path {
        // Favicon coordinates (64-unit box): 18.5,20.5 → 45.5,20.5 → 18.5,43.5 → 45.5,43.5. The Z occupies 27 x 23 units.
        let pts: [(CGFloat, CGFloat)] = [(18.5, 20.5), (45.5, 20.5), (18.5, 43.5), (45.5, 43.5)]
        let w = rect.width, h = rect.height
        let s = min(w, h) / 27
        let ox = rect.midX - 13.5 * s, oy = rect.midY - 11.5 * s
        var p = Path()
        for (i, pt) in pts.enumerated() {
            let q = CGPoint(x: ox + (pt.0 - 18.5) * s, y: oy + (pt.1 - 20.5) * s)
            if i == 0 { p.move(to: q) } else { p.addLine(to: q) }
        }
        return p
    }
}

/// An original face — two eyes and a smile, nothing borrowed — in a 64-unit box like `ZMark`, so the mark reads as a
/// small presence rather than a letter. One path (eyes, then the smile) so it can be drawn stroke-by-stroke.
struct FaceMark: Shape {
    func path(in rect: CGRect) -> Path {
        let s = min(rect.width, rect.height) / 64
        let ox = rect.midX - 32 * s, oy = rect.midY - 32 * s
        func pt(_ x: CGFloat, _ y: CGFloat) -> CGPoint { CGPoint(x: ox + x * s, y: oy + y * s) }
        var p = Path()
        p.move(to: pt(22, 25)); p.addLine(to: pt(22, 35))
        p.move(to: pt(42, 25)); p.addLine(to: pt(42, 35))
        p.move(to: pt(17, 40)); p.addQuadCurve(to: pt(47, 40), control: pt(32, 57))
        return p
    }
}

/// The app mark: the blue squircle with an original face, and one drop of Zyvor orange.
struct LogoTile: View {
    var size: CGFloat = 40
    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.23, style: .continuous).fill(Brand.gradient)
            RoundedRectangle(cornerRadius: size * 0.23, style: .continuous).strokeBorder(.white.opacity(0.25), lineWidth: 1)
            FaceMark().stroke(.white, style: StrokeStyle(lineWidth: size * 0.11, lineCap: .round, lineJoin: .round)).padding(size * 0.24)
            Circle().fill(Brand.orange).frame(width: size * 0.16, height: size * 0.16)
                .overlay(Circle().strokeBorder(.white.opacity(0.4), lineWidth: 0.5))
                .offset(x: size * 0.30, y: -size * 0.32)
        }
        .frame(width: size, height: size)
        .shadow(color: Brand.blueDeep.opacity(0.35), radius: size * 0.12, y: size * 0.06)
    }
}

struct Card: ViewModifier {
    var hover = false
    func body(content: Content) -> some View {
        content
            .glassEffect(.regular, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(Color.accentColor.opacity(hover ? 0.7 : 0), lineWidth: 1.5))
            .shadow(color: .black.opacity(hover ? 0.22 : 0.06), radius: hover ? 16 : 4, y: hover ? 9 : 2)
            .scaleEffect(hover ? 1.02 : 1)
            .offset(y: hover ? -2 : 0)
            .animation(.spring(response: 0.3, dampingFraction: 0.7), value: hover)
    }
}
extension View { func card(hover: Bool = false) -> some View { modifier(Card(hover: hover)) } }

struct ProofPill: View {
    let egress: Int
    var evidence: String?
    /// The run happened in the local simulator: show a warning instead of the proof.
    var simulated = false
    var body: some View {
        if simulated {
            HStack(spacing: 8) {
                Image(systemName: "exclamationmark.triangle.fill")
                Text("Simulated, not sealed").fontWeight(.semibold)
                Text("· no VM, no network policy").opacity(0.75)
            }
            .font(.callout).padding(.horizontal, 12).padding(.vertical, 6)
            .foregroundStyle(.orange)
            .background(Color.orange.opacity(0.16), in: Capsule())
            .help("This host is the local simulator (scripts/merion-demo-local.sh). Your file ran as an ordinary process on the host, so the connection count is not evidence. Use a Keep host with FluxVM for a sealed cell.")
        } else {
            sealedPill
        }
    }

    private var sealedPill: some View {
        HStack(spacing: 8) {
            Image(systemName: egress == 0 ? "lock.shield.fill" : "exclamationmark.shield.fill")
            Text(egress == 0 ? "0 outbound connections" : "\(egress) outbound connections").fontWeight(.semibold)
            if let evidence { Text("· \(evidence)").opacity(0.75) }
        }
        .font(.callout).padding(.horizontal, 12).padding(.vertical, 6)
        .foregroundStyle(egress == 0 ? Brand.good : .red)
        .background((egress == 0 ? Brand.good : Color.red).opacity(0.14), in: Capsule())
        .help("The cell that read your file had no network. The count is what it tried to reach; the host's operator can still read a cell's memory (evidence class software-test).")
    }
}

/// The running animation: rings close around the file inside the sealed cell. Still when Reduce Motion is on.
struct SealedCellView: View {
    var fileName: String
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: reduceMotion)) { ctx in
            let t = reduceMotion ? 0 : ctx.date.timeIntervalSinceReferenceDate
            ZStack {
                ForEach(0..<3, id: \.self) { i in
                    let phase = (t * 0.6 + Double(i) / 3).truncatingRemainder(dividingBy: 1)
                    RoundedRectangle(cornerRadius: 40 - phase * 12, style: .continuous)
                        .strokeBorder(Brand.orange.opacity(1 - phase), lineWidth: 3)
                        .frame(width: 200 - phase * 60, height: 200 - phase * 60)
                }
                RoundedRectangle(cornerRadius: 30, style: .continuous).strokeBorder(Brand.orange.opacity(0.9), lineWidth: 4).frame(width: 130, height: 130)
                    .background(RoundedRectangle(cornerRadius: 30, style: .continuous).fill(Brand.orange.opacity(0.10)))
                FaceMark().stroke(Brand.gradient, style: StrokeStyle(lineWidth: 12, lineCap: .round, lineJoin: .round)).frame(width: 58, height: 58)
            }
            .frame(width: 220, height: 220)
        }
        .overlay(alignment: .bottom) { Text(fileName).font(.caption).foregroundStyle(.secondary).lineLimit(1).offset(y: 24) }
        .padding(.bottom, 28)
    }
}


// MARK: macOS 26 look
// Buttons use the system styles: Liquid Glass on macOS 26 (glass / glassProminent). The app sets its tint to the brand orange at the window root
// (SolvorApp), so controls, selections and chips match the logo instead of the system blue; `Color.accentColor` below therefore resolves to it.
extension View {
    @ViewBuilder func primaryButton() -> some View {
        self.buttonStyle(.glassProminent).buttonBorderShape(.capsule)
    }
    @ViewBuilder func secondaryButton() -> some View {
        self.buttonStyle(.glass).buttonBorderShape(.capsule)
    }
}

/// A filled icon tile in the app's tint (the brand orange, set at the window root).
struct AccentTile: View {
    let symbol: String
    var size: CGFloat = 40
    var body: some View {
        Image(systemName: symbol).font(.system(size: size * 0.45, weight: .semibold)).foregroundStyle(.white)
            .frame(width: size, height: size).background(Color.accentColor.gradient, in: RoundedRectangle(cornerRadius: size * 0.27, style: .continuous))
    }
}
