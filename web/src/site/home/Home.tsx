// Landing page: formal models in the application development loop.
import { Island } from "../Island";
import { REPO, SiteHeader, links } from "../SiteHeader";
import { Loops } from "../docs/diagrams/Loops";
import { MiniPlayground } from "./MiniPlayground";
import { SpeedBars } from "./SpeedBars";

const BLOB = `${REPO}/blob/main`;
const l = links("./");

const CHECKS = [
  { title: "Coverage", text: "Shows which branches, handlers and scenarios the checks reached, and which they never ran.", link: { label: "Understand coverage", href: "./docs/coverage.html" } },
  { title: "Mutation testing", text: "Changes the model in small ways and reports each change your specs did not notice.", link: { label: "Mutation testing", href: `${BLOB}/docs/checking-models.md#mutation-testing` } },
  { title: "Lints", text: "Flag specs no test selects, assertions that are always true, and specs that accept everything.", link: { label: "Model lints", href: `${BLOB}/docs/linting.md` } },
  { title: "Driver limits", text: "List what the test never tried: fixed client counts, input values and event orders.", link: { label: "Driver limits", href: `${BLOB}/docs/checking-models.md#driver-limits` } },
];

const AGENT_ROWS = [
  { title: "The guide is in the binary", text: <><code>snappea guide</code> teaches the workflow: what to model, where to stop, and how to test the implementation against the specs. There is no skill file to write or keep in sync.</> },
  { title: "One command, no tuning", text: <><code>snappea check</code> lints the model, tries message orderings, and reports coverage and mutation results. It picks the search strategies; you set a time budget. One binary, no compile step.</> },
  { title: "Results an agent can act on", text: <><code>snappea check --json</code> returns structured results. Every error says what went wrong, where, and what to try next.</> },
  { title: "Weak models fail the check", text: <>Lint findings, coverage gaps and undetected mutants fail the check, so an agent cannot stop at a model that passes without testing anything.</> },
];

// Real output: the counter model with `eWrite` misspelled.
const ERROR_SAMPLE = `error: could not find name 'eWrit'
   --> Model.p:16:27
   |
15 |       receive { case eValue: (x: int) { v = x; } }
16 |       send input.counter, eWrit, v + 1;
   |                           ^^^^^
   |
   = help: did you mean 'eWrite'? Otherwise declare the name before using it`;

