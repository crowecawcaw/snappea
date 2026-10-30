#!/usr/bin/env python3
"""Minimal-prompt agent eval: provision a sandbox, relay solver commands, grade the result.

    python harness.py start --task queue --run-id q-none-01 [--skill ../../skills/p-modeling]
    python harness.py bridge q-none-01 <<'EOF'      # the solver's only tool
    ls
    EOF
    python harness.py finish q-none-01
    python harness.py grade q-none-01
    python harness.py summary

Each run gets an offline container with the task's public files in /work, the snappea
binary on PATH, pysnappea, and (optionally) a read-only skill at /opt/skill.
Grading runs in fresh offline containers and never shows anything to the solver.
"""
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[1]
RUNS = HERE / "runs"
IMAGE = "snappea-min-eval"
BINARY = Path(os.environ.get("SNAPPEA_EVAL_BINARY", ROOT / "target/release/snappea"))
OUTPUT_LIMIT = 16000

PROMPT = """You are working in a sandbox. Your only tool is a shell inside it: run each command with

    python3 {harness} bridge {run_id} <<'CMD'
    <your shell command>
    CMD

Do not use any other tool, and do not read or write files outside the sandbox: create
files from inside the command (for example `cat > file <<'EOF'`). The sandbox
has no network. You have {calls} commands and {minutes} minutes; each command may run for
{command_seconds} seconds.

{skill_line}Complete the task in /work/TASK.md. When done, reply with a short summary."""

SKILL_LINE = "Use the P modeling skill at /opt/skill/SKILL.md.\n\n"


def tree_hash(path: Path) -> str:
    h = hashlib.sha256()
    for f in sorted(p for p in path.rglob("*") if p.is_file()):
        h.update(str(f.relative_to(path)).encode() + b"\0" + f.read_bytes() + b"\0")
    return h.hexdigest()


def container(run_id: str) -> str:
    return f"snappea-min-{run_id}"


def docker_run_args(work: Path, extra: list[str] = ()) -> list[str]:
    return ["docker", "run", "--network", "none", "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges", "--memory", "1g", "--cpus", "2",
            "--pids-limit", "256", "--user", "1000:1000",
            "--mount", f"type=bind,src={BINARY},dst=/opt/bin/snappea,readonly",
            "--mount", f"type=bind,src={work},dst=/work", *extra]


def cmd_start(a):
    run = RUNS / a.run_id
    if run.exists():
        sys.exit(f"run {a.run_id} exists; pick another id")
    task = HERE / "tasks" / a.task / "public"
    work = run / "work"
    shutil.copytree(task, work)
    extra = []
    meta = {"run_id": a.run_id, "task": a.task, "skill": None, "model": a.model,
            "limits": {"calls": a.calls, "seconds": a.minutes * 60, "command_seconds": a.command_seconds},
            "binary_sha256": hashlib.sha256(BINARY.read_bytes()).hexdigest(),
            "task_hash": tree_hash(task), "created": time.time()}
    if a.skill:
        skill = run / "skill"
        shutil.copytree(Path(a.skill), skill, ignore=shutil.ignore_patterns("__pycache__"))
        os.chmod(skill, 0o755)
        extra = ["--mount", f"type=bind,src={skill},dst=/opt/skill,readonly"]
        meta["skill"] = {"source": str(Path(a.skill).resolve()),
                         "label": a.skill_label or Path(a.skill).name, "tree_hash": tree_hash(skill)}
    os.chmod(work, 0o777)
    for p in work.rglob("*"):
        os.chmod(p, 0o777 if p.is_dir() else 0o666)
    subprocess.run([*docker_run_args(work, extra), "-d", "--name", container(a.run_id),
                    IMAGE, "sleep", "infinity"], check=True, capture_output=True)
    prompt = PROMPT.format(harness=HERE / "harness.py", run_id=a.run_id, calls=a.calls,
                           minutes=a.minutes, command_seconds=a.command_seconds,
                           skill_line=SKILL_LINE if a.skill else "")
    (run / "prompt.txt").write_text(prompt)
    meta["prompt_sha256"] = hashlib.sha256(prompt.encode()).hexdigest()
    (run / "meta.json").write_text(json.dumps(meta, indent=2))
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


