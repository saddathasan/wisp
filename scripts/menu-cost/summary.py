# Summarises run_variant.sh output dirs: summary.py <runs dir> -> per variant footprint (MiB) per launch (run1/run2/run3),
# first/later open latency (ms, driver log: click -> dropdown window reported visible), close time, CPU closed/open.
# Variant g: latency is main's "open i" to the helper's "visible"; exit = helper's "close" to main reaping it.
import os, re, sys, statistics as st
R = sys.argv[1]
def fp(p, key='phys_footprint'):
    m = re.search(key + r': (\d+)', open(p).read()) if os.path.exists(p) else None
    return int(m.group(1)) / 2**20 if m else float('nan')
def cpu(p):
    m = re.search(r'cpu ([\d.]+)%', open(p).read()) if os.path.exists(p) else None
    return m.group(1) if m else '-'
def events(log):
    # -> [(ms, pid, event)]
    out = []
    for l in open(log, errors='replace'):
        m = re.match(r'([\d.]+) (\d+) (.*)$', l.rstrip())
        if m: out.append((float(m.group(1)), m.group(2), m.group(3)))
    return out
def lat(log):
    ev = events(log)
    if not ev: return [], [], []
    main = ev[0][1]
    opens = {int(e.split()[1]): t for t, p, e in ev if p == main and re.fullmatch(r'open \d+', e)}
    vis, gone, exits = [], [], []
    for t, p, e in ev:
        m = re.match(r'visible (\d+) ([\d.]+) ms', e)
        if m: vis.append(float(m.group(2)) if p == main else t - opens[int(m.group(1))])
        m = re.match(r'gone (\d+) ([\d.]+) ms', e)
        if m and p == main: gone.append(float(m.group(2)))
    closes = [t for t, p, e in ev if p != main and e.startswith('close')]
    reaped = [t for t, p, e in ev if p == main and e.startswith('helper') and e.endswith('reaped')]
    exits = [b - a for a, b in zip(closes, reaped)]
    return vis, gone, exits
f = lambda xs: '/'.join(f'{x:.1f}' for x in xs)
print(f"{'variant':8} {'before':>16} {'open':>16} {'after1':>16} {'after5':>11} {'first ms':>16} {'later ms med [range] (n)':>26} {'close ms':>9} {'cpu closed':>10} {'cpu open':>9}  helper")
for v in sorted(os.listdir(R)):
    d = os.path.join(R, v)
    if not os.path.isdir(d): continue
    runs = sorted(int(x[3:]) for x in os.listdir(d) if re.fullmatch(r'run\d+', x))
    cols = {k: [fp(f'{d}/run{i}/{k}.footprint.txt') for i in runs] for k in ('before', 'open', 'after1', 'after5')}
    cols['after5'] = [x for x in cols['after5'] if x == x]
    first, later, gone, exits = [], [], [], []
    for i in runs:
        vis, g, ex = lat(f'{d}/run{i}/log.txt') if os.path.exists(f'{d}/run{i}/log.txt') else ([], [], [])
        first += vis[:1]; later += vis[1:]; gone += g; exits += ex
    lt = f"{st.median(later):.0f} [{min(later):.0f}-{max(later):.0f}] ({len(later)})" if later else '-'
    hp = [fp(f'{d}/run{i}/open.helper.footprint.txt') for i in runs if os.path.exists(f'{d}/run{i}/open.helper.footprint.txt')]
    hpk = [fp(f'{d}/run{i}/open.helper.footprint.txt', 'phys_footprint_peak') for i in runs if os.path.exists(f'{d}/run{i}/open.helper.footprint.txt')]
    helper = f"open {f(hp)} peak {f(hpk)} MiB, exit {f(exits)} ms, cpu open {cpu(d + '/cpu-open-helper.txt')}" if hp else ''
    cl = f"{st.median(gone):.0f}" if gone else '-'
    print(f"{v:8} {f(cols['before']):>16} {f(cols['open']):>16} {f(cols['after1']):>16} {f(cols['after5']):>11} {f(first):>16} {lt:>26} {cl:>9} {cpu(d + '/cpu-closed.txt'):>10} {cpu(d + '/cpu-open.txt'):>9}  {helper}")
