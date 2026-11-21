#!/usr/bin/env python3
"""Model-gaps eval: does the checker lead an agent to the bugs a passing model hides?

    python harness.py calibrate                 # every task: starter passes, reference finds each bug
    python harness.py start --task job-queue --arm snappea --run-id a-jq-snappea-01 --model haiku
    python harness.py bridge a-jq-snappea-01 <<'CMD'     # the solver's only tool
    ls
    CMD
    python harness.py finish a-jq-snappea-01
    python harness.py grade a-jq-snappea-01
    python report.py a-

Arms differ only in the checker on PATH:
  snappea  `snappea` (this repo's CLI: guide, lint, coverage, driver limits, mutation)
  compat   `p` (snappea-compat: upstream P's command line and messages)
"""
from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
import tomllib
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
RUNS = HERE / "runs"
IMAGE = "python:3.12-slim"
OUTPUT_LIMIT = 16000
LIMITS = {"calls": 40, "seconds": 20 * 60, "command_seconds": 120}
GRADE_TIMEOUT = "10s"
MODEL_DIRS = ("PSrc", "PSpec", "PTst")

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

TASK = """# Is this design ready to implement?

`/work/DESIGN.md` describes a design and the contract it must keep. `/work/model` is a
P model of it:

- `PSrc/`: the design, as the team plans to implement it.
- `PSpec/`: specs for the contract.
- `PTst/`: the environment and the test cases.

Every test case passes, so the team believes the design is correct. Find out whether
it is. Look for design bugs: behaviors of the design in `PSrc/` that break the contract
in DESIGN.md under the conditions DESIGN.md says can happen. {tool_line}

Deliverables:

- For each design bug, a test case that fails because of it.
- `/work/FINDINGS.md`: one section per design bug, with the test case that shows it,
  the sequence of events, and the fix.

Rules:

- Add or change anything in `PSpec/` and `PTst/`. Put new machines, specs and tests
  there, not in `PSrc/`.
- You may fix a bug in `PSrc/` once a test shows it, so it does not hide the next
  one. Do not change event declarations or what a machine takes when created.
- Your `PSpec/` and `PTst/` will be run against other versions of the design,
  including a correct one. A test that fails on the correct design counts against
  you; a test that only passes proves nothing.
"""


BUILD_TASK = """# Design this component in P

`/work/DESIGN.md` describes a component and the contract it must keep. `/work/model`
is the start of a P model of it:

- `PSrc/`: the design. The events and the machines' names are fixed; the machines are
  empty. Write them.
- `PSpec/`: specs for the contract. They observe events your design must announce.
- `PTst/`: an environment and first test cases.

Write the design in `PSrc/` so that the contract holds under everything DESIGN.md
says can happen. {tool_line}

Your `PSrc/` will be judged by running it, with the given `PSpec/`, against hidden
test cases that cover the conditions in DESIGN.md. Do not change event declarations,
machine names or what a machine takes when created, and keep `PSpec/` as it is. Add
whatever you need to `PTst/`.

Also write `/work/NOTES.md`: the design decisions you made and why, and any problem a
check found and how you fixed it.
"""


GUARD_TASK = """# Make this model catch design mistakes

`/work/DESIGN.md` describes a component and the contract it must keep. `/work/model` is
a P model of it:

- `PSrc/`: the design. It is correct today.
- `PSpec/`: specs for the contract. They may be incomplete.
- `PTst/`: the environment and the test cases. They may be incomplete.

The team will keep changing the design. Make the specs and test cases strong enough
that a change to `PSrc/` that breaks the contract in DESIGN.md, under the conditions
DESIGN.md says can happen, makes a test case fail. {tool_line}

Your `PSpec/` and `PTst/` will be run against the design as it is, and against hidden
versions of `PSrc/` that each carry one realistic design mistake. You score one point
for each mistake that some test case catches. A test case that fails on the correct
design is discarded and counts against you.

Rules:

- Add or change anything in `PSpec/` and `PTst/`. Put new machines, specs and tests
  there.
- Do not change `PSrc/`. Your work is run against other versions of it, with the same
  events, machine names and creation inputs.

Also write `/work/NOTES.md`: what you added and which mistakes each addition catches.
"""


