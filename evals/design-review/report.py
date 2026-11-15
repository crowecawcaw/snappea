#!/usr/bin/env python3
"""Summarize graded runs by arm.

    python report.py            # all runs
    python report.py i1-        # runs whose id starts with i1-
"""
import json
import sys
from collections import defaultdict
from pathlib import Path

RUNS = Path(__file__).resolve().parent / "runs"
prefix = sys.argv[1] if len(sys.argv) > 1 else ""
rows, by_arm = [], defaultdict(list)
for run in sorted(RUNS.glob(f"{prefix}*")):
    if (run / "EXCLUDE").exists() or not (run / "grade.json").exists():
        continue
    meta = json.loads((run / "meta.json").read_text())
    grade = json.loads((run / "grade.json").read_text())
    stats = json.loads((run / "stats.json").read_text())
    s, p = grade["score"], stats["process"]
    found = [k for k, v in grade["issues"].items() if v["verdict"] == "found"]
    rows.append((run.name, meta["arm"], s["found"], s["partial"], s["checker_confirmed"],
                 s["other_valid"], s["other_invalid"], p["checker_runs"], p["checker_runs_reporting_bug"],
                 stats["final_check"]["status"], ",".join(found)))
    by_arm[meta["arm"]].append(s)
hdr = ("run", "arm", "found", "partial", "chk_conf", "oth_ok", "oth_bad", "checks", "chk_bugs", "final", "issues")
print(" | ".join(hdr))
for r in rows:
    print(" | ".join(str(x) for x in r))
print()
for arm, ss in sorted(by_arm.items()):
    n = len(ss)
    mean = lambda k: sum(x[k] for x in ss) / n
    print(f"{arm}: n={n} found={mean('found'):.2f} partial={mean('partial'):.2f} "
          f"checker_confirmed={mean('checker_confirmed'):.2f} invalid_claims={mean('other_invalid'):.2f}")
