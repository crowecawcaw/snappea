#!/usr/bin/env python3
"""Benchmarks `p check` on the tutorial projects.

  scripts/bench.py run --cmd target/release/snappea-compat --out result.json [--instructions]
  scripts/bench.py run --cmd "p" --tool upstream --out upstream.json
  scripts/bench.py report --current result.json [--history main.jsonl]
  scripts/bench.py compare snappea.json upstream.json

`run` copies each project to a temp folder and runs `check` with a fixed test,
schedule count and seed. `snappea-compat` runs one worker and explores the schedules
upstream does; `--cmd target/release/snappea --workers N` measures N workers. snappea has
no compile step (`check` type-checks the sources itself); upstream P compiles
each project once first, and that compile time is recorded separately. The seed fixes which
schedules are explored, so between commits only the speed changes. For each
workload it records the median wall-clock time of the check over --runs runs,
the schedules explored, schedules per second (schedules / median time), the
median time of a one-schedule check (startup: parsing, type checking and, for
upstream, .NET start and JIT) and warm schedules per second (the schedules after
the first over the time after startup) and, with --instructions, the instructions executed (one run under Valgrind's
cachegrind). Instruction counts are stable on shared CI runners, so they are
kept to tell a real change from runner noise; wall-clock time is what reports
show.

`report` prints a markdown table comparing a result with the previous entry in a
history file (one JSON result per line). `compare` prints snappea vs upstream, led by
a one-line summary for Raft (HEADLINE), the slowest model per schedule, as quoted
in the README.
"""
import argparse
import datetime
import json
import os
import platform
import re
import shlex
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TUTORIAL = REPO / "tests/upstream/Tutorial"
SEED = 42

# (project folder, test case, schedules). Tests that find no bug, so every run
# explores the full schedule count. Counts are set for about a second each.
WORKLOADS = [
    ("1_ClientServer", "tcMultipleClients", 1500),
    ("2_TwoPhaseCommit", "tcParametricTests___pNumClients_3__pNumParticipants_4__pNumTransPerClient_2__pFailParticipants_0", 1500),
    ("3_EspressoMachine", "tcResourceConstraints___waterLevel_50__beanLevel_50", 5000),
    ("4_FailureDetector", "tcTest_BalancedLoad___numNodes_5__numClients_2", 2000),
    ("5_Paxos", "testBasicPaxos3on5", 9000),
    ("6_Raft", "oneClientFiveServersReliable", 120),
]


# The workload `compare` sums up in one line: the model with the longest schedules.
HEADLINE = "6_Raft"

TIMEOUT = 900  # seconds per command; a hung tool fails the run instead of the job


def sh(args, cwd, **kw):
    try:
        return subprocess.run(args, cwd=cwd, capture_output=True, text=True, timeout=TIMEOUT, **kw)
    except subprocess.TimeoutExpired:
        sys.exit(f"timed out after {TIMEOUT} s: {shlex.join(args)} (in {cwd})")


def git(*args):
    r = sh(["git", *args], REPO)
    return r.stdout.strip() if r.returncode == 0 else ""


def cpu_model():
    try:
        for line in Path("/proc/cpuinfo").read_text().splitlines():
            if line.startswith("model name"):
                return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or "unknown"


def parse_check(out):
    """(schedules explored, bugs found) from a check's console output."""
    # snappea -v: `ok    TEST (proved, N schedules, ...)` or `FAIL  TEST (bug found after N schedules, ...)`
    m = re.search(r"^(ok|FAIL)\s+\S+ \([^)]*?(\d+) schedules?\b", out, re.M)
    if m:
        return int(m.group(2)), int(m.group(1) == "FAIL")
    # snappea-compat and upstream P: `..... Explored N schedules` and `Found N bugs.`
    m = re.search(r"Explored (\d+) schedules?", out)
    b = re.search(r"Found (\d+) bugs?", out)
    return (int(m.group(1)) if m else None, int(b.group(1)) if b else None)


