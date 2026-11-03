import AppKit
import SwiftUI

// The building blocks every Solvor screen is made from, so a screen is assembled from a few named pieces instead of ad-hoc sizes and colours.
// Nothing here talks to a host. Motion is opt-out: anything that moves checks Reduce Motion.

/// Spacing on a 4-point grid.
enum Space {
    static let xxs: CGFloat = 4, xs: CGFloat = 8, s: CGFloat = 12, m: CGFloat = 16, l: CGFloat = 24, xl: CGFloat = 32, xxl: CGFloat = 48
}

/// Corner radii (continuous corners throughout).
enum Radius {
    static let chip: CGFloat = 8, card: CGFloat = 16, panel: CGFloat = 22, sheet: CGFloat = 28
}

/// The type scale. SF Pro with rounded headlines for the brand moments, the system text styles elsewhere so Dynamic Type-style scaling keeps working.
enum Typo {
    static let hero = Font.system(size: 38, weight: .bold, design: .rounded)
    static let title = Font.system(size: 24, weight: .bold, design: .rounded)
    static let heading = Font.title3.weight(.semibold)
    static let body = Font.body
    static let callout = Font.callout
    static let caption = Font.caption
    static let mono = Font.system(.callout, design: .monospaced)
}

/// What a status chip is saying. The colour never carries the meaning alone: each state also has a symbol and a word.
enum StatusKind: String, CaseIterable {
    case running, waiting, done, failed, simulated, info

    var symbol: String {
        switch self {
        case .running: return "arrow.triangle.2.circlepath"
        case .waiting: return "hourglass"
        case .done: return "checkmark.circle.fill"
        case .failed: return "xmark.octagon.fill"
        case .simulated: return "exclamationmark.triangle.fill"
        case .info: return "info.circle.fill"
        }
    }

    var defaultLabel: String {
        switch self {
        case .running: return "Working"
        case .waiting: return "Waiting for you"
        case .done: return "Done"
        case .failed: return "Failed"
        case .simulated: return "Simulated"
        case .info: return "Note"
        }
    }

    var color: Color {
        switch self {
        case .running: return Brand.orange
        case .waiting: return .orange
        case .done: return Brand.good
        case .failed: return .red
        case .simulated: return .orange
        case .info: return .secondary
        }
    }
}

struct StatusChip: View {
    let kind: StatusKind
    var label: String?
    var body: some View {
        Label {
            Text(label ?? kind.defaultLabel)
        } icon: {
            Image(systemName: kind.symbol)
                .symbolEffect(.rotate, isActive: kind == .running)
                .symbolEffect(.bounce, value: kind)
        }
            .font(.caption.weight(.semibold))
            .labelStyle(.titleAndIcon)
            .padding(.horizontal, Space.xs).padding(.vertical, Space.xxs)
            .foregroundStyle(kind.color)
            .background(kind.color.opacity(0.14), in: Capsule())
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(label ?? kind.defaultLabel)
    }
}

/// A screen with nothing in it yet: says what belongs here and offers the one thing to do about it.
struct EmptyState: View {
    let symbol: String
    let title: String
    let message: String
    var actionTitle: String?
    var action: (() -> Void)?
    var body: some View {
        ContentUnavailableView {
            Label(title, systemImage: symbol).symbolEffect(.pulse, options: .repeating)
        } description: {
            Text(message)
        } actions: {
            if let actionTitle, let action { Button(actionTitle, action: action).primaryButton() }
        }
    }
}

/// A section title with an optional trailing accessory (a count, a button).
struct SectionHeader<Accessory: View>: View {
    let title: String
    var subtitle: String?
    @ViewBuilder var accessory: () -> Accessory
    init(_ title: String, subtitle: String? = nil, @ViewBuilder accessory: @escaping () -> Accessory) {
        self.title = title; self.subtitle = subtitle; self.accessory = accessory
    }
    var body: some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 2) {
                Text(title).font(Typo.heading)
                if let subtitle { Text(subtitle).font(Typo.caption).foregroundStyle(.secondary) }
            }
            Spacer()
            accessory()
        }
    }
}
extension SectionHeader where Accessory == EmptyView {
    init(_ title: String, subtitle: String? = nil) { self.init(title, subtitle: subtitle) { EmptyView() } }
}

/// Glass shapes that sit next to each other blend and morph as one surface on macOS 26. Wrap them in this instead of a bare stack.
struct GlassGroup<Content: View>: View {
    var spacing: CGFloat = Space.s
    @ViewBuilder var content: () -> Content
    var body: some View { GlassEffectContainer(spacing: spacing) { content() } }
}

/// Haptic feedback on the trackpad. Behind a protocol so tests can watch what would have been felt.
protocol HapticPerformer { func perform(_ pattern: NSHapticFeedbackManager.FeedbackPattern) }
struct SystemHaptics: HapticPerformer {
    func perform(_ pattern: NSHapticFeedbackManager.FeedbackPattern) {
        NSHapticFeedbackManager.defaultPerformer.perform(pattern, performanceTime: .default)
    }
}

enum Haptics {
    /// Swapped in tests.
    nonisolated(unsafe) static var performer: HapticPerformer = SystemHaptics()
    /// Something finished well (a sealed result appears).
    static func success() { performer.perform(.levelChange) }
    /// A choice landed (a drop was accepted, a switch flipped).
    static func tick() { performer.perform(.alignment) }
}

extension Pane {
    /// Each sidebar entry has its own colour, so the sidebar reads as a set of places, not a column of identical grey icons.
    var tint: Color {
        switch self {
        case .home: return Brand.blue
        case .useCases: return Color(red: 0.36, green: 0.55, blue: 1.0)
        case .runs: return Color(red: 0.62, green: 0.45, blue: 0.95)
        case .approvals: return Brand.orange
        case .goals: return Color(red: 0.24, green: 0.78, blue: 0.62)
        case .memory: return Color(red: 0.95, green: 0.4, blue: 0.55)
        case .done: return Brand.good
        case .folders: return Color(red: 0.85, green: 0.7, blue: 0.2)
        case .settings: return .gray
        }
    }
}
