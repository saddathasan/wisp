// mv <x> <y>: moves the pointer (no click), for hover checks.
import CoreGraphics
let a = CommandLine.arguments
CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: CGPoint(x: Double(a[1])!, y: Double(a[2])!), mouseButton: .left)!.post(tap: .cghidEventTap)
