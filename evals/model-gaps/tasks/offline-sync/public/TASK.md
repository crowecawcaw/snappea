# Design this component in P

`/work/DESIGN.md` describes a component and the contract it must keep. `/work/model`
is the start of a P model of it:

- `PSrc/`: the design. The events, the types and the machines' names are fixed; the
  machines are empty. Write them.
- `PSpec/`: specs for the contract. They observe events your design must announce.
- `PTst/`: an environment and first test cases.

Write the design in `PSrc/` so that the contract holds under everything DESIGN.md
says can happen. {tool_line}

The given specs may not cover the whole contract in DESIGN.md; the hidden specs cover
all of it, so you may add specs and tests, but you must keep the declared events.

Your `PSrc/` will be judged by running it, with hidden specs, against hidden test
cases that cover the conditions in DESIGN.md. Do not change event declarations, type
declarations, machine names or what a machine takes when created. Add whatever you
need to `PSpec/` and `PTst/`.

Also write `/work/NOTES.md`: the design decisions you made and why, and any problem a
check found and how you fixed it.
