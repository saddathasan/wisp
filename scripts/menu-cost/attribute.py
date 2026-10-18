# Apportions each malloc region's dirty+swapped growth (before -> after vmmap -noCoalesce) to the new allocations that live in it,
# bucketed by leaf-most non-generic library of their allocation stack (malloc_history ALLOC lines).
# usage: attribute.py before.vmmap after.vmmap new-stacks.txt
import re, sys, collections, bisect
KB = {'K': 1024, 'M': 1 << 20, 'G': 1 << 30}
def size(s): return float(s[:-1]) * KB[s[-1]] if s[-1] in KB else float(s)
def regions(p):
    out = {}
    for l in open(p):
        m = re.match(r'(.{26})\s*([0-9a-f]+)-([0-9a-f]+)\s+\[\s*(\S+)\s+(\S+)\s+(\S+)\s+(\S+)\]', l)
        if m and m.group(1).strip().startswith('Malloc'):
            out[int(m.group(2), 16)] = (int(m.group(3), 16), size(m.group(6)) + size(m.group(7)), m.group(1).strip())
    return out
b, a = regions(sys.argv[1]), regions(sys.argv[2])
starts = sorted(a)
GENERIC = {'libsystem_malloc.dylib', 'libsystem_c.dylib', 'libc++.1.dylib', 'libc++abi.dylib', 'com.apple.CoreFoundation', 'com.apple.Foundation',
           'libdispatch.dylib', 'libsystem_blocks.dylib', 'libsystem_pthread.dylib', 'libsystem_kernel.dylib', 'libsystem_platform.dylib', 'libobjc.A.dylib'}
MERGE = {'com.apple.CoreGraphics': None, 'com.apple.vImage': 'ColorSync/vImage', 'libCGInterfaces.dylib': 'ColorSync/vImage', 'com.apple.ColorSync': 'ColorSync/vImage'}
CAUSES = [('ColorSync/vImage colour transforms', r'ColorSync|vImage|CGCMS|ColorConversion|libCGInterfaces'),
          ('SF Symbols/CoreUI/CoreSVG', r'CoreSVG|com\.apple\.coreui|SFSymbols|NSSymbolImage|CUI'),
          ('SwiftUI/AttributeGraph/Glass hosting', r'SwiftUI|AttributeGraph|NSGlassEffectView|DesignLibrary|Glass|NSHostingView|NSCoreHostingView|ViewGraph'),
          ('MenuBar XPC session', r'MenuBarClientCore|BaseBoard|BoardServices|libxpc|MBMenuBar'),
          ('CoreText/fonts', r'CoreText|libFontParser|FontRegistry|NSFont|CTFont')]
def cause(stack):
    # Nearest cause wins: the matching frame closest to the leaf, so an allocation made by SwiftUI deep inside the
    # MenuBar XPC callback is charged to SwiftUI, not to the XPC entry point every menu-open stack shares.
    b = bucket(stack)
    if b in ('objc runtime (method caches, class data)', 'Swift runtime metadata', 'ColorSync/vImage'):
        return {'ColorSync/vImage': 'ColorSync/vImage colour transforms'}.get(b, b)
    frames = stack.split(' | ')[::-1]
    for f in frames:
        for k, r in CAUSES + [('NSMenu/AppKit menu machinery', r'NSMenu|NSContextMenu|NSTableView|NSCocoaMenuImpl|NSStatusItem|NSStatusBarButton|HIToolbox')]:
            if re.search(r, f): return k
    return 'other (' + b + ')'
def bucket(stack):
    fr = [re.match(r'0x[0-9a-f]+ \(([^)]*)\) (.*)', f.strip()) for f in stack.split(' | ')]
    fr = [(f.group(1), f.group(2)) for f in fr if f][::-1]
    if any(i in ('com.apple.ColorSync', 'com.apple.vImage') or 'CGCMS' in s or 'ColorConversion' in s for i, s in fr[:12]): return 'ColorSync/vImage'
    leaf = next((i for i, _ in fr if i != 'libsystem_malloc.dylib'), '')
    if leaf == 'libobjc.A.dylib' or leaf.startswith('libobjcMsgSend'): return 'objc runtime (method caches, class data)'
    img = next((i for i, _ in fr if i not in GENERIC and not i.startswith('libobjcMsgSend')), 'generic')
    return {'com.apple.coreui': 'CoreUI/CoreSVG/SF Symbols', 'com.apple.CoreSVG': 'CoreUI/CoreSVG/SF Symbols', 'com.apple.SFSymbolsFramework': 'CoreUI/CoreSVG/SF Symbols',
            'com.apple.DesignLibrary': 'CoreUI/CoreSVG/SF Symbols', 'libswiftCore.dylib': 'Swift runtime metadata', 'com.apple.AttributeGraph': 'SwiftUI/AttributeGraph',
            'com.apple.SwiftUICore': 'SwiftUI/AttributeGraph', 'com.apple.SwiftUI': 'SwiftUI/AttributeGraph', 'com.apple.BaseBoard': 'BaseBoard/BoardServices/XPC',
            'com.apple.BoardServices': 'BaseBoard/BoardServices/XPC', 'libxpc.dylib': 'BaseBoard/BoardServices/XPC', 'com.apple.CoreText': 'CoreText/fonts',
            'libFontParser.dylib': 'CoreText/fonts', 'libFontRegistry.dylib': 'CoreText/fonts'}.get(img, img)
alloc = collections.defaultdict(list)
for l in open(sys.argv[3]):
    m = re.match(r'ALLOC (0x[0-9a-f]+)-0x[0-9a-f]+ \[size=(\d+)\]:\s+(.*)', l)
    if not m: continue
    addr = int(m.group(1), 16); i = bisect.bisect_right(starts, addr) - 1
    if i >= 0 and addr < a[starts[i]][0]: alloc[starts[i]].append((int(m.group(2)), (cause if '--cause' in sys.argv else bucket)(m.group(3))))
charge = collections.Counter(); newbytes = collections.Counter(); unattributed = 0; grew = 0
for s, (e, d, kind) in a.items():
    # Regions are matched by start address; a region that split or merged counts against its after-state start.
    before = b.get(s, (0, 0, ''))[1]; delta = d - before
    grew += delta
    items = alloc.get(s, [])
    for sz, bk in items: newbytes[bk] += sz
    tot = sum(sz for sz, _ in items)
    if delta == 0: continue
    if tot == 0: unattributed += delta; continue
    for sz, bk in items: charge[bk] += delta * sz / tot
print(f"malloc dirty+swapped growth over all regions: {grew/1e6:.2f} MB  (unattributed: {unattributed/1e6:.2f} MB)")
print(f"{'charged MB':>10} {'new alloc MB':>12}  bucket")
for k, v in charge.most_common(): print(f"{v/1e6:>10.2f} {newbytes[k]/1e6:>12.2f}  {k}")