def cmd_finish(a):
    run = RUNS / a.run_id
    state_path = run / "bridge-state.json"
    state = json.loads(state_path.read_text()) if state_path.exists() else {"calls": 0, "started": time.time()}
    state["closed"] = True
    state["finished"] = time.time()
    state_path.write_text(json.dumps(state))
    subprocess.run(["docker", "rm", "-f", container(a.run_id)], capture_output=True)
    sub = run / "submission"
    if sub.exists():
        shutil.rmtree(sub)
    shutil.copytree(run / "work", sub, ignore=shutil.ignore_patterns(
        "__pycache__", ".pytest_cache", "PCheckerOutput", "*.pyc", ".snappea"))
    if a.reply:
        (run / "agent-final.txt").write_text(Path(a.reply).read_text() if Path(a.reply).exists() else a.reply)
    print(f"frozen {sub}")


# ---------------------------------------------------------------- grading


def run_in_sandbox(files: dict[str, str | Path], args: list[str], timeout: int = 180) -> dict:
    """Run `args` in a fresh offline container whose /work holds `files` (dest -> src)."""
    with tempfile.TemporaryDirectory(prefix="snappea-min-grade-") as temp:
        work = Path(temp)
        for dest, src in files.items():
            target = work / dest
            target.parent.mkdir(parents=True, exist_ok=True)
            if isinstance(src, Path):
                shutil.copy2(src, target)
            else:
                target.write_text(src)
        os.chmod(work, 0o777)
        for p in work.rglob("*"):
            os.chmod(p, 0o777 if p.is_dir() else 0o666)
        name = f"snappea-min-grade-{os.getpid()}-{time.time_ns()}"
        try:
            r = subprocess.run([*docker_run_args(work), "--rm", "--name", name, IMAGE, *args],
                               text=True, capture_output=True, timeout=timeout)
            out = {"exit_code": r.returncode, "output": r.stdout + r.stderr, "timed_out": False}
        except subprocess.TimeoutExpired:
            subprocess.run(["docker", "rm", "-f", name], capture_output=True)
            out = {"exit_code": None, "output": "", "timed_out": True}
        report = work / "report.xml"
        out["cases"] = parse_junit(report) if report.exists() else {}
        return out


def parse_junit(path: Path) -> dict[str, str]:
    cases = {}
    for case in ET.parse(path).iter("testcase"):
        cid = f"{case.get('classname')}::{case.get('name')}"
        status = "passed"
        for child in case:
            if child.tag in ("failure", "error"):
                status = "failed"
            elif child.tag == "skipped":
                status = "skipped"
        cases[cid] = status
    return cases


PYTEST = ["python", "-m", "pytest", "-q", "-p", "no:cacheprovider", "--junitxml=report.xml"]


def parallel(fn, items, workers=3):
    from concurrent.futures import ThreadPoolExecutor
    with ThreadPoolExecutor(workers) as pool:
        return list(pool.map(fn, items))


def submission_files(sub: Path, solution: str | None = None) -> dict:
    files = {str(p.relative_to(sub)): p for p in sub.rglob("*") if p.is_file()}
    if solution is not None:
        files["solution.py"] = solution
    return files


def load_module(path: Path):
    spec = importlib.util.spec_from_file_location(path.stem, path)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def observation_files(sub: Path) -> set[str]:
    """Agent test files that check real calls against P specs via pysnappea."""
    found = set()
    for p in sub.rglob("test*.py"):
        text = p.read_text(errors="replace")
        if re.search(r"^\s*(import snappea|from snappea)", text, re.M):
            found.add(p.relative_to(sub).with_suffix("").as_posix().replace("/", "."))
    return found


def in_files(case_id: str, modules: set[str]) -> bool:
    cls = case_id.split("::")[0]
    return any(cls == m or cls.startswith(m + ".") for m in modules)


