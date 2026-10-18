# Dropdown run 2026-10-18-fafc024

Open latency and footprint of the dropdown helper ([METHOD.md](../METHOD.md), "Dropdown: open latency and
footprint") · note: **activity unknown: started by an agent; the owner's use of the machine was not observed**

- Git: `fafc024` on `ms/3-helper-dropdown` with `scripts/open-latency.patch` applied, bundled with `scripts/bundle.sh`
- Machine: Apple M5, 32 GB RAM, macOS 27.0.1 (26A434); 2 displays (2560 × 1664 built-in, 3440 × 1440 external)
- Two fresh launches of `target/Wisp.app`, 5 opens each, 3 s hold, 5 s gap; dark menu bar
  (`NSAppearanceNameVibrantDark` passed to the helper)

## Results

| | Run 1 | Run 2 |
|---|---|---|
| Open latency, click → menu visible (ms) | 133.5, 141.8, 133.4, 150.6, 144.9 | 133.4, 130.4, 138.0, 142.9, 139.0 |
| of which process start, click → `popUp` (ms) | 69.3, 77.6, 67.3, 84.4, 75.7 | 75.2, 73.2, 77.8, 86.8, 72.2 |
| Helper peak footprint while open (MiB) | 18.31, 18.39, 18.22, 18.22, 18.39 | 17.97, 18.08, 18.17, 18.19, 18.09 |
| Main footprint before → after 5 opens | 14435144 → 14451528 B (Δ +16 KiB) | 14402400 → 14435144 B (Δ +32 KiB) |
| Main `__TEXT` images before → after | 711 → 711, identical | 711 → 711, identical |

Open latency 130–151 ms (median 138.5 ms), about half of it process start. The helper peaks at 18.0–18.4 MiB, all of
which goes back to the system when it exits. The main process keeps one or two 16 KiB pages after 5 opens and loads no new image: no
ServiceManagement, no SwiftUI. `heap` on the main process after run 2 lists no NSMenu or NSTask instance. Every exit
notification arrived on the main thread.

## Raw

### Run 1

```
$ cat stderr.log   # main and helpers
1792336348277.5 56001 click 1
1792336348346.8 56599 popUp
1792336348411.0 56599 visible
1792336351884.1 56599 closed
1792336351889.6 56001 exit 1 helper 56599 status 0 main_thread true
1792336356890.7 56001 click 2
1792336356968.3 57125 popUp
1792336357032.5 57125 visible
1792336360505.8 57125 closed
1792336360512.8 56001 exit 2 helper 57125 status 0 main_thread true
1792336365514.0 56001 click 3
1792336365581.3 57625 popUp
1792336365647.4 57625 visible
1792336369122.1 57625 closed
1792336369128.9 56001 exit 3 helper 57625 status 0 main_thread true
1792336374130.1 56001 click 4
1792336374214.5 58178 popUp
1792336374280.7 58178 visible
1792336377757.4 58178 closed
1792336377763.9 56001 exit 4 helper 58178 status 0 main_thread true
1792336382764.5 56001 click 5
1792336382840.2 58690 popUp
1792336382909.4 58690 visible
1792336386388.8 58690 closed
1792336386396.6 56001 exit 5 helper 58690 status 0 main_thread true
1792336386396.7 56001 done
$ cat notes.txt    # helper pids and arguments as seen by ps (run 2)
main 56001
open 1 helper 56599
open 2 helper 57125
open 3 helper 57625
open 4 helper 58178
open 5 helper 58690
finished
$ footprint -f bytes --noCategories   # main-before
wisp [56001]: 64-bit    Footprint: 14435144 B (16384 bytes per page)
    phys_footprint: 14435144 B
    phys_footprint_peak: 15549256 B
$ footprint -f bytes --noCategories   # helper1
wisp [56599]: 64-bit    Footprint: 18662336 B (16384 bytes per page)
    phys_footprint: 18662336 B
    phys_footprint_peak: 19203008 B
$ footprint -f bytes --noCategories   # helper2
wisp [57125]: 64-bit    Footprint: 19284928 B (16384 bytes per page)
    phys_footprint: 19284928 B
    phys_footprint_peak: 19284928 B
$ footprint -f bytes --noCategories   # helper3
wisp [57625]: 64-bit    Footprint: 18990016 B (16384 bytes per page)
    phys_footprint: 18990016 B
    phys_footprint_peak: 19104704 B
$ footprint -f bytes --noCategories   # helper4
wisp [58178]: 64-bit    Footprint: 19104704 B (16384 bytes per page)
    phys_footprint: 19104704 B
    phys_footprint_peak: 19104704 B
$ footprint -f bytes --noCategories   # helper5
wisp [58690]: 64-bit    Footprint: 19284928 B (16384 bytes per page)
    phys_footprint: 19284928 B
    phys_footprint_peak: 19284928 B
$ footprint -f bytes --noCategories   # main-after
wisp [56001]: 64-bit    Footprint: 14451528 B (16384 bytes per page)
    phys_footprint: 14451528 B
    phys_footprint_peak: 15549256 B
$ diff <(vmmap main | __TEXT images, before) <(after)
(no difference; 711 images)
```

### Run 2

```
$ cat stderr.log   # main and helpers
1792336955988.0 68420 click 1
1792336956063.2 68959 popUp
1792336956121.4 68959 visible
1792336959610.5 68959 closed
1792336959618.9 68420 exit 1 helper 68959 status 0 main_thread true
1792336964620.0 68420 click 2
1792336964693.2 69536 popUp
1792336964750.4 69536 visible
1792336968238.0 69536 closed
1792336968245.5 68420 exit 2 helper 69536 status 0 main_thread true
1792336973246.7 68420 click 3
1792336973324.5 70083 popUp
1792336973384.7 70083 visible
1792336976869.9 70083 closed
1792336976874.6 68420 exit 3 helper 70083 status 0 main_thread true
1792336981875.1 68420 click 4
1792336981961.9 70596 popUp
1792336982018.0 70596 visible
1792336985505.1 70596 closed
1792336985512.4 68420 exit 4 helper 70596 status 0 main_thread true
1792336990513.6 68420 click 5
1792336990585.8 71112 popUp
1792336990652.6 71112 visible
1792336994134.1 71112 closed
1792336994140.5 68420 exit 5 helper 71112 status 0 main_thread true
1792336994140.6 68420 done
$ cat notes.txt    # helper pids and arguments as seen by ps (run 2)
main 68420
open 1 helper 68959: /Users/saddat/codeRepository/wisp/target/Wisp.app/Contents/MacOS/wisp --dropdown 914 1082 NSAppearanceNameVibrantDark
open 2 helper 69536: /Users/saddat/codeRepository/wisp/target/Wisp.app/Contents/MacOS/wisp --dropdown 914 1082 NSAppearanceNameVibrantDark
open 3 helper 70083: /Users/saddat/codeRepository/wisp/target/Wisp.app/Contents/MacOS/wisp --dropdown 914 1082 NSAppearanceNameVibrantDark
open 4 helper 70596: /Users/saddat/codeRepository/wisp/target/Wisp.app/Contents/MacOS/wisp --dropdown 914 1082 NSAppearanceNameVibrantDark
open 5 helper 71112: /Users/saddat/codeRepository/wisp/target/Wisp.app/Contents/MacOS/wisp --dropdown 914 1082 NSAppearanceNameVibrantDark
finished
$ footprint -f bytes --noCategories   # main-before
wisp [68420]: 64-bit    Footprint: 14402400 B (16384 bytes per page)
    phys_footprint: 14402400 B
    phys_footprint_peak: 15713120 B
$ footprint -f bytes --noCategories   # helper1
wisp [68959]: 64-bit    Footprint: 18351040 B (16384 bytes per page)
    phys_footprint: 18351040 B
    phys_footprint_peak: 18842560 B
$ footprint -f bytes --noCategories   # helper2
wisp [69536]: 64-bit    Footprint: 18957248 B (16384 bytes per page)
    phys_footprint: 18957248 B
    phys_footprint_peak: 18957248 B
$ footprint -f bytes --noCategories   # helper3
wisp [70083]: 64-bit    Footprint: 19055576 B (16384 bytes per page)
    phys_footprint: 19055576 B
    phys_footprint_peak: 19055576 B
$ footprint -f bytes --noCategories   # helper4
wisp [70596]: 64-bit    Footprint: 19071936 B (16384 bytes per page)
    phys_footprint: 19071936 B
    phys_footprint_peak: 19071936 B
$ footprint -f bytes --noCategories   # helper5
wisp [71112]: 64-bit    Footprint: 18973656 B (16384 bytes per page)
    phys_footprint: 18973656 B
    phys_footprint_peak: 18973656 B
$ footprint -f bytes --noCategories   # main-after
wisp [68420]: 64-bit    Footprint: 14435144 B (16384 bytes per page)
    phys_footprint: 14435144 B
    phys_footprint_peak: 15713120 B
$ diff <(vmmap main | __TEXT images, before) <(after)
(no difference; 711 images)
```

### Main process heap after run 2

```
$ heap <main pid> | grep -E 'NSMenu|NSTask|NSConcreteTask'
       1        128     128.0   NSMenuBarPresentationInstance                     ObjC    AppKit
       1        112     112.0   NSMenuBarNavigationSceneExtension.ClientComponent  Swift   AppKit
       1         32      32.0   NSMenuBarDisplayManager                           ObjC    AppKit
       1         32      32.0   NSMenuBarWindowManager                            ObjC    AppKit
```

These are the menu bar's own objects, which any status item has; there is no `NSMenu`.
