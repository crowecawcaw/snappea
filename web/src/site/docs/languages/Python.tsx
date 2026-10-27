// Docs: the Python helper (pysnappea). Outline: docs/language-support.md#docs-page.
import { REPO, links } from "../../SiteHeader";
import { Code } from "../Code";
import { ApiReference, BugIntro, EnvTable, ErrorKinds, InventorySpec, SettingsTable } from "./shared";

export function Python({ root }: { root: string }) {
  const l = links(root);
  return (
    <>
      <section className="hero">
        <p className="eyebrow">Languages</p>
        <h1>Python</h1>
        <p className="subtitle">Check P specs against the events your code emits during its pytest tests.</p>
        <p>
          Application code calls <code>emit("eReserved", ...)</code> where a change becomes visible. After each test,
          the package checks that test's events against your P specs and fails the test if one was broken. You need
          only the specs (event declarations and <code>spec</code> machines), not a full P model.
        </p>
        <p>
          It sees only the orderings your tests produce: it checks the implementation against the design, it does not
          explore other interleavings. For how this fits with model checking, see the <a href={l.overview}>overview</a>.
        </p>
      </section>

      <section>
        <h2 id="install">Install</h2>
        <Code lang="shell">{`
pip install pysnappea
`}</Code>
        <p>
          The package is imported as <code>snappea</code>. Its platform wheels bring the <code>snappea</code> checker,
          so no Rust or Java is needed. Python 3.9 or later. Set <code>SNAPPEA_BIN</code> to use another binary;{" "}
          <code>pip install ./python</code> in a checkout builds one with cargo.
        </p>
        <p>
          Production code depends on the same package and imports only <code>emit</code>, which has no dependencies
          and does nothing unless a test is recording or a production sink is set.
        </p>
      </section>

      <section>
        <h2 id="quick-start">Quick start</h2>
        <p>
          The examples on every language page check one spec: an inventory where reservations never exceed the stock
          and releases match reservations.
        </p>
        <InventorySpec />
        <p>The service emits an event after each change:</p>
        <Code lang="python" title="inventory/__init__.py">{`
from snappea import emit


class Inventory:
    def __init__(self):
        self.stock = {}
        self.reservations = {}

    def add_product(self, sku, units):
        self.stock[sku] = units
        emit("eStocked", key=sku, units=units)

    def reserve(self, sku, order, units):
        if units > self.stock[sku]:
            raise ValueError("insufficient stock")
        self.stock[sku] -= units
        self.reservations[sku, order] = units
        emit("eReserved", key=sku, order=order, units=units)  # after the change
`}</Code>
        <p>
          Point the pytest plugin at the specs. From then on every test also checks every spec:
        </p>
        <Code lang="toml" title="pyproject.toml">{`
[tool.snappea]
spec_files = ["specs/*.p"]
`}</Code>
        <Code lang="python" title="test_inventory.py">{`
import contextlib

from inventory import Inventory


def test_two_orders_for_the_last_units():
    inventory = Inventory()
    inventory.add_product("coffee", 3)
    inventory.reserve("coffee", "order-1", 2)
    with contextlib.suppress(ValueError):  # rejecting order-2 is fine too
        inventory.reserve("coffee", "order-2", 2)
`}</Code>
        <p>
          <code>pytest</code> passes. <BugIntro>delete the stock check from <code>reserve</code>.</BugIntro>
        </p>
        <Code lang="text">{`
FAILED test_inventory.py::test_two_orders_for_the_last_units

E   snappea.SpecViolation: P spec InventoryCorrect was broken: reservation exceeds available stock

      Events (key "coffee"):
          1  eStocked   units=3                inventory/__init__.py:11
          2  eReserved  order=order-1 units=2  inventory/__init__.py:16
          3  eReserved  order=order-2 units=2  inventory/__init__.py:16  <- broke the spec here

      Spec: specs/inventory.p:27
      snappea output: /tmp/pytest-of-me/pytest-1/snappea/snappea-check-zxk7z0st
`}</Code>
      </section>

      <section>
        <h2 id="test-setup">Test setup</h2>
        <h3>Every test: the pytest plugin</h3>
        <p>
          Installing the package adds the plugin. It does nothing until a <code>[tool.snappea]</code> table (or{" "}
          <code>SNAPPEA_SPEC_FILES</code>) exists:
        </p>
        <Code lang="toml" title="pyproject.toml">{`
[tool.snappea]
spec_files = ["specs/*.p"]       # globs, relative to this file; folders mean every .p in them
exclude = ["DraftSpec"]          # optional: specs not checked anywhere yet
mode = "auto"                    # optional: "auto" (every test) | "marked" | "off"
`}</Code>
        <ul>
          <li>Events emitted while fixtures are set up count. The check runs when the test body ends, so a broken spec is a test failure, not a teardown error.</li>
          <li>A test that already failed is not checked. A test that emits nothing passes without running the checker.</li>
          <li>
            <code>pytest --snappea off</code> (or <code>marked</code>, <code>auto</code>) overrides the mode for one run.
          </li>
        </ul>
        <p>Per-test options are marks:</p>
        <Code lang="python">{`
@pytest.mark.snappea(skip=True)              # don't check this test
@pytest.mark.snappea(skip=["DraftSpec"])     # leave out some specs here
@pytest.mark.snappea(check_hot=True)         # also fail on a spec left waiting
`}</Code>
        <p>
          The <code>snappea_events</code> fixture is the test's recorder, for ordinary assertions:{" "}
          <code>snappea_events.count("eEnter")</code>, <code>snappea_events.events_named("eEnter")</code>.
        </p>

        <h3>Explicit checks</h3>
        <p>Three functions make the check visible in the test. They work without the plugin.</p>
        <Code lang="python">{`
from snappea import check_spec, check_events, expect_violation

def test_release_makes_stock_available_again():
    with check_spec("InventoryCorrect") as events:     # no names: every spec
        inventory = Inventory()
        inventory.add_product("coffee", 3)
        inventory.reserve("coffee", "order-1", 3)
        inventory.release("coffee", "order-1")
    assert events.count("eReleased") == 1
`}</Code>
        <ul>
          <li>
            <code>check_spec(*specs, ...)</code> checks the events emitted inside the block and raises{" "}
            <code>SpecViolation</code> at the end of it. Options left as <code>None</code> come from{" "}
            <code>[tool.snappea]</code>, then the defaults.
          </li>
          <li>
            <code>check_events(events, *specs)</code> checks a hand-written list of events, for testing a spec on its own.
          </li>
          <li>
            <code>Recorder(specs)</code> is the manual form: <code>start()</code>, <code>stop()</code>, <code>check()</code>.
          </li>
          <li>Checks nest, and every active recorder gets every event, including events from threads the test starts.</li>
        </ul>

        <h3>Negative controls</h3>
        <p>Prove the spec catches the bug it is meant to catch:</p>
        <Code lang="python">{`
def test_spec_rejects_overselling():
    with expect_violation("InventoryCorrect", match="exceeds available stock"):
        check_events([
            {"event": "eStocked", "key": "coffee", "units": 1},
            {"event": "eReserved", "key": "coffee", "order": "o1", "units": 2},
        ], "InventoryCorrect")
`}</Code>
        <p>
          It fails unless that spec, and only it, is broken with a matching message. Enclosing checks ignore that spec
          for the rest of the test, so the deliberate violation is not reported twice.
        </p>

        <h3>Property tests</h3>
        <p>
          With Hypothesis, give each generated case its own recording: <code>with check_spec(...)</code> inside the{" "}
          <code>@given</code> body, or a <code>Recorder</code> per <code>RuleBasedStateMachine</code>.
        </p>
      </section>

      <section>
        <h2 id="emitting-events">Emitting events</h2>
        <Code lang="python">{`
emit(event, payload=..., /, *, key=None, at=None, **fields)

emit("eReserved", key=sku, order=order, units=units)
emit("eCommitted", key=tx, at=row.commit_time)   # when it happened, not when logged
`}</Code>
        <ul>
          <li>
            <strong>Payload</strong>: keyword arguments, matched by name against the P event's payload type. A single
            positional value works for payloads that are not named tuples, or whose fields are called <code>key</code>{" "}
            or <code>at</code>. Enums are sent by name, sets and tuples as lists.
          </li>
          <li>
            <strong>
              <code>key</code>
            </strong>{" "}
            splits events into independent streams, such as one per product. Each stream gets its own instance of every spec.
          </li>
          <li>
            <strong>
              <code>at</code>
            </strong>{" "}
            sets the event time: a <code>datetime</code>, or an integer of epoch nanoseconds (a logical clock works). Default: now.
          </li>
          <li>Emit when a change becomes visible (after the commit), not when it is attempted, or specs see states that never existed.</li>
          <li>Each event records the file and line it was emitted from, for failure reports.</li>
        </ul>

        <h3>In production</h3>
        <p>
          The same calls can feed <code>snappea observe</code> on production logs. Set a sink once at start-up:
        </p>
        <Code lang="python">{`
import snappea

sink = snappea.set_sink("/var/log/app/events.jsonl")   # a file, appended to
snappea.set_sink("-")                                  # or stdout, or any text stream
snappea.set_sink(logger.info)                          # or a callable taking one JSON line
`}</Code>
        <Code lang="shell">{`
snappea observe specs/*.p /var/log/app/events.jsonl --spec InventoryCorrect
`}</Code>
        <p>
          A background thread writes the lines. <code>emit</code> never raises or blocks: when the queue (10,000 lines)
          is full, a write fails or a payload cannot be encoded, the event is dropped and counted in{" "}
          <code>sink.dropped</code>. Queued lines are written at exit; <code>sink.flush()</code> waits for them earlier and{" "}
          <code>snappea.set_sink(None)</code> turns the sink off. In a test, the recording takes precedence over the sink.
        </p>
      </section>

      <section>
        <h2 id="other-processes">Other processes</h2>
        <table className="compare ref">
          <thead>
            <tr>
              <th>Source</th>
              <th>How its events reach the check</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <th>The test process</th>
              <td>
                <code>emit</code> adds them to the active recorders
              </td>
            </tr>
            <tr>
              <th>Python processes the test starts</th>
              <td>
                Start them with <code>env=rec.env()</code>; <code>emit</code> writes to a file of their own in{" "}
                <code>SNAPPEA_EVENTS_DIR</code>
              </td>
            </tr>
            <tr>
              <th>Other services</th>
              <td>
                Write the same JSON lines to <code>rec.events_dir()</code> (a mounted volume for containers)
              </td>
            </tr>
            <tr>
              <th>A folder something else writes</th>
              <td>
                <code>rec.add_events_dir(path)</code>
              </td>
            </tr>
            <tr>
              <th>Existing logs</th>
              <td>
                <code>rec.add_log("server.log", parse=fn)</code>, where <code>fn</code> turns a line into a dict like{" "}
                <code>emit</code>'s arguments, or <code>None</code>
              </td>
            </tr>
          </tbody>
        </table>
        <Code lang="python">{`
with check_spec("InventoryCorrect") as rec:
    subprocess.run(["./worker"], env=rec.env(), check=True)
`}</Code>
        <p>
          Stop or join child processes before the check. A service started once per session (a session-scoped fixture)
          starts before any test records, so its events reach no test. Give it a folder of its own and check that folder
          at the end of the session:
        </p>
        <Code lang="python">{`
@pytest.fixture(scope="session")
def server(tmp_path_factory):
    events = tmp_path_factory.mktemp("server-events")
    proc = start_server(env={**os.environ, "SNAPPEA_EVENTS_DIR": str(events)})
    yield proc
    proc.terminate()
    rec = Recorder()
    rec.add_events_dir(str(events))
    rec.check()
`}</Code>
      </section>

      <section>
        <h2 id="failures">Failures</h2>
        <p>
          A failure shows the spec's assert message, the events of the key that broke it (the last 20) with where each
          was emitted, the assert in the P file, and the folder that keeps the events and the checker's output. An event
          whose time was set with <code>at=</code> says so, which explains an order that differs from the emit order.
        </p>
        <p>
          <code>snappea.SpecViolation</code> is an <code>AssertionError</code>, so pytest and Hypothesis treat it as an
          ordinary failure. Its fields: <code>.spec</code>, <code>.message</code>, <code>.key</code>, <code>.events</code>{" "}
          (each with <code>.event</code>, <code>.payload</code>, <code>.src</code>), <code>.location</code>,{" "}
          <code>.liveness</code> (a hot state at the end) and <code>.others</code> (further violations). Anything else
          is a <code>snappea.SetupError</code> with a <code>.kind</code>:
        </p>
        <ErrorKinds lang="python" />
      </section>

      <section>
        <h2 id="reference">Reference</h2>
        <h3>Settings</h3>
        <p>
          Set in <code>[tool.snappea]</code> of the nearest <code>pyproject.toml</code> (paths relative to it), or pass
          as arguments to <code>check_spec</code>, <code>check_events</code>, <code>expect_violation</code> and{" "}
          <code>Recorder</code>. Precedence: argument, environment variable, <code>[tool.snappea]</code>, default.
          Unknown keys are an error.
        </p>
        <SettingsTable lang="python" />
        <h3>Strict checks</h3>
        <p>
          Strict means every selected spec checked at least one relevant event, which catches instrumentation that was
          removed or never ran. With named specs (<code>check_spec("Name")</code>, <code>[tool.snappea] specs</code>) it
          defaults to true, and a named spec that saw no relevant event fails the check with a{" "}
          <code>SetupError</code> of kind <code>"validation"</code>. Pass <code>strict=False</code> (or set{" "}
          <code>strict = false</code>) where a spec may rightly see no events. (<code>snappea.StrictDefaultWarning</code>,
          which warned about this change for one release, is no longer emitted.) With every
          spec selected (the plugin's default) it defaults to false; turn it on with <code>strict = true</code> or{" "}
          <code>pytest --snappea-strict</code>, and mark uninstrumented tests <code>skip=True</code>.
        </p>
        <h3>Environment variables</h3>
        <EnvTable lang="python" />
        <p>
          Deprecated for one release, with a warning: <code>SNAPPEA_BINARY</code> (use <code>SNAPPEA_BIN</code>),{" "}
          <code>liveness=</code> (use <code>check_hot=</code>) and <code>snappea.SnappeaError</code> (use{" "}
          <code>SetupError</code>).
        </p>
        <h3>Suite coverage</h3>
        <p>
          <code>pytest --snappea-coverage=coverage/monitor.json</code> (or <code>SNAPPEA_COVERAGE</code>) reports the P
          spec handlers, statements and branches the suite's checks reached, and writes them as JSON for CI. Runs merge into an
          existing file under a lock, so pytest-xdist workers can share one path; delete the file to start over.
        </p>
        <h3>How it works</h3>
        <p>
          Each check writes the test's events to a file and runs <code>snappea observe</code> on them with{" "}
          <code>--json-report</code>, then reads the report. A check costs about 5 ms; tests that emit nothing cost nothing.
        </p>
        <h3>API</h3>
        <ApiReference lang="python" root={root} />
        <p>
          Runnable projects: <a href={`${REPO}/tree/main/examples/python`}>Python examples</a>.
        </p>
      </section>
    </>
  );
}
