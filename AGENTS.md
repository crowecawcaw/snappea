# AGENTS.md

```sh
cargo build --workspace
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets
cargo build -p snappea --no-default-features --features emit   # production use: no checker
cargo +nightly fuzz run check fuzz/corpus/check -- -dict=fuzz/p.dict   # see docs/fuzzing.md
```

## Goal

Make P easier for developers to incorporate into application design and development.

## Tenets

1. **The primary audience is agents driven by developers.** Help agents make better
   designs and find bugs by writing quality models and getting good feedback from them.
   Documentation, examples, tools and interfaces aim at implementors, not theorists.
2. **`snappea-compat` is a drop-in for upstream P, and thin.** The `snappea-compat` CLI should be
   as compatible with upstream P as possible, while keeping its own code minimal so the
   main functionality lives in the core library. Outside `snappea-compat`, the library is
   shaped by the other tenets, not by upstream P.
3. **Fail early, fast and loudly.** In most cases, fail on likely mistakes so they are
   quick to debug, with explicit suppression for intentional cases. Runtime
   instrumentation is the exception: it stays best-effort and never breaks application code.
4. **Errors are clear and suggest a next step or fix.** See [Error messages](#error-messages).
5. **The CLI and library target agents; the playground targets humans.** Choose defaults,
   options and output for that audience. When they differ, keep one mechanism in the core
   and let each interface set its own default.
6. **Code is idiomatic for its language.**
7. **One source of truth.** Do not duplicate code paths, constants or behaviours. In order
   of preference: a single source of truth; a structural check that turns drift into a
   mismatch; a test that pins equivalence; a comment reference (a weak check, last resort).
8. **Prevent repeats with hard checks.** When you find a bug or bad pattern, prefer a
   build, test or CI check that stops it recurring. Fix bugs red/green: failing test first.
9. **Nothing is stable yet.** Interfaces and code may change freely. Write no
   backwards-compatibility code, do not defer to existing code, and refactor whenever
   there is something to gain, even something small.
10. **Comments are terse and say why.** Prefer self-describing code. A needed comment
    explains why, not what, and describes the current code, never what came before;
    history belongs in commit messages.

For compatibility details, see [Compatibility](docs/compatibility.md).

## Writing

CLI help and doc strings say what a thing does, when to use it, and give an example.
Plain and short.

In documentation and website copy:

* Avoid first person (`we`, `our`). Name the project or the thing that acts.
* Avoid catchy taglines. Say what the thing does in direct language:
  "`snappea check --json` prints structured results", not "Results an agent can act on";
  "One command runs lint, search, coverage and mutation testing", not "One command, no tuning".

## Documentation

Use Diátaxis. Give each page one primary type: `tutorial`, `how-to`,
`explanation`, or `reference`. Tutorials teach through a guided experience;
how-to guides solve a task; explanations develop understanding; reference
states exact behavior. Split substantial material serving a different purpose.

Start Markdown documentation pages with non-rendered YAML front matter:

```yaml
---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - How do I run model checks in CI?
  - How do I reproduce a failing run locally?
---
```

For website docs, put equivalent `doc:type`, `doc:personas` and `doc:questions`
meta tags in the HTML entry's head; encode personas and questions as JSON arrays.
These headers are authoring context and machine-readable metadata, not visible
page labels. Use the declared questions to define scope and review completeness.
Choose specific personas such as application-developer, coding-agent, p-user,
or contributor. Preserve language-helper documentation requirements in
[language support](docs/language-support.md).

Lead with the application development task. Connect design, model checks,
implementation events and tests through shared specs. Distinguish model checking
from checking observed application executions, and state assumptions and bounds.
Keep exact command behavior and defaults in canonical reference pages. Reuse the
built-in guide and runnable examples rather than inventing a separate agent workflow.

## Error messages

Errors should be clear and, when appropriate, point towards the fix.
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
