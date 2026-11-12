#!/usr/bin/env python3
"""Profile-guided optimization (PGO) of the release binary (.github/workflows/release.yml).

  scripts/pgo.py train BINARY PROFILE    run BINARY, built with RUSTFLAGS=-Cprofile-generate=DIR,
                                         over the tutorial models, then merge DIR's raw
                                         profiles into PROFILE (a .profdata file)

Then build again with RUSTFLAGS=-Cprofile-use=PROFILE, keeping every other flag the same.
The profile tells the compiler which code is hot, mostly the interpreter loop, so it keeps
that code's variables in registers and lays it out for the common paths. Behavior is
unchanged; checking is about 10-15% faster.

The raw profiles go to the directory given to -Cprofile-generate, which this script finds
from RUSTFLAGS. Needs the llvm-tools rustup component (`rustup component add llvm-tools`)
for llvm-profdata. Standard library only; runs on every release runner, Windows included.

Example:
  RUSTFLAGS="-Cprofile-generate=/tmp/pgo" cargo build --release -p snappea --bin snappea
  python scripts/pgo.py train target/release/snappea /tmp/pgo/merged.profdata
  RUSTFLAGS="-Cprofile-use=/tmp/pgo/merged.profdata" cargo build --release -p snappea --bin snappea
"""

import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parent.parent
TUTORIAL = ROOT / "tests" / "upstream" / "Tutorial"

# (project, test cases, extra options): search, lint and mutation testing over models of
# different shapes. Each check runs for its timeout, so the total is about 30 seconds.
TRAINING = [
    ("1_ClientServer", ["tcMultipleClients"], ["--mutation-timeout", "5s"]),
    ("2_TwoPhaseCommit", ["tcMultipleClientsWithFailure"], ["--no-mutation"]),
    ("3_EspressoMachine", ["tcCrazyUserUsingCoffeeMachine"], ["--no-mutation"]),
    ("4_FailureDetector", ["tcTest_FailureDetector"], ["--no-mutation"]),
    ("5_Paxos", ["testBasicPaxos3on3"], ["--no-mutation"]),
    ("6_Raft", ["oneClientThreeServersUnreliable", "twoClientsThreeServersReliable"], ["--no-mutation"]),
    ("PriorityQueue", ["tcCheck_PriorityQueue"], ["--no-mutation"]),
]


def fail(msg):
    print(f"error: {msg}", file=sys.stderr)
    sys.exit(1)


def raw_profile_dir():
    """The directory of -Cprofile-generate=DIR in RUSTFLAGS."""
    for flag in os.environ.get("RUSTFLAGS", "").split():
        if flag.startswith("-Cprofile-generate="):
            return pathlib.Path(flag.split("=", 1)[1])
    fail("RUSTFLAGS has no -Cprofile-generate=DIR\n"
         "help: export the RUSTFLAGS the instrumented binary was built with")


def llvm_profdata():
    """llvm-profdata from the llvm-tools component, which matches rustc's LLVM."""
    sysroot = subprocess.run(["rustc", "--print", "sysroot"], capture_output=True, text=True, check=True)
    exe = "llvm-profdata.exe" if os.name == "nt" else "llvm-profdata"
    found = sorted(pathlib.Path(sysroot.stdout.strip()).glob(f"lib/rustlib/*/bin/{exe}"))
    if not found:
        fail("could not find llvm-profdata in the Rust toolchain\n"
             "help: rustup component add llvm-tools")
    return found[0]


def train(binary, profile):
    binary = pathlib.Path(binary).resolve()
    raw = raw_profile_dir()
    with tempfile.TemporaryDirectory() as tmp:
        # a copy, so check output stays out of the repository
        models = pathlib.Path(tmp) / "Tutorial"
        shutil.copytree(TUTORIAL, models)
        for project, tests, extra in TRAINING:
            cmd = [str(binary), "--format", "text", "check", *tests, "--timeout", "2s", "--warn", "all", "--seed", "1", *extra]
            print(f"$ (cd {project} && {' '.join(cmd[1:])})", flush=True)
            r = subprocess.run(cmd, cwd=models / project, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
            # 0: no bugs; 1: a bug found, which is fine for training
            if r.returncode not in (0, 1):
                fail(f"snappea check in {project} exited with {r.returncode}:\n{r.stderr}")
    raws = sorted(raw.glob("*.profraw"))
    if not raws:
        fail(f"no .profraw files in {raw}\nhelp: build BINARY with RUSTFLAGS=-Cprofile-generate={raw}")
    subprocess.run([str(llvm_profdata()), "merge", "-o", str(profile), *map(str, raws)], check=True)
    print(f"merged {len(raws)} raw profiles into {profile}")


def main(argv):
    if len(argv) == 3 and argv[0] == "train":
        train(argv[1], argv[2])
    else:
        print(__doc__, file=sys.stderr)
        sys.exit(2)


if __name__ == "__main__":
    main(sys.argv[1:])
