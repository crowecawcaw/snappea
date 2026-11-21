#!/usr/bin/env python3
"""Measure p90 CPU instructions for auto to settle every benchmark case.

A case is settled when every declared bug site has failed or, for a safe case,
when exhaustive search has finished. Counts come from valgrind (cachegrind), so
they do not depend on the machine's load or its performance counters.
"""
import argparse
import concurrent.futures
import datetime
import gzip
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import subprocess
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('bench_checker', Path(__file__).with_name('bench-checker.py'))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)

SYNTHETIC_BUDGET = 25000


def project_files(project, seen):
    """The .p files a .pproj lists, after those of the projects it includes."""
    project = project.resolve()
    if project in seen:
        return []
    seen.add(project)
    root = ET.parse(project).getroot()
    files = []
    for entry in root.iter('IncludeProject'):
        files += project_files(project.parent / entry.text.strip(), seen)
    for entry in root.iter('PFile'):
        path = (project.parent / entry.text.strip()).resolve()
        files += sorted(path.rglob('*.p')) if path.is_dir() else [path]
    return files


def project_sources(project):
    """A project's sources as [repository path, text] pairs."""
    files = list(dict.fromkeys(project_files(ROOT / project, set())))
    return [[str(f.relative_to(ROOT)), f.read_text()] for f in files]


def load_cases(manifests, name_filter, budget=SYNTHETIC_BUDGET):
    cases = []
    for manifest in manifests:
        for case in json.loads(manifest.read_text())['cases']:
            case = case.copy()
            if 'project' in case:
                case['sources'] = project_sources(case['project'])
                digest = json.dumps(case['sources']).encode()
            else:
                case['source'] = (manifest.parent / case['path']).read_text()
                case.setdefault('budget', budget)
                digest = case['source'].encode()
            case['source_sha256'] = hashlib.sha256(digest).hexdigest()
            # an unbounded safe model has nothing to settle
            if (case['expected'] or case.get('expect_exhausted')) and name_filter in case['name']:
                cases.append(case)
    names = [c['name'] for c in cases]
    if len(set(names)) != len(names):
        raise SystemExit('duplicate case name')
    return cases


def settled(row):
    """Every declared site failed, or a safe case finished its exhaustive search."""
    if row['expected']:
        return row['schedules_to_all'] is not None
    return row['exhausted'] and not row['first']


def summaries(rows):
    groups = {}
    for row in rows:
        groups.setdefault(row['case'], []).append(row)
    output = []
    for case, group in sorted(groups.items()):
        done = [r for r in group if r['settled']]
        p90 = bench.percentile([r['instructions'] if r['instructions'] is not None else math.inf for r in group])
        output.append(dict(
            case=case, runs=len(group), settled=len(done), p90=None if math.isinf(p90) else p90,
            p90_schedules=bench.percentile([r['schedules'] for r in group]),
        ))
    return output


def aggregate(summary):
    """Geometric mean of the settled cases' p90, and how many cases are not settled."""
    finite = [s['p90'] for s in summary if s['p90'] is not None]
    mean = math.exp(sum(math.log(max(v, 1)) for v in finite) / len(finite)) if finite else None
    return dict(geomean_p90=mean, total_p90=sum(finite), censored=len(summary) - len(finite), cases=len(summary))


