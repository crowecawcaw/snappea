#!/usr/bin/env python3
"""Measure all-known-bug completion, recall and censored p90 across fixed seeds."""
import argparse
import concurrent.futures
import datetime
import gzip
import hashlib
import json
import math
import platform
from pathlib import Path
import statistics
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_STRATEGIES = 'auto,feedback,random,pct-0,pct-2,pct-10'


def percentile(values, q=0.9):
    """Nearest-rank quantile over every run; misses remain infinity."""
    return sorted(values)[max(0, math.ceil(len(values) * q) - 1)]


def summaries(rows, budget):
    groups = {}
    for row in rows:
        if row['expected']:
            groups.setdefault((row['case'], row['strategy']), []).append(row)
    output = []
    for (case, strategy), group in sorted(groups.items()):
        completed = sum(r['schedules_to_all'] is not None for r in group)
        p90 = percentile([r['schedules_to_all'] if r['schedules_to_all'] is not None else math.inf for r in group])
        output.append(dict(
            case=case, strategy=strategy, runs=len(group), completed=completed,
            p90=None if math.isinf(p90) else p90,
            recall=statistics.mean(len(r['first']) / len(r['expected']) for r in group),
            par2=statistics.mean(r['schedules_to_all'] if r['schedules_to_all'] is not None else 2 * budget for r in group),
        ))
    return output


def check_failures(rows, cases):
    """CI checks every seed's entire oracle, including safe-control exhaustion."""
    controls = {c['name']: c['expect_exhausted'] for c in cases if 'expect_exhausted' in c}
    failures = []
    for row in rows:
        label = f"{row['case']} {row['strategy']} seed={row['seed']}"
        missing = set(row['expected']) - set(row['first'])
        if missing:
            failures.append(f"{label}: missing {', '.join(sorted(missing))}")
        if set(row['first']) - set(row['expected']):
            failures.append(f'{label}: unexpected failure')
        if row['time_censored']:
            failures.append(f'{label}: time limit reached')
        if row['case'] in controls and row['exhausted'] != controls[row['case']]:
            failures.append(f'{label}: incorrect exhaustive-completion result')
    return failures


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True)
    parser.add_argument('--binary', type=Path, help='use a frozen executable without rebuilding')
    parser.add_argument('--manifest', type=Path, default=ROOT / 'benchmarks/checker/manifest.json')
    parser.add_argument('--split', choices=['develop', 'evaluation', 'all'], default='develop')
    parser.add_argument('--strategies', default=DEFAULT_STRATEGIES)
    parser.add_argument('--seeds', type=int, default=10)
    parser.add_argument('--seed-start', type=int, default=0)
    parser.add_argument('--budget', type=int, default=10000)
    parser.add_argument('--steps', type=int, default=2000)
    parser.add_argument('--seconds', type=float, default=30)
    parser.add_argument('--filter', default='')
    parser.add_argument('--jobs', type=int, default=2)
    parser.add_argument('--check', action='store_true', help='fail unless every seed finds every declared bug and safe controls pass')
    args = parser.parse_args()
    if min(args.seeds, args.budget, args.steps, args.seconds, args.jobs) <= 0 or args.seed_start < 0 or args.seed_start + args.seeds > 2**32:
        parser.error('positive budgets and a valid unsigned seed range are required')
    valid = {'auto', 'feedback', 'random', 'dfs', 'pct-0', 'pct-2', 'pct-10'}
    strategies = args.strategies.split(',')
    if any(s not in valid for s in strategies) or len(set(strategies)) != len(strategies):
        parser.error('invalid or duplicate strategy')
    manifest = json.loads(args.manifest.read_text())
    cases = [c.copy() for c in manifest['cases'] if (args.split == 'all' or c['split'] == args.split) and args.filter in c['name']]
    if not cases:
        parser.error('no selected cases')
    for case in cases:
        case['source'] = (args.manifest.parent / case['path']).read_text()
        case['source_sha256'] = hashlib.sha256(case['source'].encode()).hexdigest()
        if len({e['id'] for e in case['expected']}) != len(case['expected']):
            parser.error('duplicate oracle ID')
    if args.binary is None:
        subprocess.run(['cargo', 'build', '--release', '--locked', '-p', 'snappea', '--example', 'checker_bench'], cwd=ROOT, check=True)
    binary = (args.binary or ROOT / 'target/release/examples/checker_bench').resolve()
    run = dict(cases=cases, strategies=strategies, budget=args.budget, steps=args.steps, seconds=args.seconds, seeds=args.seeds, seed_start=args.seed_start)
    rows = []
    with tempfile.TemporaryDirectory(prefix='checker-bench-') as temp:
        def measure(item):
            index, case = item
            path = Path(temp) / f'run-{index}.json'
            path.write_text(json.dumps(dict(run, cases=[case])))
            result = [json.loads(line) for line in subprocess.check_output([str(binary), str(path)], cwd=ROOT, text=True).splitlines()]
            print(f"{case['name']}: {len(result)} runs", flush=True)
            return result

        partial = Path(args.out).with_suffix('.jsonl')
        partial.parent.mkdir(parents=True, exist_ok=True)
        with partial.open('w') as journal, concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            futures = [pool.submit(measure, item) for item in enumerate(cases)]
            for future in concurrent.futures.as_completed(futures):
                result = future.result()
                rows.extend(result)
                for row in result:
                    journal.write(json.dumps(row) + '\n')
                journal.flush()
    rows.sort(key=lambda r: (r['case'], r['strategy'], r['seed']))
    expected = {(c['name'], s, seed) for c in cases for s in strategies for seed in range(args.seed_start, args.seed_start + args.seeds)}
    actual = {(r['case'], r['strategy'], r['seed']) for r in rows}
    if actual != expected or len(rows) != len(expected):
        raise SystemExit('incomplete or duplicate benchmark measurements')
    summary = summaries(rows, args.budget)
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    data = dict(
        platform=platform.platform(), manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest(),
        suite_version=manifest['suite_version'], timestamp=datetime.datetime.now(datetime.timezone.utc).isoformat(), revision=revision,
        dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True)),
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), settings={k: v for k, v in run.items() if k != 'cases'},
        cases=cases, rows=rows, summary=summary,
    )
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    encoded = (json.dumps(data, indent=2) + '\n').encode()
    Path(args.out).write_bytes(gzip.compress(encoded, mtime=0) if str(args.out).endswith('.gz') else encoded)
    print('| Case | Strategy | All bugs | Bug recall | p90 schedules to all | PAR2 |\n|---|---|---:|---:|---:|---:|')
    for result in summary:
        p90 = result['p90'] if result['p90'] is not None else '> budget / censored'
        print(f"| {result['case']} | {result['strategy']} | {result['completed']}/{result['runs']} | {result['recall']:.1%} | {p90} | {result['par2']:.1f} |")
    print('Rank completion first, then recall and all-bug tail cost. Misses are included in p90; PAR2 charges twice the budget.')
    if args.check:
        failures = check_failures(rows, cases)
        if failures:
            raise SystemExit('\n'.join(failures))


if __name__ == '__main__':
    main()