def instructions(cmd, args, cwd):
    """Instructions executed by one run, from cachegrind's summary."""
    r = sh(["valgrind", "--tool=cachegrind", "--cache-sim=no",
            "--cachegrind-out-file=/dev/null", *cmd, *args], cwd)
    m = re.search(r"I\s+refs:\s+([\d,]+)", r.stderr)
    if not m:
        sys.exit(f"cachegrind gave no instruction count:\n{r.stderr[-2000:]}")
    return int(m.group(1).replace(",", ""))


# Old commits named the compat binary prs-compat; the benchmark builds them too.
COMPAT_NAMES = ("snappea-compat", "prs-compat")


def check_args(opts, cmd, test, n):
    """The `check` arguments for the command benchmarked."""
    upstream_style = ["check", "-tc", test, "-s", str(n), "--seed", str(SEED)]
    if opts.tool == "upstream" or Path(cmd[0]).name in COMPAT_NAMES:
        if opts.workers != 1:
            sys.exit("--workers needs the snappea binary; snappea-compat and upstream P run one worker")
        return upstream_style
    if any((Path(cmd[0]).parent / name).exists() for name in COMPAT_NAMES):
        return ["check", test, "-v", "-n", str(n), "--seed", str(SEED), "-j", str(opts.workers)]
    # a snappea binary from before snappea-compat existed: upstream's options and --workers
    return upstream_style + ["--workers", str(opts.workers)]


def run(opts):
    cmd = shlex.split(opts.cmd)
    if not shutil.which(cmd[0]) and not Path(cmd[0]).exists():
        sys.exit(f"not found: {cmd[0]}")
    if Path(cmd[0]).exists():
        cmd[0] = str(Path(cmd[0]).resolve())
    workloads = [w for w in WORKLOADS if not opts.only or any(o in w[0] for o in opts.only)]
    results = []
    with tempfile.TemporaryDirectory() as tmp:
        shutil.copytree(TUTORIAL, tmp, dirs_exist_ok=True)
        compiled = {}
        for project, test, n in workloads:
            proj = Path(tmp) / project
            name = f"{project.split('_', 1)[-1]}/{test.split('___', 1)[0]}"
            if opts.tool == "upstream" and project not in compiled:
                t0 = time.perf_counter()
                r = sh([*cmd, "compile"], proj)
                compiled[project] = time.perf_counter() - t0
                if r.returncode != 0:
                    sys.exit(f"{project}: compile failed\n{r.stdout}{r.stderr}")
            args = check_args(opts, cmd, test, n)
            times, schedules, bugs = [], None, None
            for i in range(opts.runs):
                print(f"{name}: run {i + 1}/{opts.runs}", file=sys.stderr, flush=True)
                shutil.rmtree(proj / "PCheckerOutput", ignore_errors=True)
                t0 = time.perf_counter()
                r = sh([*cmd, *args], proj)
                times.append(time.perf_counter() - t0)
                schedules, bugs = parse_check(r.stdout + r.stderr)
                if schedules is None:
                    sys.exit(f"{name}: no schedule count in output\n{r.stdout[-2000:]}{r.stderr[-2000:]}")
            check_s = statistics.median(times)
            # Startup: a check of one schedule. The rest of the time is warm.
            one = check_args(opts, cmd, test, 1)
            startup = []
            for i in range(opts.runs):
                print(f"{name}: startup run {i + 1}/{opts.runs}", file=sys.stderr, flush=True)
                shutil.rmtree(proj / "PCheckerOutput", ignore_errors=True)
                t0 = time.perf_counter()
                sh([*cmd, *one], proj)
                startup.append(time.perf_counter() - t0)
            startup_s = statistics.median(startup)
            res = {
                "name": name,
                "project": project,
                "test": test,
                "schedules": schedules,
                "bugs": bugs,
                "check_s": round(check_s, 4),
                "check_s_min": round(min(times), 4),
                "check_s_max": round(max(times), 4),
                "schedules_per_s": round(schedules / check_s, 1),
                "startup_s": round(startup_s, 4),
                "warm_schedules_per_s": round((schedules - 1) / max(check_s - startup_s, 1e-9), 1),
            }
            if project in compiled:
                res["compile_s"] = round(compiled[project], 4)
            if opts.instructions:
                print(f"{name}: counting instructions", file=sys.stderr, flush=True)
                res["instructions"] = instructions(cmd, args, proj)
            results.append(res)
            print(f"{name:32} {res['check_s']:8.3f} s  {res['schedules_per_s']:10.1f} sched/s"
                  + (f"  {res['instructions']:,} instr" if "instructions" in res else ""), file=sys.stderr, flush=True)

    commit = git("rev-parse", opts.commit)
    record = {
        "tool": opts.tool,
        "commit": commit,
        "message": git("log", "-1", "--format=%s", commit),
        # Order entries by commit date: runs for different commits can finish in any order.
        "commit_date": datetime.datetime.fromtimestamp(int(git("log", "-1", "--format=%ct", commit)),
                                                       datetime.timezone.utc).isoformat(timespec="seconds"),
        "date": datetime.datetime.now(datetime.timezone.utc).isoformat(timespec="seconds"),
        "version": opts.version or "",
        "seed": SEED,
        "runs": opts.runs,
        "workers": opts.workers if opts.tool != "upstream" else 1,
        "runner": {"os": platform.platform(), "cpu": cpu_model(), "cpus": os.cpu_count()},
        "total": {
            "check_s": round(sum(r["check_s"] for r in results), 4),
            "schedules": sum(r["schedules"] for r in results),
        },
        "results": results,
    }
    if compiled:
        record["total"]["compile_s"] = round(sum(compiled.values()), 4)
    record["total"]["schedules_per_s"] = round(record["total"]["schedules"] / record["total"]["check_s"], 1)
    if opts.instructions:
        record["total"]["instructions"] = sum(r["instructions"] for r in results)
    Path(opts.out).write_text(json.dumps(record, indent=2) + "\n")