HARDEN_TASK = """# Make this design keep its contract

`/work/DESIGN.md` describes a component and the contract it must keep. `/work/model` is
a P model of it:

- `PSrc/`: the design, as the team plans to implement it.
- `PSpec/`: specs for the contract.
- `PTst/`: the environment and the test cases.

Every test case passes, so the team believes the design is correct. It may not be.
Find the behaviors of the design in `PSrc/` that break the contract in DESIGN.md under
the conditions DESIGN.md says can happen, and fix them in `PSrc/`. {tool_line}

Your `PSrc/` will be judged by running it against hidden specs and test cases that
cover the contract and the conditions in DESIGN.md. Do not change event declarations,
machine names or what a machine takes when created. Add or change anything in
`PSpec/` and `PTst/`.

Also write `/work/NOTES.md`: each problem you found, how you found it, and the fix.
"""

OWN_TASK = """# Design this component in P, and leave a model that protects it

`/work/DESIGN.md` describes a component and the contract it must keep. `/work/model`
is the start of a P model of it:

- `PSrc/`: the design. The events and the machines' names are fixed; the machines are
  empty. Write them.
- `PSpec/`: specs for the contract. They may be incomplete.
- `PTst/`: an environment and first test cases. They may be incomplete.

Do two things. {tool_line}

1. Write the design in `PSrc/` so that the contract holds under everything DESIGN.md
   says can happen.
2. The team will keep changing the design after you. Make the specs and test cases
   strong enough that a change to `PSrc/` that breaks the contract, under the
   conditions DESIGN.md says can happen, makes a test case fail.

Your work is judged twice:

- Your `PSrc/` is run, with the given `PSpec/`, against hidden test cases that cover
  the conditions in DESIGN.md.
- Your `PSpec/` and `PTst/` are run against a correct design of the team's, and
  against hidden versions of it that each carry one realistic design mistake. You
  score one point for each mistake that some test case catches. A test case that
  fails on the correct design is discarded and counts against you.

Rules:

- Do not change event declarations, machine names or what a machine takes when
  created. The team's designs use the same ones.
- Keep the given specs; add new specs in new files in `PSpec/`. Add whatever you need
  to `PTst/`.
- Specs and tests may rely only on the events and on what DESIGN.md promises, not on
  how your own `PSrc/` happens to work.

Also write `/work/NOTES.md`: the design decisions you made and why, and which mistakes
your specs and tests catch.
"""

MODES = ("review", "build", "harden", "impl", "guard", "own")


def task_dir(task: str) -> Path:
    return HERE / "tasks" / task


def load_task(task: str) -> dict:
    path = task_dir(task) / "private" / "bugs.toml"
    spec = tomllib.loads(path.read_text()) if path.exists() else {}
    spec.setdefault("bug", [])
    spec["name"] = task
    return spec


def design(task: str, bug_ids: list[str]) -> dict[str, str]:
    """The fixed design's files with the named bugs written in."""
    spec = load_task(task)
    files = {p.name: p.read_text() for p in sorted((task_dir(task) / "private" / "PSrc").glob("*.p"))}
    for bug in spec["bug"]:
        if bug["id"] not in bug_ids:
            continue
        for edit in bug["edit"]:
            text = files[edit["file"]]
            if text.count(edit["old"]) != 1:
                sys.exit(f"{task} {bug['id']}: `old` text must occur once in {edit['file']}")
            files[edit["file"]] = text.replace(edit["old"], edit["new"])
    return files


def bug_ids(task: str) -> list[str]:
    return [b["id"] for b in load_task(task)["bug"]]


