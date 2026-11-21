#!/usr/bin/env python3
"""Grade a lease-impl submission: grade.py <submission_dir>

Runs each hidden scenario against the submission's lease.py under the task's own
simulator, then checks the scenario's event log against the task's own copy of the
model's specs with `snappea observe`. A scenario passes when the run completed and
observe found no violation. Prints {"passed": [...], "failed": {name: message}}.

    grade.py <dir> --keep DIR     # keep the logs in DIR
    grade.py <dir> --public       # run the public scenarios' checks instead (for calibration)
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[4]
SNAPPEA = os.environ.get("SNAPPEA_BIN", str(ROOT / "target/release/snappea"))
TRUSTED = HERE / "solution"           # sim.py and model/ are taken from here, never from the submission
SCENARIO_SECONDS = 30


def scenario_names():
    src = (HERE / "hidden.py").read_text()
    start = src.index("SCENARIOS = [")
    names = src[start + len("SCENARIOS = ["):src.index("]", start)]
    return [n.strip() for n in names.replace("\n", " ").split(",") if n.strip()]


def observe(model: Path, log: Path) -> str | None:
    """None when the log satisfies both specs, else the first problem."""
    try:
        r = subprocess.run([SNAPPEA, "observe", "Model.pproj", str(log), "--all-specs", "--strict",
                            "--check-hot", "--json"], cwd=model, text=True, capture_output=True, timeout=60)
    except subprocess.TimeoutExpired:
        return "snappea observe timed out"
    try:
        out = json.loads(r.stdout)
    except json.JSONDecodeError:
        return f"snappea observe failed: {(r.stdout + r.stderr)[-300:]}"
    if out.get("exit_code") == 0:
        return None
    for spec in out.get("specs", []):
        for e in spec.get("errors", []):
            return f"{spec['spec']}: {e.get('message', e.get('kind'))}"[:400]
    return f"snappea observe: {out.get('error', 'exit ' + str(out.get('exit_code')))}"[:400]


def grade(submission: Path, keep: Path | None) -> dict:
    passed, failed = [], {}
    names = scenario_names()
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        work = tmp / "work"
        work.mkdir()
        # The submission's Python modules (lease.py and any helpers), then the task's own
        # simulator and scenarios on top.
        for p in sorted(submission.glob("*.py")) if submission.is_dir() else []:
            if p.is_file() and not p.is_symlink() and p.stat().st_size < 1_000_000:
                shutil.copy(p, work / p.name)
        shutil.copy(TRUSTED / "sim.py", work / "sim.py")
        shutil.copy(HERE / "hidden.py", work / "hidden.py")
        model = tmp / "model"
        shutil.copytree(TRUSTED / "model", model, ignore=shutil.ignore_patterns("PCheckerOutput", "PGenerated"))
        logs = keep or tmp / "logs"
        logs.mkdir(parents=True, exist_ok=True)
        if not (work / "lease.py").exists():
            return {"passed": [], "failed": {n: "no lease.py in the submission" for n in names}}
        for name in names:
            out = tmp / f"{name}.json"
            try:
                subprocess.run([sys.executable, "-B", "hidden.py", name, str(logs.resolve()), str(out)], cwd=work,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=SCENARIO_SECONDS,
                               env={"PATH": os.environ.get("PATH", ""), "PYTHONHASHSEED": "0"})
                res = json.loads(out.read_text())
            except subprocess.TimeoutExpired:
                res = {"ok": False, "error": f"no result after {SCENARIO_SECONDS} s"}
            except (OSError, ValueError):
                res = {"ok": False, "error": "the run died without a result"}
            if not res.get("ok"):
                failed[name] = "run did not complete: " + str(res.get("error"))
                continue
            problem = observe(model, logs / f"{name}.jsonl")
            if problem:
                failed[name] = problem
            else:
                passed.append(name)
    return {"passed": passed, "failed": failed}


def main():
    args = sys.argv[1:]
    keep = None
    if "--keep" in args:
        i = args.index("--keep")
        keep = Path(args[i + 1])
        del args[i:i + 2]
    try:
        result = grade(Path(args[0]).resolve(), keep)
    except Exception as e:  # never crash: report every scenario as failed
        result = {"passed": [], "failed": {n: f"grader error: {type(e).__name__}: {e}"[:300] for n in scenario_names()}}
    print(json.dumps(result))


if __name__ == "__main__":
    main()
