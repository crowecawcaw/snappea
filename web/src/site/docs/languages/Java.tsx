// Docs: the Java helper (io.github.crowecawcaw:snappea). Outline: docs/language-support.md#docs-page.
import { REPO, links } from "../../SiteHeader";
import { Code } from "../Code";
import { ApiReference, BugIntro, EnvTable, ErrorKinds, InventorySpec, SettingsTable } from "./shared";

export function Java({ root }: { root: string }) {
  const l = links(root);
  return (
    <>
      <section className="hero">
        <p className="eyebrow">Languages</p>
        <h1>Java</h1>
        <p className="subtitle">Check P specs against the events your code emits during its JUnit tests.</p>
        <p>
          Application code calls <code>SnappeaEmit.emit("eReserved", ...)</code> where a change becomes visible. After
          each test, the JUnit 5 extension checks that test's events against your P specs and fails the test if one was
          broken. You need only the specs (event declarations and <code>spec</code> machines), not a full P model.
        </p>
        <p>
          It sees only the orderings your tests produce: it checks the implementation against the design, it does not
          explore other interleavings. For how this fits with model checking, see the <a href={l.overview}>overview</a>.
        </p>
      </section>

      <section>
        <h2 id="install">Install</h2>
        <p>
          Java 17 or later. One artifact, <code>io.github.crowecawcaw:snappea</code>, holds both halves:
        </p>
        <ul>
          <li>
            <code>io.github.crowecawcaw.snappea.emit</code>: <code>SnappeaEmit.emit(...)</code> and production sinks. JDK
            only; this is all production code uses.
          </li>
          <li>
            <code>io.github.crowecawcaw.snappea</code>: recordings, checks and the JUnit 5 extension. JUnit and opentest4j
            are optional dependencies that your test framework brings.
          </li>
        </ul>
        <p>
          The <code>snappea</code> checker comes in a classifier jar of the same artifact, <code>natives-&lt;os&gt;-&lt;arch&gt;</code>{" "}
          (<code>linux-x86_64</code>, <code>linux-aarch64</code>, <code>macos-x86_64</code>, <code>macos-aarch64</code>,{" "}
          <code>windows-x86_64</code>). Add it in test scope, with <code>snappea.version</code> set to a{" "}
          <a href={`${REPO}/releases`}>release</a>:
        </p>
        <Code lang="xml" title="pom.xml">{`
<dependency>
  <groupId>io.github.crowecawcaw</groupId>
  <artifactId>snappea</artifactId>
  <version>\${snappea.version}</version>
</dependency>
<dependency>
  <groupId>io.github.crowecawcaw</groupId>
  <artifactId>snappea</artifactId>
  <version>\${snappea.version}</version>
  <classifier>\${snappea.natives}</classifier>
  <scope>test</scope>
</dependency>
`}</Code>
        <p>
          If everyone builds on one platform, write its classifier in place of <code>{"${snappea.natives}"}</code>.
          Otherwise pick it per platform with profiles:
        </p>
        <Code lang="xml" title="pom.xml">{`
<profiles>
  <profile><id>snappea-linux-x86_64</id><activation><os><name>linux</name><arch>amd64</arch></os></activation><properties><snappea.natives>natives-linux-x86_64</snappea.natives></properties></profile>
  <profile><id>snappea-linux-aarch64</id><activation><os><name>linux</name><arch>aarch64</arch></os></activation><properties><snappea.natives>natives-linux-aarch64</snappea.natives></properties></profile>
  <profile><id>snappea-macos-x86_64</id><activation><os><family>mac</family><arch>x86_64</arch></os></activation><properties><snappea.natives>natives-macos-x86_64</snappea.natives></properties></profile>
  <profile><id>snappea-macos-aarch64</id><activation><os><family>mac</family><arch>aarch64</arch></os></activation><properties><snappea.natives>natives-macos-aarch64</snappea.natives></properties></profile>
  <profile><id>snappea-windows-x86_64</id><activation><os><family>windows</family><arch>amd64</arch></os></activation><properties><snappea.natives>natives-windows-x86_64</snappea.natives></properties></profile>
</profiles>
`}</Code>
        <p>Gradle (Kotlin DSL):</p>
        <Code lang="kotlin" title="build.gradle.kts">{`
val snappeaOs = System.getProperty("os.name").lowercase().let {
    if (it.startsWith("mac")) "macos" else if (it.startsWith("windows")) "windows" else "linux" }
val snappeaArch = if (System.getProperty("os.arch") in setOf("aarch64", "arm64")) "aarch64" else "x86_64"
dependencies {
    implementation("io.github.crowecawcaw:snappea:$snappeaVersion")
    testRuntimeOnly("io.github.crowecawcaw:snappea:$snappeaVersion:natives-$snappeaOs-$snappeaArch")
}
`}</Code>
        <p>
          The checker is found from <code>.binary(...)</code>, then <code>SNAPPEA_BIN</code>, then the natives jar, then{" "}
          <code>snappea</code> on <code>PATH</code>. The natives jar's binary is copied to a per-version user cache on
          first use and checked against the SHA-256 in the jar.
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
        <Code lang="java" title="src/main/java/shop/Inventory.java">{`
package shop;

import io.github.crowecawcaw.snappea.emit.SnappeaEmit;
import java.util.HashMap;
import java.util.Map;

public final class Inventory {
    private final Map<String, Integer> stock = new HashMap<>();
    private final Map<String, Integer> reservations = new HashMap<>();

    public void addProduct(String sku, int units) {
        stock.put(sku, units);
        SnappeaEmit.emit("eStocked", sku, Map.of("units", units));
    }

    public void reserve(String sku, String order, int units) {
        if (units > stock.get(sku)) throw new IllegalStateException("insufficient stock");
        stock.put(sku, stock.get(sku) - units);
        reservations.put(sku + "/" + order, units);
        SnappeaEmit.emit("eReserved", sku, Map.of("order", order, "units", units)); // after the change
    }
}
`}</Code>
        <p>Point the helper at the specs and register the extension:</p>
        <Code lang="toml" title="snappea.properties">{`
spec_files = specs
`}</Code>
        <Code lang="java" title="src/test/java/shop/InventoryTest.java">{`
package shop;

import io.github.crowecawcaw.snappea.SnappeaExtension;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.ExtendWith;

@ExtendWith(SnappeaExtension.class)
class InventoryTest {
    @Test
    void twoOrdersForTheLastUnits() {
        var inventory = new Inventory();
        inventory.addProduct("coffee", 3);
        inventory.reserve("coffee", "order-1", 2);
        try {
            inventory.reserve("coffee", "order-2", 2);
        } catch (IllegalStateException rejected) {
            // rejecting order-2 is fine too
        }
    }
}
`}</Code>
        <p>
          <code>mvn test</code> passes. <BugIntro>delete the stock check from <code>reserve</code>.</BugIntro>
        </p>
        <Code lang="text">{`
[ERROR] shop.InventoryTest.twoOrdersForTheLastUnits -- Time elapsed: 0.129 s <<< FAILURE!
io.github.crowecawcaw.snappea.SpecViolation:
P spec InventoryCorrect was broken: reservation exceeds available stock

  Events (key "coffee"):
      1  eStocked   units=3                shop/Inventory.java:13
      2  eReserved  units=2 order=order-1  shop/Inventory.java:19
      3  eReserved  units=2 order=order-2  shop/Inventory.java:19  <- broke the spec here

  Spec: specs/inventory.p:27
  snappea output: /tmp/snappea/recording-6629023866151738740
`}</Code>
      </section>

      <section>
        <h2 id="test-setup">Test setup</h2>
        <h3>Every test: the JUnit 5 extension</h3>
        <p>Pick one registration:</p>
        <Code lang="java">{`
@ExtendWith(SnappeaExtension.class)                 // settings from snappea.properties
class InventoryTest { ... }

@RegisterExtension                                  // settings in code
static final SnappeaExtension SNAPPEA = SnappeaExtension.builder().specs("InventoryCorrect").build();
`}</Code>
        <p>
          Or every test class, with <code>junit.jupiter.extensions.autodetection.enabled=true</code> in{" "}
          <code>junit-platform.properties</code>. An autodetected extension steps aside in classes that register their own.
        </p>
        <ul>
          <li>
            A recording starts before <code>@BeforeEach</code> and is checked after <code>@AfterEach</code>, so fixtures
            are checked too.
          </li>
          <li>A test that already failed keeps its failure; the check's result is attached as suppressed.</li>
          <li>
            Inject the recording as a <code>Recording</code> parameter when code needs it explicitly.
          </li>
          <li>
            In <code>marked</code> mode only tests or classes annotated <code>@Snappea</code> are checked.
          </li>
          <li>A recording with no events and <code>strict</code> off passes without running the checker.</li>
          <li>
            Several extensions on one class need <code>.name("billing")</code> and{" "}
            <code>@SnappeaRecording("billing") Recording</code> parameters.
          </li>
        </ul>
        <p>Per-test options:</p>
        <Code lang="java">{`
@Test @Snappea(skip = Snappea.ALL) void notChecked() { ... }
@Test @Snappea(skip = "SmallOrders") void notThisSpec() { ... }
@Test @Snappea(checkHot = true) void everyReservationShips() { ... }
`}</Code>

        <h3>Explicit checks</h3>
        <Code lang="java">{`
var observer = SnappeaObserver.builder().build(); // settings from snappea.properties

try (var recording = observer.recording("reserve then release")) {
    var inventory = new Inventory();              // emits through SnappeaEmit
    inventory.addProduct("coffee", 3);
    inventory.reserve("coffee", "order-1", 2);
}                                                 // checked here
`}</Code>
        <p>
          Closing checks once and throws <code>SpecViolation</code> or <code>SetupError</code>. The manual form is{" "}
          <code>observer.start(label)</code>, then <code>recording.stop()</code> and <code>recording.check()</code>.{" "}
          <code>recording.emit(...)</code> and <code>emitAt(...)</code> write to that recording only, for code that takes
          an injected sink. Join worker threads and child processes before the check: an event emitted after the check
          throws.
        </p>

        <h3>Negative controls</h3>
        <p>Prove the spec catches the bug it is meant to catch:</p>
        <Code lang="java">{`
var violation = Recording.expectViolation("InventoryCorrect", "exceeds available stock", () -> {
    try (var recording = observer.recording("oversell")) { ... }
});
`}</Code>
        <p>
          It fails if the check passes or another spec is violated. Enclosing recordings, such as the extension's,
          ignore that spec for the rest of the test.
        </p>

        <h3>Property tests</h3>
        <p>
          Jupiter callbacks do not wrap jqwik tries: open <code>observer.recording(...)</code> inside each{" "}
          <code>@Property</code> method, with a <code>CoverageCollector</code> closed in <code>@AfterContainer</code>.
        </p>
      </section>

      <section>
        <h2 id="emitting-events">Emitting events</h2>
        <Code lang="java">{`
SnappeaEmit.emit("eReserved", sku, Map.of("order", order, "units", units)); // event, key, payload
SnappeaEmit.emit("eClosed", sku);                                            // no payload
SnappeaEmit.emitAt(commitTime, "eReserved", sku, payload);                   // when it happened
`}</Code>
        <ul>
          <li>
            <strong>Payload</strong>: a <code>Map</code> or record for a P named tuple, a single value, or none. Enums go
            by name, collections and arrays as lists.
          </li>
          <li>
            <strong>Key</strong> splits events into independent streams, such as one per product. Each stream gets its
            own instance of every spec.
          </li>
          <li>Emit after the change commits, at the same place production code would publish it.</li>
          <li>
            Each line records the call site, a per-process sequence number and a wall-clock time that never goes backwards.
          </li>
        </ul>
        <p>Where an event goes, in order:</p>
        <ol>
          <li>A recording in this JVM: the one the current thread (or the thread that started it) opened.</li>
          <li>
            <code>SNAPPEA_EVENTS_DIR</code>, when a test started this process.
          </li>
          <li>The production sink, if set.</li>
          <li>
            Nowhere: two volatile reads. <code>SnappeaEmit.enabled()</code> skips building an expensive payload.
          </li>
        </ol>

        <h3>In production</h3>
        <Code lang="java">{`
SnappeaEmit.setSink(Sink.file(Path.of("/var/log/app/events/inventory.jsonl")));
// or Sink.stdout(), Sink.writer(out), Sink.callback(line -> ...)
// ... at shutdown:
SnappeaEmit.close(); // writes what is queued
`}</Code>
        <Code lang="shell">{`
snappea observe specs/inventory.p '/var/log/app/events/*.jsonl' --spec InventoryCorrect
`}</Code>
        <p>
          A background thread writes the lines. <code>emit</code> never throws or waits: drops (full queue, I/O error,
          bad payload) are counted in <code>SnappeaEmit.dropped()</code>. <code>SnappeaEmit.flush(timeout)</code> waits
          for the queue.
        </p>
      </section>

      <section>
        <h2 id="other-processes">Other processes</h2>
        <Code lang="java">{`
var worker = new ProcessBuilder("java", "-cp", classpath, "example.Worker");
worker.environment().putAll(recording.environment()); // adds SNAPPEA_EVENTS_DIR
worker.start().waitFor();                             // join before the check

recording.addEventsDir(Path.of("volumes/events"));     // a folder this recording did not create
recording.addLog(Path.of("app-events.jsonl"));         // JSON event lines
recording.addLog(Path.of("server.log"), line -> ...);  // returns a LogEvent, or null to skip
`}</Code>
        <p>
          <code>recording.eventsDir()</code> is the folder itself, for containers or other services writing event lines.
          Events are ordered by timestamp per key; equal times keep this process's events first.
        </p>
      </section>

      <section>
        <h2 id="failures">Failures</h2>
        <p>
          A failure shows the spec's assert message, the events of the key that broke it with where each was emitted,
          the assert in the P file, and the folder that keeps the events and the checker's output.{" "}
          <code>SpecViolation</code> is an <code>AssertionFailedError</code>, so JUnit and jqwik treat it as an ordinary
          failure. Its fields: <code>spec()</code>, <code>key()</code>, <code>message()</code>, <code>location()</code>,{" "}
          <code>events()</code> (each with <code>event()</code>, <code>payload()</code>, <code>src()</code>),{" "}
          <code>liveness()</code> and <code>others()</code>. Anything else is a <code>SetupError</code> whose{" "}
          <code>kind()</code> is:
        </p>
        <ErrorKinds lang="java" />
        <p>
          Both keep their artifacts folder (<code>artifacts()</code>) and the checker's report (<code>report()</code>).
          Assert on fields, not message text.
        </p>
      </section>

      <section>
        <h2 id="reference">Reference</h2>
        <h3>Settings</h3>
        <p>
          Set in <code>snappea.properties</code> (searched from the working directory upwards, then the classpath), or
          as <code>snappea.*</code> keys in <code>junit-platform.properties</code>. Paths resolve against the file's
          folder. The builders (<code>SnappeaObserver.builder()</code>, <code>SnappeaExtension.builder()</code>) have a
          method per key in camel case: <code>.specFiles(...)</code>, <code>.checkHot(...)</code>, and so on.
          Precedence: builder, environment variable, file, default. Unknown keys are an error.
        </p>
        <Code lang="toml" title="snappea.properties">{`
spec_files = specs/*.p          # paths, folders (every .p) or globs; comma-separated
specs = InventoryCorrect        # default: every spec in spec_files
strict = true                   # default: true when specs is set, else false
unknown_events = error          # ignore | warn | error
timeout = 30s
artifacts = target/snappea      # default: <tmp>/snappea; kept only for failed checks
coverage = target/coverage/monitor.json
mode = auto                     # auto | marked | off
`}</Code>
        <SettingsTable lang="java" />
        <h3>Environment variables</h3>
        <EnvTable lang="java" />
        <h3>Suite coverage</h3>
        <p>
          When the run ends, the extension prints the P spec states, handlers, statements and branches all checks reached, and
          lists what no check reached. It writes no file. Set <code>coverage</code> (or <code>SNAPPEA_COVERAGE</code>) to a
          path to also keep it as JSON; <code>SNAPPEA_COVERAGE=off</code> (or <code>coverage = off</code>) turns it off.
          Outside JUnit, <code>observer.coverage()</code> gives a collector to pass to{" "}
          <code>observer.recording(label, collector)</code>; closing it prints the summary and writes the configured path, if
          any. Collectors merge into an existing file under a lock, so forks share one path; delete it before a run.
        </p>
        <h3>API</h3>
        <ApiReference lang="java" root={root} />
        <p>
          Runnable projects: <a href={`${REPO}/tree/main/examples/java`}>Java examples</a>.
        </p>
      </section>
    </>
  );
}
