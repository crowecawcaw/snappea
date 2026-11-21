#!/usr/bin/env python3
"""Show when a build-mode run's design reached its final score.

    python trajectory.py <run-id>...

Grades the design in each snapshot with the hidden tests and prints
(command number, minute, tests passed; -1 means it did not compile). Commands after
the score stops changing did not improve the design.
"""
import hashlib
import json
import sys
from pathlib import Path

import harness

for run_id in sys.argv[1:]:
    run = harness.HERE / "runs" / run_id
    task = json.loads((run / "meta.json").read_text())["task"]
    calls = [json.loads(line) for line in (run / "transcript.jsonl").open()]
    minute = {c["call"]: (c["elapsed"] + c["seconds"]) / 60 for c in calls}
    seen, points = {}, []
    for snap in sorted(p for p in (run / "snapshots").iterdir() if p.is_dir()):
        src = snap / "PSrc"
        if not src.exists():
            continue
        digest = hashlib.sha256(b"".join(p.read_bytes() for p in sorted(src.rglob("*.p")))).hexdigest()
        if digest not in seen:
            g = harness.grade_design(task, src)
            seen[digest] = len(g["passed"]) if g["status"] == "ok" else -1
        points.append((int(snap.name), round(minute[int(snap.name)], 1), seen[digest]))
    final = points[-1][2] if points else None
    settled = next((p for p in points if p[2] == final), None)
    print(f"{run_id}: {len(calls)} commands, {max(minute.values()):.1f} min; "
          f"final score first reached at command {settled[0]}, minute {settled[1]}; {points}")
