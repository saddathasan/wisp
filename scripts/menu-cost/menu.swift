// Opens/closes a process's status item menu with synthetic CGEvents. Never touches the target's AX tree:
// on this macOS the status item has no window of its own (MenuBarAgent hosts the bar), so the caller passes the
// item's global point (read off a screencapture of the bar); success is checked by the target's own menu window
// (layer 101, owned by its pid) appearing in CGWindowList.
// usage: menu open <pid> <x> <y> | menu close <pid> | menu cycle <pid> <x> <y> <n>
import CoreGraphics
import Foundation

let args = CommandLine.arguments
let pid = Int(args[2])!

func menuWindow() -> CGRect? {
    let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
    guard let w = list.first(where: { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 101 }) else { return nil }
    return CGRect(dictionaryRepresentation: w[kCGWindowBounds as String] as! CFDictionary)
}
func now() -> Double { Date().timeIntervalSince1970 * 1000 }
/// Polls every 2 ms until the menu window is (or is not) on screen; -1 on timeout.
func waitFor(_ open: Bool, _ timeoutMs: Double = 3000) -> Double {
    let t0 = now()
    while (menuWindow() != nil) != open { if now() - t0 > timeoutMs { return -1 }; usleep(2000) }
    return now() - t0
}
func post(_ t: CGEventType, _ p: CGPoint) {
    CGEvent(mouseEventSource: CGEventSource(stateID: .hidSystemState), mouseType: t, mouseCursorPosition: p, mouseButton: .left)!.post(tap: .cghidEventTap)
}
func open(_ p: CGPoint) {
    let orig = CGEvent(source: nil)!.location
    post(.mouseMoved, p); usleep(100_000)
    let t0 = now()
    post(.leftMouseDown, p); usleep(60_000); post(.leftMouseUp, p)
    let ok = waitFor(true) >= 0
    let ms = now() - t0
    post(.mouseMoved, orig)
    guard ok, let r = menuWindow() else { fputs("menu did not appear\n", stderr); exit(1) }
    print(String(format: "open: menu window %@ on screen %.0f ms after mouseDown", NSStringFromRect(r), ms))
}
func close() {
    let src = CGEventSource(stateID: .hidSystemState)
    for down in [true, false] { CGEvent(keyboardEventSource: src, virtualKey: 53, keyDown: down)!.post(tap: .cghidEventTap); usleep(30_000) }
    let ms = waitFor(false)
    if ms < 0 { fputs("menu did not close\n", stderr); exit(1) }
    print(String(format: "close: menu window gone %.0f ms after Escape", ms))
}

switch args[1] {
case "open": open(CGPoint(x: Double(args[3])!, y: Double(args[4])!))
case "close": close()
case "cycle":
    for _ in 0..<Int(args[5])! { open(CGPoint(x: Double(args[3])!, y: Double(args[4])!)); usleep(700_000); close(); usleep(700_000) }
default: fatalError("unknown command")
}
