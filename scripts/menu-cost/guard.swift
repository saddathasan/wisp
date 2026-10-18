// guard <pid>: exit 0 only if <pid> has a menu window (layer 101) on screen and no other process does, a first check
// before synthetic keys (it cannot see where the system routes them).
import CoreGraphics
import Foundation
let pid = Int(CommandLine.arguments[1])!
let l = (CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]).filter { ($0[kCGWindowLayer as String] as? Int) == 101 }
let mine = l.filter { ($0[kCGWindowOwnerPID as String] as? Int) == pid }.count
let other = l.count - mine
if mine == 0 || other > 0 { print("guard: wisp menus \(mine), other menus \(other)"); exit(1) }
