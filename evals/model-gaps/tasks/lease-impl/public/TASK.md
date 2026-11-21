# Implement the lease lock from its model

`/work/DESIGN.md` describes a lease lock with fencing tokens, the contract it must
keep, and the environment the implementation runs in. `/work/model` is a P model of
the design; every test case in it passes, so treat it as the design to implement:

- `model/PSrc/`: the lock service, storage and client.
- `model/PSpec/Contract.p`: the contract, as specs over the events the services announce.
- `model/PTst/`: the model's drivers and test cases.

Implement it in Python: complete `LockService`, `Storage` and `Client` in
`/work/lease.py`. They run under the simulator in `/work/sim.py`, which gives each
node a clock, timers, a network, a disk and `emit`. Your implementation must emit the
contract's events with `self.emit(event, payload)` at the moments the model announces
them (clients report their outcome with `self.done(outcome)`).

`python3 run_scenarios.py` runs three easy scenarios and writes each one's event log
to `logs/<name>.jsonl`. They pass when every client ends with the expected outcome,
which says little about the contract; add scenarios of your own.

{tool_line}

Your `lease.py` will be judged by hidden scenarios that cover the conditions in
DESIGN.md. Each one runs your classes under the same simulator (the original
`sim.py`, so do not change it or rely on changes to it) and checks the event log
against the specs in `model/PSpec/Contract.p`, including that every client and every
lease ends. A scenario passes only if its log satisfies the contract.

Rules: Python 3.12 standard library only. Put your code in `lease.py` (other `.py`
files next to it are allowed). Do not use wall-clock time, threads or randomness;
use the node's `now`, `set_timer`, `send`, `disk` and `emit`. Nodes talk to each
other only through `send`.

Also write `/work/NOTES.md`: the decisions you made, any problem you found in your
implementation, how you found it, and the fix.
