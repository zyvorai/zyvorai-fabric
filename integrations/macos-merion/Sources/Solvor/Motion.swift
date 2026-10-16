import SwiftUI

// The animation kit. Everything that moves in Solvor comes from here, so it feels like one hand made it, and so a person who turns on Reduce Motion
// gets a calm, complete app: every effect below checks `accessibilityReduceMotion` and settles to its end state at once when it is on.
// The maths (where a blob is, where a spark goes) is in plain functions so it can be unit-tested without a screen.

enum Motion {
    static let spring = Animation.spring(response: 0.45, dampingFraction: 0.78)
    static let bouncy = Animation.spring(response: 0.5, dampingFraction: 0.58)
    static let gentle = Animation.easeInOut(duration: 0.7)

    /// The delay for the i-th item of a list that appears one after another. Capped, so a long list never waits seconds for its last row.
    static func stagger(_ index: Int, step: Double = 0.055, cap: Int = 14) -> Double { Double(max(0, min(index, cap))) * step }

    /// Where a soft colour blob of the aurora sits at time `t` (seconds), as fractions of the width and height, always inside the frame.
    static func blob(_ index: Int, at t: Double) -> (x: Double, y: Double) {
        let i = Double(index)
        let x = 0.5 + 0.34 * sin(t * (0.11 + 0.037 * i) + i * 2.1)
        let y = 0.5 + 0.30 * cos(t * (0.09 + 0.041 * i) + i * 1.3)
        return (x, y)
    }
}

// MARK: entrances

/// Fades, lifts and un-blurs a view into place. `delay` staggers a list. Instant with Reduce Motion.
struct Appear: ViewModifier {
    var delay: Double = 0
    var rise: CGFloat = 14
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var shown = false
    func body(content: Content) -> some View {
        content
            .opacity(shown ? 1 : 0)
            .offset(y: shown ? 0 : rise)
            .blur(radius: shown ? 0 : 6)
            .scaleEffect(shown ? 1 : 0.98)
            .onAppear {
                if reduceMotion { shown = true } else { withAnimation(Motion.spring.delay(delay)) { shown = true } }
            }
    }
}

/// A proof or a seal arriving: it lands from a little larger with a bounce, like a stamp.
struct Stamp: ViewModifier {
    var delay: Double = 0.15
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var landed = false
    func body(content: Content) -> some View {
        content
            .scaleEffect(landed ? 1 : 1.45)
            .opacity(landed ? 1 : 0)
            .rotationEffect(.degrees(landed ? 0 : -4))
            .onAppear {
                if reduceMotion { landed = true } else { withAnimation(Motion.bouncy.delay(delay)) { landed = true } }
            }
    }
}

extension View {
    func appear(delay: Double = 0, rise: CGFloat = 14) -> some View { modifier(Appear(delay: delay, rise: rise)) }
    func stamp(delay: Double = 0.15) -> some View { modifier(Stamp(delay: delay)) }
}

// MARK: loops

/// A slow, gentle scale pulse for things that are alive and waiting.
struct Breathe: ViewModifier {
    var amount: CGFloat = 0.04
    var period: Double = 2.4
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var on = false
    func body(content: Content) -> some View {
        content
            .scaleEffect(on ? 1 + amount : 1)
            .onAppear { if !reduceMotion { withAnimation(.easeInOut(duration: period).repeatForever(autoreverses: true)) { on = true } } }
    }
}

/// A soft sweep of light across a view, for "working on it".
struct Shimmer: ViewModifier {
    var active = true
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    func body(content: Content) -> some View {
        if active && !reduceMotion {
            TimelineView(.animation(minimumInterval: 1 / 30)) { ctx in
                let t = ctx.date.timeIntervalSinceReferenceDate
                let phase = (t.truncatingRemainder(dividingBy: 1.6)) / 1.6
                content.overlay {
                    GeometryReader { geo in
                        LinearGradient(colors: [.clear, .white.opacity(0.55), .clear], startPoint: .leading, endPoint: .trailing)
                            .frame(width: geo.size.width * 0.5)
                            .offset(x: (phase * 1.6 - 0.5) * geo.size.width)
                            .blendMode(.plusLighter)
                    }
                    .mask(content)
                    .allowsHitTesting(false)
                }
            }
        } else { content }
    }
}

