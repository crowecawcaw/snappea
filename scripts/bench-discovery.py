#!/usr/bin/env python3
"""Draw how long auto takes to find each known bug: a histogram over every bug site
and seed, by schedules and by seconds.

The bulk shows the usual cost; the tail and the `not found` bar are the uncertainty
left after a short run. Use it to judge a search change: the whole shape should move
left. Schedules repeat exactly for a seed; seconds are what a user waits and vary with
the machine.
"""
import argparse
import concurrent.futures
import datetime
import gzip
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('bench_instructions', Path(__file__).with_name('bench-instructions.py'))
instr = importlib.util.module_from_spec(spec)
spec.loader.exec_module(instr)

QUANTILES = (0.5, 0.9, 0.99)


def discoveries(rows, large=()):
    """One point per bug site and seed: where it was first found, or None values for a miss.
    `large` names the cases that are bugs in large models."""
    points = []
    for row in rows:
        for site in row['expected']:
            points.append(dict(
                case=row['case'], site=site, seed=row['seed'], large=row['case'] in large,
                schedules=row['first'].get(site), seconds=row.get('first_seconds', {}).get(site),
            ))
    return points


def quantile(values, q):
    """Nearest-rank quantile over every point; a miss is infinite."""
    ordered = sorted(math.inf if v is None else v for v in values)
    return ordered[max(0, math.ceil(len(ordered) * q) - 1)]


def edges(low, high):
    """1-2-5 bin edges covering [low, high]."""
    out, power = [], math.floor(math.log10(low))
    while True:
        for m in (1, 2, 5):
            edge = m * 10.0 ** power
            if edge > high and out and out[-1] > high:
                return [e for i, e in enumerate(out) if i + 1 >= len(out) or out[i + 1] > low]
            out.append(edge)
        power += 1


def histogram(values, bins):
    """Counts of found values per bin [bins[i], bins[i+1]), and the number missed."""
    counts = [0] * (len(bins) - 1)
    for v in values:
        if v is None:
            continue
        i = max(0, min(len(counts) - 1, max(j for j, e in enumerate(bins[:-1]) if e <= v or j == 0)))
        counts[i] += 1
    return counts, sum(v is None for v in values)


def tail(points, key, count=8):
    """The slowest discoveries per site (its worst seed), misses first."""
    worst = {}
    for p in points:
        v = math.inf if p[key] is None else p[key]
        name = (p['case'], p['site'])
        worst[name] = max(worst.get(name, 0), v)
    return sorted(worst.items(), key=lambda kv: -kv[1])[:count]


def label(value, unit):
    """A short axis or summary label, to three significant figures."""
    short = lambda v: f'{float(f"{v:.3g}"):g}'
    if unit == 's':
        return f'{short(value * 1000)} ms' if value < 1 else f'{short(value)} s'
    return f'{short(value / 1000)}k' if value >= 1000 else short(value)


INK, MUTED, GRID, SURFACE, BAR, LARGE, BEFORE = '#0b0b0b', '#52514e', '#e4e3df', '#fcfcfb', '#2a78d6', '#d9730d', '#52514e'