def pct(new, old):
    if not old:
        return ""
    d = (new - old) / old * 100
    return f"{d:+.1f}%"


def report(opts):
    cur = json.loads(Path(opts.current).read_text())
    prev_cpu = prev_any = None
    if opts.history and Path(opts.history).exists():
        recs = [json.loads(l) for l in Path(opts.history).read_text().splitlines() if l.strip()]
        when = lambda r: datetime.datetime.fromisoformat(r.get("commit_date") or r["date"])
        older = [r for r in recs if r["commit"] != cur["commit"] and when(r) <= when(cur)]
        # Runners differ by up to ~40% in speed, so times are compared only with
        # an earlier run on the same CPU model. Instruction counts do not depend
        # on the CPU, so those are compared with the latest earlier run on any.
        same_cpu = [r for r in older if r["runner"]["cpu"] == cur["runner"]["cpu"]]
        prev_cpu = max(same_cpu, key=when) if same_cpu else None
        prev_any = max(older, key=when) if older else None

    names = [r["name"] for r in cur["results"]]

    def lookup(prev):
        # Totals are comparable only over the same workloads.
        if not prev:
            return {}
        out = {r["name"]: r for r in prev["results"]}
        out["**total**"] = prev["total"] if [r["name"] for r in prev["results"]] == names else None
        return out

    old_t, old_i = lookup(prev_cpu), lookup(prev_any)
    out = [f"### Benchmarks for {cur['commit'][:7]}", ""]
    cpu = cur["runner"]["cpu"]
    if prev_cpu:
        out.append(f"Times: wall clock, median of {cur['runs']} runs, compared with {prev_cpu['commit'][:7]} "
                   f"({prev_cpu['message']}), the latest earlier run on the same CPU ({cpu}).")
    else:
        out.append(f"Times: wall clock, median of {cur['runs']} runs. No earlier run on this CPU ({cpu}) "
                   "to compare with; runners differ in speed, so times are not compared across CPUs.")
    if prev_any:
        out.append(f"Instructions: compared with {prev_any['commit'][:7]} (any CPU); "
                   "they do not vary with the runner, so this is the noise-free signal.")
    out += ["", "| workload | schedules | time | Δ time | schedules/s | Δ sched/s | Δ instructions |",
            "|---|---:|---:|---:|---:|---:|---:|"]
    rows = cur["results"] + [{"name": "**total**", **cur["total"]}]
    for r in rows:
        o, oi = old_t.get(r["name"]) or {}, old_i.get(r["name"]) or {}
        out.append("| {} | {} | {:.3f} s | {} | {:,.0f} | {} | {} |".format(
            r["name"], r["schedules"], r["check_s"], pct(r["check_s"], o.get("check_s")),
            r["schedules_per_s"], pct(r["schedules_per_s"], o.get("schedules_per_s")),
            pct(r.get("instructions", 0), oi.get("instructions")) if r.get("instructions") else ""))
    print("\n".join(out))


