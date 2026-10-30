# AGENTS.md

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets
cargo build -p snappea --no-default-features --features emit   # production use: no checker
```

## Goal

Make P easier for developers to incorporate into application design and development.

## Tenets

1. **The audience is implementors, not theorists.** Documentation, examples, tools and
   interfaces should focus on clarity, ergonomics and practicality.
2. **`snappea-compat` is a drop-in for upstream P, and thin.** The `snappea-compat` CLI should be
   as compatible with upstream P as possible, while keeping its own code minimal so the
   main functionality lives in the core library. Outside `snappea-compat`, the library is
   shaped by the other tenets, not by upstream P.
3. **Agentic development is the primary use case.** P is especially well suited to it.
   Help developers understand what P is and how to use it strategically, and help agents
   incorporate P effectively into development for design and implementation validation.
4. **Fail on likely modeling mistakes, with explicit suppression for intentional cases.**
   Models and validation fail by default to avoid undetected issues; runtime instrumentation
   must remain best-effort and must not introduce errors that break application code.
5. **The CLI and library target agents; the playground targets humans.** Choose defaults,
   options and output for that audience. When they differ, keep one mechanism in the core
   and let each interface set its own default.

For compatibility details, see [Compatibility](docs/compatibility.md).

## Writing

CLI help and doc strings say what a thing does, when to use it, and give an example.
Plain and short.

## Error messages

Every error says what went wrong, where, and what to do next.

* **What**: one plain sentence. Name the thing (`could not find event 'eB'`), not the
  internals.
* **Where**: `file:line:col`, and for source errors the lines around it with the span
  underlined. Use `sema::diag::render_snippet`.
* **Next step**: a concrete fix or command, as a `help:`/`hint:` line. Give the likely
  fix first (`did you mean 'eA'?`), then the general one.
* **Compat**: `snappea-compat` prints upstream's exact text. Put hints in
  `Diagnostic::help` or `report_and_exit_hint`, which only `snappea` shows; never edit
  an upstream message.

```text
error: could not find name 'eB'
  --> a.p:7:18
  |
5 |     entry {
6 |       x = 1;
7 |       send this, eB;
  |                  ^^
8 |     }
  |
  = help: did you mean 'eA'? Otherwise declare the name before using it
```