def panel(values, earlier, unit, title, y0, width, height, large=()):
    """One histogram as SVG elements: found values on a log axis, misses in their own bar.
    `large` is the part of `values` from large models, stacked on top in its own color."""
    found = [v for v in values + (earlier or []) if v is not None]
    floor = 1 if unit == '' else 1e-4
    bins = edges(max(floor, min(found)), max(found))
    counts, missed = histogram(values, bins)
    old = histogram(earlier, bins) if earlier else None
    big, big_missed = histogram(list(large), bins)
    left, right, top, bottom = 56, 24, 62, 40
    slots = len(counts) + 2  # a gap, then the `not found` bar
    step = (width - left - right) / slots
    peak = max(counts + [missed] + (old[0] + [old[1]] if old else []) + [1])
    ticks = [t for t in (peak, peak / 2) if t >= 1]
    plot = height - top - bottom
    base = y0 + top + plot
    scale = lambda n: plot * n / peak
    out = [f'<text x="{left}" y="{y0 + 18}" font-size="14" font-weight="600" fill="{INK}">{title}</text>']
    total = len(values)
    marks = ', '.join(
        f"p{round(q * 100)} {label(quantile(values, q), unit) if not math.isinf(quantile(values, q)) else 'not found'}"
        for q in QUANTILES)
    out.append(f'<text x="{left}" y="{y0 + 35}" font-size="12" fill="{MUTED}">{total} bug discoveries ({marks}); {missed} not found</text>')
    for t in ticks:
        y = base - scale(t)
        out.append(f'<line x1="{left}" x2="{width - right}" y1="{y:.1f}" y2="{y:.1f}" stroke="{GRID}"/>')
        out.append(f'<text x="{left - 8}" y="{y + 4:.1f}" font-size="11" text-anchor="end" fill="{MUTED}">{round(t)}</text>')
    out.append(f'<line x1="{left}" x2="{width - right}" y1="{base}" y2="{base}" stroke="{MUTED}"/>')

    def bar(i, n, upper):
        """A bar of n points; the upper `upper` of them are from large models."""
        if n == 0:
            return ''
        x, w, h = left + i * step + 1, step - 2, max(scale(n), 2)
        r = min(4, h, w / 2)
        whole = (f'<path d="M{x:.1f},{base} v{-(h - r):.1f} q0,{-r} {r},{-r} h{w - 2 * r:.1f} q{r},0 {r},{r} v{h - r:.1f} z" '
                 f'fill="{LARGE if upper else BAR}"><title>{n} bugs, {upper} from the large set</title></path>')
        lower = scale(n - upper)
        if not upper or lower < 1:
            return whole
        return whole + f'<rect x="{x:.1f}" y="{base - lower:.1f}" width="{w:.1f}" height="{lower:.1f}" fill="{BAR}"/>'

    def outline(i, n):
        x, y = left + i * step, base - scale(n)
        return f'<line x1="{x:.1f}" x2="{x + step:.1f}" y1="{y:.1f}" y2="{y:.1f}" stroke="{BEFORE}" stroke-width="2"/>'

    for i, n in enumerate(counts):
        out.append(bar(i, n, big[i]))
        if n:
            out.append(f'<text x="{left + (i + 0.5) * step:.1f}" y="{base - max(scale(n), 2) - 4:.1f}" font-size="10" text-anchor="middle" fill="{MUTED}">{n}</text>')
    miss_slot = len(counts) + 1
    out.append(bar(miss_slot, missed, big_missed))
    out.append(f'<text x="{left + (miss_slot + 0.5) * step:.1f}" y="{base - (max(scale(missed), 2) if missed else 0) - 4:.1f}" font-size="10" text-anchor="middle" fill="{MUTED}">{missed}</text>')
    if old:
        for i, n in enumerate(old[0]):
            out.append(outline(i, n))
        out.append(outline(miss_slot, old[1]))
    every = max(1, round(len(bins) / 9))
    for i, e in enumerate(bins):
        if i % every == 0:
            x = left + i * step
            out.append(f'<line x1="{x:.1f}" x2="{x:.1f}" y1="{base}" y2="{base + 4}" stroke="{MUTED}"/>')
            out.append(f'<text x="{x:.1f}" y="{base + 17}" font-size="11" text-anchor="middle" fill="{MUTED}">{label(e, unit)}</text>')
    out.append(f'<text x="{left + (miss_slot + 0.5) * step:.1f}" y="{base + 17}" font-size="11" text-anchor="middle" fill="{MUTED}">not found</text>')
    axis = 'schedules run when the bug was found' if unit == '' else 'seconds into the run when the bug was found'
    out.append(f'<text x="{left + (width - left - right) / 2:.1f}" y="{base + 34}" font-size="11" text-anchor="middle" fill="{MUTED}">{axis} (log scale)</text>')
    return out


def svg(points, earlier=None, width=820, height=290):
    body = []
    for n, (key, unit, title) in enumerate((('schedules', '', 'Bugs found, by schedules'), ('seconds', 's', 'Bugs found, by time'))):
        before = [p[key] for p in earlier] if earlier else None
        large = [p[key] for p in points if p.get('large')]
        body += panel([p[key] for p in points], before, unit, title, n * height + 8, width, height, large)
    total = 2 * height + 16
    if any(p.get('large') for p in points):
        body.append(f'<rect x="{width - 470}" y="16" width="14" height="12" rx="2" fill="{BAR}"/>'
                    f'<text x="{width - 450}" y="26" font-size="11" fill="{MUTED}">small models</text>'
                    f'<rect x="{width - 370}" y="16" width="14" height="12" rx="2" fill="{LARGE}"/>'
                    f'<text x="{width - 350}" y="26" font-size="11" fill="{MUTED}">large models</text>')
    if earlier:
        body.append(f'<line x1="{width - 250}" x2="{width - 226}" y1="22" y2="22" stroke="{BEFORE}" stroke-width="2"/>'
                    f'<text x="{width - 220}" y="26" font-size="11" fill="{MUTED}">before</text>'
                    f'<rect x="{width - 170}" y="16" width="14" height="12" rx="2" fill="{BAR}"/>'
                    f'<text x="{width - 150}" y="26" font-size="11" fill="{MUTED}">after</text>')
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {total}" width="{width}" height="{total}" '
            f'font-family="system-ui, -apple-system, Segoe UI, sans-serif">'
            f'<rect width="{width}" height="{total}" fill="{SURFACE}"/>' + ''.join(body) + '</svg>\n')