def write_model(dst: Path, task: str, bugs: list[str], tests: Path):
    """A project at dst: the design with `bugs`, and PSpec/PTst copied from `tests`."""
    (dst / "PSrc").mkdir(parents=True)
    for name, text in design(task, bugs).items():
        (dst / "PSrc" / name).write_text(text)
    for d in ("PSpec", "PTst"):
        if (tests / d).exists():
            shutil.copytree(tests / d, dst / d)
    # Grade what the tests find, not what they declare they expect to find.
    for p in list((dst / "PSpec").rglob("*.p")) + list((dst / "PTst").rglob("*.p")):
        p.write_text(re.sub(r"^\s*//\s*snappea:\s*expect-bug.*$", "", p.read_text(), flags=re.M))
    (dst / "Model.pproj").write_text(
        "<Project>\n<ProjectName>Model</ProjectName>\n<InputFiles>\n"
        + "".join(f"\t<PFile>./{d}/</PFile>\n" for d in MODEL_DIRS) + "</InputFiles>\n</Project>\n")


def run_tests(model: Path, timeout: str = GRADE_TIMEOUT) -> dict:
    """Each test case's result with the snappea checker: ok, bug, or an error for the project."""
    r = subprocess.run([str(ROOT / "target/release/snappea"), "check", "Model.pproj", "--json", "--no-mutation",
                        "--warn", "all", "--no-driver-facts", "--timeout", timeout, "--seed", "1", "--max-bugs", "1"],
                       cwd=model, text=True, capture_output=True)
    try:
        out = json.loads(r.stdout)
    except json.JSONDecodeError:
        return {"error": (r.stdout + r.stderr)[-1500:]}
    if not out.get("test_cases"):
        return {"error": json.dumps(out.get("summary", out))[:1500]}
    return {"tests": {t["name"]: {"result": "ok" if t["result"] == "ok" else "bug",
                                  "message": (t.get("bug") or {}).get("message", t.get("message", ""))}
                      for t in out["test_cases"]}}


def grade_tests(task: str, tests: Path) -> dict:
    """Run `tests` (a folder with PSpec and PTst) against the fixed design and each single-bug design."""
    ids = bug_ids(task)
    results = {}
    with tempfile.TemporaryDirectory() as tmp:
        for name, bugs in [("fixed", []), *((b, [b]) for b in ids), ("public", ids)]:
            write_model(Path(tmp) / name, task, bugs, tests)
            results[name] = run_tests(Path(tmp) / name)
    fixed = results["fixed"]
    if "error" in fixed:
        return {"status": "error", "error": fixed["error"], "found": [], "false_alarms": [], "tests": 0}
    sound = {t for t, r in fixed["tests"].items() if r["result"] == "ok"}
    found = {}
    for b in ids:
        hits = sorted(t for t, r in results[b].get("tests", {}).items() if r["result"] == "bug" and t in sound)
        if hits:
            found[b] = hits
    public = results["public"].get("tests", {})
    return {
        "status": "ok",
        "found": sorted(found),
        "found_by": found,
        # Fails on the correct design: the test or spec is wrong, or asks more than the contract.
        "false_alarms": sorted(t for t, r in fixed["tests"].items() if r["result"] != "ok"),
        "false_alarm_messages": {t: r["message"] for t, r in fixed["tests"].items() if r["result"] != "ok"},
        # Passes on the correct design and fails on the public one, but on no single bug.
        "fails_public_only": sorted(t for t in sound if public.get(t, {}).get("result") == "bug"
                                    and not any(t in h for h in found.values())),
        "tests": len(fixed["tests"]),
        "bugs": len(ids),
    }


def grade_design(task: str, psrc: Path) -> dict:
    """Run the hidden reference tests against a solver's design."""
    ref = task_dir(task) / "private" / "reference"
    with tempfile.TemporaryDirectory() as tmp:
        model = Path(tmp) / "m"
        write_model(model, task, [], ref)
        shutil.rmtree(model / "PSrc")
        shutil.copytree(psrc, model / "PSrc")
        res = run_tests(model)
    if "error" in res:
        return {"status": "error", "error": res["error"], "passed": [], "failed": {}, "tests": 0}
    return {"status": "ok", "tests": len(res["tests"]),
            "passed": sorted(t for t, r in res["tests"].items() if r["result"] == "ok"),
            "failed": {t: r["message"] for t, r in res["tests"].items() if r["result"] != "ok"}}


