#!/usr/bin/env python3
"""Run the Python parts of the `snappea guide` examples against a snappea binary.

The model checks of the examples run in `cargo test -p snappea --test cli_guide`.
This script writes every example with `snappea guide example NAME --out DIR` and runs
its Python tests (needs pysnappea, imported as `snappea`, and pytest).

    python scripts/check-guide-examples.py --binary target/debug/snappea
"""
import argparse
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

RUNS = [
    ("cas-retry", [sys.executable, "check_examples.py", "--binary", "{binary}"]),
    ("forced-interleaving", [sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider"]),
    ("reservation", [sys.executable, "-m", "unittest", "-v"]),
    ("durable-retry", [sys.executable, "-m", "unittest", "-v"]),
    ("versioned-publication", [sys.executable, "-m", "unittest", "-v"]),
]


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--binary", default=shutil.which("snappea"), help="snappea binary (default: on PATH)")
    args = parser.parse_args()
    if not args.binary:
        parser.error("set --binary or put snappea on PATH")
    binary = str(Path(args.binary).resolve())
    env = dict(os.environ, SNAPPEA_BIN=binary)
    failed = []
    with tempfile.TemporaryDirectory(prefix="snappea-guide-") as tmp:
        for name, command in RUNS:
            subprocess.run([binary, "guide", "example", name, "--out", tmp], check=True, capture_output=True)
            command = [c.format(binary=binary) for c in command]
            r = subprocess.run(command, cwd=Path(tmp) / name, env=env, capture_output=True, text=True, timeout=300)
            print(f"{name}: {'PASS' if r.returncode == 0 else 'FAIL'}")
            if r.returncode:
                failed.append(name)
                print(r.stdout + r.stderr)
    if failed:
        sys.exit(f"failed: {', '.join(failed)}")


if __name__ == "__main__":
    main()
