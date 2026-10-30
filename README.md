# 🫛 Snap Pea

**Fast formal models that also check your code.**

Find bugs in how your system behaves before they reach production. Snap Pea uses the
[P modeling language](https://p-org.github.io/P/) to explore different event orderings,
failures and interactions—and report a reproducible trace when something breaks.

Use the same rules to check your design, validate events from your application tests,
and check production logs. Helpers for Python, Rust, Go and Java bring those checks
into your existing test suite.

**[Try the playground](https://snappea.dev/playground.html) ·
[Read the overview](https://snappea.dev/docs/overview.html) ·
[Documentation](https://snappea.dev/docs/cli.html)**

![At design time, snappea check explores many orderings of a P model. At test time, your tests run your application code. Events from both are checked against the same P spec.](docs/images/overview.svg)

## See it catch a bug

Two orders each want 2 units of stock. There are 3 available. Both read the stock
before either reserves it, so both think there is enough. Together, they oversell.

A P model describes the service and the orders as state machines exchanging messages.
A spec states the rule: **stock must never go negative**. Snap Pea explores message
orderings until it finds the one that breaks that rule, then saves a trace you can
replay. Changing the design to check and reserve in one operation fixes the bug.

The [inventory example](examples/model/README.md) includes both designs. Its spec is
also used by the application examples in all four languages.

## From design to production

- **Explore the design.** Model components, messages, retries and failures in P.
  The checker tries thousands of event orderings; exhaustive search can cover every
  choice in a small model. Parallel workers and direct interpretation keep iteration
  fast, with no compilation step or .NET runtime.
- **Check your implementation.** Your code emits events as operations succeed.
  Ordinary unit, integration and property tests check those events against the same
  specs. These checks validate the orderings your tests produce; they do not explore
  alternative executions of your application.
- **Check production behavior.** The same emit calls can write logs that
  `snappea observe` checks against your specs. See [testing and production](docs/production.md).

You or an AI agent can write the model from a design document, rerun the checker after
changes, and carry the specs into implementation tests. Review the specs closely:
they state what the system must get right. `snappea guide` walks through that workflow,
with runnable examples (`snappea guide examples`); telling an agent "use P, snappea is
installed" is enough for it to find.

## Get started

Install the prebuilt CLI on Linux or macOS; no Rust or Python required:

```sh
curl -fsSL https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.sh | sh
export PATH="$HOME/.local/bin:$PATH"
snappea version
```

On Windows, in PowerShell:

```powershell
irm https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.ps1 | iex
```

The installers support Linux x86-64/ARM64 with glibc, macOS Intel/Apple silicon and
Windows x86-64. See [installation options](https://snappea.dev/docs/cli.html#install)
for release archives, Cargo, version pinning and install locations.

With Git installed, try both inventory designs:

```sh
git clone https://github.com/crowecawcaw/snappea.git
cd snappea/examples/model
snappea tests
snappea check tcAtomicReserve
snappea check tcCheckThenReserve
```

The atomic design passes exhaustive checking. The model scopes coverage exceptions to
check-then-reserve paths covered by the other test. The check-then-reserve design fails
with exit status 1 because it oversells. The failure includes:

```text
Assertion Failed: PSpec/inventory.p:27:7 reservation exceeds available stock
Replay: snappea replay PCheckerOutput/tcCheckThenReserve/bug.schedule
```

Run the printed replay command from the same directory to reproduce the bug. The
[command line guide](https://snappea.dev/docs/cli.html) and
[model-checking reference](docs/checking-models.md) cover options, output and search strategies.
`snappea check` also runs [model lints](docs/linting.md); `snappea check lint` runs them separately, and `snappea check mutation` runs only
[mutation testing](docs/checking-models.md#mutation-testing).

## Use it from your tests

Each language helper records application events and checks them against P specs.
Follow your language's guide for a complete example and test-framework setup.

| Language | Install | Guide and examples |
| --- | --- | --- |
| Python | `pip install pysnappea` | [pytest guide](https://snappea.dev/docs/python.html) · [Examples](examples/python/README.md) |
| Rust | `cargo add snappea` | [Rust guide](https://snappea.dev/docs/rust.html) · [Examples](examples/rust/README.md) |
| Go | `go get github.com/crowecawcaw/snappea` | [Go guide](https://snappea.dev/docs/go.html) · [Examples](examples/go/README.md) |
| Java | Maven `io.github.crowecawcaw:snappea` | [Java setup, including native jars](https://snappea.dev/docs/java.html#install) · [Examples](examples/java/README.md) |

## Already using P?

Snap Pea is a Rust implementation of the P toolchain that reads `.p` and `.pproj`
files directly. `snappea` provides concise output and parallel checking;
`snappea-compat` accepts upstream `p` options and reproduces its output formats.

In the Raft tutorial benchmark, checking 120 schedules takes 0.4 seconds with Snap Pea
and 38 seconds with P 3.1.0, including compilation. See the
[benchmark methodology](docs/development.md#benchmarks) and
[comparison charts](https://snappea.dev/bench/) for context.

See [coming from P](https://snappea.dev/docs/coming-from-p.html),
[measured upstream parity](https://snappea.dev/docs/parity.html) and the
[full compatibility reference](docs/compatibility.md). PVerifier and foreign code
written in C# or Java are not supported.

## Status and documentation

Early, but checks real models. Validation covers upstream's 424 compiler regression
tests, 249 checker tests, tutorials, golden traces and PObserve outputs.
[Validation details](docs/development.md#validation) explain what is checked.

- [Overview](https://snappea.dev/docs/overview.html): models, specs and how they fit alongside unit tests.
- [Examples](examples/README.md): the same inventory rules checking real code in four languages.
- [GitHub Actions](https://snappea.dev/docs/github-actions.html): check models in CI.
- [Observe reference](docs/observe.md): check events and logs, including live input.
- [Rust library](docs/library.md): embed the checker in your own tools.
- [Benchmarks](https://snappea.dev/bench/): speed and search effectiveness on every main commit.
- [P language manual](https://p-org.github.io/P/): upstream language documentation and tutorials.

## Contributing

See [development](docs/development.md) for the crate layout, tests and benchmarks,
and [AGENTS.md](AGENTS.md) for the project's goals and required checks.
[Report a bug](https://github.com/crowecawcaw/snappea/issues) if something breaks.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT), at your option.

Parts of snappea, including the tests vendored in `tests/upstream`, are derived from
[P](https://github.com/p-org/P) and remain under
[P's MIT license](tests/upstream/LICENSE.txt) (Copyright (c) 2015 P Developers).

Unless you explicitly state otherwise, any contribution you submit for inclusion in
this project, as defined in the Apache-2.0 license, is dual licensed as above, without
any additional terms or conditions.
