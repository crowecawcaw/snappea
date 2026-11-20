# Design this component in P

`/work/DESIGN.md` describes a component and the contract it must keep. `/work/model`
is the start of a P model of it:

- `PSrc/`: the design. `Events.p` fixes the events, and the machine's name and what it
  takes when created are fixed; the machine is empty. Write it.
- `PSpec/`: nothing yet. No spec for the contract has been written.
- `PTst/`: a minimal environment and one smoke test, which makes a single call and
  checks nothing.

Write the design in `PSrc/` so that the contract holds under everything DESIGN.md
says can happen. Write whatever specs and test cases you need to convince yourself
that it does: the smoke test passing shows almost nothing. {tool_line}

Your `PSrc/` will be judged by running it against hidden specs and hidden test cases
that cover the contract and the conditions in DESIGN.md. They bring their own
environment machines and replace your `PSpec/` and `PTst/`. So:

- Do not change `PSrc/Events.p`, the machine's name or what it takes when created,
  and announce the events as `Events.p` says.
- Keep `PSrc/` to the design. Put specs in `PSpec/` and drivers (dependency, timer
  service, callers), helper functions and tests in `PTst/`; a declaration you add to
  `PSrc/` can clash with the hidden ones.

Also write `/work/NOTES.md`: the design decisions you made and why, and any problem a
check found and how you fixed it.
