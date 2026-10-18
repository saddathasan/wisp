// haswin <pid>: prints how many layer-101 (menu) windows the pid has on screen.
import CoreGraphics
let pid = Int(CommandLine.arguments[1])!
let l = CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID) as! [[String: Any]]
print(l.filter { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowLayer as String] as? Int) == 101 }.count)
