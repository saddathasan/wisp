# dyld_diff.py <log> <main pid>: images the main process loads between its first "open 1" and "gone 1" driver lines
# (DYLD_PRINT_LIBRARIES lines interleave with the driver's), and every image the variant g helper loads in total.
import re, sys
L, P = sys.argv[1], sys.argv[2]
lines = open(L, errors='replace').read().splitlines()
start = next(i for i, l in enumerate(lines) if re.match(rf'[\d.]+ {P} open 1$', l))
end = next(i for i, l in enumerate(lines) if re.match(rf'[\d.]+ {P} gone 1 ', l))
img = lambda l: re.sub(r'^dyld\[\d+\]: (<[^>]*> )?', '', l).rsplit('/', 1)[-1]
main = [l for l in lines if l.startswith(f'dyld[{P}]')]
during = [img(l) for l in lines[start:end] if l.startswith(f'dyld[{P}]')]
print(f"main: launch {len(main) - len(during)} images; first open loaded {len(during)}: {' '.join(during)}")
helpers = sorted({m.group(1) for l in lines for m in [re.match(r'dyld\[(\d+)\]', l)] if m and m.group(1) != P})
for h in helpers:
    hl = [img(l) for l in lines if l.startswith(f'dyld[{h}]')]
    key = [x for x in hl if re.search(r'SwiftUI|MenuBar|Glass|ColorSync|AttributeGraph', x)]
    print(f"helper {h}: {len(hl)} images; of note: {' '.join(key)}")
