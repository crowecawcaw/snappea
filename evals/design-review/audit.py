#!/usr/bin/env python3
"""Flag solver tool calls that did not go through the bridge.

    python audit.py <run_id> <subagent transcript .jsonl>

Writes runs/<run_id>/audit.json. Any violation means the run should get an EXCLUDE
file unless the call was harmless (say why in EXCLUDE or in the audit notes).
"""
import json
import sys
from pathlib import Path

run_id, path = sys.argv[1], Path(sys.argv[2])
uses = []
for line in path.read_text().splitlines():
    try:
        ev = json.loads(line)
    except json.JSONDecodeError:
        continue
    msg = ev.get("message") or {}
    for part in msg.get("content") or [] if isinstance(msg.get("content"), list) else []:
        if part.get("type") == "tool_use":
            uses.append(part)
bad = []
for u in uses:
    cmd = (u.get("input") or {}).get("command", "")
    if u.get("name") == "SubagentHandback":
        continue
    if u.get("name") != "Bash" or f"harness.py bridge {run_id}" not in cmd:
        bad.append({"tool": u.get("name"), "input": json.dumps(u.get("input"))[:300]})
out = {"tool_calls": len(uses), "violations": bad}
(Path(__file__).resolve().parent / "runs" / run_id / "audit.json").write_text(json.dumps(out, indent=2))
print(f"{len(uses)} tool calls, {len(bad)} outside the bridge")
for b in bad:
    print(" ", b["tool"], b["input"][:200])
