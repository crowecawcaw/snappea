import { REPO, links } from "../SiteHeader";
import { Code } from "./Code";

export function Agents({ root }: { root: string }) {
  const l = links(root);
  const blob = `${REPO}/blob/main`;
  return <>
    <section className="hero">
      <p className="eyebrow">Work with your agent</p><h1>Model a feature with your agent</h1>
      <p className="lede">Use a model to improve the design, then carry its rules and counterexamples into implementation tests.</p>
      <p>Prerequisites: an installed <a href={l.cli}>Snap Pea CLI</a>, an application repository,
        and a feature involving shared state, concurrency, retries or recovery.
        For an introduction to P, start with <a href="./first-model.html">your first model</a>.</p>
    </section>
    <section>
      <h2>Give the agent the contract</h2>
      <p>Describe what must remain true, the storage primitives that are actually atomic, and the failures you expect.
        Ask the agent to record assumptions and model bounds alongside the design. A useful starting instruction is:</p>
      <Code lang="text">{`Use P to model this feature before implementing it. Snappea is installed.
Read snappea guide. State the contract, atomic primitives, failure assumptions
and model bounds. Check the model and investigate lints, coverage gaps and
undetected mutations. Turn counterexamples into regression tests, then check
real application events against the same specs. Report what remains unchecked.`}</Code>
      <Code lang="shell">{`snappea guide
snappea guide modeling
snappea guide testing
snappea guide examples`}</Code>
      <p>The guide ships with the binary, so the agent can read it without browsing documentation.
        For a complete model-to-application example, ask it to read
        <code>snappea guide example reservation</code>: an atomic reservation versus a stale read/write, with Python tests.</p>
    </section>
    <section>
      <h2>Review the model before implementation</h2>
      <p>Check that separate operations can interleave where the real system allows them to.
        A model that treats a read and a later write as one atomic operation assumes away the lost-update bug.
        Review specs against the contract independently of the implementation algorithm.</p>
      <Code lang="shell">{`snappea check
snappea check --json
snappea check lint
snappea check mutation`}</Code>
      <p>Use the reported replay command to inspect a failure. Fix the design when the trace exposes a real bug;
        fix the model when its assumptions are wrong. Keep the scenario that exposed it.</p>
      <p><a href={l.coverage}>Coverage</a> identifies behavior the workload did not exercise.
        <a href={`${blob}/docs/linting.md`}> Lints</a> catch common weak models.
        <a href={`${blob}/docs/checking-models.md#mutation-testing`}> Mutation testing</a> asks whether checks reject
        changed behavior. Investigate findings rather than removing the checks to obtain a pass.</p>
    </section>
    <section>
      <h2>Connect the implementation to the spec</h2>
      <p>Ask the agent to implement the reviewed design and record actual operation arguments and results.
        Use the helpers for <a href={l.python}>Python</a>, <a href={l.rust}>Rust</a>,
        <a href={l.go}> Go</a> or <a href={l.java}>Java</a> to check those events against the model's specs.</p>
      <p>Force the important interleavings in implementation tests: for example, ensure two calls read the same
        value before either writes. Run the real application calls; do not replace them with invented event lists.
        Keep each model counterexample as a regression test where practical.</p>
      <p>The <a href={`${blob}/examples/model/README.md`}>inventory model</a> and
        <a href={`${blob}/examples/README.md`}> application examples</a> share a stock-reservation spec across languages.</p>
    </section>
    <section>
      <h2>Keep the loop running in CI</h2>
      <p>Add <code>snappea check</code> and your ordinary application tests to CI.
        Start with automatic scheduling, then choose a time budget appropriate to the model.
        See <a href={l.githubActions}>GitHub Actions</a> and the <a href={l.cli}>CLI reference</a>.</p>
      <p>Review search completion, model bounds, quality findings and instrumentation alongside a passing status.
        The model and observed events can both omit requirements. Update the design, model and tests together as the feature changes.</p>
      <p>For how we measure agent outcomes, see the
        <a href={`${blob}/evals/minimal-prompt/README.md`}> minimal-prompt eval</a>.</p>
    </section>
  </>;
}
