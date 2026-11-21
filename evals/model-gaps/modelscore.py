#!/usr/bin/env python3
"""Score the specs and tests of finished runs against the task's hidden design mistakes.

    python3 modelscore.py d1-cb      # runs whose id starts with d1-cb

Works for any mode: a build or harden run also leaves a model behind, and this asks
how many seeded mistakes that model would catch. Writes runs/<id>/modelscore.json.
"""
import json
import sys
from collections import defaultdict
from pathlib import Path

import harness

agg = defaultdict(list)
for run in sorted(harness.RUNS.glob(f"{sys.argv[1] if len(sys.argv) > 1 else ''}*")):
    meta = json.loads((run / "meta.json").read_text())
    model = run / "submission" / "model"
    if (run / "EXCLUDE").exists() or not model.exists() or not harness.load_task(meta["task"]).get("bug"):
        continue
    out = run / "modelscore.json"
    g = json.loads(out.read_text()) if out.exists() else harness.grade_tests(meta["task"], model)
    out.write_text(json.dumps(g, indent=2))
    missed = sorted(set(harness.bug_ids(meta["task"])) - set(g["found"]))
    print(f"{run.name} | {meta['mode']} | {meta.get('label', meta['arm'])} | caught {len(g['found'])}/{g.get('bugs', '?')} | "
          f"false alarms {len(g['false_alarms'])} | tests {g['tests']} | missed: {' '.join(missed)} {g.get('error', '')[:200]}")
    agg[(meta["task"], meta["mode"], meta.get("label", meta["arm"]))].append(g)
print()
for (task, mode, label), gs in sorted(agg.items()):
    print(f"{task} {mode} {label}: runs={len(gs)} caught={sum(len(g['found']) for g in gs)}/{sum(g.get('bugs', 0) for g in gs)} "
          f"false alarms={sum(len(g['false_alarms']) for g in gs)}")
