// Docs overview: what P is, for developers who know unit tests.
import { Island } from "../Island";
import { REPO, links } from "../SiteHeader";
import { Code } from "./Code";
import { Loops } from "./diagrams/Loops";
import { Schedules } from "./diagrams/Schedules";
import { Workflow } from "./diagrams/Workflow";

const BLOB = `${REPO}/blob/main`;

export function Overview({ root }: { root: string }) {
  const l = links(root);
  const DEMO = `${l.playground}?demo=lost-update`;
  return (
    <>
      <section className="hero">
        <p className="eyebrow">Docs</p>
        <h1>Overview</h1>
        <p className="subtitle">What P is, and how it differs from the tests you already write.</p>
        <p className="lede">
          Two clients each read a shared counter, add one and write it back. Your unit tests pass because they
          run the calls in one fixed order, but in production both clients can read 0, and one update is lost. P
          finds that ordering in milliseconds, from a short model of the design, before any service code exists.
        </p>
      </section>

      <section>
        <h2>A model, a spec and your code</h2>
        <p>
          P is a language for writing small, runnable descriptions of a system's design. You use it to check two
          kinds of thing:
        </p>
        <Island name="loops"><Loops /></Island>
        <dl className="faq">
          <div>
            <dt>Is P my code?</dt>
            <dd>
              No. A <b>model</b> describes your design: the components, the messages they send, and what each does
              when a message arrives. It's usually a few hundred lines, and it runs only inside the checker.
            </dd>
          </div>
          <div>
            <dt>Does it check my code?</dt>
            <dd>
              The checker runs the model, not your code. A <b>spec</b> is a rule written in P, like "stock never goes
              negative". The same spec checks the model at design time and, through events your tests record, your
              code at test time.
            </dd>
          </div>
          <div>
            <dt>How does a model help?</dt>
            <dd>
              It finds bugs that depend on timing: races, reordered or lost messages, retries and crashes. Unit tests
              rarely hit these bugs, and they cost the most to fix once the code is written.
            </dd>
          </div>
        </dl>
      </section>

      <section>
        <h2>How it differs from a unit test</h2>
        <p>
          In a unit test, you pick one scenario and its order of events. In a P test, you describe the setup, and
          the checker picks the order: which message arrives first and which component runs next. It runs thousands
          of orderings and stops at the first one that breaks a rule.
        </p>
        <Island name="schedules"><Schedules /></Island>
        <table className="compare">
          <thead>
            <tr>
              <th />
              <th>Unit test</th>
              <th>P model check</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <th>Runs</th>
              <td>your code</td>
              <td>a model of your design</td>
            </tr>
            <tr>
              <th>Who picks the order</th>
              <td>you, once</td>
              <td>the checker, thousands of times</td>
            </tr>
            <tr>
              <th>Finds</th>
              <td>logic bugs on the path you wrote</td>
              <td>bugs that need an unlucky ordering</td>
            </tr>
            <tr>
              <th>When</th>
              <td>after the code exists</td>
              <td>before it, and alongside it</td>
            </tr>
          </tbody>
        </table>
      </section>

      <section id="worked-example">
        <h2>Follow a counterexample</h2>
        <p><a href="./first-model.html">Your first model</a> walks through the lost-update P code,
          its failure trace, and a design change that fixes it in the Playground.</p>
      </section>

      <section>
        <h2>The same rules, checking your code</h2>
        <p>
          A spec <b>observes</b> events and asserts rules about them. This one checks an inventory service. Specs
          check models in the same way.
        </p>
        <Code lang="p" title="inventory.p (excerpt)">{`
spec InventoryCorrect observes eStocked, eReserved, eReleased {
var available: int;
state Ready {
  on eReserved do (p: (order: string, units: int)) {
    available = available - p.units;
    assert available >= 0, "reservation exceeds available stock";
  }
  // ...
}
}
`}</Code>
        <p>Your code emits the events after each change succeeds, and your ordinary tests check them:</p>
        <Code lang="python" title="Python">{`
def reserve(self, sku, order, units):
    ...
    emit("eReserved", key=sku, order=order, units=units)

def test_release_makes_stock_available_again(inventory_factory):
    with check_spec("InventoryCorrect"):
        inventory = inventory_factory()
        inventory.reserve("coffee", "order-1", 3)
        inventory.release("coffee", "order-1")
`}</Code>
        <p>
          There are helpers for <a href={l.python}>Python</a>, <a href={l.rust}>Rust</a>,{" "}
          <a href={l.java}>Java</a> and <a href={l.go}>Go</a>. To check events from running services, see{" "}
          <a href={`${BLOB}/examples/multiprocess/docker/README.md`}>live observation across processes</a> and{" "}
          <a href={`${BLOB}/docs/observe.md`}>snappea observe</a>. The same emit calls can write production logs
          for <code>snappea observe</code>: see <a href={`${BLOB}/docs/production.md`}>testing and production</a>.
        </p>
      </section>

      <section>
        <h2>Where P fits</h2>
        <Island name="workflow"><Workflow /></Island>
        <p>
          AI agents are well suited to writing models: they can turn a design doc into machines and specs quickly,
          and rerun the checker after every change. Review the model boundaries and assumptions as well as the specs. The specs are the short, plain
          statement of what must always be true, and the part of the model worth reviewing line by line.
        </p>
      </section>

      <section className="next">
        <h2>What a passing check establishes</h2>
        <p>Checks explore the model you wrote, including its input bounds, atomic operations and failure assumptions.
          Complete exploration covers that bounded model. A timed-out search may miss a bug. Coverage and mutation
          testing help find weak checks, but cannot establish that every requirement is represented.</p>
        <p>Application tests check the recorded events against the same specs; they do not automatically compare
          all implementation behavior with the model. Instrumentation and forced-interleaving tests need review too.</p>
        <h2>Next</h2>
        <ul>
          <li><a href="./agents.html">Work with your agent</a>: model a feature and test its implementation.</li>
          <li>
            <a href={DEMO}>Run the lost-update model</a> in the Playground, then try the bigger tutorials in its menu.
          </li>
          <li>
            <a href={`${BLOB}/examples/README.md`}>Integration examples</a>: specs checking real code in four
            languages.
          </li>
          <li>
            <a href="https://p-org.github.io/P/" target="_blank" rel="noreferrer">The P language manual</a>, from the
            upstream project.
          </li>
        </ul>
      </section>
    </>
  );
}