def grade_impl(task: str, submission: Path) -> dict:
    """Run the task's own grader on a frozen /work; it prints {"passed": [...], "failed": {...}}."""
    r = subprocess.run([sys.executable, str(task_dir(task) / "private" / "grade.py"), str(submission)],
                       text=True, capture_output=True, timeout=900)
    try:
        g = json.loads(r.stdout)
    except json.JSONDecodeError:
        return {"status": "error", "error": (r.stdout + r.stderr)[-1500:], "passed": [], "failed": {}, "tests": 0}
    return {"status": "ok", "tests": len(g["passed"]) + len(g["failed"]), "passed": sorted(g["passed"]), "failed": g["failed"]}


def task_modes(task: str) -> list[str]:
    if (task_dir(task) / "private" / "grade.py").exists():
        return ["impl"]
    return load_task(task).get("modes") or (["review", "build"] if (task_dir(task) / "skeleton").exists() else ["review"])


def calibrate_impl(task: str) -> bool:
    sol = grade_impl(task, task_dir(task) / "private" / "solution")
    start = grade_impl(task, task_dir(task) / "public")
    ok = sol["status"] == "ok" and not sol["failed"] and sol["tests"] > 0 and (start["status"] != "ok" or start["failed"])
    print(f"{task}: impl, solution passes {len(sol['passed'])}/{sol['tests']}, starter passes "
          f"{len(start['passed'])}/{start['tests']}: " + ("ok" if ok else f"FAILED {sol} {start}"))
    return ok


def cmd_calibrate(a):
    ok = True
    for task in a.tasks or sorted(p.name for p in (HERE / "tasks").iterdir()):
        if "impl" in task_modes(task):
            ok = calibrate_impl(task) and ok
            continue
        ids = bug_ids(task)
        start = grade_tests(task, task_dir(task) / "public")
        ref = grade_tests(task, task_dir(task) / "private" / "reference")
        problems = []
        if start["status"] != "ok" or start["found"] or start["false_alarms"] or start["fails_public_only"]:
            problems.append(f"starter must pass on every design: {start}")
        if ref["status"] != "ok" or ref["found"] != sorted(ids) or ref["false_alarms"]:
            problems.append(f"reference must pass on the fixed design and find each bug: {ref}")
        if "build" in task_modes(task):
            with tempfile.TemporaryDirectory() as tmp:
                model = Path(tmp) / "m"
                write_model(model, task, [], task_dir(task) / "public")
                shutil.rmtree(model / "PSrc")
                shutil.copytree(task_dir(task) / "skeleton", model / "PSrc")
                sk = run_tests(model, "2s")
            if "error" in sk:
                problems.append(f"skeleton must compile with the public specs and tests: {sk['error']}")
        print(f"{task} [{', '.join(task_modes(task))}]: {len(ids)} bugs, starter {start['tests']} tests, reference {ref['tests']} tests: "
              + ("ok" if not problems else "FAILED"))
        for b in ids:
            print(f"  {b}: {', '.join(ref.get('found_by', {}).get(b, ['NOT FOUND']))}")
        for p in problems:
            ok = False
            print("  " + p)
    sys.exit(0 if ok else 1)


def container(run_id: str) -> str:
    return f"snappea-mg-{run_id}"


WRAPPER = """#!/bin/sh
# Adds default flags to `check`, to try a different default output without rebuilding.
if [ "$1" = check ]; then
  shift
  case "$1" in mutation|lint|coverage) exec /usr/local/lib/checker-real check "$@" ;; esac
  exec /usr/local/lib/checker-real check {flags} "$@"
fi
exec /usr/local/lib/checker-real "$@"
"""