def grade_hidden(sub: Path, private: Path) -> dict:
    if not (sub / "solution.py").exists():
        return {"passed": 0, "total": None, "note": "no solution.py"}
    files = {"solution.py": sub / "solution.py", "runtime.py": private.parent / "public" / "runtime.py",
             "test_hidden.py": private / "test_hidden.py"}
    for p in sub.glob("*.py"):  # helper modules the solution may import
        if p.name not in files and not p.name.startswith("test") and p.name != "conftest.py":
            files[p.name] = p
    r = run_in_sandbox(files, PYTEST + ["test_hidden.py"])
    passed = sorted(c.split("::")[1] for c, s in r["cases"].items() if s == "passed")
    failed = sorted(c.split("::")[1] for c, s in r["cases"].items() if s != "passed")
    return {"passed": len(passed), "total": len(r["cases"]) or None, "failed": failed,
            "timed_out": r["timed_out"]}


def grade_tests(sub: Path, private: Path) -> dict:
    """Mutation score of the agent's saved tests, counting only tests the reference passes."""
    reference = (private / "reference.py").read_text()
    mutants = load_module(private / "mutants.py")
    runs = [run_in_sandbox(submission_files(sub, reference), PYTEST) for _ in range(2)]
    stable = {c for c, s in runs[0]["cases"].items() if s == "passed" and runs[1]["cases"].get(c) == "passed"}
    own = run_in_sandbox(submission_files(sub), PYTEST)
    obs_modules = observation_files(sub)
    result = {
        "agent_tests": len(runs[0]["cases"]),
        "agent_tests_pass_own_solution": sum(s == "passed" for s in own["cases"].values()),
        "reference_passing": len(stable),
        "reference_rejecting": sorted(c for c in runs[0]["cases"] if c not in stable),
        "observation_tests": sum(in_files(c, obs_modules) for c in stable),
        "mutants": {},
    }
    sources = [(name, mutants.apply(reference, edits)) for name, _, edits in mutants.MUTANTS]
    outcomes = parallel(lambda src: run_in_sandbox(submission_files(sub, src), PYTEST, timeout=120),
                        [src for _, src in sources])
    for (name, _), r in zip(sources, outcomes):
        killers = sorted(c for c in stable if r["cases"].get(c, "failed" if r["timed_out"] else None) == "failed")
        result["mutants"][name] = {
            "killed": bool(killers) or (r["timed_out"] and bool(stable)),
            "killed_by_observation": any(in_files(c, obs_modules) for c in killers),
            "timed_out": r["timed_out"], "killers": killers[:5]}
    m = result["mutants"].values()
    result["killed"] = sum(x["killed"] for x in m)
    result["killed_by_observation"] = sum(x["killed_by_observation"] for x in m)
    result["total_mutants"] = len(result["mutants"])
    return result


def grade_own_mutation(sub: Path, limit: int = 40) -> dict:
    """Standard mutation testing on the agent's own solution.py with all its tests,
    including tests that rely on its own emit() calls."""
    import ast_mutants
    if not (sub / "solution.py").exists():
        return {"total": 0}
    runs = [run_in_sandbox(submission_files(sub), PYTEST) for _ in range(2)]
    stable = {c for c, s in runs[0]["cases"].items() if s == "passed" and runs[1]["cases"].get(c) == "passed"}
    obs_modules = observation_files(sub)
    result = {"stable_tests": len(stable), "mutants": {}}
    generated = ast_mutants.mutants((sub / "solution.py").read_text(), limit=limit)
    outcomes = parallel(lambda src: run_in_sandbox(submission_files(sub, src), PYTEST, timeout=60),
                        [src for _, src in generated])
    for (label, _), r in zip(generated, outcomes):
        killers = sorted(c for c in stable if r["cases"].get(c, "failed") != "passed")
        killed = bool(stable) and (r["timed_out"] or bool(killers))
        result["mutants"][label] = {"killed": killed, "timed_out": r["timed_out"],
                                    "killed_by_observation": any(in_files(c, obs_modules) for c in killers)}
    m = result["mutants"].values()
    result.update(total=len(result["mutants"]), killed=sum(x["killed"] for x in m),
                  killed_by_observation=sum(x["killed_by_observation"] for x in m))
    return result