extension View {
    func breathe(amount: CGFloat = 0.04, period: Double = 2.4) -> some View { modifier(Breathe(amount: amount, period: period)) }
    func shimmer(_ active: Bool = true) -> some View { modifier(Shimmer(active: active)) }
}

// MARK: aurora

/// Slow-drifting soft colour behind a screen: the brand orange and its neighbours. Still, and just as pretty, with Reduce Motion.
struct AuroraBackground: View {
    var intensity: Double = 1
    /// The default is Solvor's warm palette (used for the welcome and use-cases screens); pass a different one where orange
    /// should not appear at all, such as behind the chat.
    var palette: [Color] = [Brand.orange, Color(red: 1, green: 0.62, blue: 0.3), Color(red: 0.95, green: 0.35, blue: 0.55), Color(red: 0.55, green: 0.42, blue: 0.95)]
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    /// An all-blue palette, no orange anywhere in it.
    static let blue: [Color] = [Brand.blue, Brand.blueGlow, Color(red: 0.42, green: 0.55, blue: 0.95), Color(red: 0.3, green: 0.75, blue: 0.9)]

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 24, paused: reduceMotion)) { ctx in
            let t = reduceMotion ? 3 : ctx.date.timeIntervalSinceReferenceDate
            GeometryReader { geo in
                ZStack {
                    ForEach(0..<palette.count, id: \.self) { i in
                        let p = Motion.blob(i, at: t)
                        Circle()
                            .fill(palette[i].opacity(0.22 * intensity))
                            .frame(width: max(geo.size.width, geo.size.height) * 0.55)
                            .blur(radius: 70)
                            .position(x: p.x * geo.size.width, y: p.y * geo.size.height)
                    }
                }
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

// MARK: sparks

/// One spark of a burst. Deterministic for a given seed, so tests can check them.
struct Spark: Equatable {
    var angle: Double, speed: Double, size: Double, delay: Double, hue: Int
}

enum SparkField {
    /// `count` sparks flying outward in a ring, from a fixed seed (a simple linear congruential sequence, so no two runs differ).
    static func make(seed: UInt64, count: Int) -> [Spark] {
        var s = seed &* 6364136223846793005 &+ 1442695040888963407
        func next() -> Double { s = s &* 6364136223846793005 &+ 1442695040888963407; return Double(s >> 11) / Double(1 << 53) }
        return (0..<count).map { i in
            Spark(angle: (Double(i) / Double(max(count, 1))) * 2 * .pi + next() * 0.4, speed: 90 + next() * 110, size: 3 + next() * 5, delay: next() * 0.12, hue: Int(next() * 3))
        }
    }

    /// Position (from the centre), size scale and opacity of a spark `t` seconds after the burst. Everything fades out by t = 1.1.
    static func state(_ p: Spark, t: Double) -> (dx: Double, dy: Double, scale: Double, opacity: Double) {
        let u = max(0, min(1, (t - p.delay) / 1.0))
        let ease = 1 - pow(1 - u, 3)
        return (cos(p.angle) * p.speed * ease, sin(p.angle) * p.speed * ease + 26 * u * u, 1 - u * 0.7, u <= 0 ? 0 : 1 - pow(u, 2))
    }
}

/// A short burst of sparks from the centre each time `trigger` changes. Nothing at all with Reduce Motion.
struct SparkBurst: View {
    var trigger: Int
    var count = 26
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var startedAt: Date?
    private let colors: [Color] = [Brand.orange, Brand.glow, .white]

    var body: some View {
        Group {
            if !reduceMotion, let startedAt {
                TimelineView(.animation) { ctx in
                    let t = ctx.date.timeIntervalSince(startedAt)
                    Canvas { gc, size in
                        for p in SparkField.make(seed: UInt64(trigger), count: count) {
                            let st = SparkField.state(p, t: t)
                            guard st.opacity > 0.01 else { continue }
                            let c = CGPoint(x: size.width / 2 + st.dx, y: size.height / 2 + st.dy)
                            let r = p.size * st.scale
                            gc.fill(Path(ellipseIn: CGRect(x: c.x - r, y: c.y - r, width: r * 2, height: r * 2)), with: .color(colors[p.hue % colors.count].opacity(st.opacity)))
                        }
                    }
                }
            }
        }
        .allowsHitTesting(false)
        .accessibilityHidden(true)
        .onChange(of: trigger) { _, _ in startedAt = Date(); Task { try? await Task.sleep(nanoseconds: 1_300_000_000); startedAt = nil } }
        .onAppear { if trigger > 0 { startedAt = Date(); Task { try? await Task.sleep(nanoseconds: 1_300_000_000); startedAt = nil } } }
    }
}

// MARK: small pieces

/// Three dots that bounce in turn: "the agent is answering".
struct TypingDots: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 30, paused: reduceMotion)) { ctx in
            let t = reduceMotion ? 0 : ctx.date.timeIntervalSinceReferenceDate
            HStack(spacing: 5) {
                ForEach(0..<3, id: \.self) { i in
                    Circle().fill(.secondary).frame(width: 7, height: 7).offset(y: -4 * max(0, sin(t * 6 - Double(i) * 0.7)))
                }
            }
        }
        .accessibilityLabel("Working")
    }
}