def compare(opts):
    snappea = json.loads(Path(opts.snappea).read_text())
    up = json.loads(Path(opts.upstream).read_text())
    upr = {r["name"]: r for r in up["results"]}
    out = [f"### snappea {snappea['version'] or snappea['commit'][:7]} vs upstream P {up['version']}", ""]
    hp = next((r for r in snappea["results"] if r["project"] == HEADLINE), None)
    hu = upr.get(hp["name"]) if hp else None
    if hp and hu and "warm_schedules_per_s" in hp and "warm_schedules_per_s" in hu:
        up_total = hu["compile_s"] + hu["check_s"]
        out += [f"Checking the Raft tutorial ({hp['schedules']} schedules) takes {hp['check_s']:.1f} s with snappea "
                f"and {up_total:.0f} s with P {up['version']} ({up_total / hp['check_s']:.0f}× longer, "
                f"including {hu['compile_s']:.0f} s to compile); once warmed up, snappea checks "
                f"{hp['warm_schedules_per_s']:,.0f} schedules per second and P {hu['warm_schedules_per_s']:,.1f}.", ""]
    out += [f"Same runner, same tests, schedule counts and seed. Wall clock, median of {snappea['runs']} runs. "
            "Speedup = upstream time / snappea time. snappea type-checks inside `check`; "
            "P compiles first (P compile). Warm = schedules per second after a one-schedule check's time.", "",
            "| workload | schedules | snappea time | P time | speedup | snappea sched/s | P sched/s | snappea warm | P warm | P compile |",
            "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|"]
    rows = [(r, upr.get(r["name"])) for r in snappea["results"]]
    rows.append(({"name": "**total**", **snappea["total"]}, up["total"]))
    same = [r["name"] for r in snappea["results"]] == [r["name"] for r in up["results"]]
    for p, u in rows:
        if not u or (p["name"] == "**total**" and not same):
            continue
        warm = lambda r: f"{r['warm_schedules_per_s']:,.1f}" if "warm_schedules_per_s" in r else ""
        out.append("| {} | {} | {:.3f} s | {:.3f} s | {:.1f}× | {:,.0f} | {:,.0f} | {} | {} | {:.2f} s |".format(
            p["name"], p["schedules"], p["check_s"], u["check_s"], u["check_s"] / p["check_s"],
            p["schedules_per_s"], u["schedules_per_s"], warm(p), warm(u), u["compile_s"]))
    out += ["", f"Runner: {snappea['runner']['cpu']}."]
    print("\n".join(out))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd_name", required=True)
    r = sub.add_parser("run")
    r.add_argument("--cmd", default="target/release/snappea-compat", help="the p command to benchmark")
    r.add_argument("--tool", default="snappea", help="label stored in the result: snappea or upstream")
    r.add_argument("--version", default="", help="version label stored in the result")
    r.add_argument("--commit", default="HEAD", help="commit the binary was built from (default HEAD)")
    r.add_argument("--out", required=True)
    r.add_argument("--runs", type=int, default=5)
    r.add_argument("--workers", type=int, default=1,
                   help="workers, with the snappea binary (default 1: the schedules upstream explores, comparable with history)")
    r.add_argument("--instructions", action="store_true", help="also count instructions with cachegrind")
    r.add_argument("--only", nargs="*", help="run only workloads whose project name contains one of these")
    r.set_defaults(fn=run)
    p = sub.add_parser("report")
    p.add_argument("--current", required=True)
    p.add_argument("--history")
    p.set_defaults(fn=report)
    c = sub.add_parser("compare")
    c.add_argument("snappea")
    c.add_argument("upstream")
    c.set_defaults(fn=compare)
    opts = ap.parse_args()
    opts.fn(opts)


if __name__ == "__main__":
    main()
