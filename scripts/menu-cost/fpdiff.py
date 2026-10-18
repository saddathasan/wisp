# Diffs `footprint -f bytes` category tables: fpdiff.py a.txt b.txt [c.txt ...] -> dirty bytes per category per file.
import re, sys
def load(p):
    d = {}
    for line in open(p):
        m = re.match(r'\s*(\d+) B\s+(\d+) B\s+(\d+) B\s+(\d+)\s+(.+?)\s*$', line)
        if m and m.group(5) not in ('TOTAL',): d[m.group(5)] = (int(m.group(1)), int(m.group(4)))
        m = re.search(r'phys_footprint: (\d+)', line)
        if m: d['= phys_footprint'] = (int(m.group(1)), 0)
    return d
fs = [load(p) for p in sys.argv[1:]]
cats = sorted(set().union(*fs), key=lambda c: -(fs[-1].get(c, (0, 0))[0] - fs[0].get(c, (0, 0))[0]))
print(f"{'category':34}" + ''.join(f"{'f'+str(i):>12}" for i in range(len(fs))) + f"{'delta(last-first)':>20}  regions")
for c in cats:
    v = [f.get(c, (0, 0)) for f in fs]
    if all(x == v[0] for x in v): continue
    print(f"{c:34}" + ''.join(f"{x[0]:>12}" for x in v) + f"{v[-1][0]-v[0][0]:>20}  " + '/'.join(str(x[1]) for x in v))
