#!/usr/bin/env python3
"""Grade a frozen queue-flow submission in fresh offline containers."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

from calibration import variants
import reference_solution
from model_cases import cases as trace_cases


HERE = Path(__file__).resolve().parent
PHASE = HERE.parent
PUBLIC = PHASE / "public"
TEST_RUN_RE = re.compile(r"Ran\s+(\d+)\s+tests?", re.MULTILINE)
FAILURE_RE = re.compile(r"FAILED \(([^)]*)\)")
SETUP_MARKERS = (
    "ModuleNotFoundError", "ImportError", "SyntaxError", "SnappeaNotFound",
    "SetupError", "CompilationError", "compilation failed", "could not parse",
    "matches no files", "Start directory is not importable", "unknown event",
    "unselected-spec", "no tests ran", "does not type-check", "syntax error:",
    "error[",
)
REQUIRED = ("solution.py", "model.p", "monitor.p", "tests.p", "DESIGN.md", "test_observation.py")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def copy_python_helpers(submission: Path, workspace: Path) -> list[str]:
    files = []
    for item in submission.rglob("*.py"):
        if not item.is_file() or item.is_symlink() or "__pycache__" in item.parts:
            continue
        rel = item.relative_to(submission)
        if rel.name in {"solution.py", "runtime.py"}:
            continue
        target = workspace / rel
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(item, target)
        files.append(str(rel))
    return files


def classify(exit_code: int | None, output: str, timed_out: bool) -> dict:
    if timed_out:
        return {"status": "timeout", "test_count": None}
    match = TEST_RUN_RE.search(output)
    count = int(match.group(1)) if match else 0
    failure = FAILURE_RE.search(output)
    failed_tests = bool(failure and count > 0 and
                        ("failures=" in failure.group(1) or "errors=" in failure.group(1)))
    infra = count == 0 or any(marker.lower() in output.lower() for marker in SETUP_MARKERS)
    if infra:
        status = "infrastructure_error"
    elif exit_code == 0:
        status = "passed"
    elif failed_tests:
        status = "tests_rejected"
    else:
        status = "infrastructure_error"
    return {"status": status, "test_count": count}


def run_container(work: Path, binary: Path, image: str, args: list[str], timeout: float) -> dict:
    name = f"queue-flow-grade-{os.getpid()}-{time.time_ns()}"
    command = [
        "docker", "run", "--rm", "--name", name, "--network", "none", "--cap-drop", "ALL",
        "--security-opt", "no-new-privileges", "--user", f"{os.getuid()}:{os.getgid()}",
        "--memory", "768m", "--cpus", "1", "--pids-limit", "128", "--read-only",
        "--tmpfs", "/tmp:rw,exec,size=128m",
        "--mount", f"type=bind,src={binary},dst=/opt/snappea,readonly",
        "--mount", f"type=bind,src={work},dst=/work",
        "--workdir", "/work", "--env", "PATH=/opt:/usr/local/bin:/usr/local/sbin:/usr/bin:/bin",
        "--env", "SNAPPEA_BIN=/opt/snappea", image, *args,
    ]
    start = time.monotonic()
    timed_out = False
    try:
        result = subprocess.run(command, text=True, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, timeout=timeout, check=False)
        exit_code, output = result.returncode, result.stdout
    except subprocess.TimeoutExpired as exc:
        timed_out = True
        exit_code = None
        output = exc.stdout.decode("utf-8", "replace") if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        subprocess.run(["docker", "rm", "-f", name], stdout=subprocess.DEVNULL,
                       stderr=subprocess.DEVNULL, check=False)
    return {**classify(exit_code, output, timed_out), "exit_code": exit_code,
            "seconds": round(time.monotonic() - start, 3), "output": output}


def workspace_base(work: Path):
    for name in ("runtime.py", "events.p"):
        shutil.copy2(PUBLIC / name, work / name)


def run_contract(submission: Path, binary: Path, image: str, timeout: float) -> dict:
    with tempfile.TemporaryDirectory(prefix="queue-flow-contract-") as temp:
        work = Path(temp)
        workspace_base(work)
        solution = submission / "solution.py"
        if not solution.is_file():
            return {"status": "infrastructure_error", "output": "solution.py is missing", "test_count": 0}
        shutil.copy2(solution, work / "solution.py")
        copy_python_helpers(submission, work)
        if (submission / "monitor.p").is_file():
            shutil.copy2(submission / "monitor.p", work / "monitor.p")
        hidden = HERE / "hidden_tests" / "test_contract.py"
        shutil.copy2(hidden, work / "test_hidden_contract.py")
        result = run_container(work, binary, image,
                               ["python", "-m", "unittest", "-v", "test_hidden_contract"], timeout)
        result["forced_initial_read_race"] = "QUEUE_RACE_FORCED=1" in result.get("output", "")
        return result


def run_candidate_suite(submission: Path, solution_text: str, binary: Path,
                        image: str, timeout: float) -> dict:
    with tempfile.TemporaryDirectory(prefix="queue-flow-observation-") as temp:
        work = Path(temp)
        workspace_base(work)
        (work / "solution.py").write_text(solution_text, encoding="utf-8")
        copy_python_helpers(submission, work)
        for name in ("monitor.p",):
            path = submission / name
            if path.is_file():
                shutil.copy2(path, work / name)
        return run_container(work, binary, image,
                             ["python", "-m", "unittest", "discover", "-v", "-p", "test*.py"], timeout)


def run_submitted_suite(submission: Path, binary: Path, image: str, timeout: float) -> dict:
    """Run the saved tests against the submitted implementation and its monitor."""
    with tempfile.TemporaryDirectory(prefix="queue-flow-submitted-tests-") as temp:
        work = Path(temp)
        workspace_base(work)
        for item in submission.rglob("*"):
            if not item.is_file() or item.is_symlink() or "__pycache__" in item.parts:
                continue
            rel = item.relative_to(submission)
            if rel.name in {"runtime.py", "events.p"}:
                continue
            target = work / rel
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(item, target)
        return run_container(work, binary, image,
                             ["python", "-m", "unittest", "discover", "-v", "-p", "test*.py"], timeout)


def run_portable_probes(submission: Path, binary: Path, image: str, timeout: float) -> dict:
    """Diagnostic only: compare tests to the private reference, then seeded variants."""
    reference = Path(reference_solution.__file__).read_text(encoding="utf-8")
    baseline = run_candidate_suite(submission, reference, binary, image, timeout)
    if baseline["status"] != "passed":
        return {
            "portable_probe_valid": False,
            "reference_status": baseline["status"],
            "reference_test_count": baseline.get("test_count"),
            "mutation_probes_run": False,
            "note": "Reference incompatibility leaves mutation behavior uninterpreted.",
        }
    results = []
    for number, (_, source) in enumerate(list(variants(reference).items())[1:], start=1):
        outcome = run_candidate_suite(submission, source, binary, image, timeout)
        results.append({"case": f"probe-{number:02d}", "status": outcome["status"],
                        "test_count": outcome.get("test_count"), "output": outcome.get("output", "")})
    return {
        "portable_probe_valid": True,
        "reference_status": baseline["status"],
        "reference_test_count": baseline.get("test_count"),
        "mutation_probes_run": True,
        "mutation_results": results,
        "mutations_rejected": sum(r["status"] == "tests_rejected" for r in results),
        "mutations_total": len(results),
    }


def run_model(submission: Path, binary: Path, image: str, timeout: float) -> dict:
    needed = ("model.p", "monitor.p", "tests.p", "DESIGN.md")
    absent = [name for name in needed if not (submission / name).is_file()]
    if absent:
        return {"status": "infrastructure_error", "missing": absent, "selected_tests": 0,
                "search_complete": False, "output": "Required model deliverable missing."}
    with tempfile.TemporaryDirectory(prefix="queue-flow-model-") as temp:
        work = Path(temp)
        shutil.copy2(PUBLIC / "events.p", work / "events.p")
        for name in needed:
            shutil.copy2(submission / name, work / name)
        outcome = run_container(work, binary, image,
                                ["snappea", "check", "events.p", "monitor.p", "model.p", "tests.p",
                                 "--timeout", "5s", "--mutation-timeout", "5s", "--json"], timeout)
        parsed = None
        try:
            parsed = json.loads(outcome["output"])
        except ValueError:
            pass
        test_cases = parsed.get("test_cases", []) if isinstance(parsed, dict) else []
        outcome["selected_tests"] = len(test_cases)
        outcome["search_complete"] = bool(test_cases) and all(c.get("search_complete", False) for c in test_cases)
        outcome["model_check_passed"] = (outcome["exit_code"] == 0 and bool(test_cases) and
                                          outcome["search_complete"])
        if outcome["status"] != "timeout":
            outcome["status"] = ("passed" if outcome["model_check_passed"] else
                                 "infrastructure_error" if outcome["exit_code"] == 2 else "failed")
        return outcome


def run_monitor_traces(submission: Path, binary: Path, image: str, timeout: float) -> dict:
    monitor = submission / "monitor.p"
    if not monitor.is_file():
        return {"status": "infrastructure_error", "valid_accepted": 0, "valid_total": 4,
                "invalid_rejected": 0, "invalid_total": 4}
    results = []
    for label, expected_valid, events in trace_cases():
        with tempfile.TemporaryDirectory(prefix="queue-flow-trace-") as temp:
            work = Path(temp)
            shutil.copy2(PUBLIC / "events.p", work / "events.p")
            shutil.copy2(monitor, work / "monitor.p")
            (work / "history.jsonl").write_text("".join(json.dumps(e) + "\n" for e in events), encoding="utf-8")
            outcome = run_container(work, binary, image,
                ["snappea", "observe", "events.p", "monitor.p", "history.jsonl", "--spec", "QueueContract",
                 "--strict", "--unknown-events", "error", "--json"], timeout)
            text = outcome["output"].lower()
            checker_error = any(m in text for m in SETUP_MARKERS) or "error:" in text
            if outcome["status"] == "timeout":
                case_status = "timeout"
            elif outcome["exit_code"] == 2 or checker_error:
                case_status = "infrastructure_error"
            elif expected_valid:
                case_status = "passed" if outcome["exit_code"] == 0 else "failed"
            else:
                case_status = ("passed" if outcome["exit_code"] == 1 and "spec_violation" in text
                               else "failed")
            passed = case_status == "passed"
            results.append({"name": label, "expected_valid": expected_valid,
                            "status": case_status, "passed": passed,
                            "output": outcome["output"]})
    return {
        "status": "passed" if all(r["passed"] for r in results) else
                  ("infrastructure_error" if any(r["status"] == "infrastructure_error" for r in results)
                   else "timeout" if any(r["status"] == "timeout" for r in results) else "failed"),
        "valid_accepted": sum(r["passed"] for r in results if r["expected_valid"]),
        "valid_total": sum(r["expected_valid"] for r in results),
        "invalid_rejected": sum(r["passed"] for r in results if not r["expected_valid"]),
        "invalid_total": sum(not r["expected_valid"] for r in results),
        "cases": results,
    }


def observation_audit(files: list[Path]) -> dict:
    code = "\n".join(f.read_text(encoding="utf-8", errors="replace") for f in files)
    return {"uses_check_events": "check_events(" in code,
            "uses_check_spec": "check_spec(" in code,
            "references_queue_events": any(e in code for e in ("ePut", "eTake", "eSnapshot")),
            "lexical_flags_informational_only": True,
            "not_a_full_validity_proof": True,
            "scored_separately": True}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--submission", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--image", required=True)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=90)
    args = parser.parse_args()
    submission, binary = args.submission.resolve(), args.binary.resolve()
    if not binary.is_file():
        parser.error(f"checker binary does not exist: {binary}")
    args.output.parent.mkdir(parents=True, exist_ok=True)

    test_files = sorted(p for p in submission.rglob("*.py") if p.is_file() and
                        not p.is_symlink() and p.name.startswith("test") and "__pycache__" not in p.parts)
    missing = [name for name in REQUIRED if not (submission / name).is_file()]
    report = {"version": "queue-flow-v2", "missing_deliverables": missing,
              "test_files": [str(p.relative_to(submission)) for p in test_files],
              "observation_audit": observation_audit(test_files),
              "implementation": run_contract(submission, binary, args.image, args.timeout),
              "submitted_tests": run_submitted_suite(submission, binary, args.image, args.timeout),
              "model": run_model(submission, binary, args.image, args.timeout),
              "monitor_traces": run_monitor_traces(submission, binary, args.image, args.timeout),
              "portable_probes": run_portable_probes(submission, binary, args.image, args.timeout)}
    report["summary"] = {
        "implementation_passed": report["implementation"]["status"] == "passed",
        "submitted_tests_passed": report["submitted_tests"]["status"] == "passed",
        "model_check_passed": report["model"].get("model_check_passed", False),
        "model_search_complete": report["model"].get("search_complete", False),
        "model_selected_tests": report["model"].get("selected_tests", 0),
        "monitor_valid_accepted": report["monitor_traces"]["valid_accepted"],
        "monitor_valid_total": report["monitor_traces"]["valid_total"],
        "monitor_invalid_rejected": report["monitor_traces"]["invalid_rejected"],
        "monitor_invalid_total": report["monitor_traces"]["invalid_total"],
        "monitor_traces_passed": report["monitor_traces"]["status"] == "passed",
        "portable_probe_valid": report["portable_probes"]["portable_probe_valid"],
        "portable_probe_reference_status": report["portable_probes"]["reference_status"],
        "timeouts": sum(r.get("status") == "timeout" for r in
                         [report["implementation"], report["submitted_tests"], report["model"]]
                         + [case for case in report["monitor_traces"].get("cases", [])]),
        "missing_deliverables": missing,
    }
    if report["portable_probes"]["portable_probe_valid"]:
        report["summary"]["portable_probe_mutations_rejected"] = report["portable_probes"]["mutations_rejected"]
        report["summary"]["portable_probe_mutations_total"] = report["portable_probes"]["mutations_total"]
    report["summary"]["valid"] = (not missing and report["summary"]["implementation_passed"] and
        report["summary"]["submitted_tests_passed"] and report["summary"]["model_check_passed"] and
        report["summary"]["monitor_traces_passed"])
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report["summary"], sort_keys=True))
    return 0 if report["summary"]["valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