export function Home() {
  return (
    <div className="site home">
      <SiteHeader root="./" current="home" />
      <main>
        <section className="home-hero">
          <h1>Formal models for developers and their agents.</h1>
          <p className="home-lede">
            Explore how your application behaves under retries, failures and unexpected event orderings.
            Check the design before you build, then use the same specs in your application tests.
          </p>
          <div className="home-ctas">
            <a className="button primary" href={l.playground}>Try the playground</a>
            <a className="button" href={l.overview}>Read the docs</a>
            <a className="button ghost" href={REPO} target="_blank" rel="noreferrer">GitHub</a>
          </div>
        </section>

        <section className="home-p" id="what-is-p">
          <h2>What a model looks like</h2>
          <p className="section-intro">
            A model is a short program that describes a design. This one is written in P: state machines
            that <code>send</code> each other messages. The checker runs it under many message orderings
            and reports any ordering that breaks a rule.
          </p>
          <p>Here, two clients each add one to a shared counter.</p>
          <Island name="mini-playground"><MiniPlayground /></Island>
          <p><a href={l.firstModel}>Walk through this model step by step →</a></p>
        </section>

        <section className="home-p">
          <h2>One spec checks the design and the running code</h2>
          <p className="section-intro">
            A spec is a state machine that watches events and checks rules about them. Check the model's
            events while you design. Then check your application's events from ordinary tests, against the same spec.
          </p>
          <Island name="loops"><Loops /></Island>
          <p>
            When the checker finds a failing ordering in the model, turn it into a regression test that
            forces the same ordering on the real code.
          </p>
          <p className="muted small">
            The diagram is an illustration, not a live check. Model checks cover the behavior and assumptions you
            modeled. Application checks cover the events your tests produce; they do not explore every execution
            of your code.
          </p>
        </section>

        <section className="home-p">
          <h2>Models are tested like code</h2>
          <p className="section-intro">
            A model can pass because it is right, or because its checks never look at the part that is wrong.
            Four checks tell the two apart.
          </p>
          <div className="home-features">
            {CHECKS.map((f) => <article key={f.title} className="feature">
              <h3>{f.title}</h3><p>{f.text}</p><a href={f.link.href}>{f.link.label} →</a>
            </article>)}
          </div>
        </section>

        <section className="home-p home-agents" id="agents">
          <h2>Built for agents</h2>
          <p className="section-intro">
            Most model checkers were designed for a person at a terminal. Snap Pea's commands, guide, error
            messages and checks were designed for an agent to run, and we measure changes to them with agent evals.
          </p>
          <div className="home-agents-grid">
            <div className="home-stat">
              <p className="home-stat-n">56%</p>
              <p className="home-stat-label">fewer missed design mistakes</p>
              <p>
                In our pilot evals, agents modeled a design, and we ran hidden broken versions of that design
                against the specs and tests they wrote. Agents using <code>snappea</code> missed 22 of 180.
                Agents using the same checker through the upstream <code>p</code> command line missed 50.
              </p>
              <p className="muted small">
                Haiku agents, three to six runs per task. <a href={`${BLOB}/docs/evals.md#model-gaps`}>Eval setup and results</a>
              </p>
            </div>
            <div className="home-agent-rows">
              {AGENT_ROWS.map((r) => <article key={r.title}><h3>{r.title}</h3><p>{r.text}</p></article>)}
            </div>
          </div>
          <p className="muted small home-sample-label">What the checker prints when the counter model misspells an event:</p>
          <pre className="terminal"><code>{ERROR_SAMPLE}</code></pre>
          <p className="home-p-links">
            <a href={l.agents}>Work with your agent</a>
            <a href={l.githubActions}>Run checks in CI</a>
          </p>
        </section>

        <section className="home-p" id="compare">
          <h2>How it compares</h2>
          <h3>Upstream P</h3>
          <p>
            Snap Pea runs the P models you already have. <code>snappea-compat</code> is a drop-in replacement for
            the <code>p</code> CLI in existing scripts: the same options, output and replayable schedules, with no
            compile step. The native <code>snappea</code> CLI adds automatic search and the model checks above.
            See <a href={l.comingFromP}>compatibility and migration</a> for what is supported and what differs.
          </p>
          <div className="home-band home-comparison">
            <div className="home-band-text">
              <p>Checking the Raft tutorial, 120 schedules:</p>
              <p className="muted small">
                Warmed up, snappea checks 290 schedules per second, and P checks 4. Results vary by model.{" "}
                <a href={`${BLOB}/docs/development.md#benchmarks`}>Benchmark setup and results</a>
              </p>
            </div>
            <Island name="speed"><SpeedBars /></Island>
          </div>
          <p className="muted small">
            On our synthetic search benchmarks, the guided search also finds bugs in fewer schedules than upstream's
            feedback strategy. Results vary by model and seed.{" "}
            <a href={`${BLOB}/benchmarks/search/README.md`}>Search measurements and methodology</a>
          </p>
          <h3>TLA+, Alloy and other model checkers</h3>
          <p>
            TLA+ and Alloy describe a system as logic formulas over its state. A P model is state machines that
            send each other messages, so it reads like the services it describes. Snap Pea checks a model by running
            it: a small model is searched completely, a larger one is sampled within a time budget, and the report
            says which happened. It does not write proofs about systems of unbounded size.
          </p>
        </section>

        <section className="home-p home-install" id="install">
          <h2>Install</h2>
          <p>On Linux or macOS, install the checker and read the built-in guide:</p>
          <pre className="terminal">
            <code>
              <span className="prompt">$ </span>curl -fsSL https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.sh | sh{"\n"}
              <span className="prompt">$ </span>export PATH="$HOME/.local/bin:$PATH"{"\n"}
              <span className="prompt">$ </span>snappea guide
            </code>
          </pre>
          <p>
            The <a href={l.cli}>installation guide</a> covers Windows, pinned versions, Cargo and PyPI.
            To check events from application tests, add the helper for{" "}
            <a href={l.python}>Python</a>, <a href={l.rust}>Rust</a>, <a href={l.go}>Go</a> or{" "}
            <a href={l.java}>Java</a>.
          </p>
          <div className="home-ctas">
            <a className="button primary" href={l.playground}>Try the playground</a>
            <a className="button" href={l.overview}>Read the docs</a>
          </div>
        </section>
      </main>
      <footer className="site-footer">
        <span>MIT or Apache-2.0</span>
        <a href={REPO} target="_blank" rel="noreferrer">GitHub</a>
        <a href="https://github.com/p-org/P" target="_blank" rel="noreferrer">Upstream P</a>
      </footer>
    </div>
  );
}
