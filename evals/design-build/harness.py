#!/usr/bin/env python3
"""Design-and-build agent eval: an agent designs and implements a distributed protocol
from a moderately detailed description, with snappea, the upstream-compatible `p` CLI,
or no checker. Hidden adversarial scenarios find the flaws.

    python harness.py start --task dlock --arm snappea --run-id dlock-snappea-01
    python harness.py bridge dlock-snappea-01 <<'CMD'      # the solver's only tool
    ls
    CMD
    python harness.py finish dlock-snappea-01
    python harness.py grade dlock-snappea-01
    python harness.py calibrate --task dlock
    python harness.py summary

The sandbox and bridge are shared with ../minimal-prompt. Arms:

- snappea: the snappea binary on PATH (`snappea guide` explains P and the workflow).
- compat:  snappea-compat on PATH as `p` (upstream P's CLI: `p compile`, `p check`).
- none:    no checker.
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(HERE.parent / "minimal-prompt"))
import harness as mp  # noqa: E402  (bridge, finish and sandbox helpers)

mp.RUNS = RUNS = HERE / "runs"
RELEASE = ROOT / "target/release"

ARMS = {
    "snappea": {"binary": RELEASE / "snappea", "dst": "/opt/bin/snappea",
                "line": "The `snappea` command is installed: a model checker for the P language. "
                        "Run `snappea guide` to see how to use it to design and check a protocol.\n\n"},
    "compat": {"binary": RELEASE / "snappea-compat", "dst": "/opt/bin/p",
               "line": "The `p` command is installed: the P language toolchain "
                       "(`p compile`, `p check`), for modeling and model checking protocols.\n\n"},
    "none": {"binary": None, "dst": None, "line": ""},
}

PROMPT = """You are working in a sandbox. Your only tool is a shell inside it. Every command,
including the first, must go through this bridge; a bare command runs on a different
machine where /work does not exist. Run each command with

    python3 {harness} bridge {run_id} <<'CMD'
    <your shell command>
    CMD

Do not use any other tool, and do not read or write files outside the sandbox: create
files from inside the command (for example `cat > file <<'EOF'`). The sandbox has no
network. You have {calls} commands and {minutes} minutes; each command may run for
{command_seconds} seconds.

