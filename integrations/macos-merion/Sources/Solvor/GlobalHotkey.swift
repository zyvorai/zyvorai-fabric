import AppKit

/// A single global key combo (⌥Space by default) that calls `action` whether or not Solvor is the frontmost app.
/// Built on `NSEvent`'s global and local monitors — no Accessibility permission needed, unlike posting synthetic
/// events. The local monitor is what makes it also work while one of Solvor's own windows has key focus, which a
/// Spotlight-style shortcut needs; it swallows the event so the space does not also land in a focused text field.
final class GlobalHotkey {
    let keyCode: UInt16
    let modifiers: NSEvent.ModifierFlags
    private var globalMonitor: Any?
    private var localMonitor: Any?

    /// The modifiers a hotkey combo can be built from; anything else (caps lock, fn, a mouse button) is ignored
    /// when deciding whether an event matches, the same way most global-hotkey pickers behave.
    static let relevantModifiers: NSEvent.ModifierFlags = [.command, .option, .control, .shift]

    init(keyCode: UInt16 = 49 /* Space */, modifiers: NSEvent.ModifierFlags = .option) {
        self.keyCode = keyCode
        self.modifiers = modifiers.intersection(Self.relevantModifiers)
    }

    /// Pure, so it is unit-tested without registering anything: whether `event`'s key and modifiers are exactly
    /// this combo's.
    func matches(_ event: NSEvent) -> Bool {
        event.keyCode == keyCode
            && event.modifierFlags.intersection(Self.relevantModifiers) == modifiers
    }

    func start(_ action: @escaping () -> Void) {
        stop()
        globalMonitor = NSEvent.addGlobalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, self.matches(event) else { return }
            DispatchQueue.main.async(execute: action)
        }
        localMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self, self.matches(event) else { return event }
            DispatchQueue.main.async(execute: action)
            return nil
        }
    }

    func stop() {
        if let m = globalMonitor { NSEvent.removeMonitor(m) }
        if let m = localMonitor { NSEvent.removeMonitor(m) }
        globalMonitor = nil
        localMonitor = nil
    }

    deinit { stop() }
}
