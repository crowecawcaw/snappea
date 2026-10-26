"""Calibrate the frozen observation suite against correct variants and queue faults."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from mutants import variants


HERE = Path(__file__).resolve().parent
PHASE = HERE.parent.parent
PUBLIC = PHASE / "public"
REFERENCE = HERE / "reference_tests"


def run_one(source, timeout):
    with tempfile.TemporaryDirectory(prefix="priority-calibration-") as temp:
        work = Path(temp)
        shutil.copytree(REFERENCE, work / "tests")
        public = work / "public"
        public.mkdir()
        for filename in ("events.p", "contract.p"):
            shutil.copy(PUBLIC / filename, public / filename)
        (public / "queue_service.py").write_text(source, encoding="utf-8")
        env = os.environ.copy()
        env["PYTHONPATH"] = os.pathsep.join(
            p for p in (str(public), env.get("PYTHONPATH", "")) if p)
        try:
            result = subprocess.run(
                [sys.executable, "-m", "unittest", "discover", "-s", "../tests", "-t", "..", "-v"],
                cwd=public, env=env, text=True, stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT, timeout=timeout, check=False)
        except subprocess.TimeoutExpired:
            return "TIMEOUT", "test process exceeded calibration limit"
        output = result.stdout
        if result.returncode == 0:
            return "PASS", output
        if "FAILED (failures=" in output and "errors=" not in output:
            return "ASSERTION_FAILURE", output
        return "INFRA_OR_UNEXPECTED", output


def direct_observe(binary, events, should_pass):
    with tempfile.TemporaryDirectory(prefix="priority-observe-") as temp:
        history = Path(temp) / "history.jsonl"
        lines = []
        for event in events:
            payload = {name: value for name, value in event.items() if name != "event"}
            lines.append(json.dumps({"event": event["event"], "payload": payload}) + "\n")
        history.write_text("".join(lines), encoding="utf-8")
        result = subprocess.run(
            [str(binary), "observe", "events.p", "contract.p", str(history),
             "--spec", "QueueContract", "--strict", "--unknown-events", "error", "--json"],
            cwd=PUBLIC, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            timeout=20, check=False)
        passed = result.returncode == (0 if should_pass else 1)
        if not should_pass:
            try:
                report = json.loads(result.stdout)
                passed = passed and any(
                    error.get("kind") == "spec_violation"
                    for spec in report.get("specs", []) for error in spec.get("errors", []))
            except (ValueError, AttributeError):
                passed = False
        return passed, result.stdout


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout", type=float, default=20)
    args = parser.parse_args()
    correct = (PUBLIC / "queue_service.py").read_text(encoding="utf-8")
    failed = False
    for name, source in variants(correct).items():
        status, output = run_one(source, args.timeout)
        expected = "PASS" if name in ("correct", "valid-counter-offset") else "ASSERTION_FAILURE"
        print(f"{name}: {status} (expected {expected})")
        if status != expected:
            failed = True
            print(output)
    binary = Path(os.environ.get("SNAPPEA_BIN", ""))
    if not binary.is_file():
        print("direct observe: SNAPPEA_BIN must name the pinned checker binary")
        failed = True
    else:
        probes = {
            "valid": ([
                {"event": "eSubmit", "item_id": "later", "priority": 4, "accepted": True},
                {"event": "eSubmit", "item_id": "urgent", "priority": -2, "accepted": True},
                {"event": "ePop", "item_id": "urgent"},
            ], True),
            "invalid-duplicate-result": ([
                {"event": "eSubmit", "item_id": "same", "priority": 4, "accepted": True},
                {"event": "eSubmit", "item_id": "same", "priority": 3, "accepted": True},
            ], False),
        }
        for name, (events, should_pass) in probes.items():
            passed, output = direct_observe(binary, events, should_pass)
            print(f"direct observe {name}: {'PASS' if passed else 'FAIL'}")
            if not passed:
                failed = True
                print(output)
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
