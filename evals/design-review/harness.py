#!/usr/bin/env python3
"""Design-review eval: does better P tooling help an agent find known design bugs?

    python harness.py prepare minio-dsync
    python harness.py start --task minio-dsync --arm snappea --run-id s-01 --model haiku
    python harness.py bridge s-01 <<'CMD'      # the solver's only tool
    ls
    CMD
    python harness.py finish s-01
    python harness.py grade-prompt s-01 > grade-prompt.txt   # give to a grader agent
    python harness.py record-grade s-01 grade.json
    python report.py

Arms differ only in the checker on PATH:
  snappea  `snappea` (this repo's CLI: guide, hints, JSON output)
  compat   `p` (snappea-compat: upstream P's command line and messages)
"""
from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
RUNS = HERE / "runs"
CACHE = HERE / "cache"
IMAGE = "debian:bookworm-slim"
OUTPUT_LIMIT = 16000
LIMITS = {"calls": 80, "seconds": 45 * 60, "command_seconds": 180}

ARMS = {
    "snappea": {"bin": "snappea", "dst": "/usr/local/bin/snappea",
                "tool_line": "The P checker is installed as `snappea`; run `snappea --help`."},
    "compat": {"bin": "snappea-compat", "dst": "/usr/local/bin/p",
               "tool_line": "The P checker is installed as `p`; run `p --help`."},
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

Complete the task in /work/TASK.md. When done, reply with a short summary."""


def task_dir(task: str) -> Path:
    return HERE / "tasks" / task


def container(run_id: str) -> str:
    return f"snappea-dr-{run_id}"


def binary(arm: str) -> Path:
    return Path(ROOT / "target/release" / ARMS[arm]["bin"])


def docker_args(work: Path, arm: str) -> list[str]:
    return ["docker", "run", "--network", "none", "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges", "--memory", "2g", "--cpus", "2",
            "--pids-limit", "256", "--user", "1000:1000", "-e", "HOME=/tmp",
            "--mount", f"type=bind,src={binary(arm)},dst={ARMS[arm]['dst']},readonly",
            "--mount", f"type=bind,src={work},dst=/work"]


def cmd_prepare(a):
    """Export the task's source files at the pinned commit into cache/<task>/src."""
    spec = json.loads((task_dir(a.task) / "task.json").read_text())
    repo = CACHE / "repos" / Path(spec["repo"]).name
    if not repo.exists():
        repo.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(["git", "clone", "--filter=blob:none", "--no-checkout", spec["repo"], str(repo)],
                       check=True)
    out = CACHE / a.task / "src"
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    for p in spec["paths"]:
        blob = subprocess.run(["git", "-C", str(repo), "show", f"{spec['commit']}:{p}"],
                              check=True, capture_output=True).stdout
        (out / p).parent.mkdir(parents=True, exist_ok=True)
        (out / p).write_bytes(blob)
    print(f"exported {len(spec['paths'])} files to {out}")


def cmd_start(a):
    run = RUNS / a.run_id
    if run.exists():
        sys.exit(f"run {a.run_id} exists")
    src = CACHE / a.task / "src"
    if not src.exists():
        sys.exit(f"run `python harness.py prepare {a.task}` first")
    if not binary(a.arm).exists():
        sys.exit(f"build {binary(a.arm)} first: cargo build --release -p snappea --bins")
    work = run / "work"
    shutil.copytree(src, work / "src")
    (work / "model").mkdir()
    task_md = (task_dir(a.task) / "TASK.md").read_text().replace("{tool_line}", ARMS[a.arm]["tool_line"])
    (work / "TASK.md").write_text(task_md)
    subprocess.run(["chown", "-R", "1000:1000", str(work)], check=False)
    subprocess.run([*docker_args(work, a.arm), "-d", "--name", container(a.run_id), "-w", "/work",
                    IMAGE, "sleep", "infinity"], check=True, capture_output=True)
    sha = subprocess.run(["sha256sum", str(binary(a.arm))], capture_output=True, text=True).stdout.split()[0]
    rev = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    meta = {"run_id": a.run_id, "task": a.task, "arm": a.arm, "model": a.model, "limits": LIMITS,
            "binary_sha256": sha, "snappea_commit": rev, "started": time.time()}
    (run / "meta.json").write_text(json.dumps(meta, indent=2))
    prompt = PROMPT.format(harness=HERE / "harness.py", run_id=a.run_id, calls=LIMITS["calls"],
                           minutes=LIMITS["seconds"] // 60, command_seconds=LIMITS["command_seconds"])
    (run / "prompt.txt").write_text(prompt)
    print(prompt)


def cmd_bridge(a):
    run = RUNS / a.run_id
    meta = json.loads((run / "meta.json").read_text())
    limits = meta["limits"]
    state_path = run / "bridge-state.json"
    state = json.loads(state_path.read_text()) if state_path.exists() else {"started": time.time(), "calls": 0}
    elapsed = time.time() - state["started"]
    if state.get("closed") or elapsed >= limits["seconds"] or state["calls"] >= limits["calls"]:
        print("Budget exhausted or submission frozen. Stop and reply with your summary.")
        return
    command = sys.stdin.read()
    state["calls"] += 1
    state_path.write_text(json.dumps(state))
    start = time.time()
    timeout = min(limits["command_seconds"], max(1, limits["seconds"] - elapsed))
    try:
        r = subprocess.run(["docker", "exec", "-w", "/work", container(a.run_id), "bash", "-c", command],
                           text=True, capture_output=True, timeout=timeout)
        result = {"exit_code": r.returncode, "stdout": r.stdout, "stderr": r.stderr}
    except subprocess.TimeoutExpired:
        subprocess.run(["docker", "exec", container(a.run_id), "pkill", "-u", "1000", "-f", "-x", ".*"],
                       capture_output=True)
        result = {"exit_code": 124, "stdout": "", "stderr": f"command timed out after {timeout:.0f}s"}
    with (run / "transcript.jsonl").open("a") as out:
        out.write(json.dumps({"call": state["calls"], "elapsed": round(elapsed, 1),
                              "seconds": round(time.time() - start, 1), "command": command,
                              "result": result}) + "\n")
    text = result["stdout"] + (("\n[stderr]\n" + result["stderr"]) if result["stderr"] else "")
    if len(text) > OUTPUT_LIMIT:
        text = text[:OUTPUT_LIMIT // 2] + "\n[... output truncated ...]\n" + text[-OUTPUT_LIMIT // 2:]
    left = limits["calls"] - state["calls"]
    print(f"{text}\n[exit {result['exit_code']}; {left} commands, "
          f"{max(0, limits['seconds'] - elapsed) / 60:.0f} min left]")


CHECK_RE = re.compile(r"(^|[;&|\s])(snappea|p)\s+(check|compile)\b")


def process_stats(run: Path) -> dict:
    """Mechanical facts from the transcript: checker use and outcomes."""
    calls = [json.loads(l) for l in (run / "transcript.jsonl").read_text().splitlines()] \
        if (run / "transcript.jsonl").exists() else []
    checks = [c for c in calls if CHECK_RE.search(c["command"])]
    out = lambda c: c["result"]["stdout"] + c["result"]["stderr"]
    bug_re = re.compile(r"Checker found a bug|\"result\":\s*\"bug\"|\bfound \d+ bug", re.I)
    return {
        "commands": len(calls),
        "minutes": round(calls[-1]["elapsed"] / 60, 1) if calls else 0,
        "checker_runs": len(checks),
        "checker_runs_ok_exit0": sum(c["result"]["exit_code"] == 0 for c in checks),
        "checker_runs_exit1": sum(c["result"]["exit_code"] == 1 for c in checks),
        "checker_runs_error": sum(c["result"]["exit_code"] not in (0, 1) for c in checks),
        "checker_runs_reporting_bug": sum(bool(bug_re.search(out(c))) and c["result"]["exit_code"] != 0
                                          for c in checks),
        "used_guide": any(re.search(r"snappea\s+guide", c["command"]) for c in calls),
    }


def final_check(run: Path, arm: str) -> dict:
    """Run the arm's checker on the frozen model in a fresh container."""
    model = run / "submission" / "model"
    if not model.exists() or not any(model.rglob("*.p")):
        return {"status": "no-model"}
    cmd = "cd /work/model && " + ("snappea check --json" if arm == "snappea" else "p check")
    try:
        r = subprocess.run([*docker_args(run / "submission", arm), "--rm", IMAGE, "bash", "-c", cmd],
                           text=True, capture_output=True, timeout=600)
    except subprocess.TimeoutExpired:
        return {"status": "timeout"}
    tail = (r.stdout + r.stderr)[-4000:]
    return {"status": {0: "pass", 1: "bug"}.get(r.returncode, "error"), "exit_code": r.returncode, "tail": tail}


def cmd_finish(a):
    run = RUNS / a.run_id
    meta = json.loads((run / "meta.json").read_text())
    state_path = run / "bridge-state.json"
    state = json.loads(state_path.read_text()) if state_path.exists() else {"calls": 0, "started": time.time()}
    state.update(closed=True, finished=time.time())
    state_path.write_text(json.dumps(state))
    subprocess.run(["docker", "rm", "-f", container(a.run_id)], capture_output=True)
    sub = run / "submission"
    if sub.exists():
        shutil.rmtree(sub)
    shutil.copytree(run / "work", sub, ignore=shutil.ignore_patterns("PCheckerOutput", ".snappea", "src"))
    subprocess.run(["chown", "-R", "1000:1000", str(sub)], check=False)
    if a.reply:
        reply = Path(a.reply).read_text() if Path(a.reply).is_file() else a.reply
        (run / "agent-final.txt").write_text(reply)
    stats = {"process": process_stats(run), "final_check": final_check(run, meta["arm"])}
    (run / "stats.json").write_text(json.dumps(stats, indent=2))
    print(json.dumps(stats, indent=2))


GRADER = """You are grading a design review. A reviewer was given the source of MinIO's
distributed locking subsystem at commit {commit} and asked to find design bugs. Below are
the answer key of known issues and the reviewer's report.

For each known issue K1..K{n}, decide:
- "found": the report describes this issue's mechanism (the cause) and its consequence.
- "partial": the report names the symptom or a closely related cause, but the mechanism is
  missing or materially wrong.
- "missed": otherwise.
Also note for each found/partial issue whether the report says the P checker produced a
counterexample for it ("checker_confirmed": true/false).
Then list the report's other claimed bugs that match no known issue, and classify each as
"valid" (a real problem in this code, explained), "invalid" (wrong, e.g. contradicted by the
key's "Not issues" section or the code), or "vague".

Judge the substance, not wording. One report section may cover several known issues, and
several sections may cover one. Do not reward listing every conceivable failure without a
mechanism.

Reply with only JSON:
{{"issues": {{"K1": {{"verdict": "found|partial|missed", "checker_confirmed": false, "evidence": "<quote or section title>"}}, ...}},
  "other_claims": [{{"title": "...", "class": "valid|invalid|vague", "why": "..."}}],
  "notes": "<one or two sentences>"}}

=== ANSWER KEY ===
{key}

=== REVIEWER REPORT ===
{report}
"""


def cmd_grade_prompt(a):
    run = RUNS / a.run_id
    meta = json.loads((run / "meta.json").read_text())
    spec = json.loads((task_dir(meta["task"]) / "task.json").read_text())
    report_path = run / "submission" / "REPORT.md"
    report = report_path.read_text() if report_path.exists() else "(no REPORT.md was written)"
    # Blind the grader to the arm.
    report = re.sub(r"\bsnappea\b", "<checker>", report, flags=re.I)
    report = re.sub(r"`p (check|compile)", r"`<checker> \1", report)
    key = (task_dir(meta["task"]) / "private" / "answer-key.md").read_text()
    print(GRADER.format(commit=spec["commit"][:8], n=spec["known_issues"], key=key, report=report))


def cmd_record_grade(a):
    run = RUNS / a.run_id
    text = Path(a.file).read_text() if Path(a.file).is_file() else a.file
    m = re.search(r"\{.*\}", text, re.S)
    grade = json.loads(m.group(0))
    issues = grade["issues"]
    grade["score"] = {
        "found": sum(v["verdict"] == "found" for v in issues.values()),
        "partial": sum(v["verdict"] == "partial" for v in issues.values()),
        "checker_confirmed": sum(v["verdict"] != "missed" and bool(v.get("checker_confirmed"))
                                 for v in issues.values()),
        "other_valid": sum(c["class"] == "valid" for c in grade.get("other_claims", [])),
        "other_invalid": sum(c["class"] == "invalid" for c in grade.get("other_claims", [])),
    }
    (run / "grade.json").write_text(json.dumps(grade, indent=2))
    print(json.dumps(grade["score"]))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(required=True)
    p = sub.add_parser("prepare"); p.add_argument("task"); p.set_defaults(fn=cmd_prepare)
    p = sub.add_parser("start")
    p.add_argument("--task", required=True); p.add_argument("--arm", required=True, choices=ARMS)
    p.add_argument("--run-id", required=True); p.add_argument("--model", default="unspecified")
    p.set_defaults(fn=cmd_start)
    p = sub.add_parser("bridge"); p.add_argument("run_id"); p.set_defaults(fn=cmd_bridge)
    p = sub.add_parser("finish"); p.add_argument("run_id"); p.add_argument("--reply")
    p.set_defaults(fn=cmd_finish)
    p = sub.add_parser("grade-prompt"); p.add_argument("run_id"); p.set_defaults(fn=cmd_grade_prompt)
    p = sub.add_parser("record-grade"); p.add_argument("run_id"); p.add_argument("file")
    p.set_defaults(fn=cmd_record_grade)
    a = ap.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