def docker_args(work: Path, arm: str, bin_path: Path, env: list[str] = (), wrapper: Path | None = None) -> list[str]:
    if wrapper:
        mounts = ["--mount", f"type=bind,src={bin_path},dst=/usr/local/lib/checker-real,readonly",
                  "--mount", f"type=bind,src={wrapper},dst={ARMS[arm]['dst']},readonly"]
    else:
        mounts = ["--mount", f"type=bind,src={bin_path},dst={ARMS[arm]['dst']},readonly"]
    return ["docker", "run", *(x for e in env for x in ("-e", e)), "--network", "none", "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges", "--memory", "2g", "--cpus", "1",
            "--pids-limit", "256", "--user", "1000:1000", "-e", "HOME=/tmp",
            *mounts, "--mount", f"type=bind,src={work},dst=/work"]


def cmd_start(a):
    run = RUNS / a.run_id
    if run.exists():
        sys.exit(f"run {a.run_id} exists")
    bin_path = Path(a.binary).resolve() if a.binary else ROOT / "target/release" / ARMS[a.arm]["bin"]
    if not bin_path.exists():
        sys.exit(f"build {bin_path} first: cargo build --release -p snappea --bins")
    if a.mode == "own" and not ((task_dir(a.task) / "skeleton").exists() and load_task(a.task).get("bug")):
        sys.exit(f"task {a.task} needs a skeleton and seeded mistakes for own mode")
    if a.mode not in (*task_modes(a.task), "own") and not (a.mode == "guard" and load_task(a.task).get("bug")):
        sys.exit(f"task {a.task} has modes {task_modes(a.task)}")
    work = run / "work"
    public = task_dir(a.task) / "public"
    if a.mode == "impl":
        # The task brings its whole /work, with its own TASK.md.
        shutil.copytree(public, work)
        text = (work / "TASK.md").read_text()
    else:
        write_model(work / "model", a.task, bug_ids(a.task) if a.mode in ("review", "harden") else [], public)
        if a.mode in ("build", "own"):
            shutil.rmtree(work / "model" / "PSrc")
            shutil.copytree(task_dir(a.task) / "skeleton", work / "model" / "PSrc")
        shutil.copy(public / "DESIGN.md", work / "DESIGN.md")
        # A task may word its own assignment, e.g. when its specs are incomplete on purpose.
        own = public / "TASK.md"
        text = GUARD_TASK if a.mode == "guard" else OWN_TASK if a.mode == "own" else own.read_text() if own.exists() else {
            "review": TASK, "build": BUILD_TASK, "harden": HARDEN_TASK}[a.mode]
    (work / "TASK.md").write_text(text.replace("{tool_line}", ARMS[a.arm]["tool_line"]))
    workdir = "/work" if a.mode == "impl" else "/work/model"
    wrapper = None
    if a.check_flags:
        wrapper = run / "checker-wrapper.sh"
        wrapper.write_text(WRAPPER.replace("{flags}", a.check_flags))
        wrapper.chmod(0o755)
    subprocess.run(["chown", "-R", "1000:1000", str(work)], check=False)
    subprocess.run([*docker_args(work, a.arm, bin_path, a.env, wrapper), "-d", "--name", container(a.run_id),
                    "-w", workdir, IMAGE, "sleep", "infinity"], check=True, capture_output=True)
    sha = hashlib.sha256(bin_path.read_bytes()).hexdigest()
    rev = subprocess.run(["git", "-C", str(ROOT), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    meta = {"run_id": a.run_id, "task": a.task, "mode": a.mode, "arm": a.arm, "label": a.label or a.arm, "binary": str(bin_path),
            "env": a.env, "check_flags": a.check_flags, "workdir": workdir, "model": a.model, "limits": LIMITS, "binary_sha256": sha, "snappea_commit": rev,
            "started": time.time()}
    (run / "meta.json").write_text(json.dumps(meta, indent=2))
    prompt = PROMPT.format(harness=HERE / "harness.py", run_id=a.run_id, calls=LIMITS["calls"],
                           minutes=LIMITS["seconds"] // 60, command_seconds=LIMITS["command_seconds"])
    (run / "prompt.txt").write_text(prompt)
    print(prompt)


def snapshot(run: Path, call: int):
    """Keep the model's sources after a call, when they changed, to date each finding."""
    model = run / "work" / "model"
    files = sorted(p for d in MODEL_DIRS for p in (model / d).rglob("*.p")) if model.exists() else []
    digest = hashlib.sha256(b"".join(str(p).encode() + p.read_bytes() for p in files)).hexdigest()
    last = run / "snapshots" / "last"
    if last.exists() and last.read_text() == digest:
        return
    dst = run / "snapshots" / f"{call:03d}"
    for p in files:
        (dst / p.relative_to(model)).parent.mkdir(parents=True, exist_ok=True)
        shutil.copy(p, dst / p.relative_to(model))
    last.parent.mkdir(exist_ok=True)
    last.write_text(digest)


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
        r = subprocess.run(["docker", "exec", "-w", meta.get("workdir", "/work/model"), container(a.run_id), "bash", "-c", command],
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
    snapshot(run, state["calls"])
    text = result["stdout"] + (("\n[stderr]\n" + result["stderr"]) if result["stderr"] else "")
    if len(text) > OUTPUT_LIMIT:
        text = text[:OUTPUT_LIMIT // 2] + "\n[... output truncated ...]\n" + text[-OUTPUT_LIMIT // 2:]
    left = limits["calls"] - state["calls"]
    print(f"{text}\n[exit {result['exit_code']}; {left} commands, "
          f"{max(0, limits['seconds'] - elapsed) / 60:.0f} min left]")


CHECK_RE = re.compile(r"(^|[;&|\s])(snappea|p)\s+(check|compile)\b")
# What a command was for, first match wins. Used to see where a run's effort went.
KINDS = [
    ("guide", r"\bsnappea\s+guide\b"),
    ("help", r"--help\b"),
    ("mutation", r"check\s+mutation|--deny\s+mutation|--mutation-timeout"),
    ("lint", r"check\s+lint"),
    ("replay", r"\b(snappea|p)\s+replay\b"),
    ("check", r"\b(snappea|p)\s+(check|compile)\b"),
    ("write", r"cat\s*>|tee\s|sed\s+-i|python3?\s|perl\s+-[a-z]*i|>>|\bmv\b|\bcp\b|\bpatch\b"),
]


def kind(command: str) -> str:
    return next((k for k, pat in KINDS if re.search(pat, command)), "read")


def process_stats(run: Path) -> dict:
    """Mechanical facts from the transcript: what the commands did and what the checker said."""
    path = run / "transcript.jsonl"
    calls = [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
    out = lambda c: c["result"]["stdout"] + c["result"]["stderr"]
    checks = [c for c in calls if CHECK_RE.search(c["command"])]
    kinds = {}
    for c in calls:
        kinds[kind(c["command"])] = kinds.get(kind(c["command"]), 0) + 1
    seen = lambda pat: sum(bool(re.search(pat, out(c))) for c in checks)
    return {
        "commands": len(calls),
        "minutes": round((calls[-1]["elapsed"] + calls[-1]["seconds"]) / 60, 1) if calls else 0,
        "kinds": kinds,
        "checker_runs": len(checks),
        "checker_runs_error": sum(c["result"]["exit_code"] not in (0, 1) for c in checks),
        "checker_runs_with_bug": seen(r"(?m)Checker found a bug|^FAIL\s|\b[Ff]ound [1-9]\d* bugs?\b|Assertion Failed|liveness bug"),
        "checker_runs_with_lint": seen(r"(?m)^(error|warning)\[[a-z-]+\]|lint finding"),
        "checker_runs_with_mutants": seen(r"Not detected by any test case"),
        "checker_runs_with_coverage_gaps": seen(r"never (handled|taken|true|false)|not reached"),
    }


def first_seen(run: Path, tests: list[str]) -> int | None:
    """The first call after which any of these test names was in the model."""
    for snap in sorted(p for p in (run / "snapshots").glob("[0-9]*")):
        text = "".join(p.read_text() for p in snap.rglob("*.p"))
        if any(re.search(rf"\btest\s+{re.escape(t)}\b", text) for t in tests):
            return int(snap.name)
    return None


def cmd_finish(a):
    run = RUNS / a.run_id
    state_path = run / "bridge-state.json"
    state = json.loads(state_path.read_text()) if state_path.exists() else {"calls": 0, "started": time.time()}
    state.update(closed=True, finished=time.time())
    state_path.write_text(json.dumps(state))
    subprocess.run(["docker", "rm", "-f", container(a.run_id)], capture_output=True)
    sub = run / "submission"
    if sub.exists():
        shutil.rmtree(sub)
    shutil.copytree(run / "work", sub, ignore=shutil.ignore_patterns("PCheckerOutput", ".snappea", "PGenerated"))
    if a.reply:
        reply = Path(a.reply).read_text() if Path(a.reply).is_file() else a.reply
        (run / "agent-final.txt").write_text(reply)
    print(f"froze {sub}")


def cmd_grade(a):
    for run_id in a.run_ids:
        run = RUNS / run_id
        meta = json.loads((run / "meta.json").read_text())
        if meta.get("mode") in ("build", "harden", "impl", "own"):
            if meta["mode"] == "impl":
                grade = grade_impl(meta["task"], run / "submission")
            else:
                grade = grade_design(meta["task"], run / "submission" / "model" / "PSrc")
            grade["process"] = process_stats(run)
            if meta["mode"] == "own":
                # The second judgment: the mistakes the specs and tests left behind catch.
                grade["guard"] = grade_tests(meta["task"], run / "submission" / "model")
            (run / "grade.json").write_text(json.dumps(grade, indent=2))
            print(run_id, json.dumps({k: grade[k] for k in ("status", "passed", "failed")})[:1500],
                  json.dumps({k: grade["guard"][k] for k in ("found", "false_alarms")}) if "guard" in grade else "")
            continue
        grade = grade_tests(meta["task"], run / "submission" / "model")
        grade["found_at_call"] = {b: first_seen(run, tests) for b, tests in grade.get("found_by", {}).items()}
        grade["process"] = process_stats(run)
        findings = run / "submission" / "FINDINGS.md"
        grade["findings_sections"] = len(re.findall(r"(?m)^##? ", findings.read_text())) if findings.exists() else 0
        (run / "grade.json").write_text(json.dumps(grade, indent=2))
        print(run_id, json.dumps({k: grade[k] for k in ("status", "found", "false_alarms", "tests")}))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(required=True)
    p = sub.add_parser("calibrate"); p.add_argument("tasks", nargs="*"); p.set_defaults(fn=cmd_calibrate)
    p = sub.add_parser("start")
    p.add_argument("--task", required=True); p.add_argument("--arm", required=True, choices=ARMS)
    p.add_argument("--run-id", required=True); p.add_argument("--model", default="unspecified")
    p.add_argument("--mode", default="review", choices=MODES,
                   help="review: write tests that catch the bugs in a given design. build: write the design from a "
                        "skeleton. harden: fix a given design. impl: the task's own files and grader. "
                        "guard: strengthen specs and tests of a correct design; hidden design mistakes judge it. "
                        "own: build and guard in one task, judged by hidden tests and by hidden mistakes. "
                        "Hidden tests judge build, harden and impl")
    p.add_argument("--check-flags", default="", metavar="FLAGS",
                   help="flags a wrapper adds to every `check`, e.g. '--no-mutation', to try other defaults")
    p.add_argument("--binary", help="checker binary to pin for this cohort (default: target/release)")
    p.add_argument("--label", help="condition name for report.py (default: the arm)")
    p.add_argument("--env", action="append", default=[], metavar="KEY=VALUE",
                   help="environment variable for the solver's container")
    p.set_defaults(fn=cmd_start)
    p = sub.add_parser("bridge"); p.add_argument("run_id"); p.set_defaults(fn=cmd_bridge)
    p = sub.add_parser("finish"); p.add_argument("run_id"); p.add_argument("--reply")
    p.set_defaults(fn=cmd_finish)
    p = sub.add_parser("grade"); p.add_argument("run_ids", nargs="+"); p.set_defaults(fn=cmd_grade)
    a = ap.parse_args()
    a.fn(a)


if __name__ == "__main__":
    main()