TRANSPLANT_PROMPT = """You are helping grade a test suite. Directory {dir} holds `solution.py`, an
implementation of the task in `TASK.md`. For each defect listed in `defects.json`, write
`mutant_<name>.py`: a copy of `solution.py` with exactly that defect introduced, in the
way this code is written. Keep everything else unchanged, including any `emit`, logging
or event-recording calls, imports and public names. If a defect cannot be expressed in
this code, write `mutant_<name>.skip` with one line saying why. Do not run or change
anything else, and do not read files outside {dir}. Reply "done" when finished."""


def cmd_transplant(a):
    """Prepare a directory for a helper agent to write the task's defects into the
    agent's own solution, and print its prompt."""
    run = RUNS / a.run_id
    meta = json.loads((run / "meta.json").read_text())
    mutants = load_module(HERE / "tasks" / meta["task"] / "private" / "mutants.py")
    out = run / "transplant"
    out.mkdir(exist_ok=True)
    shutil.copy2(run / "submission" / "solution.py", out / "solution.py")
    shutil.copy2(HERE / "tasks" / meta["task"] / "public" / "TASK.md", out / "TASK.md")
    (out / "defects.json").write_text(json.dumps({n: d for n, d, _ in mutants.MUTANTS}, indent=2))
    print(TRANSPLANT_PROMPT.format(dir=out))


def grade_transplant(run: Path, sub: Path, private: Path) -> dict:
    """Score the agent's full test suite on its own solution with each defect transplanted.
    A transplanted mutant counts only if it fails the hidden suite (it is a real defect)."""
    out = run / "transplant"
    if not out.exists():
        return {"status": "not prepared"}
    mutants = load_module(private / "mutants.py")
    runs = [run_in_sandbox(submission_files(sub), PYTEST) for _ in range(2)]
    stable = {c for c, s in runs[0]["cases"].items() if s == "passed" and runs[1]["cases"].get(c) == "passed"}
    obs_modules = observation_files(sub)
    base = {"runtime.py": private.parent / "public" / "runtime.py", "test_hidden.py": private / "test_hidden.py"}
    for p in sub.glob("*.py"):
        if p.name not in base and p.name not in ("solution.py", "conftest.py") and not p.name.startswith("test"):
            base[p.name] = p
    names = [n for n, _, _ in mutants.MUTANTS if (out / f"mutant_{n}.py").exists()]
    sources = {n: (out / f"mutant_{n}.py").read_text() for n in names}
    hidden = parallel(lambda n: run_in_sandbox({**base, "solution.py": sources[n]}, PYTEST + ["test_hidden.py"],
                                               timeout=60), names)
    # a real defect: fails some hidden tests but still imports and passes others
    valid = [n for n, h in zip(names, hidden)
             if any(s != "passed" for s in h["cases"].values()) and any(s == "passed" for s in h["cases"].values())]
    suites = parallel(lambda n: run_in_sandbox(submission_files(sub, sources[n]), PYTEST, timeout=120), valid)
    result = {"stable_tests": len(stable), "written": len(names),
              "skipped": sorted(p.stem[len("mutant_"):] for p in out.glob("mutant_*.skip")),
              "invalid": sorted(set(names) - set(valid)), "mutants": {}}
    for n, r in zip(valid, suites):
        killers = sorted(c for c in stable if r["cases"].get(c, "failed") != "passed")
        result["mutants"][n] = {"killed": bool(stable) and (r["timed_out"] or bool(killers)),
                                "killed_by_observation": any(in_files(c, obs_modules) for c in killers)}
    m = result["mutants"].values()
    result.update(valid=len(valid), killed=sum(x["killed"] for x in m),
                  killed_by_observation=sum(x["killed_by_observation"] for x in m))
    return result


