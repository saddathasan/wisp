# Summarises run_variant.sh output dirs: summary.py <runs dir> -> per variant footprint (MiB) per launch (run1/run2/...),
# first/later open latency (ms) and CPU closed/open.
import os, re, sys, statistics as st
R = sys.argv[1]
def fp(p):
    m = re.search(r'phys_footprint: (\d+)', open(p).read()) if os.path.exists(p) else None
    return int(m.group(1)) / 2**20 if m else float('nan')
def cpu(p):
    # Every cpu-<state>*.txt for that state, e.g. cpu-closed.txt and the repeat cpu-closed2.txt.
    d, state = os.path.dirname(p), os.path.basename(p)[:-4]
    xs = [m.group(1) for f in sorted(os.listdir(d)) if f.startswith(state) for m in [re.search(r'cpu ([\d.]+)%', open(os.path.join(d, f)).read())] if m]
    return '/'.join(xs) or '?'
print(f"{'variant':8} {'before':>16} {'open':>16} {'after1':>16} {'after5':>16} {'Δ1':>14} {'first ms':>11} {'later ms (n)':>18} {'cpu closed':>14} {'cpu open':>14}")
for v in sorted(os.listdir(R)):
    d = os.path.join(R, v)
    if not os.path.isdir(d): continue
    runs = sorted(int(x[3:]) for x in os.listdir(d) if re.fullmatch(r'run\d+', x))
    cols = {k: [fp(f'{d}/run{i}/{k}.footprint.txt') for i in runs] for k in ('before', 'open', 'after1', 'after5')}
    first, later = [], []
    for i in runs:
        ms = [int(x) for x in re.findall(r'on screen (\d+) ms', open(f'{d}/run{i}.txt').read())] if os.path.exists(f'{d}/run{i}.txt') else []
        first += ms[:1]; later += ms[1:]
    f = lambda xs: '/'.join(f'{x:.1f}' for x in xs)
    d1 = [a - b for a, b in zip(cols['after1'], cols['before'])]
    lat = f"{st.median(later):.0f} [{min(later)}-{max(later)}] ({len(later)})" if later else '?'
    print(f"{v:8} {f(cols['before']):>16} {f(cols['open']):>16} {f(cols['after1']):>16} {f(cols['after5']):>16} {f(d1):>14} {'/'.join(map(str, first)):>11} {lat:>18} {cpu(d + '/cpu-closed.txt'):>14} {cpu(d + '/cpu-open.txt'):>14}")
