// Docs: Snap Pea for people who already know P well.
import { REPO, links } from "../SiteHeader";
import { observeTotals } from "./parityData";
import { Code } from "./Code";

const BLOB = `${REPO}/blob/main`;
const P_DOCS = "https://p-org.github.io/P/";

export function ComingFromP({ root }: { root: string }) {
  const l = links(root);
  return (
    <>
      <section className="hero">
        <p className="eyebrow">Docs</p>
        <h1>Coming from P</h1>
        <p className="subtitle">The same language and the same results, with a different toolchain underneath.</p>
        <p>
          Snap Pea is another implementation of the <a href="https://github.com/p-org/P">P</a> toolchain, written in
          Rust. Your models, test cases and specs stay plain P, and checks produce the bug reports, traces and output
          folders you already know. Upstream P's language design, test suites, tutorials and golden traces are what
          Snap Pea is built and validated against, and they're the reason it can aim for exact compatibility at all.
        </p>
        <p>
          The goal is to put P in more places: in CI on every change, inside application test suites in several
          languages, and in the browser.
        </p>
      </section>

      <section>
        <h2>At a glance</h2>
        <table className="compare">
          <thead>
            <tr>
              <th />
              <th>P</th>
              <th>Snap Pea</th>
            </tr>
          </thead>
          <tbody>
            <tr>
              <th>Checker</th>
              <td>compiles models to C# and runs them on .NET</td>
              <td>interprets the typed model directly</td>
            </tr>
            <tr>
              <th>Workflow</th>
              <td>
                <code>p compile</code>, then <code>p check</code>
              </td>
              <td>
                <code>snappea check</code> reads <code>.pproj</code> or <code>.p</code> files
              </td>
            </tr>
            <tr>
              <th>Command line</th>
              <td>
                <code>p</code>
              </td>
              <td>
                <code>snappea-compat</code> (same options and output), or <code>snappea</code>
              </td>
            </tr>
            <tr>
              <th>Foreign code</th>
              <td>C#, Java</td>
              <td>Rust</td>
            </tr>
            <tr>
              <th>Checking logs</th>
              <td>PObserve, with compiled jars</td>
              <td>
                <code>snappea observe</code>, with specs read from source
              </td>
            </tr>
            <tr>
              <th>PVerifier</th>
              <td>yes</td>
              <td>not yet</td>
            </tr>
          </tbody>
        </table>
      </section>

      <section>
        <h2>Commands</h2>
        <p>
          <code>snappea-compat</code> is a drop-in replacement for <code>p</code>. Alias it and existing scripts keep working:
        </p>
        <Code lang="shell">{`
$ alias p=snappea-compat
$ p check -tc tcMyTest -s 1000          # no p compile first
$ p check -r PCheckerOutput/BugFinding/MyProject_0_0.schedule
`}</Code>
        <p>
          <code>snappea</code> is the same checker with shorter options, concise output and one folder per test case. It
          finds the project the way <code>p</code> does:
        </p>
        <Code lang="shell">{`
$ snappea check tcMyTest                    # one test case, up to ten seconds
$ snappea check 'tcMy*' --timeout 5m           # a glob, five minutes each
$ snappea replay PCheckerOutput/tcMyTest/bug.schedule
$ snappea observe --spec BankBalanceCorrect logs/
`}</Code>
      </section>

      <section>
        <h2>What stays the same</h2>
        <p>
          These match upstream exactly in <code>snappea-compat</code>. <code>snappea</code> has its own options, console output
          and output folder (<code>PCheckerOutput/&lt;test&gt;/bug.txt</code>, <code>bug.schedule</code>,{" "}
          <code>bug.trace.json</code>, replaced by each run), resolves <code>&lt;IncludeProject&gt;</code> paths against
          the including project file first, and its <code>observe</code> writes no report folder.
        </p>
        <ul>
          <li>
            <code>check</code>, <code>merge-scenario-coverage</code> and <code>observe</code> options, help, console
            output and exit codes.
          </li>
          <li>Parse and type error messages; bug reports, assertion and liveness messages.</li>
          <li>
            Trace files (<code>.txt</code>, <code>.trace.json</code>, <code>.schedule</code>), coverage reports, the
            output folder layout and history rotation.
          </li>
          <li>
            <code>.pproj</code> files, and PObserve's output files.
          </li>
          <li>
            With one worker (<code>-j 1</code>) and the same seed, the schedules upstream explores.
          </li>
        </ul>
      </section>

      <section>
        <h2>What differs</h2>
        <ul>
          <li>
            There's no compile step. <code>check</code> reads P sources, not a compiled <code>.dll</code>, and{" "}
            <code>OutputDir</code> and <code>Target</code> in <code>.pproj</code> files are ignored.
          </li>
          <li>
            Foreign functions and log parsers are written in Rust, not C# or Java. <code>snappea check</code> rejects
            a project that lists <code>.cs</code> or <code>.java</code> files, since it cannot run them; pass{" "}
            <code>--ignore-foreign-files</code> to check the P model anyway. <code>snappea-compat</code> only warns.
          </li>
          <li>
            <code>snappea check</code> runs one worker per CPU core by default. Each worker has its own seed, so a given{" "}
            <code>--seed</code> explores different schedules from upstream's, and which bug is reported can vary between
            runs.
          </li>
          <li>Findings may be reported in a different order, and elapsed time and memory figures differ.</li>
          <li>An action that loops forever without reaching a scheduling point is reported as a bug.</li>
          <li>
            Not supported yet: PVerifier, PObserve's <code>--filters</code>, socket mode and{" "}
            <code>SERIALIZEDEVENTS</code> input. Out of scope: <code>p compile</code> output, and foreign code in C# or
            Java.
          </li>
        </ul>
        <p>
          The <a href={`${BLOB}/docs/compatibility.md`}>compatibility reference</a> has every detail.
        </p>
      </section>

      <section>
        <h2>What Snap Pea adds</h2>
        <dl className="faq">
          <div>
            <dt>Parallel workers</dt>
            <dd>
              <code>-j N</code> splits the schedules across workers. The first bug stops them all, and its{" "}
              <code>.schedule</code> file replays as usual.
            </dd>
          </div>
          <div>
            <dt>Exhaustive search</dt>
            <dd>
              Exhaustive exploration runs automatically alongside sampled search, skipping states it has already
              seen. Completed exhaustive searches end early; searches that reach the timeout remain incomplete.
            </dd>
          </div>
          <div>
            <dt>Time</dt>
            <dd>
              <code>--latency</code> and <code>--timed</code> check timeouts and latency budgets with model time, using
              foreign functions and no new syntax. See the{" "}
              <a href={`${BLOB}/design/time/RetryBudget/README.md`}>RetryBudget example</a>.
            </dd>
          </div>
          <div>
            <dt>Specs in application tests</dt>
            <dd>
              Helpers for Python, Rust, Java and Go record events from ordinary tests and check them against your specs,
              with monitor coverage. See the <a href={`${BLOB}/examples/README.md`}>integration examples</a>.
            </dd>
          </div>
          <div>
            <dt>More from observe</dt>
            <dd>
              Several specs per run, <code>--all-specs</code>, <code>--check-hot</code>, <code>--json-report</code>,
              spec coverage, <code>--strict</code> and live input (<code>--live</code>). In{" "}
              <code>snappea-compat observe</code>, runs that use none of them behave as upstream. See{" "}
              <a href={`${BLOB}/docs/observe.md`}>snappea observe</a>.
            </dd>
          </div>
          <div>
            <dt>In the browser</dt>
            <dd>
              The checker builds to WebAssembly, which runs the <a href={l.playground}>Playground</a> with nothing to
              install.
            </dd>
          </div>
        </dl>
      </section>

      <section>
        <h2>How compatibility is tested</h2>
        <p>Upstream's own test suites are vendored in the repository and checked against:</p>
        <ul>
          <li>All 424 compiler regression tests compile, or fail to, as upstream expects.</li>
          <li>All 249 checker tests find a bug exactly when upstream expects one, across seeds.</li>
          <li>The tutorials give the same results as upstream's tutorial CI.</li>
          <li>Readable and JSON traces match upstream's golden files.</li>
          <li>
            <code>snappea-compat observe</code> and PObserve are compared in CI on {observeTotals.runs} vendored runs.
            {" "}{observeTotals.identical} match after normalization; the remaining runs check only exit codes
            because of a null-dereference message difference. The separate manual comparison of the full upstream
            suite has 192 runs, with 186 outputs identical after normalization.
          </li>
        </ul>
        <p>
          The <a href={l.parity}>upstream parity page</a> has the measured corpus, normalization rules,
          known differences and reproduction commands. If Snap Pea disagrees with upstream on a model, that's a Snap Pea bug.{" "}
          <a href={`${REPO}/issues`}>Please report it.</a> <a href={`${BLOB}/docs/development.md#validation`}>Validation</a>{" "}
          and <a href={`${BLOB}/docs/development.md#benchmarks`}>benchmarks</a> have the details.
        </p>
      </section>

      <section className="next">
        <h2>Learning P</h2>
        <p>
          Upstream's <a href={P_DOCS}>documentation</a>, <a href={`${P_DOCS}tutsoutline/`}>tutorials</a> and{" "}
          <a href={`${P_DOCS}casestudies/`}>case studies</a> are the best way to learn the language, and every tutorial
          runs unchanged with snappea.
        </p>
      </section>
    </>
  );
}
