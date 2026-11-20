#!/usr/bin/env python3
"""Plausible wrong implementations, and a check that the task separates them.

    python3 check.py            # rebuild buggy/<id>/lease.py and print the table

Each bug is an edit to solution/lease.py. For each one this prints: whether the public
run_scenarios.py still reports ok, whether `snappea observe` on those public logs
already shows it, and which hidden scenarios fail.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
PRIVATE = HERE.parent
SNAPPEA = os.environ.get("SNAPPEA_BIN", str(PRIVATE.parents[4] / "target/release/snappea"))

BUGS = {
    # The lock service decides every Acquire message anew.
    "no-dedupe": [("""            if prev is not None and prev["n"] > n:
                return                      # a late copy of an older attempt
            if prev is not None and prev["n"] == n:
                self.send(src, prev)        # the same attempt again: same answer
                return
""", "")],
    # Storage keeps its state in memory.
    "volatile-storage": [("""        self.disk.setdefault("highest", 0)
        self.disk.setdefault("writes", {})   # client id -> [seq, ok] of its latest write
""", """        self.mem = {"highest": 0, "writes": {}}
"""), ("""        d = self.disk
        if msg["t"] != "write":""", """        d = self.mem
        if msg["t"] != "write":""")],
    # The lock service keeps its state on disk but forgets the expiry timer died.
    "timer-not-rearmed": [("""        if d["holder"]:
            # Timers died with the old process: arm the live lease's expiry again.
            self.set_timer(d["expires"] - self.now(), d["holder"])
""", "")],
    # The lock service remembers answers (dedupe) in memory only.
    "volatile-answers": [("""        d.setdefault("answers", {})   # client id -> the answer to its latest attempt
""", """        self.answers = {}
"""), ("""prev = d["answers"].get(cid)""", """prev = self.answers.get(cid)"""),
        ("""                d["answers"][cid] = answer
                self.emit("Busy", cid)""", """                self.answers[cid] = answer
                self.emit("Busy", cid)"""),
        ("""                d["answers"][cid] = answer
                self.emit("Granted\"""", """                self.answers[cid] = answer
                self.emit("Granted\"""")],
    # The client takes any reply of the right kind as the answer to its current request.
    "stale-reply": [("""self.phase == "acquire" and msg["n"] == self.attempt:""",
                     """self.phase in ("acquire", "backoff"):"""),
                    ("""self.phase == "write" and msg["seq"] == self.seq:""", """self.phase == "write":""")],
    # Storage checks the token before it recognises a repeated write.
    "fence-before-dedupe": [("""        prev = d["writes"].get(cid)
        if prev is not None and prev[0] > seq:
            return
        if prev is not None and prev[0] == seq:
            self.send(src, {"t": "write_resp", "seq": seq, "ok": prev[1]})
            return
        ok = token >= d["highest"]
""", """        prev = d["writes"].get(cid)
        if token < d["highest"]:
            if prev is None or prev[0] <= seq:
                d["writes"][cid] = [seq, False]
                self.emit("WriteRejected", {"id": cid, "token": token})
            self.send(src, {"t": "write_resp", "seq": seq, "ok": False})
            return
        if prev is not None and prev[0] > seq:
            return
        if prev is not None and prev[0] == seq:
            self.send(src, {"t": "write_resp", "seq": seq, "ok": prev[1]})
            return
        ok = True
""")],
    # Any expiry timer ends whatever lease is live.
    "stale-timer": [("""        if tag == d["holder"] and self.now() >= d["expires"]:""", """        if d["holder"]:""")],
    # The client reports its outcome as soon as it has sent the release.
    "done-before-ack": [("""                # Release even when fenced: the lease may still be ours.
                self.phase = "release"
            self._request()""", """                self.phase = "done"
                self.send(LOCK, {"t": "release", "id": self.client_id, "token": self.token})
                self.done("Wrote" if self.ok else "Fenced")
                return
            self._request()""")],
    # The lock service keeps the token counter in memory.
    "volatile-token": [("""        d.setdefault("last", 0)       # highest token granted
""", """        d["last"] = 0                 # highest token granted
""")],
}


def build():
    base = (PRIVATE / "solution" / "lease.py").read_text()
    for bug, edits in BUGS.items():
        text = base
        for old, new in edits:
            assert text.count(old) == 1, (bug, old)
            text = text.replace(old, new)
        d = HERE / bug
        d.mkdir(exist_ok=True)
        (d / "lease.py").write_text(text)


def public_run(bug):
    """(run_scenarios.py reports ok, problems observe finds in the public logs)."""
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp) / "work"
        shutil.copytree(PRIVATE / "solution", work)
        shutil.copy(HERE / bug / "lease.py", work / "lease.py")
        r = subprocess.run([sys.executable, "-B", "run_scenarios.py"], cwd=work, capture_output=True, text=True, timeout=60)
        o = subprocess.run([SNAPPEA, "observe", "model/Model.pproj", "logs", "--all-specs", "--strict", "--check-hot", "--json"],
                           cwd=work, capture_output=True, text=True, timeout=60)
        errors = [f"{e['key']}: {e['message'].split(' ', 3)[-1]}" for s in json.loads(o.stdout).get("specs", []) for e in s["errors"]]
        return r.returncode == 0, errors


def main():
    build()
    ok = True
    for bug in BUGS:
        public_ok, seen = public_run(bug)
        g = json.loads(subprocess.run([sys.executable, str(PRIVATE / "grade.py"), str(HERE / bug)],
                                      capture_output=True, text=True).stdout)
        ok = ok and public_ok and bool(g["failed"])
        print(f"{bug}: public scenarios {'ok' if public_ok else 'FAIL'}; observe on public logs: "
              f"{len(seen)} error(s); hidden failed {len(g['failed'])}/{len(g['failed']) + len(g['passed'])}")
        for e in seen[:2]:
            print(f"    public log: {e[:150]}")
        for name, msg in g["failed"].items():
            print(f"    {name}: {msg.split(' ', 3)[-1][:130]}")
    print("ok" if ok else "FAILED: a bug fails the public scenarios or passes every hidden one")
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
