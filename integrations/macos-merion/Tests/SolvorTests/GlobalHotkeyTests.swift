import AppKit
import XCTest
@testable import Solvor

final class GlobalHotkeyTests: XCTestCase {
    private func key(_ code: UInt16, _ modifiers: NSEvent.ModifierFlags) -> NSEvent {
        NSEvent.keyEvent(
            with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: 0, windowNumber: 0,
            context: nil, characters: " ", charactersIgnoringModifiers: " ", isARepeat: false, keyCode: code
        )!
    }

    func testMatchesExactlyOptionSpaceAndIgnoresIrrelevantModifiers() {
        let hotkey = GlobalHotkey(keyCode: 49, modifiers: .option)
        XCTAssertTrue(hotkey.matches(key(49, .option)))
        XCTAssertTrue(hotkey.matches(key(49, [.option, .capsLock])), "caps lock is not a relevant modifier")
        XCTAssertFalse(hotkey.matches(key(49, [])), "no modifier at all")
        XCTAssertFalse(hotkey.matches(key(49, [.option, .shift])), "an extra relevant modifier changes the combo")
        XCTAssertFalse(hotkey.matches(key(49, .command)), "the wrong modifier")
        XCTAssertFalse(hotkey.matches(key(36, .option)), "the wrong key (return, not space)")
    }

    func testADifferentComboIsConfigurable() {
        let hotkey = GlobalHotkey(keyCode: 0, modifiers: [.command, .shift])
        XCTAssertTrue(hotkey.matches(key(0, [.command, .shift])))
        XCTAssertFalse(hotkey.matches(key(0, .command)))
    }

    func testStartingAndStoppingRegistersAndRemovesBothMonitorsWithoutCrashing() {
        let hotkey = GlobalHotkey()
        hotkey.start {}
        hotkey.stop()
        hotkey.stop()   // stopping twice, or before ever starting, must not crash
        let another = GlobalHotkey()
        another.stop()
    }
}
