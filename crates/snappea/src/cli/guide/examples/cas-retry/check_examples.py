#!/usr/bin/env python3
"""Check the correct model, independent histories and one-fault negative controls.

Each control is materialized in a temporary copy. Exact replacement counts keep
it from silently becoming a no-op as the example evolves. Fail only on the named
property assertion, never accept a compile/coverage/timeout failure as detection.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent
FAULTS = {
    "lost-update": (
        "if (p.expected == version)",
        "if (true)",
        "operation changed balance incorrectly",
    ),
    "retry-reapplies": (
        "      assert p.command.id in memos;",
        "      assert p.command.id in memos;\n"
        "      if (p.command.credit) { balance = balance + p.command.units; }\n"
        "      else { balance = balance - p.command.units; }",
        "operation changed balance incorrectly",
    ),
    "rejected-write": (
        "        balance = p.balance;",
        "        balance = p.balance;\n        if (!p.accepted) { balance = balance - p.command.units; }",
        "operation changed balance incorrectly",
    ),
    "wrong-reply": (
        "        send p.client, eResult, p.accepted;",
        "        send p.client, eResult, !p.accepted;",
        "client received wrong result",
    ),
}


def run(binary):
    with tempfile.TemporaryDirectory(prefix="cas-retry-example-") as tmp:
        work = Path(tmp)
        shutil.copytree(ROOT, work, dirs_exist_ok=True,
                        ignore=shutil.ignore_patterns("__pycache__", "PCheckerOutput"))
        source = (work / "Balance.p").read_text()
        for label, fault in [("correct", None), *FAULTS.items()]:
            text = source
            if fault:
                old, new, message = fault
                assert source.count(old) == 1, label
                text = source.replace(old, new)
            (work / "Balance.p").write_text(text)
            result = subprocess.run(
                [str(binary), "check", "Balance.pproj", "--no-mutation", "--timeout", "5s", "--json"],
                cwd=work, capture_output=True, text=True, timeout=30,
            )
            report = json.loads(result.stdout)
            cases = report.get("test_cases", [])
            if fault:
                failures = [c for c in cases if c.get("result") == "bug"]
                reports = [c.get("bug", {}).get("report", "") for c in failures]
                assert result.returncode == 1 and any(message in r for r in reports), (label, reports)
            else:
                assert result.returncode == 0 and cases, result.stdout + result.stderr
                assert all(c["search_complete"] for c in cases), result.stdout
            print(f"{label}: PASS", flush=True)
        valid_lines = (work / "valid-concurrent.jsonl").read_text().splitlines()
        (work / "unfinished.jsonl").write_text("\n".join(valid_lines[:-1]) + "\n")
        (work / "empty-work.jsonl").write_text('{"event":"eDone"}\n')
        probes = [("valid-concurrent.jsonl", 0, None),
                  ("invalid-retry.jsonl", 1, "operation changed balance incorrectly"),
                  ("unfinished.jsonl", 1, "liveness bug"),
                  ("empty-work.jsonl", 1, "scenario omitted a fresh request")]
        for name, expected, diagnostic in probes:
            result = subprocess.run([str(binary), "observe", "Balance.p", name,
                                     "--spec", "BalanceContract", "--strict", "--check-hot", "--json"],
                                    cwd=work, capture_output=True, text=True, timeout=30)
            assert result.returncode == expected, result.stdout + result.stderr
            if expected:
                observed = json.loads(result.stdout)
                errors = [e for spec in observed["specs"] for e in spec["errors"]]
                assert any(e["kind"] == "spec_violation" and
                           diagnostic in e["message"] for e in errors)
            print(f"{name}: PASS", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=shutil.which("snappea"))
    args = parser.parse_args()
    if not args.binary:
        parser.error("install snappea or pass --binary")
    run(Path(args.binary).resolve())