/// The Solvor mark, drawn on: orange rings pulse outward, the squircle settles, the face is traced stroke by stroke, and it blinks
/// once it has settled — a small sign of life, like the animated presence at the top of a chat while an agent works. Still with
/// Reduce Motion (no rings, no blink; the face is simply there).
struct AnimatedLogo: View {
    var size: CGFloat = 88
    /// The rings and the small spark dot. Default is Zyvor's orange (Welcome, About); pass `Brand.blue` where orange must not appear.
    var accent: Color = Brand.orange
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var settled = false
    @State private var trace: CGFloat = 0
    @State private var blink = false
    @State private var blinkTask: Task<Void, Never>?
    var body: some View {
        ZStack {
            if !reduceMotion {
                TimelineView(.animation(minimumInterval: 1 / 30)) { ctx in
                    let t = ctx.date.timeIntervalSinceReferenceDate
                    ZStack {
                        ForEach(0..<3, id: \.self) { i in
                            let phase = (t * 0.45 + Double(i) / 3).truncatingRemainder(dividingBy: 1)
                            RoundedRectangle(cornerRadius: size * 0.23 + phase * size * 0.2, style: .continuous)
                                .strokeBorder(accent.opacity(0.5 * (1 - phase)), lineWidth: 2)
                                .frame(width: size * (1 + phase * 0.9), height: size * (1 + phase * 0.9))
                        }
                    }
                }
            }
            ZStack {
                RoundedRectangle(cornerRadius: size * 0.23, style: .continuous).fill(Brand.gradient)
                RoundedRectangle(cornerRadius: size * 0.23, style: .continuous).strokeBorder(.white.opacity(0.25), lineWidth: 1)
                FaceMark().trim(from: 0, to: reduceMotion ? 1 : trace)
                    .stroke(.white, style: StrokeStyle(lineWidth: size * 0.11, lineCap: .round, lineJoin: .round)).padding(size * 0.24)
                    .scaleEffect(x: 1, y: blink ? 0.12 : 1, anchor: .init(x: 0.5, y: 0.42))
                Circle().fill(accent).frame(width: size * 0.16, height: size * 0.16)
                    .overlay(Circle().strokeBorder(.white.opacity(0.4), lineWidth: 0.5))
                    .offset(x: size * 0.30, y: -size * 0.32)
            }
            .frame(width: size, height: size)
            .shadow(color: Brand.blueDeep.opacity(0.35), radius: size * 0.12, y: size * 0.06)
            .scaleEffect(settled || reduceMotion ? 1 : 0.6)
            .opacity(settled || reduceMotion ? 1 : 0)
        }
        .frame(width: size * 1.9, height: size * 1.9)
        .onAppear {
            guard !reduceMotion else { return }
            withAnimation(Motion.bouncy) { settled = true }
            withAnimation(.easeInOut(duration: 1.1).delay(0.25)) { trace = 1 }
            blinkTask = Task {
                try? await Task.sleep(nanoseconds: 1_600_000_000)
                while !Task.isCancelled {
                    withAnimation(.easeInOut(duration: 0.09)) { blink = true }
                    try? await Task.sleep(nanoseconds: 90_000_000)
                    withAnimation(.easeInOut(duration: 0.12)) { blink = false }
                    try? await Task.sleep(nanoseconds: UInt64.random(in: 2_600_000_000...4_200_000_000))
                }
            }
        }
        .onDisappear { blinkTask?.cancel() }
        .accessibilityLabel("Solvor")
    }
}
