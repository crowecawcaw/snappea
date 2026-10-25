"""Private calibration: ensure the reference observation suite kills known faults."""

import argparse
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


HERE = Path(__file__).resolve().parent
PHASE = HERE.parent.parent
PUBLIC = PHASE / "public"
REFERENCE = HERE / "reference_tests"


def variants(correct):
    stale_write = correct.replace(
        "if not self.__active() or self.__owner != client or self.__token != token:",
        "if not self.__active() or self.__owner != client:", 1)
    expired_write = correct.replace(
        "if not self.__active() or self.__owner != client or self.__token != token:",
        "if self.__owner != client or self.__token != token:", 1)
    stale_release = correct.replace(
        "        if not self.__active() or self.__owner != client or self.__token != token:\n"
        "            return False\n"
        "        self.__owner = None",
        "        if self.__owner is None:\n"
        "            return False\n"
        "        self.__owner = None", 1)
    reset_token = correct.replace(
        "        self.__expires_at = None\n        return True\n\n    def advance",
        "        self.__expires_at = None\n        self.__last_token = 0\n        return True\n\n    def advance", 1)
    inclusive_expiry = correct.replace(
        "return self.__owner is not None and self.__expires_at > self.__now",
        "return self.__owner is not None and self.__expires_at >= self.__now", 1)
    always_reject = correct.replace(
        "    def acquire(self, client, ttl):\n        if self.__active():",
        "    def acquire(self, client, ttl):\n        return None\n        if self.__active():", 1)
    silent_rejected_write = correct.replace(
        "    def write(self, client, token, value):\n",
        "    def write(self, client, token, value):\n        self.__value = value\n", 1)
    stale_release_clobber = correct.replace(
        "    def release(self, client, token):\n",
        "    def release(self, client, token):\n"
        "        if self.__active() and token < self.__token:\n"
        "            self.__owner = None\n"
        "            self.__token = None\n"
        "            self.__expires_at = None\n"
        "            return False\n", 1)
    no_write_effect = correct.replace("        self.__value = value\n", "        pass  # lost durable effect\n", 1)
    valid_gaps = correct.replace("        self.__last_token += 1\n", "        self.__last_token += 3\n", 1)
    result = {
        "correct": correct,
        "valid-token-gaps": valid_gaps,
        "fault-a": stale_write,
        "fault-b": expired_write,
        "fault-c": stale_release,
        "fault-d": reset_token,
        "fault-e": inclusive_expiry,
        "fault-f": always_reject,
        "fault-g": silent_rejected_write,
        "fault-h": stale_release_clobber,
        "fault-i": no_write_effect,
    }
    assert all(source != correct for name,source in result.items() if name != "correct")
    assert len(set(result.values())) == len(result)
    return result


def run_one(name, source, timeout):
    with tempfile.TemporaryDirectory(prefix="lease-calibration-") as temp:
        work = Path(temp)
        shutil.copytree(REFERENCE, work / "tests")
        public = work / "public"
        public.mkdir()
        shutil.copy(PUBLIC / "lease_contract.p", public / "lease_contract.p")
        shutil.copy(PUBLIC / "events.p", public / "events.p")
        (public / "lease.py").write_text(source, encoding="utf-8")
        env = os.environ.copy()
        pythonpath = [str(public), *env.get("PYTHONPATH", "").split(os.pathsep)]
        env["PYTHONPATH"] = os.pathsep.join(p for p in pythonpath if p)
        try:
            result = subprocess.run(
                [sys.executable, "-m", "unittest", "discover", "-s", "../tests", "-t", "..", "-v"],
                cwd=public, env=env, text=True, stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT, timeout=timeout, check=False)
        except subprocess.TimeoutExpired:
            return "TIMEOUT", "test process exceeded the calibration limit"
        output = result.stdout
        if result.returncode == 0:
            return "PASS", output
        if "FAILED (failures=" in output and "errors=" not in output:
            return "ASSERTION_FAILURE", output
        return "INFRA_OR_UNEXPECTED", output


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout", type=float, default=15)
    args = parser.parse_args()
    correct = (PUBLIC / "lease.py").read_text(encoding="utf-8")
    failed = False
    for name, source in variants(correct).items():
        status, output = run_one(name, source, args.timeout)
        expected = "PASS" if name in ("correct", "valid-token-gaps") else "ASSERTION_FAILURE"
        print(f"{name}: {status} (expected {expected})")
        if status != expected:
            failed = True
            print(output)
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
