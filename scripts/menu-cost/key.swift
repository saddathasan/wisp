// key <virtual keycode> [cmd]: posts one key press (125 down, 126 up, 124 right, 123 left, 36 return, 53 escape, 12 q).
import CoreGraphics
import Foundation
let a = CommandLine.arguments
let src = CGEventSource(stateID: .hidSystemState)
for down in [true, false] {
    let e = CGEvent(keyboardEventSource: src, virtualKey: CGKeyCode(a[1])!, keyDown: down)!
    if a.count > 2 { e.flags = .maskCommand }
    e.post(tap: .cghidEventTap); usleep(40_000)
}