def cmd_grade_transplant(a):
    run = RUNS / a.run_id
    meta = json.loads((run / "meta.json").read_text())
    path = run / "grade.json"
    grade = json.loads(path.read_text())
    grade["transplant"] = grade_transplant(run, run / "submission", HERE / "tasks" / meta["task"] / "private")
    path.write_text(json.dumps(grade, indent=2))
    t = grade["transplant"]
    print(f"{a.run_id}: transplanted defects killed {t.get('killed')}/{t.get('valid')} "
          f"(observation {t.get('killed_by_observation')}; invalid {t.get('invalid')}; skipped {t.get('skipped')})")


def grade_model(sub: Path) -> dict:
    p_files = [p for p in sub.rglob("*.p")] + [p for p in sub.rglob("*.pproj")]
    if not p_files:
        return {"p_files": 0}
    r = run_in_sandbox(submission_files(sub), ["snappea", "check", "--json", "--timeout", "20s", "--mutation-timeout", "30s"], timeout=240)
    summary = {"p_files": len([p for p in p_files if p.suffix == ".p"]), "exit_code": r["exit_code"]}
    try:
        data = json.loads(r["output"][r["output"].index("{"):])
    except (ValueError, json.JSONDecodeError):
        try:
            data = json.JSONDecoder().raw_decode(r["output"][r["output"].index("{"):])[0]
        except (ValueError, json.JSONDecodeError):
            summary["output_head"] = r["output"][:800]
            return summary
    summary.update({k: data.get("summary", {}).get(k) for k in
                    ("status", "test_cases", "failed", "incomplete", "coverage_gaps")})
    summary["tests"] = [{"name": t.get("name"), "result": t.get("result"),
                         "complete": t.get("search_complete"), "schedules": t.get("schedules")}
                        for t in data.get("test_cases", [])][:30]
    text = (sub / "DESIGN.md").read_text() if (sub / "DESIGN.md").exists() else ""
    summary["spec_monitors"] = sum(len(re.findall(r"^\s*spec\s+\w+", p.read_text(), re.M))
                                   for p in sub.rglob("*.p"))
    summary["test_decls"] = sum(len(re.findall(r"^\s*test\s+\w+", p.read_text(), re.M))
                                for p in sub.rglob("*.p"))
    summary["hot_states"] = sum(len(re.findall(r"\bhot\s+state\b", p.read_text())) for p in sub.rglob("*.p"))
    summary["design_doc_chars"] = len(text)
    return summary


WRITE_RE = re.compile(r"(>|tee|write_text|open\()[^\n]*solution\.py|solution\.py[^\n]*<<|cat\s*>\s*solution\.py")


def grade_process(run: Path) -> dict:
    path = run / "transcript.jsonl"
    if not path.exists():
        return {"calls": 0}
    rows = [json.loads(l) for l in path.read_text().splitlines()]
    first_check = next((r["call"] for r in rows if re.search(r"\bsnappea\s+check\b", r["command"])), None)
    first_impl = next((r["call"] for r in rows if WRITE_RE.search(r["command"])), None)
    checks = [r for r in rows if re.search(r"\bsnappea\s+check\b", r["command"])]
    bugs = [r for r in checks if r["result"]["exit_code"] == 1]
    return {
        "calls": len(rows), "minutes": round(rows[-1]["elapsed"] / 60, 1) if rows else 0,
        "read_skill": any("/opt/skill" in r["command"] for r in rows),
        "first_check_call": first_check, "first_solution_write_call": first_impl,
        "model_first": bool(first_check and (first_impl is None or first_check < first_impl)),
        "check_runs": len(checks), "check_runs_exit1": len(bugs),
        "observe_runs": sum(bool(re.search(r"\bsnappea\s+observe\b", r["command"])) for r in rows),
    }


