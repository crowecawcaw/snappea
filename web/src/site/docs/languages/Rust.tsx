// Docs: the Rust helper (snappea::test and snappea::emit). Outline: docs/language-support.md#docs-page.
import { REPO, links } from "../../SiteHeader";
import { Code } from "../Code";
import { ApiReference, BugIntro, EnvTable, ErrorKinds, InventorySpec, SettingsTable } from "./shared";

export function Rust({ root }: { root: string }) {
  const l = links(root);
  return (
    <>
      <section className="hero">
        <p className="eyebrow">Languages</p>
        <h1>Rust</h1>
        <p className="subtitle">Check P specs against the events your code emits during its #[test]s.</p>
        <p>
          Application code calls <code>emit("eReserved", ...)</code> where a change becomes visible. A test records
          those events and checks them against your P specs, in process: no separate binary. You need only the specs
          (event declarations and <code>spec</code> machines), not a full P model.
        </p>
        <p>
          It sees only the orderings your tests produce: it checks the implementation against the design, it does not
          explore other interleavings. For how this fits with model checking, see the <a href={l.overview}>overview</a>.
        </p>
      </section>

      <section>
        <h2 id="install">Install</h2>
        <p>
          One crate, <code>snappea</code>, with features. Production code needs only the emitter, which compiles no
          checker and depends only on <code>serde</code> and <code>serde_json</code>:
        </p>
        <Code lang="toml" title="Cargo.toml">{`
[dependencies]
snappea = { version = "0.2", default-features = false, features = ["emit"] }

[dev-dependencies]
snappea = { version = "0.2", default-features = false, features = ["test"] }
`}</Code>
        <table className="compare ref">
          <thead>
            <tr>
              <th>Feature</th>
              <th>Adds</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <th>
                <code>emit</code>
              </th>
              <td>
                <code>snappea::emit</code>: send events to tests, child-process folders or production logs
              </td>
            </tr>
            <tr>
              <th>
                <code>test</code> (default)
              </th>
              <td>
                <code>snappea::test</code>: in-process checks in <code>#[test]</code>s. Implies <code>emit</code>
              </td>
            </tr>
            <tr>
              <th>
                <code>cli</code> (default)
              </th>
              <td>
                The <code>snappea</code> and <code>snappea-compat</code> binaries. <code>default-features = false,
                features = ["test"]</code> skips them
              </td>
            </tr>
          </tbody>
        </table>
      </section>

      <section>
        <h2 id="quick-start">Quick start</h2>
        <p>
          The examples on every language page check one spec: an inventory where reservations never exceed the stock
          and releases match reservations.
        </p>
        <InventorySpec />
        <p>The service emits an event after each change:</p>
        <Code lang="rust" title="src/lib.rs">{`
use snappea::emit::{emit, json};
use std::collections::HashMap;

#[derive(Default)]
pub struct Inventory {
    stock: HashMap<String, i64>,
    reservations: HashMap<(String, String), i64>,
}

impl Inventory {
    pub fn add_product(&mut self, sku: &str, units: i64) {
        self.stock.insert(sku.into(), units);
        emit("eStocked", sku, json!({"units": units}));
    }

    pub fn reserve(&mut self, sku: &str, order: &str, units: i64) -> Result<(), String> {
        let available = self.stock.get_mut(sku).ok_or("unknown product")?;
        if units > *available {
            return Err("insufficient stock".into());
        }
        *available -= units;
        self.reservations.insert((sku.into(), order.into()), units);
        emit("eReserved", sku, json!({"order": order, "units": units})); // after the change
        Ok(())
    }
}
`}</Code>
        <p>Point the crate at the specs, then wrap a test body in <code>check</code>:</p>
        <Code lang="toml" title="Cargo.toml">{`
[package.metadata.snappea]
spec_files = ["specs"]
`}</Code>
        <Code lang="rust" title="tests/inventory.rs">{`
use inventory::Inventory;
use snappea::test::Model;
use std::sync::LazyLock;

// Compiled once, from [package.metadata.snappea] in Cargo.toml.
static MODEL: LazyLock<Model> = LazyLock::new(|| Model::load().unwrap());

#[test]
fn two_orders_for_the_last_units() {
    MODEL.check(|_recording| {
        let mut inventory = Inventory::default();
        inventory.add_product("coffee", 3);
        inventory.reserve("coffee", "order-1", 2).unwrap();
        let _ = inventory.reserve("coffee", "order-2", 2); // rejecting order-2 is fine too
    });
}
`}</Code>
        <p>
          <code>cargo test</code> passes. <BugIntro>delete the stock check from <code>reserve</code>.</BugIntro>
        </p>
        <Code lang="text">{`
---- two_orders_for_the_last_units stdout ----

thread 'two_orders_for_the_last_units' panicked at tests/inventory.rs:10:11:
P spec InventoryCorrect was broken: reservation exceeds available stock

  Events (key "coffee"):
      1  eStocked   units=3                src/lib.rs:13
      2  eReserved  order=order-1 units=2  src/lib.rs:20
      3  eReserved  order=order-2 units=2  src/lib.rs:20  <- broke the spec here

  Spec: specs/inventory.p:27
`}</Code>
      </section>

      <section>
        <h2 id="test-setup">Test setup</h2>
        <h3>A checked fixture</h3>
        <p>
          <code>model.check(|recording| ...)</code> records while the closure runs, then verifies and panics with the
          failure output. Cargo has no global per-test hook, so each test opts in by calling it; put it in your existing
          fixture factory.
        </p>
        <ul>
          <li>Put setup and cleanup inside the closure, and join worker threads there. Nothing is checked in <code>Drop</code>.</li>
          <li>A panic in the closure propagates unchanged, with no second check.</li>
          <li>Never share a recording between tests: Cargo runs tests in parallel.</li>
          <li>
            Compile the <code>Model</code> once (in a <code>LazyLock</code>) and share it; clones are cheap.
          </li>
          <li>
            <code>SNAPPEA_MODE=off</code> runs the closures without checking.
          </li>
        </ul>
        <p>
          <code>Model::load()</code> reads <code>[package.metadata.snappea]</code>. <code>Model::from_files(paths)</code>{" "}
          and <code>Model::compile(&[(name, text)])</code> (for <code>include_str!</code>) set up explicitly. Builders
          override any setting: <code>.specs([..])</code>, <code>.exclude([..])</code>, <code>.strict(..)</code>,{" "}
          <code>.check_hot(..)</code>, <code>.unknown_events(..)</code>, <code>.coverage(path)</code>.
        </p>

        <h3>Explicit checks</h3>
        <p>The explicit form returns the result instead of panicking:</p>
        <Code lang="rust">{`
let recording = MODEL.recording();
let mut app = App::new(recording.emitter());
app.reserve("coffee", "o1", 2)?;
recording.verify()?; // Result<CheckReport, CheckError>
`}</Code>
        <p>
          <code>verify</code> consumes the recording; join worker threads and finish cleanup first.
        </p>

        <h3>Negative controls</h3>
        <p>Prove the spec catches the bug it is meant to catch:</p>
        <Code lang="rust">{`
let violation = recording.expect_violation("InventoryCorrect", "exceeds available stock");
assert_eq!(violation.key, "coffee");
`}</Code>
        <p>It panics unless exactly that spec is broken with a message matching the regular expression.</p>

        <h3>Property tests</h3>
        <p>
          Create the recording inside the property body, which runs once per case and shrink attempt, and report with{" "}
          <code>prop_assert!</code> so proptest shrinks the input:
        </p>
        <Code lang="rust">{`
proptest! {
    #[test]
    fn ops(commands in commands()) {
        let recording = MODEL.recording();
        // ... run commands against a fresh application ...
        let checked = recording.verify();
        prop_assert!(checked.is_ok(), "{}", checked.unwrap_err());
    }
}
`}</Code>
      </section>

      <section>
        <h2 id="emitting-events">Emitting events</h2>
        <Code lang="rust">{`
use snappea::emit::{emit, emit_at, json};

emit("eReserved", "coffee", json!({"order": "o1", "units": 2}));
emit_at(row.commit_time, "eReserved", sku, json!({"order": order, "units": units})); // when it happened
`}</Code>
        <ul>
          <li>
            <strong>Payload</strong>: anything <code>Serialize</code>: <code>json!</code>, a{" "}
            <code>#[derive(Serialize)]</code> struct, a scalar, or <code>()</code> for none. Named tuples are objects;
            enums go by name.
          </li>
          <li>
            <strong>Key</strong>: anything <code>Display</code>. It splits events into independent streams, such as one
            per product; each stream gets its own instance of every spec.
          </li>
          <li>
            <strong>Time</strong>: <code>emit_at</code> takes a <code>SystemTime</code>, or <code>i64</code> epoch
            nanoseconds for a logical clock. Each key's events are checked in time order.
          </li>
          <li>Emit after the change commits, or specs see states that never existed.</li>
          <li>
            The call site is recorded with <code>#[track_caller]</code> and shown in failure reports.
          </li>
        </ul>
        <p>Three ways to get events into a recording:</p>
        <ul>
          <li>
            <code>recording.emit(...)</code> in the test body, for setup and events the test itself knows. A payload
            that cannot be serialized fails <code>verify()</code>.
          </li>
          <li>
            <code>recording.emitter()</code>: a cloneable <code>snappea::emit::Emitter</code> to inject into application
            code or move into threads, as in the explicit check above. It panics if used after{" "}
            <code>verify()</code>.
          </li>
          <li>
            The global <code>snappea::emit::emit</code>: a recording binds the thread that creates it. Bind the scoped
            threads the test spawns with <code>let _guard = recording.bind();</code>. For{" "}
            <code>thread::spawn</code> threads, thread pools or async workers, move a{" "}
            <code>recording.binder()</code> in and call <code>binder.bind()</code> there. With several recordings open and an unbound thread, the emit fails them all, so
            bind or inject.
          </li>
        </ul>

        <h3>In production</h3>
        <Code lang="rust">{`
use snappea::emit::Emitter;

snappea::emit::set_sink(Emitter::file("/var/log/app/events/inventory.jsonl")?);
// or Emitter::stdout(), Emitter::writer(w), Emitter::callback(|event| ...)
// ... at shutdown:
snappea::emit::close(); // writes what is queued
`}</Code>
        <Code lang="shell">{`
snappea observe specs/inventory.p '/var/log/app/events/*.jsonl' --spec InventoryCorrect
`}</Code>
        <p>
          In production, emitting never panics, returns an error, or blocks on I/O. Writers queue up to 16,384 lines and drop events
          when the queue is full or a write fails; <code>snappea::emit::dropped()</code> counts them. With no sink,{" "}
          <code>emit</code> costs a few atomic loads, but the payload is still built; code holding an{" "}
          <code>Emitter</code> can skip a costly one with <code>is_on()</code>.
        </p>
      </section>

      <section>
        <h2 id="other-processes">Other processes</h2>
        <Code lang="rust">{`
let recording = MODEL.recording();
Command::new(server_bin).envs(recording.env()).status()?;   // sets SNAPPEA_EVENTS_DIR
recording.add_events_dir("/shared/events");                  // a folder the test did not create
recording.add_log("logs/events.jsonl");                      // JSON event lines
recording.add_log_with("logs/app.log", |line| parse(line));  // -> Option<Value>
recording.verify()?;
`}</Code>
        <p>
          A child process that calls <code>snappea::emit::emit</code> writes to <code>recording.events_dir()</code>{" "}
          through <code>SNAPPEA_EVENTS_DIR</code>; anything else can write the same JSON lines there. Stop or join
          children before <code>verify()</code>. A failed check keeps the folder and names it.
        </p>
      </section>

      <section>
        <h2 id="failures">Failures</h2>
        <p>
          A failure shows the spec's assert message, the events of the key that broke it (the last 20) with where each
          was emitted, and the assert in the P file. <code>Model::check</code> panics with it; <code>verify()</code>{" "}
          returns <code>CheckError::Violation(SpecViolation)</code> or <code>CheckError::Setup(SetupError)</code>.
          Assert on fields, not text:
        </p>
        <Code lang="rust">{`
let Err(CheckError::Violation(v)) = recording.verify() else { panic!() };
assert_eq!((v.spec.as_str(), v.key.as_str()), ("InventoryCorrect", "coffee"));
// v.message, v.location (P assert file:line), v.events (last 20, with src),
// v.liveness (hot state under check_hot), v.others (more violations)
`}</Code>
        <p>
          <code>SetupError.kind</code> is one of:
        </p>
        <ErrorKinds lang="rust" />
      </section>

      <section>
        <h2 id="reference">Reference</h2>
        <h3>Settings</h3>
        <p>
          Set in <code>[package.metadata.snappea]</code> of the crate's <code>Cargo.toml</code> (paths relative to it),
          or with the <code>Model</code> builders. Precedence: builder, environment variable, <code>Cargo.toml</code>,
          default.
        </p>
        <SettingsTable lang="rust" />
        <h3>Environment variables</h3>
        <EnvTable lang="rust" />
        <h3>Suite coverage</h3>
        <p>
          Set <code>coverage</code> (or <code>SNAPPEA_COVERAGE</code>) and every check merges the P spec handlers, statements and
          branches it reached into that file, under a lock, so parallel tests and test binaries share it. The summary
          prints when coverage changes (<code>cargo test -- --nocapture</code> shows it). Delete a stale file before a run.
        </p>
        <h3>API</h3>
        <ApiReference lang="rust" root={root} />
        <p>
          Runnable projects: <a href={`${REPO}/tree/main/examples/rust`}>Rust examples</a>. The checker itself is also a
          library: <a href={`${REPO}/blob/main/docs/library.md`}>using the checker from Rust</a>.
        </p>
      </section>
    </>
  );
}
