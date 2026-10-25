"""Run private contract tests against the reference and each seeded fault."""

from pathlib import Path
import os
import re
import shutil
import subprocess
import sys
import tempfile
import argparse
import json

from calibration import variants
from model_cases import cases as trace_cases


HERE = Path(__file__).resolve().parent
PUBLIC = HERE.parent / "public"
TESTS = HERE / "hidden_tests" / "test_contract.py"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path,
        default=Path(__file__).resolve().parents[5] / "target" / "release" / "snappea")
    args = parser.parse_args()
    correct = (HERE / "reference_solution.py").read_text(encoding="utf-8")
    bad = False
    for case, source in variants(correct).items():
        with tempfile.TemporaryDirectory(prefix="queue-flow-calibration-") as temp:
            work = Path(temp)
            shutil.copy2(PUBLIC / "runtime.py", work / "runtime.py")
            shutil.copy2(TESTS, work / "test_contract.py")
            (work / "solution.py").write_text(source, encoding="utf-8")
            result = subprocess.run([sys.executable, "-m", "unittest", "-v", "test_contract"],
                cwd=work, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                timeout=12, check=False)
            if case == "reference":
                status = "PASS" if result.returncode == 0 else "FAIL"
                expected = "PASS"
            else:
                has_failures = re.search(r"FAILED \(failures=\d+", result.stdout) is not None
                status = "KILLED" if result.returncode != 0 and has_failures else "NOT_KILLED_OR_INFRA"
                expected = "KILLED"
            print(f"{case}: {status} (expected {expected})")
            if status != expected:
                bad = True
                print(result.stdout)
    binary = args.binary.resolve()
    monitor = HERE / "reference_monitor.p"
    for label, valid, events in trace_cases():
        with tempfile.TemporaryDirectory(prefix="queue-flow-monitor-calibration-") as temp:
            work = Path(temp)
            shutil.copy2(PUBLIC / "events.p", work / "events.p")
            shutil.copy2(monitor, work / "monitor.p")
            (work / "history.jsonl").write_text(
                "".join(json.dumps(event) + "\n" for event in events), encoding="utf-8")
            result = subprocess.run([str(binary), "observe", "events.p", "monitor.p", "history.jsonl",
                "--spec", "QueueContract", "--strict", "--unknown-events", "error", "--json"],
                cwd=work, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                timeout=15, check=False)
            expected_code = 0 if valid else 1
            status = "PASS" if result.returncode == expected_code and (valid or "spec_violation" in result.stdout) else "FAIL"
            print(f"monitor-{label}: {status}")
            if status != "PASS":
                bad = True
                print(result.stdout)
    return 1 if bad else 0


if __name__ == "__main__":
    raise SystemExit(main())
