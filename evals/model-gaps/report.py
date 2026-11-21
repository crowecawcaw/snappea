#!/usr/bin/env python3
"""Summarize graded runs by condition.

    python report.py            # all runs
    python report.py a-         # runs whose id starts with a-
"""
import json
import sys
from collections import defaultdict
from pathlib import Path

RUNS = Path(__file__).resolve().parent / "runs"
prefix = sys.argv[1] if len(sys.argv) > 1 else ""
rows = []
for run in sorted(RUNS.glob(f"{prefix}*")):
    if (run / "EXCLUDE").exists() or not (run / "grade.json").exists():
        continue
    meta = json.loads((run / "meta.json").read_text())
    rows.append((run.name, meta["task"], meta.get("label", meta["arm"]), json.loads((run / "grade.json").read_text())))

build = [r for r in rows if "passed" in r[3]]
rows = [r for r in rows if "passed" not in r[3]]
if build:
    print("run | task | condition | hidden tests passed | commands | minutes | command kinds | failed")
    agg = defaultdict(list)
    for name, task, label, g in build:
        p = g["process"]
        kinds = " ".join(f"{k}={v}" for k, v in sorted(p["kinds"].items()))
        print(f"{name} | {task} | {label} | {len(g['passed'])}/{g['tests']} | {p['commands']} | {p['minutes']} | "
              f"{kinds} | {' '.join(sorted(g['failed']))}")
        agg[(task, label)].append(g)
    print()
    for (task, label), gs in sorted(agg.items()):
        n = len(gs)
        print(f"{task} {label}: runs={n} passed={sum(len(g['passed']) for g in gs)}/{sum(g['tests'] for g in gs)} "
              f"all-pass runs={sum(not g['failed'] and g['tests'] > 0 for g in gs)}/{n} "
              f"commands/run={sum(g['process']['commands'] for g in gs) / n:.0f} "
              f"minutes/run={sum(g['process']['minutes'] for g in gs) / n:.1f}"
              + (f" mistakes caught={sum(len(g['guard']['found']) for g in gs)}/{sum(g['guard'].get('bugs', 0) for g in gs)}"
                 f" ({' '.join(str(len(g['guard']['found'])) for g in gs)})"
                 f" false alarms={sum(len(g['guard']['false_alarms']) for g in gs)}" if all("guard" in g for g in gs) else ""))
    print()
if not rows:
    sys.exit(0)
print("run | task | condition | found | false alarms | tests | commands | minutes | found at call | command kinds")
for name, task, label, g in rows:
    p = g["process"]
    at = ",".join(str(g["found_at_call"].get(b) or "?") for b in g["found"])
    kinds = " ".join(f"{k}={v}" for k, v in sorted(p["kinds"].items()))
    print(f"{name} | {task} | {label} | {len(g['found'])}/{g.get('bugs', '?')} | {len(g['false_alarms'])} | "
          f"{g['tests']} | {p['commands']} | {p['minutes']} | {at} | {kinds}")

by_label = defaultdict(list)
by_bug = defaultdict(lambda: defaultdict(list))
for _, task, label, g in rows:
    by_label[label].append(g)
    for b in g.get("found_by", {}):
        by_bug[(task, b)][label].append(1)
print()
for label, gs in sorted(by_label.items()):
    n = len(gs)
    found, bugs = sum(len(g["found"]) for g in gs), sum(g.get("bugs", 0) for g in gs)
    # Calls before the last test that found a bug was written; the rest of the run found nothing new.
    productive = [max((c for c in g["found_at_call"].values() if c), default=0) for g in gs]
    print(f"{label}: runs={n} bugs found={found}/{bugs} ({found / max(bugs, 1):.0%}) "
          f"false alarms/run={sum(len(g['false_alarms']) for g in gs) / n:.2f} "
          f"runs with none={sum(not g['found'] for g in gs)} "
          f"commands/run={sum(g['process']['commands'] for g in gs) / n:.0f} "
          f"commands after last finding/run={sum(g['process']['commands'] - c for g, c in zip(gs, productive)) / n:.0f}")
labels = sorted(by_label)
tasks = defaultdict(lambda: defaultdict(int))
for _, task, label, _ in rows:
    tasks[task][label] += 1
print("\nbug | " + " | ".join(labels))
seen = sorted({(task, b) for _, task, _, g in rows for b in g.get("found_by", {})} | set(by_bug))
for task, b in seen:
    print(f"{task}/{b} | " + " | ".join(f"{len(by_bug[(task, b)][l])}/{tasks[task][l]}" for l in labels))
