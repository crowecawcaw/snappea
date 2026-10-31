// Landing page: formal models in the application development loop.
import { Island } from "../Island";
import { REPO, SiteHeader, links } from "../SiteHeader";
import { Loops } from "../docs/diagrams/Loops";
import { Schedules } from "../docs/diagrams/Schedules";
import { Workflow } from "../docs/diagrams/Workflow";
import { SpeedBars } from "./SpeedBars";

const BLOB = `${REPO}/blob/main`;
const P_DOCS = "https://p-org.github.io/P/";
const l = links("./");

const FEATURES = [
  { title: "Exercise the behavior", text: "Coverage shows which branches, handlers and scenarios your checks reached, and where the model needs a better workload.", link: { label: "Understand coverage", href: "./docs/coverage.html" } },
  { title: "Check the checks", text: "Mutation testing changes the model and asks whether your specs notice. A passing run is more useful when the checks can reject mistakes.", link: { label: "Mutation testing", href: `${BLOB}/docs/checking-models.md#mutation-testing` } },
  { title: "Catch modeling mistakes", text: "Lints catch unselected specs, assertions that are always true, and monitors that accept everything, before exploration starts.", link: { label: "Model lints", href: `${BLOB}/docs/linting.md` } },
  { title: "Use your existing tests", text: "Helpers for Python, Rust, Go and Java check real application events against the same specs that checked the design.", link: { label: "Application examples", href: `${BLOB}/examples/README.md` } },
];

export function Home() {
  return (
    <div className="site home">
      <SiteHeader root="./" current="home" />
      <main>
        <section className="home-hero">
          <p className="eyebrow">Design, model, implement, test</p>
          <h1>Formal models for developers and their agents.</h1>
          <p className="home-lede">
            Explore how your application behaves under retries, failures and unexpected event orderings.
            Check the design before you build, then use the same specs in your application tests.
          </p>
          <div className="home-ctas">
            <a className="button primary" href="#what-is-p">See how it works</a>
            <a className="button" href={l.agents}>Start with your agent</a>
            <a className="button ghost" href={REPO} target="_blank" rel="noreferrer">GitHub</a>
          </div>
        </section>

        <section className="home-p" id="what-is-p">
          <h2>A small model. An ordering you missed.</h2>
          <p>
            P is a language for writing executable models of your design: components, the messages they exchange,
            and what each does when a message arrives. You describe the choices and the rules that must hold.
            Snap Pea explores executions and reports a trace when a rule breaks.
          </p>
          <p>
            Two clients read a counter, add one and write it back. Try the sequential test, then explore
            other orderings. Both clients can read 0, so two increments leave the counter at 1.
          </p>
          <Island name="schedules"><Schedules /></Island>
          <p className="muted small">Scripted illustration, not a live checker run. The Playground runs actual P models in your browser.</p>
          <p>
            The fix is a design change: let the counter apply each increment in one operation.
            <a href={l.firstModel}> Read the P model and its failure trace</a>, or{" "}
            <a href={`${l.playground}?demo=lost-update`}>edit and check it in the Playground</a>.
          </p>
        </section>

        <section className="home-p">
          <h2>The same rules, from design to application tests</h2>
          <p>
            A spec is a state machine that observes events and checks rules about them.
            Check the model's events while designing, then check your application's events from ordinary tests.
          </p>
          <Island name="loops"><Loops /></Island>
          <Island name="workflow"><Workflow /></Island>
          <p>
            When a check fails, review the design, model and implementation together. Turn counterexamples
            into regression tests that force the troublesome ordering on real code.
          </p>
          <p className="muted small">
            Model checks cover the behavior and assumptions you modeled. Application checks validate the events
            your tests produce; they do not explore every execution of your code or establish full equivalence.
          </p>
        </section>

        <section className="home-p">
          <h2>Treat models like code you test</h2>
          <p>A model that passes without testing the important behavior gives false confidence. Snap Pea helps you find weak checks.</p>
          <div className="home-features">
            {FEATURES.map((f) => <article key={f.title} className="feature">
              <h3>{f.title}</h3><p>{f.text}</p><a href={f.link.href}>{f.link.label} →</a>
            </article>)}
          </div>
        </section>

        <section className="home-p" id="agents">
          <h2>Give your agent a model checker</h2>
          <p>
            Ask your agent to model a feature before implementing it. The built-in guide teaches the workflow,
            modeling boundaries and implementation tests. Structured CLI results and actionable diagnostics
            help it inspect failures and revise the model. You review the contract, assumptions and specs.
          </p>
          <pre className="terminal"><code>{`snappea guide                  # workflow, built into the binary
snappea guide examples         # runnable examples
snappea check                  # lint, explore, report coverage and mutations
snappea check --json           # structured results for agents`}</code></pre>
          <p className="home-p-links">
            <a href={l.agents}>Work with your agent</a>
            <a href={`${BLOB}/evals/minimal-prompt/README.md`}>How we evaluate agent workflows</a>
          </p>
        </section>

        <section className="home-band home-feedback">
          <div className="home-band-text">
            <h2>Fast feedback, on every change</h2>
            <p>
              One binary, no compile step or runtime toolchain. Automatic exploration combines exhaustive
              search for small models with guided scheduling for larger ones. Start with the defaults;
              set a time budget for CI without choosing a scheduler.
            </p>
            <p>Checking the Raft tutorial, 120 schedules:</p>
            <p className="muted small">
              Warmed up, snappea checks 290 schedules per second, and P checks 4. Results vary by model. {" "}
              <a href={`${BLOB}/docs/development.md#benchmarks`}>Benchmark setup and results</a>
            </p>
            <p><a href={l.githubActions}>Run checks in CI →</a></p>
          </div>
          <Island name="speed"><SpeedBars /></Island>
        </section>

        <section className="home-install" id="install">
          <h2>Install</h2>
          <p>Install the prebuilt checker on Linux or macOS, then read the built-in guide:</p>
          <pre className="terminal">
            <code>
              <span className="prompt">$ </span>curl -fsSL https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.sh | sh{"\n"}
              <span className="prompt">$ </span>snappea guide
            </code>
          </pre>
          <p>
            See the <a href={l.cli}>installation guide</a> for PATH setup, Windows, pinned versions,
            Cargo and PyPI options. <a href={`${REPO}/releases`}>GitHub releases</a> include binaries for Linux,
            macOS and Windows. Add a helper to check events from your application tests:
          </p>
          <pre className="terminal">
            <code>
              <span className="prompt">$ </span>pip install pysnappea{"                       "}<span className="cmt"># Python</span>{"\n"}
              <span className="prompt">$ </span>cargo add --dev snappea{"                   "}<span className="cmt"># Rust</span>{"\n"}
              <span className="prompt">$ </span>go get github.com/crowecawcaw/snappea{"     "}<span className="cmt"># Go</span>
            </code>
          </pre>
          <p>
            Java: <code>io.github.crowecawcaw:snappea</code> on Maven Central, plus a natives jar with the checker (
            <a href={`${l.java}#install`}>Maven and Gradle snippets</a>). Each language has a{" "}
            guide: <a href={l.python}>Python</a>, <a href={l.rust}>Rust</a>, <a href={l.go}>Go</a>,{" "}
            <a href={l.java}>Java</a>.
          </p>
        </section>
        <section className="home-p">
          <h2>Already using P?</h2>
          <p>Snap Pea runs P models directly and is validated against upstream tests and tutorials.
            <a href={l.comingFromP}> Coming from P</a> covers compatibility and migration.
            The <a href={P_DOCS}>P language manual</a> covers the language.</p>
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