{tool_line}Complete the task in /work/TASK.md. When done, reply with a short summary."""

# What each hidden test probes. A run's flaws are the classes with a failing test.
FLAW_CLASSES = {
    "restart-amnesia": ["test_restarted_server_does_not_regrant_a_held_lock",
                        "test_whole_cluster_restart_while_held"],
    "stale-lock-liveness": ["test_crashed_holder_is_cleaned_up", "test_lost_releases_are_cleaned_up",
                            "test_lost_grant_reply_does_not_strand_the_lock",
                            "test_restarted_servers_recover", "test_writer_after_lossy_period"],
    "partial-purge": ["test_partially_reached_holder_is_not_purged"],
    "quorum-intersection": ["test_even_cluster_writers_need_overlapping_quorums",
                            "test_reader_and_writer_quorums_intersect"],
    "late-grant": ["test_late_grants_are_not_used", "test_slow_grant_still_pending"],
    "duplicate-grant": ["test_duplicated_grant_counts_once"],
    "stale-release": ["test_delayed_release_does_not_free_a_later_hold"],
    "read-sharing": ["test_readers_share", "test_independent_resources"],
    "clock-drift-or-lost-lease": ["test_long_hold_is_kept", "test_cut_off_holder_steps_down",
                                  "test_holder_that_reaches_a_minority_steps_down",
                                  "test_slow_renewals_are_not_trusted", "test_long_hold_after_heal_is_kept",
                                  "test_late_replies_then_silence"],
    "restart-under-long-hold": ["test_restart_under_a_long_hold"],
    "contention-liveness": ["test_competing_writers_take_turns", "test_split_vote_resolves",
                            "test_minority_down", "test_single_writer_acquires_and_releases"],
    "fuzz-safety": ["test_random_faults_safety", "test_random_faults_long_holds"],
}


def flaw_class(test: str) -> str:
    base = test.split("[")[0]
    return next((c for c, tests in FLAW_CLASSES.items() if base in tests), "other")


def cmd_start(a):
    run = RUNS / a.run_id
    if run.exists():
        sys.exit(f"run {a.run_id} exists; pick another id")
    arm = ARMS[a.arm]
    task = HERE / "tasks" / a.task / "public"
    work = run / "work"
    shutil.copytree(task, work)
    os.chmod(work, 0o777)
    for p in work.rglob("*"):
        os.chmod(p, 0o777 if p.is_dir() else 0o666)
    args = ["docker", "run", "--network", "none", "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges", "--memory", "1g", "--cpus", "2",
            "--pids-limit", "256", "--user", "1000:1000",
            "--mount", f"type=bind,src={work},dst=/work"]
    meta = {"run_id": a.run_id, "task": a.task, "arm": a.arm, "model": a.model,
            "limits": {"calls": a.calls, "seconds": a.minutes * 60, "command_seconds": a.command_seconds},
            "task_hash": mp.tree_hash(task), "created": time.time()}
    if arm["binary"]:
        args += ["--mount", f"type=bind,src={arm['binary']},dst={arm['dst']},readonly"]
        meta["binary_sha256"] = hashlib.sha256(arm["binary"].read_bytes()).hexdigest()
    else:
        args += ["-e", "SNAPPEA_BIN="]
    subprocess.run([*args, "-d", "--name", mp.container(a.run_id), mp.IMAGE, "sleep", "infinity"],
                   check=True, capture_output=True)
    prompt = PROMPT.format(harness=HERE / "harness.py", run_id=a.run_id, calls=a.calls,
                           minutes=a.minutes, command_seconds=a.command_seconds, tool_line=arm["line"])
    (run / "prompt.txt").write_text(prompt)
    meta["prompt_sha256"] = hashlib.sha256(prompt.encode()).hexdigest()
    (run / "meta.json").write_text(json.dumps(meta, indent=2))
    print(prompt)


def hidden_results(task: str, solution_files: dict) -> dict:
    """Run the hidden tests against a solution (dest -> src) in a fresh sandbox."""
    t = HERE / "tasks" / task
    files = {"sim.py": t / "public" / "sim.py", "test_hidden.py": t / "private" / "test_hidden.py",
             **solution_files}
    r = mp.run_in_sandbox(files, mp.PYTEST + ["test_hidden.py"], timeout=900)
    cases = {c.split("::")[1]: s for c, s in r["cases"].items()}
    failed = sorted(c for c, s in cases.items() if s != "passed")
    out = {"passed": sum(s == "passed" for s in cases.values()), "total": len(cases), "failed": failed,
           "flaws": sorted({flaw_class(c) for c in failed}), "timed_out": r["timed_out"]}
    if not cases:
        out["error"] = r["output"][-3000:]
    return out


def process_stats(run: Path) -> dict:
    calls = [json.loads(line) for line in (run / "transcript.jsonl").read_text().splitlines()] \
        if (run / "transcript.jsonl").exists() else []
    cmds = [c["command"] for c in calls]
    sub = run / "submission"
    p_files = list(sub.rglob("*.p")) if sub.exists() else []
    return {
        "commands": len(calls),
        "minutes": round((calls[-1]["elapsed"] + calls[-1]["seconds"]) / 60, 1) if calls else 0,
        "checker_runs": sum(bool(re.search(r"(^|[\s;&|(])(snappea|p)\s+(check|compile|run|test)\b", c))
                            for c in cmds),
        "checker_failures": sum(bool(re.search(r"(^|[\s;&|(])(snappea|p)\s+check\b", c["command"]))
                                and c["result"]["exit_code"] != 0 for c in calls),
        "p_files": len(p_files),
        "p_lines": sum(len(p.read_text(errors="replace").splitlines()) for p in p_files),
        "own_tests": sum(len(re.findall(r"^\s*def test_", p.read_text(errors="replace"), re.M))
                         for p in sub.rglob("test*.py")) if sub.exists() else 0,
        "design_md": (sub / "DESIGN.md").exists(),
    }


def cmd_grade(a):
    run = RUNS / a.run_id
    meta = json.loads((run / "meta.json").read_text())
    sub = run / "submission"
    if not (sub / "solution.py").exists():
        grade = {"hidden": {"passed": 0, "total": None, "flaws": ["no-solution"]}}
    else:
        # the solution and any helper modules it imports; not the solver's tests or sim.py
        files = {p.name: p for p in sub.glob("*.py")
                 if not p.name.startswith("test") and p.name not in ("conftest.py", "sim.py")}
        grade = {"hidden": hidden_results(meta["task"], files)}
        own = mp.run_in_sandbox(mp.submission_files(sub), mp.PYTEST, timeout=600)
        grade["own_tests"] = {"passed": sum(s == "passed" for s in own["cases"].values()),
                              "total": len(own["cases"])}
    grade["process"] = process_stats(run)
    grade["arm"], grade["task"], grade["model"] = meta["arm"], meta["task"], meta.get("model")
    (run / "grade.json").write_text(json.dumps(grade, indent=2))
    print(json.dumps(grade, indent=2))


def cmd_calibrate(a):
    """The reference must pass every hidden test; every mutant and baseline must fail one."""
    t = HERE / "tasks" / a.task / "private"
    mutants = mp.load_module(t / "mutants.py")
    ref = (t / "reference.py").read_text()
    cases = [("reference", {"solution.py": ref})]
    cases += [(name, {"solution.py": mutants.apply(ref, edits)}) for name, _, edits in mutants.MUTANTS]
    for name, files in getattr(mutants, "BASELINES", {}).items():
        cases.append((name, {dest: t / src for dest, src in files.items()}))
    results = dict(zip([c[0] for c in cases], mp.parallel(lambda c: hidden_results(a.task, c[1]), cases)))
    ok = results["reference"]["failed"] == [] and results["reference"]["total"] > 0
    for name, r in results.items():
        bad = (name == "reference") == bool(r["failed"]) or r.get("error")
        ok &= not bad
        print(f"{'FAIL' if bad else 'ok  '} {name:24} {r['passed']}/{r['total']}  {' '.join(r['flaws'])}")
        if r.get("error"):
            print(r["error"])
    sys.exit(0 if ok else 1)


def cmd_summary(a):
    rows = []
    for run in sorted(RUNS.glob("*")):
        g = run / "grade.json"
        if g.exists():
            g = json.loads(g.read_text())
            h, p = g["hidden"], g["process"]
            rows.append(f"{run.name:28} {g['arm']:8} {h['passed']:>3}/{h['total'] or '-':<3} "
                        f"checks={p['checker_runs']:<3} p_lines={p['p_lines']:<4} "
                        f"flaws={','.join(h.get('flaws', [])) or '-'}")
    print("\n".join(rows) or "no graded runs")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("start")
    s.add_argument("--task", default="dlock")
    s.add_argument("--arm", choices=sorted(ARMS), required=True)
    s.add_argument("--run-id", required=True)
    s.add_argument("--model", default=None)
    s.add_argument("--calls", type=int, default=120)
    s.add_argument("--minutes", type=int, default=90)
    s.add_argument("--command-seconds", type=int, default=300)
    s.set_defaults(fn=cmd_start)
    b = sub.add_parser("bridge")
    b.add_argument("run_id")
    b.set_defaults(fn=mp.cmd_bridge)
    f = sub.add_parser("finish")
    f.add_argument("run_id")
    f.add_argument("--reply", default=None)
    f.set_defaults(fn=mp.cmd_finish)
    g = sub.add_parser("grade")
    g.add_argument("run_id")
    g.set_defaults(fn=cmd_grade)
    c = sub.add_parser("calibrate")
    c.add_argument("--task", default="dlock")
    c.set_defaults(fn=cmd_calibrate)
    sub.add_parser("summary").set_defaults(fn=cmd_summary)
    a = ap.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
