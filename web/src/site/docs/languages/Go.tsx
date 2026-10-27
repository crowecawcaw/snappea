// Docs: the Go helper (snappeatest and snappeaemit). Outline: docs/language-support.md#docs-page.
import { REPO, links } from "../../SiteHeader";
import { Code } from "../Code";
import { ApiReference, BugIntro, EnvTable, ErrorKinds, InventorySpec, SettingsTable } from "./shared";

export function Go({ root }: { root: string }) {
  const l = links(root);
  return (
    <>
      <section className="hero">
        <p className="eyebrow">Languages</p>
        <h1>Go</h1>
        <p className="subtitle">Check P specs against the events your code emits during go test.</p>
        <p>
          Application code calls <code>snappeaemit.Emit("eReserved", ...)</code> where a change becomes visible. A test
          records those events and, when it ends, checks them against your P specs. You need only the specs (event
          declarations and <code>spec</code> machines), not a full P model.
        </p>
        <p>
          It sees only the orderings your tests produce: it checks the implementation against the design, it does not
          explore other interleavings. For how this fits with model checking, see the <a href={l.overview}>overview</a>.
        </p>
      </section>

      <section>
        <h2 id="install">Install</h2>
        <Code lang="shell">{`
go get github.com/crowecawcaw/snappea
`}</Code>
        <p>Go 1.22 or later. The module has two packages, both standard library only:</p>
        <ul>
          <li>
            <code>go/snappeaemit</code>: <code>Emit</code> from application code, in tests, in processes tests start, and
            to production logs. This is all production code imports.
          </li>
          <li>
            <code>go/snappeatest</code>: record and check events from tests.
          </li>
        </ul>
        <p>
          <code>snappeatest</code> runs the <code>snappea</code> checker. It uses <code>SNAPPEA_BIN</code>, else{" "}
          <code>snappea</code> on <code>PATH</code>, else downloads the release binary for the module's version once into
          the user cache and verifies it against checksums in the module. Offline, or with{" "}
          <code>SNAPPEA_DOWNLOAD=0</code>, install it yourself: <code>cargo install snappea</code> or{" "}
          <code>pip install pysnappea</code>.
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
        <Code lang="go" title="inventory/inventory.go">{`
package inventory

import (
	"errors"

	"github.com/crowecawcaw/snappea/go/snappeaemit"
)

type Inventory struct {
	stock        map[string]int
	reservations map[[2]string]int
}

func New() *Inventory {
	return &Inventory{stock: map[string]int{}, reservations: map[[2]string]int{}}
}

func (i *Inventory) AddProduct(sku string, units int) {
	i.stock[sku] = units
	snappeaemit.Emit("eStocked", sku, map[string]any{"units": units})
}

func (i *Inventory) Reserve(sku, order string, units int) error {
	if units > i.stock[sku] {
		return errors.New("insufficient stock")
	}
	i.stock[sku] -= units
	i.reservations[[2]string{sku, order}] = units
	snappeaemit.Emit("eReserved", sku, map[string]any{"order": order, "units": units}) // after the change
	return nil
}
`}</Code>
        <p>
          Start a recording at the top of the test. It checks when the test ends:
        </p>
        <Code lang="go" title="inventory/inventory_test.go">{`
package inventory

import (
	"testing"

	"github.com/crowecawcaw/snappea/go/snappeatest"
)

func TestTwoOrdersForTheLastUnits(t *testing.T) {
	snappeatest.Start(t, snappeatest.Config{SpecFiles: []string{"../specs"}})

	inventory := New()
	inventory.AddProduct("coffee", 3)
	if err := inventory.Reserve("coffee", "order-1", 2); err != nil {
		t.Fatal(err)
	}
	_ = inventory.Reserve("coffee", "order-2", 2) // rejecting order-2 is fine too
}
`}</Code>
        <p>
          <code>go test ./...</code> passes. <BugIntro>delete the stock check from <code>Reserve</code>.</BugIntro>
        </p>
        <Code lang="text">{`
--- FAIL: TestTwoOrdersForTheLastUnits (0.01s)
    inventory_test.go:10: P spec InventoryCorrect was broken: reservation exceeds available stock

          Events (key "coffee"):
              1  eStocked   units=3                inventory.go:18
              2  eReserved  order=order-1 units=2  inventory.go:24
              3  eReserved  order=order-2 units=2  inventory.go:24  <- broke the spec here

          Spec: ../specs/inventory.p:27
          snappea output: /tmp/snappeatest-TestTwoOrdersForTheLastUnits-3725067642
`}</Code>
      </section>

      <section>
        <h2 id="test-setup">Test setup</h2>
        <h3>A recording per test</h3>
        <p>
          Go has no global per-test hook, so each test opts in by calling <code>snappeatest.Start</code>. Put it in your
          existing fixture, next to the shared <code>Config</code>:
        </p>
        <Code lang="go">{`
func stocked(t *testing.T) *inventory.Inventory {
	t.Helper()
	snappeatest.Start(t, snappeatest.Config{
		SpecFiles: []string{"../specs/inventory.p"},
		Specs:     []string{"InventoryCorrect"},
	})
	app := inventory.New()
	app.AddProduct("coffee", 3)
	return app
}
`}</Code>
        <ul>
          <li>
            <code>Start</code> checks in a <code>t.Cleanup</code>. Register cleanups that stop workers <strong>after</strong>{" "}
            <code>Start</code>, so they run first. A test that already failed is not checked.
          </li>
          <li>Each test, subtest and fuzz callback needs its own recording. Emitting after the check fails it.</li>
          <li>
            <code>snappeatest.New</code> plus <code>r.Check(t)</code> checks where you call it; <code>r.Verify(ctx)</code>{" "}
            returns <code>(Result, error)</code> instead.
          </li>
          <li>
            <code>SNAPPEA_MODE=off</code> makes recordings inert.
          </li>
        </ul>

        <h3>Parallel tests</h3>
        <p>
          The global <code>snappeaemit.Emit</code> cannot tell parallel tests apart: with several recordings active, it
          fails them all with a <code>config</code> error. For <code>t.Parallel()</code> tests, inject the recording's{" "}
          <code>r.Emit</code> into the application, or pass a context:{" "}
          <code>snappeaemit.EmitCtx(ctx, ...)</code> with <code>ctx := r.Context(context.Background())</code>.
        </p>

        <h3>Negative controls</h3>
        <p>Prove the spec catches the bug it is meant to catch:</p>
        <Code lang="go">{`
v := r.ExpectViolation(t, "InventoryCorrect", "exceeds available")
if v.Key != "coffee" {
	t.Errorf("violation on %q", v.Key)
}
`}</Code>
        <p>
          It checks now and fails the test unless that spec, and only it, is broken with a message matching the regular
          expression. Later checks of <code>r</code> ignore that spec.
        </p>

        <h3>Fuzzing</h3>
        <p>
          Call <code>Start</code> inside the <code>f.Fuzz</code> callback with its <code>t</code>, so each input gets a
          fresh recording.
        </p>
      </section>

      <section>
        <h2 id="emitting-events">Emitting events</h2>
        <Code lang="go">{`
snappeaemit.Emit("eReserved", sku, map[string]any{"order": order, "units": units})
snappeaemit.EmitAt(row.CommitTime, "eReserved", sku, payload) // when it happened, not when logged
`}</Code>
        <ul>
          <li>
            <strong>Payload</strong>: a struct or map whose JSON names match the P named tuple, a single value, a slice
            for a tuple, or nil. It is encoded at once.
          </li>
          <li>
            <strong>Key</strong> splits events into independent streams, such as one per product. Each stream gets its
            own instance of every spec.
          </li>
          <li>Emit after the change commits, or specs see states that never existed.</li>
          <li>The call site is recorded for failure reports.</li>
        </ul>
        <table className="compare ref">
          <thead>
            <tr>
              <th>Process</th>
              <th>Events go to</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <th>A test with a recording</th>
              <td>that recording</td>
            </tr>
            <tr>
              <th>Started by a test</th>
              <td>
                its own file in <code>SNAPPEA_EVENTS_DIR</code>
              </td>
            </tr>
            <tr>
              <th>
                After <code>SetSink</code>
              </th>
              <td>
                JSON lines for <code>snappea observe</code>, written by a background goroutine
              </td>
            </tr>
            <tr>
              <th>Otherwise</th>
              <td>nowhere: one atomic load</td>
            </tr>
          </tbody>
        </table>

        <h3>In production</h3>
        <Code lang="go">{`
sink, err := snappeaemit.File("/var/log/app/events/inventory.jsonl") // or Stdout(), Writer(w), Callback(f)
if err != nil {
	log.Fatal(err)
}
snappeaemit.SetSink(sink)
defer snappeaemit.Close() // writes what is queued, closes the file
`}</Code>
        <Code lang="shell">{`
snappea observe specs/inventory.p '/var/log/app/events/*.jsonl' --spec InventoryCorrect
`}</Code>
        <p>
          <code>Emit</code> never panics or blocks: a line that cannot be encoded or written, or that finds 4096 lines
          already queued, is dropped and counted by <code>snappeaemit.Dropped()</code>. <code>Flush(timeout)</code> waits
          for the queue; <code>SetSink(nil)</code> turns it off. In a process a test started, <code>SetSink</code> does
          nothing: events go to the test.
        </p>
      </section>

      <section>
        <h2 id="other-processes">Other processes</h2>
        <p>
          Start a child with <code>r.Env()</code>, the environment plus <code>SNAPPEA_EVENTS_DIR</code>. If it uses{" "}
          <code>snappeaemit</code>, its events join the check. Wait for it before the check.
        </p>
        <Code lang="go">{`
cmd := exec.Command(workerBinary)
cmd.Env = r.Env()
if err := cmd.Run(); err != nil {
	t.Fatal(err)
}
`}</Code>
        <ul>
          <li>
            <code>r.EventsDir()</code> is the folder, for anything that writes the event lines itself.
          </li>
          <li>
            <code>r.AddEventsDir(path)</code> adds a folder the recording did not create, such as a container volume.
          </li>
          <li>
            <code>r.AddLog(path, parse)</code> adds a log; <code>parse(line) (snappeatest.Event, bool)</code> returns an
            event with <code>Time</code> set, or false to skip the line. <code>nil</code> reads JSON lines as they are.
          </li>
        </ul>
      </section>

      <section>
        <h2 id="failures">Failures</h2>
        <p>
          A failure shows the spec's assert message, the events of the key that broke it with where each was emitted,
          the assert in the P file, and the folder that keeps the events and the checker's output.{" "}
          <code>Check</code> reports it with <code>t.Errorf</code>; <code>Verify</code> returns it:
        </p>
        <Code lang="go">{`
var v *snappeatest.SpecViolation
if errors.As(err, &v) && v.Key == "coffee" { ... }
`}</Code>
        <p>
          <code>*SpecViolation</code> has <code>Spec</code>, <code>Key</code>, <code>Message</code>, <code>Location</code>{" "}
          (the P assert), <code>Events</code> (each with <code>Event</code>, <code>Payload</code>, <code>Src</code>),{" "}
          <code>Liveness</code>, <code>Others</code> and <code>ArtifactsDir</code>. Anything else is a{" "}
          <code>*snappeatest.SetupError</code> whose <code>Kind</code> is:
        </p>
        <ErrorKinds lang="go" />
      </section>

      <section>
        <h2 id="reference">Reference</h2>
        <h3>Settings</h3>
        <p>
          Go has no config file: settings are <code>snappeatest.Config</code> fields, with paths relative to{" "}
          <code>WorkDir</code> (default: the package folder). Environment variables override fields, except{" "}
          <code>SNAPPEA_BIN</code>, which <code>Binary</code> overrides. <code>Strict</code> is a <code>*bool</code>: use{" "}
          <code>snappeatest.Bool(false)</code>. <code>AllowUnobserved</code> and <code>ArtifactRoot</code> are deprecated
          aliases for <code>Strict: snappeatest.Bool(false)</code> and <code>Artifacts</code>.
        </p>
        <SettingsTable lang="go" />
        <h3>Environment variables</h3>
        <EnvTable lang="go" />
        <h3>Suite coverage</h3>
        <p>
          Add <code>{"func TestMain(m *testing.M) { snappeatest.Main(m) }"}</code>. After the tests, <code>Main</code> prints
          the P spec states, handlers, statements and branches the checks reached together, and lists what no check reached. It
          writes no file. Set <code>Config.Coverage</code> (or <code>SNAPPEA_COVERAGE</code>) to a path to also keep it as
          JSON: the file is merged under a lock, so <code>go test ./...</code> packages can share it; delete a stale file
          before a run. Without <code>Main</code>, nothing is printed and checks with a path merge into it as each completes.{" "}
          <code>SNAPPEA_COVERAGE=off</code> (or <code>Coverage: "off"</code>) turns suite coverage off.
        </p>
        <h3>API</h3>
        <ApiReference lang="go" root={root} />
        <p>
          Runnable projects: <a href={`${REPO}/tree/main/examples/go`}>Go examples</a>.
        </p>
      </section>
    </>
  );
}
