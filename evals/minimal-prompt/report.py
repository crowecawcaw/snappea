#!/usr/bin/env python3
"""Summarize graded runs by condition (skill label, or `none`): python report.py [RUN-PREFIX]"""
import json
import re
import statistics
import sys
from collections import defaultdict
from pathlib import Path

RUNS = Path(__file__).resolve().parent / "runs"
# defects that only show when two calls interleave between a load and a compare-and-swap
RACE = {"ignore-cas-put", "ignore-cas-take", "stale-retry", "acquire-ignores-cas", "write-ignores-cas",
        "write-stale-retry", "claim-ignores-cas"}


def signals(run: Path) -> dict:
    sub = run / "submission"
    tests = "\n".join(p.read_text(errors="replace") for p in sub.rglob("test*.py"))
    rows = [json.loads(l) for l in (run / "transcript.jsonl").read_text().splitlines()] \
        if (run / "transcript.jsonl").exists() else []
    checks = [r["command"] for r in rows if re.search(r"\bsnappea\s+check\b", r["command"])]
    last = checks[-1] if checks else ""
    return {
        "forced_interleaving": bool(re.search(r"Barrier|def compare_swap|def load\(", tests)),
        "observation": bool(re.search(r"check_events|check_spec|Recorder\(", tests)),
        "design_doc": (sub / "DESIGN.md").exists(),
        "final_check_bypass": bool(re.search(r"--no-mutation|--allow-incomplete-coverage|--warn", last)),
        "minutes": round(rows[-1]["elapsed"] / 60, 1) if rows else 0,
    }


def frac(k, n):
    return k / n if n else None


def main():
    prefix = sys.argv[1] if len(sys.argv) > 1 else ""
    by = defaultdict(list)
    for g in sorted(RUNS.glob(f"{prefix}*/grade.json")):
        d = json.loads(g.read_text())
        run = g.parent
        if (run / "EXCLUDE").exists():  # e.g. a run cut short by an outage; say why in the file
            continue
        h, t, o, tr = d["hidden"], d["tests"], d.get("own_mutation", {}), d.get("transplant", {})
        m, p = d["model_check"], d["process"]
        s = signals(run)
        row = {
            "run": d["run_id"], "task": d["task"],
            "hidden": frac(h["passed"], h["total"]),
            "ref_kill": frac(t["killed"], t["total_mutants"]),
            "transplant_kill": frac(tr.get("killed", 0), tr.get("valid")) if tr.get("valid") else None,
            "own_kill": frac(o.get("killed", 0), o.get("total")),
            "race_kill": frac(sum(v["killed"] for k, v in tr.get("mutants", {}).items() if k in RACE),
                              sum(k in RACE for k in tr.get("mutants", {}))),
            "check_passed": m.get("status") == "passed",
            "model_first": p.get("model_first"), "calls": p.get("calls"), **s,
        }
        by[d["skill"]].append(row)

    cols = ["hidden", "transplant_kill", "race_kill", "ref_kill", "own_kill"]
    flags = ["check_passed", "model_first", "forced_interleaving", "observation", "design_doc",
             "final_check_bypass"]
    print(f"| condition | n | " + " | ".join(cols) + " | " + " | ".join(flags) + " | calls | minutes |")
    print("|" + "---|" * (len(cols) + len(flags) + 4))
    for cond in sorted(by):
        rows = by.get(cond, [])
        if not rows:
            continue
        def mean(k):
            vals = [r[k] for r in rows if r[k] is not None]
            return f"{statistics.mean(vals):.2f}" if vals else "-"
        def count(k):
            return f"{sum(bool(r[k]) for r in rows)}/{len(rows)}"
        print(f"| {cond} | {len(rows)} | " + " | ".join(mean(c) for c in cols) + " | "
              + " | ".join(count(f) for f in flags)
              + f" | {statistics.mean(r['calls'] for r in rows):.0f} | {statistics.mean(r['minutes'] for r in rows):.0f} |")
    print()
    print("| run | " + " | ".join(cols) + " | " + " | ".join(flags) + " |")
    print("|" + "---|" * (len(cols) + len(flags) + 1))
    for cond in sorted(by):
        for r in by.get(cond, []):
            print(f"| {r['run']} | " + " | ".join("-" if r[c] is None else f"{r[c]:.2f}" for c in cols) + " | "
                  + " | ".join("y" if r[f] else "" for f in flags) + " |")


if __name__ == "__main__":
    main()
