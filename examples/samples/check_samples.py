#!/usr/bin/env python3
"""Check every sample project under examples/samples.

Runs native test-intent checks (lint, coverage and expected failures across three
seeds), then mutation testing. Expected bugs use the checker's own directive:

    // snappea: expect-bug "double booking of seat"
    test tcBugCheckThenHold [main=CheckThenHold]: ...

When to use: after editing a sample, and in CI.

Example:
    cargo build -p snappea --bin snappea
    python3 examples/samples/check_samples.py                  # every sample
    python3 examples/samples/check_samples.py flawed/lost-update  # one sample
    python3 examples/samples/check_samples.py --snappea /path/to/snappea --jobs 4
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent

def run(cmd: list[str], cwd: Path) -> tuple[int, dict]:
    proc = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError:
        data = {"raw": proc.stdout}
    if proc.stderr:
        # Some check advisories are rendered on stderr even in JSON mode. Keep
        # their source locations and explanations with the saved result.
        data["diagnostics_stderr"] = proc.stderr
    return proc.returncode, data


def check_project(snappea: str, project: Path, timeout: str, mutation_timeout: str,
                  workers: str | None = None) -> list[str]:
    """Delegate intent, coverage and mutation semantics to the native checker."""
    cwd = project.parent
    rel = project.relative_to(HERE)
    extra = ["--workers", workers] if workers else []
    # The CLI rejects --expect-bug-seeds on positive-only projects. This is only
    # option detection; native parsing validates the directive and its placement.
    has_expected_bug = any(
        re.search(r"(?m)^\s*//\s*snappea:\s*expect-bug\b", source.read_text())
        for source in cwd.rglob("*.p") if "PCheckerOutput" not in source.parts
    )
    seeds = ["--expect-bug-seeds", "1,2,3"] if has_expected_bug else []
    output = cwd / "PCheckerOutput" / "samples"
    commands = [
        ("check", [snappea, "--json", "check", project.name, "--no-mutation",
                   *seeds, "--timeout", timeout,
                   "-o", "PCheckerOutput/samples/check", *extra]),
        ("mutation", [snappea, "--json", "check", "mutation", project.name,
                      "--mutation-timeout", mutation_timeout, "--timeout", timeout,
                      "-o", "PCheckerOutput/samples/mutation", *extra]),
    ]
    for stage, cmd in commands:
        code, data = run(cmd, cwd)
        output.mkdir(parents=True, exist_ok=True)
        report = output / f"{stage}.json"
        report.write_text(json.dumps(data, indent=2) + "\n")
        if code:
            # Keep all diagnostics: lint failures and expected-bug mismatches do
            # not necessarily use the same summary shape as coverage failures.
            return [f"{rel}: {stage} exited {code}; report: {report}\n"
                    + json.dumps(data, indent=2)]
        warnings = {name: count for name, count in data.get("warnings", {}).items() if count}
        if warnings:
            print(f"note  {rel}: {stage} advisories {warnings}; inspect {report}", flush=True)
        if stage == "check":
            incomplete = [t["name"] for t in data.get("test_cases", [])
                          if t.get("result") != "expected_bug" and not t.get("search_complete")]
            if incomplete:
                print(f"note  {rel}: search incomplete within {timeout} for "
                      f"{', '.join(incomplete)}", flush=True)
            for evidence in data.get("expected_bug_seed_checks", []):
                status = evidence.get("status")
                if status == "consistent":
                    continue
                # Matching text on every seed does not prove every seed hit the same
                # defect: `different_failures` means the seeds failed at different
                # sites, `incomplete` that a failure had no identity to compare.
                identities = [(run.get("seed"), run.get("identity")) for run in evidence.get("runs", [])]
                print(f"note  {rel}: {evidence.get('test_case')}: expected-bug identity is "
                      f"'{status}', not 'consistent'; seeds and identities: {identities}; "
                      f"inspect {report}", flush=True)
    return []


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("samples", nargs="*", help="sample folders, relative to examples/samples")
    ap.add_argument("--snappea", default=str(ROOT / "target" / "debug" / "snappea"))
    ap.add_argument("--timeout", default="20s", help="per test case and expected-bug seed (default 20s)")
    ap.add_argument("--mutation-timeout", default="2m")
    ap.add_argument("--jobs", type=int, default=1, help="samples checked at once")
    ap.add_argument("--workers", help="snappea -j for each check (default: one per core)")
    args = ap.parse_args()

    if args.jobs < 1:
        ap.error("--jobs must be at least 1")
    roots = [HERE / s for s in args.samples] if args.samples else [HERE]
    missing = [s for s, r in zip(args.samples, roots) if not r.is_dir()]
    if missing:
        # A mistyped path must not pass just because the other paths have samples.
        for s in missing:
            print(f"error: no sample folder '{s}' under {HERE}; give a path relative to it, "
                  f"such as flawed/lost-update", file=sys.stderr)
        return 2
    projects = sorted(p for r in roots for p in r.rglob("*.pproj")
                      if "PCheckerOutput" not in p.parts)
    if not projects:
        print("no .pproj found", file=sys.stderr)
        return 2
    # A sample folder without exactly one project would be skipped silently. Count
    # these apart from project failures: such a folder has no project to count.
    layout_failures = 0
    for group in ("use-cases", "concepts", "flawed"):
        for d in sorted((HERE / group).glob("*/")):
            selected = any(d == r or r in d.parents or d in r.parents for r in roots)
            if not selected:
                continue
            found = [p for p in d.glob("*.pproj")]
            if len(found) != 1:
                layout_failures += 1
                print(f"FAIL  {d.relative_to(HERE)}: expected one .pproj, found {len(found)}")
    failures = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=args.jobs) as pool:
        futures = {pool.submit(check_project, args.snappea, p, args.timeout,
                               args.mutation_timeout, args.workers): p for p in projects}
        for fut in concurrent.futures.as_completed(futures):
            rel = futures[fut].parent.relative_to(HERE)
            problems = fut.result()
            if problems:
                failures += 1
                print(f"FAIL  {rel}")
                for prob in problems:
                    print(f"  {prob}")
            else:
                print(f"ok    {rel}")
    print(f"{len(projects) - failures} of {len(projects)} samples behave as declared")
    if layout_failures:
        print(f"{layout_failures} sample folder(s) without exactly one .pproj")
    return 1 if failures or layout_failures else 0


if __name__ == "__main__":
    sys.exit(main())
