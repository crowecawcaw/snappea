#!/usr/bin/env python3
"""Check every sample project under examples/samples.

What it does: for each `.pproj`, runs the passing test cases with
`snappea check --deny mutation` (lint, coverage and every mutant must pass), then
runs each test case marked as an expected bug and requires it to fail with the
named assertion message.

Mark an expected bug with a comment on the line directly before its test:

    // expect-bug: double booking of seat
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
EXPECT_RE = re.compile(r"^\s*//\s*expect-bug:\s*(.+?)\s*$")
TEST_RE = re.compile(r"^\s*test\s+(\w+)")


def expected_bugs(project: Path) -> dict[str, str]:
    """Map test name -> expected failure substring, from `// expect-bug:` comments."""
    bugs: dict[str, str] = {}
    for p in sorted(project.parent.rglob("*.p")):
        if "PCheckerOutput" in p.parts:
            continue
        pending = None
        for line in p.read_text().splitlines():
            m = EXPECT_RE.match(line)
            if m:
                pending = m.group(1)
                continue
            t = TEST_RE.match(line)
            if t and pending is not None:
                bugs[t.group(1)] = pending
            if line.strip() and not line.strip().startswith("//"):
                pending = None
    return bugs


def run(cmd: list[str], cwd: Path) -> tuple[int, dict]:
    proc = subprocess.run(cmd, cwd=cwd, capture_output=True, text=True)
    try:
        data = json.loads(proc.stdout)
    except json.JSONDecodeError:
        data = {"raw": proc.stdout + proc.stderr}
    return proc.returncode, data


def check_project(snappea: str, project: Path, timeout: str, mutation_timeout: str,
                  workers: str | None = None) -> list[str]:
    """Return a list of problems (empty when the sample behaves as declared)."""
    cwd = project.parent
    rel = project.relative_to(HERE)
    problems: list[str] = []
    code, listing = run([snappea, "--json", "tests", project.name], cwd)
    if code != 0:
        return [f"{rel}: could not list tests: {listing}"]
    tests = listing["test_cases"]
    bugs = expected_bugs(project)
    for name in bugs:
        if name not in tests:
            problems.append(f"{rel}: expect-bug names unknown test {name}")
    good = [t for t in tests if t not in bugs]
    extra = ["-j", workers] if workers else []

    if good:
        cmd = [snappea, "--json", "check", project.name, *good, "--deny", "mutation",
               "--timeout", timeout, "--mutation-timeout", mutation_timeout,
               "-o", "PCheckerOutput/samples", *extra]
        code, data = run(cmd, cwd)
        summary = data.get("summary", {})
        if code != 0:
            detail = summary.get("message") or data.get("raw", "")[:2000]
            undetected = [
                f"{m.get('file')}:{m.get('line')} {m.get('change')}"
                for m in (data.get("mutation") or {}).get("undetected", [])
            ]
            uncovered = [
                f"{u.get('file')}:{u.get('line')} {u.get('what')}"
                for u in summary.get("uncovered", [])
            ]
            failed = [
                f"{t['name']}: {t.get('bug', {}).get('report')}"
                for t in data.get("test_cases", []) if t.get("result") == "bug"
            ]
            problems.append(
                f"{rel}: passing tests exit {code} ({summary.get('status')}): {detail}"
                + "".join(f"\n    bug {x}" for x in failed)
                + "".join(f"\n    uncovered {x}" for x in uncovered)
                + "".join(f"\n    mutant {x}" for x in undetected)
            )
        else:
            incomplete = [t["name"] for t in data.get("test_cases", [])
                          if not t.get("search_complete")]
            if incomplete:
                print(f"note  {rel}: search incomplete within {timeout} for {', '.join(incomplete)}")

    for name, message in bugs.items():
        cmd = [snappea, "--json", "check", project.name, name, "--no-mutation",
               "--warn", "coverage", "--timeout", timeout, "-o", "PCheckerOutput/samples", *extra]
        code, data = run(cmd, cwd)
        reports = [t.get("bug", {}).get("report", "") for t in data.get("test_cases", [])
                   if t.get("result") == "bug"]
        if code != 1 or not reports:
            problems.append(f"{rel}: {name} should find a bug, exit {code}: "
                            f"{data.get('summary', data.get('raw', ''))}")
        elif not any(message in r for r in reports):
            problems.append(f"{rel}: {name} failed for another reason: {reports[0]!r}, "
                            f"expected {message!r}")
    return problems


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("samples", nargs="*", help="sample folders, relative to examples/samples")
    ap.add_argument("--snappea", default=str(ROOT / "target" / "debug" / "snappea"))
    ap.add_argument("--timeout", default="20s", help="per test case (default 20s)")
    ap.add_argument("--mutation-timeout", default="2m")
    ap.add_argument("--jobs", type=int, default=1, help="samples checked at once")
    ap.add_argument("--workers", help="snappea -j for each check (default: one per core)")
    args = ap.parse_args()

    roots = [HERE / s for s in args.samples] if args.samples else [HERE]
    projects = sorted(p for r in roots for p in r.rglob("*.pproj")
                      if "PCheckerOutput" not in p.parts)
    if not projects:
        print("no .pproj found", file=sys.stderr)
        return 2
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
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