def instructions(command, log):
    """Runs `command` under cachegrind and returns (stdout, instructions executed)."""
    valgrind = ['valgrind', '--tool=cachegrind', '--cache-sim=no', '--cachegrind-out-file=/dev/null', f'--log-file={log}']
    out = subprocess.check_output(valgrind + command, cwd=ROOT, text=True)
    count = re.search(r'I\s+refs:\s+([\d,]+)', Path(log).read_text())
    if not count:
        raise SystemExit(f'no instruction count in {log}')
    return out, int(count.group(1).replace(',', ''))


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--out', required=True)
    parser.add_argument('--binary', type=Path, help='use a frozen executable without rebuilding')
    parser.add_argument('--manifest', type=Path, action='append', help='case lists (default: the synthetic and sample manifests)')
    parser.add_argument('--seeds', type=int, default=10)
    parser.add_argument('--seed-start', type=int, default=0)
    parser.add_argument('--budget', type=int, default=SYNTHETIC_BUDGET, help='schedule limit of the cases that set none')
    parser.add_argument('--steps', type=int, default=2000)
    parser.add_argument('--seconds', type=float, default=3600, help='wall-clock cap per run; valgrind runs many times slower than native')
    parser.add_argument('--filter', default='')
    parser.add_argument('--jobs', type=int, default=os.cpu_count() or 2)
    parser.add_argument('--native', action='store_true', help='run without valgrind: checks which cases settle, counts no instructions')
    args = parser.parse_args()
    if min(args.seeds, args.steps, args.jobs) <= 0 or args.seed_start < 0:
        parser.error('positive seeds, steps and jobs are required')
    manifests = args.manifest or [ROOT / 'benchmarks/checker/manifest.json', ROOT / 'benchmarks/checker/samples.json']
    cases = load_cases(manifests, args.filter, args.budget)
    if not cases:
        parser.error('no selected cases')
    if args.binary is None:
        subprocess.run(['cargo', 'build', '--release', '--locked', '-p', 'snappea', '--example', 'checker_bench'], cwd=ROOT, check=True)
    binary = (args.binary or ROOT / 'target/release/examples/checker_bench').resolve()
    settings = dict(strategies=['auto'], steps=args.steps, seconds=args.seconds, seeds=args.seeds, seed_start=args.seed_start)
    with tempfile.TemporaryDirectory(prefix='instr-bench-') as temp:
        def run(case, name, **overrides):
            path = Path(temp) / f"{case['name'].replace('/', '-')}-{name}.json"
            path.write_text(json.dumps(dict(settings, budget=case['budget'], cases=[case], **overrides)))
            if args.native:
                return subprocess.check_output([str(binary), str(path)], cwd=ROOT, text=True), None
            return instructions([str(binary), str(path)], path.with_suffix('.log'))

        def setup(case):
            return run(case, 'setup', setup_only=True)[1]

        def measure(job):
            case, seed, setup_cost = job
            out, count = run(case, f'seed-{seed}', seeds=1, seed_start=seed)
            row = json.loads(out)
            row['settled'] = settled(row)
            row['instructions'] = count - setup_cost if row['settled'] and count is not None else None
            return row

        with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
            costs = list(pool.map(setup, cases))
            jobs = [(case, seed, cost) for case, cost in zip(cases, costs)
                    for seed in range(args.seed_start, args.seed_start + min(args.seeds, case.get('seeds', args.seeds)))]
            rows = []
            for row in pool.map(measure, jobs):
                rows.append(row)
                print(f"{row['case']} seed={row['seed']}: {row['instructions']}", flush=True)
    rows.sort(key=lambda r: (r['case'], r['seed']))
    summary = summaries(rows)
    total = aggregate(summary)
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    data = dict(
        timestamp=datetime.datetime.now(datetime.timezone.utc).isoformat(), revision=revision,
        dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True)),
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), settings=settings, native=args.native,
        cases=[{k: v for k, v in c.items() if k not in ('source', 'sources')} for c in cases],
        setup_instructions=dict(zip((c['name'] for c in cases), costs)), rows=rows, summary=summary, aggregate=total,
    )
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    encoded = (json.dumps(data, indent=2) + '\n').encode()
    Path(args.out).write_bytes(gzip.compress(encoded, mtime=0) if str(args.out).endswith('.gz') else encoded)
    print('| Case | Settled | p90 instructions | p90 schedules |\n|---|---:|---:|---:|')
    for s in summary:
        p90 = f"{s['p90']:,}" if s['p90'] is not None else 'not counted' if args.native else 'censored'
        print(f"| {s['case']} | {s['settled']}/{s['runs']} | {p90} | {s['p90_schedules']:,} |")
    if total['geomean_p90'] is not None and not args.native:
        print(f"Geometric mean of p90 instructions over {total['cases'] - total['censored']} settled cases: {total['geomean_p90']:,.0f}; "
              f"sum {total['total_p90']:,}; censored cases: {total['censored']}")


if __name__ == '__main__':
    main()