def read(path):
    text = gzip.open(path, 'rt').read() if str(path).endswith('.gz') else Path(path).read_text()
    return json.loads(text)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--out', required=True, help='result JSON; the histogram is written beside it as .svg')
    parser.add_argument('--binary', type=Path, help='use a frozen executable without rebuilding')
    parser.add_argument('--manifest', type=Path, action='append', help='case lists (default: the synthetic, sample and large-model manifests)')
    parser.add_argument('--seeds', type=int, default=20)
    parser.add_argument('--seed-start', type=int, default=0)
    parser.add_argument('--budget', type=int, default=instr.SYNTHETIC_BUDGET, help='schedule limit of the cases that set none')
    parser.add_argument('--steps', type=int, default=2000)
    parser.add_argument('--seconds', type=float, default=60, help='wall-clock cap per run')
    parser.add_argument('--filter', default='')
    parser.add_argument('--jobs', type=int, default=max(1, (os.cpu_count() or 2) // 2), help='runs at once; fewer gives steadier seconds')
    parser.add_argument('--compare', type=Path, help='an earlier result of this script: drawn as an outline')
    parser.add_argument('--redraw', action='store_true', help='draw --out again without running')
    args = parser.parse_args()
    out = Path(args.out)
    if args.redraw:
        data = read(out)
    else:
        manifests = args.manifest or [ROOT / 'benchmarks/checker' / name for name in ('manifest.json', 'samples.json', 'large.json')]
        cases = [c for c in instr.load_cases(manifests, args.filter, args.budget) if c['expected']]
        if not cases:
            parser.error('no selected cases with a known bug')
        if args.binary is None:
            subprocess.run(['cargo', 'build', '--release', '--locked', '-p', 'snappea', '--example', 'checker_bench'], cwd=ROOT, check=True)
        binary = (args.binary or ROOT / 'target/release/examples/checker_bench').resolve()
        settings = dict(strategies=['auto'], steps=args.steps, seconds=args.seconds)
        with tempfile.TemporaryDirectory(prefix='discovery-bench-') as temp:
            def measure(job):
                case, seed = job
                path = Path(temp) / f"{case['name'].replace('/', '-')}-{seed}.json"
                path.write_text(json.dumps(dict(settings, budget=case['budget'], cases=[case], seeds=1, seed_start=seed)))
                return json.loads(subprocess.check_output([str(binary), str(path)], cwd=ROOT, text=True))

            jobs = [(case, seed) for case in cases for seed in range(args.seed_start, args.seed_start + args.seeds)]
            with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
                rows = list(pool.map(measure, jobs))
        rows.sort(key=lambda r: (r['case'], r['seed']))
        data = dict(
            timestamp=datetime.datetime.now(datetime.timezone.utc).isoformat(),
            revision=subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip(),
            dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True)),
            settings=dict(settings, seeds=args.seeds, seed_start=args.seed_start, jobs=args.jobs),
            points=discoveries(rows, {c['name'] for c in cases if c.get('family', '').startswith('large')}),
        )
        out.parent.mkdir(parents=True, exist_ok=True)
        encoded = (json.dumps(data) + '\n').encode()
        out.write_bytes(gzip.compress(encoded, mtime=0) if str(out).endswith('.gz') else encoded)
    points = data['points']
    earlier = read(args.compare)['points'] if args.compare else None
    picture = Path(str(out).removesuffix('.gz')).with_suffix('.svg')
    picture.write_text(svg(points, earlier))

    def row(name, pts):
        cells = []
        for key, unit in (('schedules', ''), ('seconds', 's')):
            for q in QUANTILES:
                v = quantile([p[key] for p in pts], q)
                cells.append('not found' if math.isinf(v) else label(v, unit))
        return f"| {name} | {len(pts)} | {sum(p['schedules'] is None for p in pts)} | " + ' | '.join(cells) + ' |'

    print('| Run | Discoveries | Not found | p50 schedules | p90 | p99 | p50 time | p90 | p99 |')
    print('|---|---:|---:|---:|---:|---:|---:|---:|---:|')
    if earlier:
        print(row('before', earlier))
    print(row('after' if earlier else 'this run', points))
    print('\nSlowest bug sites (worst seed):')
    for (case, site), v in tail(points, 'schedules'):
        print(f"  {case} {site}: {'not found' if math.isinf(v) else f'{v:,} schedules'}")
    print(f'\nHistogram: {picture}')


if __name__ == '__main__':
    main()
