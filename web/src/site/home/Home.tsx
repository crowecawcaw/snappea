// Landing page: formal models in the application development loop.
import { Island } from "../Island";
import { REPO, SiteHeader, links } from "../SiteHeader";
import { Loops } from "../docs/diagrams/Loops";
import { MiniPlayground } from "./MiniPlayground";
import { SpeedBars } from "./SpeedBars";

const BLOB = `${REPO}/blob/main`;
const l = links("./");

const FEATURES = [
  { title: "Exercise the behavior", text: "Coverage shows which branches, handlers and scenarios your checks reached, and where the model needs a better workload.", link: { label: "Understand coverage", href: "./docs/coverage.html" } },
  { title: "Check the checks", text: "Mutation testing changes the model and asks whether your specs notice. A passing run is more useful when the checks can reject mistakes.", link: { label: "Mutation testing", href: `${BLOB}/docs/checking-models.md#mutation-testing` } },
  { title: "Catch modeling mistakes", text: "Lints catch unselected specs, assertions that are always true, and monitors that accept everything, before exploration starts.", link: { label: "Model lints", href: `${BLOB}/docs/linting.md` } },
  { title: "See the driver’s limits", text: "Passing checks report limits observed in the test driver: fixed client counts, input values and event orders. Extend the driver when real clients can do more.", link: { label: "Driver limits", href: `${BLOB}/docs/checking-models.md#driver-limits` } },
];

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
          <h2>Read the model. Find the race.</h2>
          <p className="section-intro">
            Two clients each add one to a shared counter. Each reads the current value, then writes back
            that value plus one. What happens if both read before either writes?
          </p>
          <p>
            This is P: state machines exchange messages with <code>send</code> and handle them with <code>on</code>.
            The test starts two clients and checks that the final counter is 2. Run the model to explore message
            orderings, then try making the increment atomic.
          </p>
          <Island name="mini-playground"><MiniPlayground /></Island>
          <p className="muted small">The real checker runs locally in your browser. No installation, and no code leaves this page.</p>
          <p><a href={l.firstModel}>Walk through the model and its counterexample →</a></p>
        </section>

        <section className="home-p">
          <h2>The same rules, from design to application tests</h2>
          <p>
            A spec is a state machine that observes events and checks rules about them.
            Check the model's events while designing, then check your application's events from ordinary tests.
          </p>
          <Island name="loops"><Loops /></Island>
          <p className="muted small">Illustration of the shared-spec workflow; the animation is not a live check.</p>
          <div className="home-artifacts">
            <article><h3>A contract to review</h3><p>Write down the real atomic operations and what must stay true.
              For the counter: every completed increment contributes exactly one to the total.</p></article>
            <article><h3>A spec to share</h3><p>Express that contract independently of the algorithm.
              Check events from both the P model and real application calls against it.</p></article>
            <article><h3>A regression test to keep</h3><p>Force both calls to read before either writes.
              Use the counterexample to test the implementation, rather than hoping a stress test hits it.</p></article>
          </div>
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

        <section className="home-p home-feedback">
          <div className="home-band-text">
            <h2>Fast feedback, on every change</h2>
            <p>
              One binary, no compile step or runtime toolchain. Automatic exploration combines exhaustive
              search, coverage-guided random search, deliberate delays and priority scheduling. It adapts the
              mix as it discovers new behavior, so you can set a CI time budget without choosing a scheduler.
            </p>
            <p><a href={l.githubActions}>Run checks in CI →</a></p>
          </div>
        </section>

        <section className="home-install" id="install">
          <h2>Install</h2>
          <p>Install the prebuilt checker on Linux or macOS, then read the built-in guide:</p>
          <pre className="terminal">
            <code>
              <span className="prompt">$ </span>curl -fsSL https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.sh | sh{"\n"}
              <span className="prompt">$ </span>export PATH="$HOME/.local/bin:$PATH"{"\n"}
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
          <h2>Coming from P?</h2>
          <p>
            Keep your P models. <code>snappea-compat</code> is a drop-in replacement for the <code>p</code> CLI
            in existing checking scripts, preserving upstream options, output and replayable schedules, with no compile step.
            Use the native <code>snappea</code> CLI for automatic scheduling and model quality checks.
            See <a href={l.comingFromP}>compatibility and migration</a> for supported behavior and differences.
          </p>
          <p>
            Run more schedules per second and direct the search toward new behavior. On our synthetic
            search benchmarks, Snap Pea's guided feedback search finds bugs in fewer schedules than the original
            feedback strategy. Native checking combines several search strategies automatically;
            results vary by model and seed. <a href={`${BLOB}/benchmarks/search/README.md`}>Search measurements and methodology</a>.
          </p>
          <div className="home-band home-comparison">
            <div className="home-band-text">
            <p>Checking the Raft tutorial, 120 schedules:</p>
            <p className="muted small">
              Warmed up, snappea checks 290 schedules per second, and P checks 4. Results vary by model. {" "}
              <a href={`${BLOB}/docs/development.md#benchmarks`}>Benchmark setup and results</a>
            </p>
            </div>
            <Island name="speed"><SpeedBars /></Island>
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
