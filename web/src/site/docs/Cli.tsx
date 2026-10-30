// Docs: installing the snappea command line tool, and its commands and options.
import { REPO, links } from "../SiteHeader";
import { Code } from "./Code";

const BLOB = `${REPO}/blob/main`;
export const INSTALL_SH = "https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.sh";
export const INSTALL_PS1 = "https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.ps1";

type Option = [flag: string, meaning: string];

function Options({ rows }: { rows: Option[] }) {
  return (
    <table className="compare ref opts">
      <tbody>
        {rows.map(([flag, meaning]) => (
          <tr key={flag}>
            <th>
              <code>{flag}</code>
            </th>
            <td>{meaning}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const CHECK_EXPLORATION: Option[] = [
  ["--timeout <DURATION>", "Time limit per test case. Default 10s; accepts seconds or units (90s, 5m, 1h)."],
  ["-j, --workers <N>", "Worker limit. Default: one per CPU core."],
  ["--seed <N>", "Seed for reproducible choices. Not needed to reproduce a bug: its .schedule replays exactly."],
];

const CHECK_OTHER: Option[] = [
  ["--latency <FILE>[:<PROFILE>]", "Latency profiles (TOML), and the profile to use."],
  ["--timed", "Time-aware scheduling: never run a step that is certainly later than another."],
  ["-o, --out <DIR>", "Output folder. Default PCheckerOutput."],
  ["-v, --verbose", "Show progress and statistics, and log every schedule."],
];

const CHECK_MUTATION: Option[] = [
  ["--mutation-timeout <DURATION>", "Time limit for mutation testing. Default 30s."],
  ["--no-mutation", "Skip mutation testing."],
];

const OBSERVE: Option[] = [
  ["--spec <NAME>", "Spec monitor to check; repeat or separate with commas."],
  ["--all-specs", "Check every spec monitor (--exclude-specs to leave some out)."],
  ["--live <PATH>, --stdin", "Read a live stream of lines until it ends, stopping at the first error."],
  ["--p-trace", "The logs are P JSON traces (bug.trace.json from snappea check)."],
  ["--parser-config <JSON>", 'Where the JsonLines parser finds its fields, e.g. {"event": "msg.type", "key": "msg.id"}.'],
  ["--key <KEY>", "Check only this key; repeat for several."],
  ["--check-hot", "Report a key whose spec is still in a hot state after its last event."],
  ["--strict", "Fail a spec that checked no relevant event."],
];

const OUTPUT: Option[] = [
  ["--format text|json", "Text or the JSON result. Default: text on a terminal or in CI (CI set), JSON otherwise."],
  ["--json", "The same as --format json."],
  ["--json-report <FILE>", "Also write the JSON result to FILE."],
];

export function Cli({ root }: { root: string }) {
  const l = links(root);
  return (
    <>
      <section className="hero">
        <p className="eyebrow">Docs</p>
        <h1>Command line</h1>
        <p className="subtitle">Install the snappea checker and run it on P models and logs.</p>
        <p>
          <code>snappea</code> is one binary with no runtime dependencies: no .NET, Java or Python. It type-checks P
          sources, explores the schedules of their test cases, and checks logs of a running system against P specs.
        </p>
      </section>

      <section>
        <h2 id="install">Install</h2>
        <p>On Linux or macOS:</p>
        <Code lang="shell">{`
curl -fsSL ${INSTALL_SH} | sh
`}</Code>
        <p>On Windows, in PowerShell:</p>
        <Code lang="shell">{`
irm ${INSTALL_PS1} | iex
`}</Code>
        <p>
          The script downloads the latest <a href={`${REPO}/releases`}>GitHub release</a> for your machine, checks it
          against the release's <code>SHA256SUMS</code>, and puts <code>snappea</code> in <code>~/.local/bin</code>{" "}
          (Windows: <code>%LOCALAPPDATA%\snappea\bin</code>, added to your user PATH). It supports Linux x86_64 and
          arm64 with glibc, macOS on Intel and Apple silicon, and Windows x86_64.
        </p>
        <p>To pin a version or pick the folder:</p>
        <Code lang="shell">{`
curl -fsSL ${INSTALL_SH} | sh -s -- --version 0.1.0 --to /usr/local/bin
`}</Code>
        <p>
          The same settings work as environment variables, <code>SNAPPEA_VERSION</code> and{" "}
          <code>SNAPPEA_INSTALL_DIR</code>, which is how the PowerShell script takes them. Check the install with{" "}
          <code>snappea version</code>.
        </p>
        <h3>Other ways to install</h3>
        <table className="compare ref">
          <tbody>
            <tr>
              <th>Release archive</th>
              <td>
                Download <code>snappea-v&lt;version&gt;-&lt;target&gt;.tar.gz</code> (<code>.zip</code> on Windows) from
                a <a href={`${REPO}/releases`}>release</a> and put the binary on your PATH.
              </td>
            </tr>
            <tr>
              <th>Cargo</th>
              <td>
                <code>cargo install snappea</code> builds <code>snappea</code> and <code>snappea-compat</code> from
                source, for any platform Rust supports (including Alpine and other musl systems).
              </td>
            </tr>
            <tr>
              <th>Python</th>
              <td>
                <code>pip install pysnappea</code> brings the binary with the <a href={l.python}>Python helper</a>.
              </td>
            </tr>
            <tr>
              <th>CI</th>
              <td>
                See <a href={l.githubActions}>GitHub Actions</a>.
              </td>
            </tr>
          </tbody>
        </table>
      </section>

      <section>
        <h2 id="quick-start">Quick start</h2>
        <p>
          The repository's <a href={`${REPO}/tree/main/examples/model`}>example model</a> is an inventory service with
          two designs for reserving stock. One is safe; the other lets two orders oversell:
        </p>
        <Code lang="shell">{`
git clone https://github.com/crowecawcaw/snappea
cd snappea/examples/model
snappea tests
snappea check tcAtomicReserve
snappea check tcCheckThenReserve
`}</Code>
        <p>
          <code>tests</code> lists the two test cases. The safe design passes every possible ordering, and the other
          fails within a few schedules, with exit status 1:
        </p>
        <Code lang="text">{`
Checking Inventory: 1 test case
ok    tcAtomicReserve (all 20277 schedules, 0.58s)
The test case passed in 0.58s, peak memory 6 MB. Output: PCheckerOutput/

Checking Inventory: 1 test case
FAIL  tcCheckThenReserve (bug found after 3 schedules, 4 workers, 0.00s)
      Assertion Failed: PSpec/inventory.p:27:7 reservation exceeds available stock
      Replay: snappea replay PCheckerOutput/tcCheckThenReserve/bug.schedule
1 of 1 test case failed in 0.00s, peak memory 6 MB. Output: PCheckerOutput/
`}</Code>
        <p>
          <code>PCheckerOutput/tcCheckThenReserve/bug.txt</code> shows the two orders both reading 3 units of stock
          before either reserves.
        </p>
        <p>
          You can also try models in the browser, with nothing installed, in the <a href={l.playground}>playground</a>.
        </p>
      </section>

      <section>
        <h2 id="sources">Sources</h2>
        <p>
          Commands that read P take <code>.pproj</code> or <code>.p</code> files as arguments. With none, they use the{" "}
          <code>*.pproj</code> file in the current folder, or else every <code>*.p</code> file in it and its immediate
          subfolders. So in a project folder, <code>snappea check</code> is usually enough. There is no separate
          compile step.
        </p>
      </section>

      <section>
        <h2 id="exit-status">Exit status</h2>
        <table className="compare ref">
          <tbody>
            <tr>
              <th>0</th>
              <td>No bug found.</td>
            </tr>
            <tr>
              <th>1</th>
              <td>A bug (or, for observe, a spec violation) was found.</td>
            </tr>
            <tr>
              <th>2</th>
              <td>Any other error: a type error in the sources, a missing file, a bad option.</td>
            </tr>
          </tbody>
        </table>
        <p>This makes every command usable as a CI step or a pre-commit hook as it is.</p>
        <h3 id="output">Text and JSON</h3>
        <p>
          Every command prints text for people on a terminal or in CI, and a JSON result for agents and scripts
          otherwise. <code>SNAPPEA_FORMAT=text</code> or <code>json</code> sets the default; these options choose per
          run:
        </p>
        <Options rows={OUTPUT} />
        <p>
          Output options work before or after the command. Unknown options and missing values also produce a JSON
          error result and write <code>--json-report</code> when requested. JSON keys retain insertion order.
        </p>
      </section>

      <section>
        <h2 id="check">snappea check</h2>
        <Code lang="text">{`
snappea check [SOURCE...] [TEST...] [options]
`}</Code>
        <p>
          Checks test cases by exploring schedules: orderings of messages between machines, and the choices the model
          leaves open. A <code>TEST</code> is a test case name or a glob (<code>tc*</code>); with none, every test
          case is checked. It prints one line per test case with its coverage and, for a bug, the failing assertion
          and a <code>snappea replay</code> command. Then it lists the code no test case ran: handlers never
          triggered, <code>if</code> conditions never true or never false, and statements never reached.
        </p>
        <p>
          Coverage from a test case stopped at its first bug is marked partial in the console and with
          <code>coverage.partial: true</code> in JSON, including the combined total. Scenario coverage appears
          as a console summary and as <code>scenarios</code> artifacts in suite and test case JSON results.
          {" "}The <a href={links(root).coverage}>coverage guide</a> explains condition outcomes,
          transition pairs, and components excluded from the selected tests.
        </p>
        <p>
          Search is automatic: small exhaustive searches can finish early; larger searches also try guided
          scheduling, deliberate actor delays, and priority scheduling. Competing sends and behavioral progress
          guide the allocation, while each scheduler keeps an exploration share. A timeout or coverage plateau
          does not establish that the model is correct. The default timeout is fixed at ten seconds per test.
        </p>
        <p>
          When every test case passes, <code>check</code> then runs them against mutants: copies of the model with
          one small change, such as a removed statement, an <code>if</code> forced one way, or <code>&lt;</code>{" "}
          swapped for <code>&lt;=</code>. A change no test case detects is listed with its line: the code ran, but no
          spec or assertion depends on what it does. Add a spec, an assertion or a test case that would fail with the
          change, or suppress it with <code>// snappea-lint: allow mutation[:kind] -- reason</code> on the line before.
          Specs and test drivers are not mutated, and undetected mutants do not change the exit status. JSON results
          list them under <code>mutation.undetected</code>. Changes share the time limit round robin, so a run cut
          short gives each about the same number of schedules.
        </p>
        <p>
          <code>snappea check lint [SOURCE...]</code> runs only the model lints, without exploring schedules.{" "}
          <code>snappea check mutation [SOURCE...] [TEST...]</code> runs only mutation testing, with a 5 minute
          limit by default, and exits with 1 when a change goes undetected, so CI can require every change to be
          detected or suppressed.
        </p>
        <h3>How much to explore</h3>
        <Options rows={CHECK_EXPLORATION} />
        <h3>Mutation testing</h3>
        <Options rows={CHECK_MUTATION} />
        <h3>Time and output</h3>
        <Options rows={CHECK_OTHER} />
        <h3>Output files</h3>
        <p>
          A test case that finds a bug writes to <code>&lt;DIR&gt;/&lt;TEST&gt;/</code>, replacing the previous
          run's files: <code>bug.txt</code> (the log of the failing schedule), <code>bug.schedule</code> and{" "}
          <code>bug.trace.json</code>. Scenario monitors also write <code>scenario_coverage.json</code> there.
        </p>
      </section>

      <section>
        <h2 id="replay">snappea replay</h2>
        <Code lang="text">{`
snappea replay <FILE.schedule> [SOURCE...] [--test <NAME>] [-v]
`}</Code>
        <p>
          Runs a schedule written by <code>snappea check</code> again, so you can fix the model or the design and
          confirm the bug is gone. Exit status 1 means the bug was reproduced. If the schedule was found with step
          options (<code>--max-steps</code> and the others), pass the same ones; the replay line that{" "}
          <code>check</code> prints includes them.
        </p>
      </section>

      <section>
        <h2 id="tests">snappea tests</h2>
        <Code lang="text">{`
snappea tests [SOURCE...]
`}</Code>
        <p>Type-checks the sources and lists their test cases, one per line. A fast first step in CI.</p>
      </section>

      <section>
        <h2 id="observe">snappea observe</h2>
        <Code lang="text">{`
snappea observe [SOURCE...] [LOG...] --spec <NAME> [options]
`}</Code>
        <p>
          Checks logs of a running system against P spec monitors. A <code>LOG</code> is a file, a folder of logs, or a
          quoted glob (<code>"logs/**/*.jsonl"</code>). By default each line is one JSON event. It prints one line per
          spec with its coverage (the spec's handlers, statements and branches the events ran), and lists the spec code
          no event ran. The <a href={`${BLOB}/docs/observe.md`}>observe guide</a> covers log formats and live streams.
        </p>
        <Options rows={OBSERVE} />
      </section>

      <section>
        <h2 id="coverage">snappea coverage</h2>
        <Code lang="text">{`
snappea coverage merge [DIR]
snappea coverage merge-observe [RESULT.json ...] [--stdin]
`}</Code>
        <p>
          <code>merge</code> combines the scenario coverage reports under a folder (for example, from several CI jobs)
          into one. <code>merge-observe</code> merges the coverage of <code>snappea observe</code> JSON results, without
          the P sources.
        </p>
      </section>

      <section className="next">
        <h2>More</h2>
        <p>
          Every command has <code>--help</code>. If you have scripts written for upstream P's <code>p check</code>,{" "}
          <code>snappea-compat</code> takes the same options and prints the same output; see{" "}
          <a href={l.comingFromP}>Coming from P</a>. To check P in CI, see <a href={l.githubActions}>GitHub Actions</a>.
        </p>
      </section>
    </>
  );
}
