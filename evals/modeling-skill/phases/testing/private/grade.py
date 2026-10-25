#!/usr/bin/env python3
"""Grade saved lease observation tests in fresh offline containers."""

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

from calibration.calibrate import variants


HERE = Path(__file__).resolve().parent
PHASE = HERE.parent
PUBLIC = PHASE / "public"
CONTRACT_FILES = ("TASK.md", "lease.py", "events.p", "lease_contract.p")
TEST_RUN_RE = re.compile(r"Ran\s+(\d+)\s+tests?", re.MULTILINE)
SUMMARY_RE = re.compile(r"FAILED \(([^)]*)\)")
SETUP_MARKERS = (
    "ModuleNotFoundError", "ImportError", "SyntaxError", "Could not find the snappea binary",
    "SNAPPEA_BIN=", "matches no files", "no tests ran", "Start directory is not importable",
    "SetupError", "SnappeaCompilationError", "compilation failed", "could not parse",
    "unknown event", "unselected-spec", "type error in p source",
)


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare_submission(source: Path, workspace: Path) -> list[str]:
    copied: list[str] = []
    if not source.is_dir():
        raise ValueError(f"submission is not a directory: {source}")
    for item in source.rglob("*"):
        if not item.is_file() or item.is_symlink() or "__pycache__" in item.parts:
            continue
        relative = item.relative_to(source)
        if relative.name in CONTRACT_FILES:
            continue
        target = workspace / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(item, target)
        copied.append(str(relative))
    for name in CONTRACT_FILES:
        shutil.copy2(PUBLIC / name, workspace / name)
    return copied


def classify(exit_code: int | None, output: str, timed_out: bool) -> dict:
    if timed_out:
        return {"status": "timeout", "test_count": None}
    match = TEST_RUN_RE.search(output)
    count = int(match.group(1)) if match else 0
    summary = SUMMARY_RE.search(output)
    has_test_failures = bool(summary and count > 0 and ("failures=" in summary.group(1) or "errors=" in summary.group(1)))
    setup_error = any(marker.lower() in output.lower() for marker in SETUP_MARKERS)
    if count == 0 or setup_error:
        status = "infrastructure_error"
    elif exit_code == 0:
        status = "passed"
    elif has_test_failures:
        status = "tests_rejected"
    else:
        status = "infrastructure_error"
    return {"status": status, "test_count": count}


def run_container(
    workspace: Path,
    binary: Path,
    image: str,
    output_dir: Path,
    timeout: float,
) -> dict:
    container_name = f"lease-test-{os.getpid()}-{time.time_ns()}"
    command = [
        "docker", "run", "--rm", "--network", "none", "--cap-drop", "ALL",
        "--name", container_name,
        "--security-opt", "no-new-privileges", "--user", f"{os.getuid()}:{os.getgid()}",
        "--memory", "768m", "--cpus", "1", "--pids-limit", "128", "--read-only",
        "--tmpfs", "/tmp:rw,exec,size=128m",
        "--mount", f"type=bind,src={binary},dst=/opt/snappea,readonly",
        "--mount", f"type=bind,src={workspace},dst=/work",
        "--mount", f"type=bind,src={output_dir},dst=/output",
        "--workdir", "/work", "--env", "PATH=/opt:/usr/local/bin:/usr/bin:/bin",
        "--env", "SNAPPEA_BIN=/opt/snappea", image,
        "python", "-m", "unittest", "discover", "-v", "-p", "test*.py",
    ]
    started = time.monotonic()
    try:
        completed = subprocess.run(command, text=True, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, timeout=timeout, check=False)
        output = completed.stdout
        exit_code = completed.returncode
        timed_out = False
    except subprocess.TimeoutExpired as exc:
        output = (exc.stdout or b"").decode("utf-8", "replace") if isinstance(exc.stdout, bytes) else (exc.stdout or "")
        exit_code = None
        timed_out = True
        subprocess.run(["docker", "rm", "-f", container_name],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=False)
    result = classify(exit_code, output, timed_out)
    result.update({
        "exit_code": exit_code,
        "duration_seconds": round(time.monotonic() - started, 3),
        "output": output,
    })
    return result


def observation_audit(files: list[Path]) -> dict:
    source = "\n".join(path.read_text(encoding="utf-8", errors="replace") for path in files)
    return {
        "uses_check_events": "check_events(" in source,
        "uses_check_spec": "check_spec(" in source,
        "event_names_referenced": sorted(set(re.findall(r"e(?:Acquire|Write|Release|Advance|Read|Peek)", source))),
        "included_as_independent_score": False,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--submission", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--image", required=True)
    parser.add_argument("--binary", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=90)
    args = parser.parse_args()
    submission = args.submission.resolve()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"checker binary does not exist: {binary}")
    args.output.parent.mkdir(parents=True, exist_ok=True)

    candidate_tests = sorted(
        path for path in submission.rglob("*.py")
        if path.is_file() and not path.is_symlink() and path.name.startswith("test")
        and "__pycache__" not in path.parts
    )
    report = {
        "version": "lease-testing-v2",
        "submission_files": [],
        "test_files": [str(path.relative_to(submission)) for path in candidate_tests],
        "observation_audit": observation_audit(candidate_tests),
        "runs": [],
        "summary": {},
    }
    correct_source = (PUBLIC / "lease.py").read_text(encoding="utf-8")
    all_variants = variants(correct_source)
    # Mutation sources remain in the private grader and are copied into one disposable
    # workspace at a time. Only lease.py, contract files, and submitted Python files
    # enter a grading container.
    order = [("reference", all_variants["correct"]), ("valid-token-gaps", all_variants["valid-token-gaps"])] + [
        (f"mutation-{index:02d}", source)
        for index, (_, source) in enumerate([(n,s) for n,s in all_variants.items() if n.startswith("fault-")], start=1)
    ]
    for label, source in order:
        run_record = {"case": label}
        if not candidate_tests:
            run_record.update({"status": "infrastructure_error", "test_count": 0,
                               "output": "No submitted test*.py module was found."})
            report["runs"].append(run_record)
            continue
        with tempfile.TemporaryDirectory(prefix="lease-test-grade-") as temp:
            workspace = Path(temp) / "work"
            workspace.mkdir()
            copied = prepare_submission(submission, workspace)
            report["submission_files"] = copied
            (workspace / "lease.py").write_text(source, encoding="utf-8")
            outcome = run_container(workspace, binary, args.image, Path(temp), args.timeout)
            run_record.update(outcome)
        report["runs"].append(run_record)

    reference = report["runs"][0]["status"]
    valid_runs = report["runs"][:2]
    mutation_runs = report["runs"][2:]
    rejected = sum(run["status"] == "tests_rejected" for run in mutation_runs)
    infra = sum(run["status"] == "infrastructure_error" for run in report["runs"])
    timeouts = sum(run["status"] == "timeout" for run in report["runs"])
    report["summary"] = {
        "reference_passed": reference == "passed",
        "valid_variants_passed": sum(r["status"] == "passed" for r in valid_runs),
        "valid_variants_total": len(valid_runs),
        "mutations_rejected": rejected,
        "mutations_total": len(mutation_runs),
        "mutation_detection_rate": rejected / len(mutation_runs) if mutation_runs else None,
        "infrastructure_errors": infra,
        "timeouts": timeouts,
        "valid": all(r["status"] == "passed" for r in valid_runs) and infra == 0 and timeouts == 0,
    }
    args.output.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(report["summary"], sort_keys=True))
    return 0 if report["summary"]["valid"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