def cmd_grade(a):
    run = RUNS / a.run_id
    meta = json.loads((run / "meta.json").read_text())
    sub = run / "submission"
    private = HERE / "tasks" / meta["task"] / "private"
    grade = {"run_id": a.run_id, "task": meta["task"],
             "skill": meta["skill"]["label"] if meta["skill"] else "none",
             "model": meta.get("model"),
             "hidden": grade_hidden(sub, private), "tests": grade_tests(sub, private),
             "own_mutation": grade_own_mutation(sub),
             "model_check": grade_model(sub), "process": grade_process(run)}
    (run / "grade.json").write_text(json.dumps(grade, indent=2))
    h, t, o = grade["hidden"], grade["tests"], grade["own_mutation"]
    print(f"{a.run_id}: hidden {h['passed']}/{h['total']}  mutants killed {t['killed']}/{t['total_mutants']} "
          f"(observation {t['killed_by_observation']})  ref-passing tests {t['reference_passing']}/{t['agent_tests']}"
          f"  own-mutants killed {o.get('killed')}/{o.get('total')} (observation {o.get('killed_by_observation')})")


def cmd_calibrate(a):
    """Check hidden tests and mutants: reference passes all; every mutant fails some hidden test."""
    private = HERE / "tasks" / a.task / "private"
    public = private.parent / "public"
    mutants = load_module(private / "mutants.py")
    reference = (private / "reference.py").read_text()
    base = {"runtime.py": public / "runtime.py", "test_hidden.py": private / "test_hidden.py"}
    for name, source in [("reference", reference)] + [(n, mutants.apply(reference, e)) for n, _, e in mutants.MUTANTS]:
        r = run_in_sandbox({**base, "solution.py": source}, PYTEST + ["test_hidden.py"], timeout=60)
        failed = sorted(c.split("::")[1] for c, s in r["cases"].items() if s != "passed")
        print(f"{name:28} {'TIMEOUT' if r['timed_out'] else ''} failed={failed}")


def cmd_summary(a):
    rows = []
    for g in sorted(RUNS.glob("*/grade.json")):
        d = json.loads(g.read_text())
        h, t, m, p = d["hidden"], d["tests"], d["model_check"], d["process"]
        o = d.get("own_mutation", {})
        tr = d.get("transplant", {})
        rows.append([d["run_id"], d["task"], d["skill"], f"{h['passed']}/{h['total']}",
                     f"{t['killed']}/{t['total_mutants']}", str(t["killed_by_observation"]),
                     f"{o.get('killed')}/{o.get('total')}", str(o.get("killed_by_observation")),
                     f"{tr.get('killed')}/{tr.get('valid')}",
                     f"{t['reference_passing']}/{t['agent_tests']}", str(m.get("spec_monitors", 0)),
                     str(m.get("exit_code")), str(p.get("model_first")), str(p.get("check_runs")),
                     str(p.get("calls"))])
    head = ["run", "task", "skill", "hidden", "killed", "obs-kill", "own-killed", "own-obs", "transplant", "ref-ok tests", "specs",
            "check-exit", "model-first", "checks", "calls"]
    print("| " + " | ".join(head) + " |\n|" + "---|" * len(head))
    for r in rows:
        print("| " + " | ".join(r) + " |")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("start")
    s.add_argument("--task", required=True)
    s.add_argument("--run-id", required=True)
    s.add_argument("--skill")
    s.add_argument("--skill-label")
    s.add_argument("--model", default="unrecorded")
    s.add_argument("--calls", type=int, default=60)
    s.add_argument("--minutes", type=int, default=40)
    s.add_argument("--command-seconds", type=int, default=120)
    for name in ("bridge", "finish", "grade", "transplant", "grade_transplant"):
        p = sub.add_parser(name)
        p.add_argument("run_id")
        if name == "finish":
            p.add_argument("--reply")
    c = sub.add_parser("calibrate")
    c.add_argument("task")
    sub.add_parser("summary")
    a = ap.parse_args()
    if getattr(a, "run_id", None) and not re.fullmatch(r"[a-z0-9-]+", a.run_id):
        sys.exit("run ids use lowercase letters, digits and dashes")
    globals()["cmd_" + a.cmd](a)


if __name__ == "__main__":
    main()
