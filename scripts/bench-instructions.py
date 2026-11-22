#!/usr/bin/env python3
"""Measure, per benchmark case, the p90 across seeds of the CPU instructions auto
executes to settle it.

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
        # upstream projects list folders they do not have, such as PForeign
        files += sorted(path.rglob('*.p')) if path.is_dir() else [path] if path.exists() else []
    return files


def project_sources(project):
    """A project's sources as [repository path, text] pairs."""
    files = list(dict.fromkeys(project_files(ROOT / project, set())))
    return [[str(f.relative_to(ROOT)), f.read_text()] for f in files]


def patched(sources, patches):
    """The sources with each planted bug applied. A patch names a file and a line of it to
    find and replace; the text must occur exactly once, so a sample that drifts fails here
    rather than silently measuring the correct design."""
    sources = [list(pair) for pair in sources]
    for patch in patches:
        hits = [pair for pair in sources if pair[0].endswith(patch['file'])]
        if len(hits) != 1 or hits[0][1].count(patch['find']) != 1:
            raise SystemExit(f"patch does not apply exactly once: {patch['file']}: {patch['find']!r}")
        hits[0][1] = hits[0][1].replace(patch['find'], patch['replace'])
    return sources


def load_cases(manifests, name_filter, budget=SYNTHETIC_BUDGET):
    cases = []
    for manifest in manifests:
        for case in json.loads(manifest.read_text())['cases']:
            case = case.copy()
            if 'project' in case:
                case['sources'] = patched(project_sources(case['project']), case.get('patch', []))
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
        counts = [r['instructions'] for r in done if r['instructions'] is not None]
        output.append(dict(
            case=case, runs=len(group), settled=len(done), p90=None if math.isinf(p90) else p90,
            least=min(counts, default=None), most=max(counts, default=None),
            p90_schedules=bench.percentile([r['schedules'] for r in group]),
        ))
    return output


def compare(before, after):
    """Per case, the p90 after over the p90 before. Cases are not combined: a mean or a
    sum over models of different sizes says mostly how the largest ones moved."""
    old = {s['case']: s for s in before}
    rows = []
    for new in after:
        was = old.get(new['case'])
        if was is None:
            continue
        ratio = new['p90'] / was['p90'] if new['p90'] and was['p90'] else None
        rows.append(dict(case=new['case'], before=was['p90'], after=new['p90'], ratio=ratio))
    return rows


def verdicts(rows, tolerance=0.1):
    """How many cases got cheaper, dearer or stayed within `tolerance`, and how many
    settle only before or only after."""
    count = dict(cheaper=0, dearer=0, same=0, newly_settled=0, no_longer_settled=0, unsettled=0)
    for row in rows:
        if row['ratio'] is not None:
            key = 'cheaper' if row['ratio'] < 1 - tolerance else 'dearer' if row['ratio'] > 1 + tolerance else 'same'
        elif row['after'] is not None:
            key = 'newly_settled'
        elif row['before'] is not None:
            key = 'no_longer_settled'
        else:
            key = 'unsettled'
        count[key] += 1
    return count


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
    parser.add_argument('--compare', type=Path, help='an earlier result of this script: adds each case\'s change')
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
    revision = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    data = dict(
        timestamp=datetime.datetime.now(datetime.timezone.utc).isoformat(), revision=revision,
        dirty=bool(subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True)),
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), settings=settings, native=args.native,
        cases=[{k: v for k, v in c.items() if k not in ('source', 'sources')} for c in cases],
        setup_instructions=dict(zip((c['name'] for c in cases), costs)), rows=rows, summary=summary,
    )
    Path(args.out).parent.mkdir(parents=True, exist_ok=True)
    encoded = (json.dumps(data, indent=2) + '\n').encode()
    Path(args.out).write_bytes(gzip.compress(encoded, mtime=0) if str(args.out).endswith('.gz') else encoded)
    earlier = None
    if args.compare:
        text = gzip.open(args.compare, 'rt').read() if str(args.compare).endswith('.gz') else args.compare.read_text()
        earlier = {r['case']: r for r in compare(json.loads(text)['summary'], summary)}
    count = (lambda n: f'{n:,}' if n is not None else 'not counted' if args.native else 'unsettled')
    print('| Case | Settled | p90 instructions | Least to most over seeds | p90 schedules |' + (' Before | Change |' if earlier else ''))
    print('|---|---:|---:|---:|---:|' + ('---:|---:|' if earlier else ''))
    for s in summary:
        spread = f"{s['least']:,} to {s['most']:,}" if s['least'] is not None else ''
        line = f"| {s['case']} | {s['settled']}/{s['runs']} | {count(s['p90'])} | {spread} | {s['p90_schedules']:,} |"
        if earlier:
            row = earlier.get(s['case'])
            change = f"x{row['ratio']:.2f}" if row and row['ratio'] is not None else ''
            line += f" {count(row['before']) if row else 'not run'} | {change} |"
        print(line)
    if not args.native:
        print(f"The p90 is over the seeds of one case. {sum(s['p90'] is not None for s in summary)} of {len(summary)} cases "
              f"settle on at least 9 seeds in 10.")
    if earlier:
        v = verdicts(list(earlier.values()))
        print(f"Against {args.compare}: {v['cheaper']} cases cheaper by more than 10%, {v['dearer']} dearer by more than 10%, "
              f"{v['same']} within 10%; {v['newly_settled']} newly settled, {v['no_longer_settled']} no longer settled.")

if __name__ == '__main__':
    main()
