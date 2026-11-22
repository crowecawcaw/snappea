// Docs: installing the snappea command line tool, and what each command is for. Exact
// options, defaults and exit statuses live in `--help` and docs/checking-models.md.
import { REPO, links } from "../SiteHeader";
import { Code } from "./Code";

const BLOB = `${REPO}/blob/main`;
export const INSTALL_SH = "https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.sh";
export const INSTALL_PS1 = "https://raw.githubusercontent.com/crowecawcaw/snappea/main/scripts/install.ps1";

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
curl -fsSL ${INSTALL_SH} | sh -s -- --version 0.2.0 --to /usr/local/bin
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
ok    tcAtomicReserve (proved, ...)
FAIL  tcCheckThenReserve (bug found after ...)
      Assertion Failed: PSpec/inventory.p:27:7 reservation exceeds available stock
`}</Code>
        <p>
          The reference has the <a href={`${BLOB}/docs/checking-models.md#errors-and-output`}>whole output</a>, with
          coverage and the replay command.
        </p>
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
        <h2 id="exit-status">Exit status and output</h2>
        <p>
          Every command exits with 0 when nothing failed and non-zero otherwise, so it works as a CI step or a
          pre-commit hook as it is. <a href={`${BLOB}/docs/checking-models.md#what-fails-a-check`}>What fails a
          check</a> lists each exit status, and every command's <code>--help</code> states its own.
        </p>
        <p>
          Every command prints text by default, for people and agents alike. <code>--json</code> or{" "}
          <code>SNAPPEA_FORMAT=json</code> prints the JSON result instead, and <code>--json-report FILE</code> also
          writes it to a file.
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
          Search is automatic: small models are searched exhaustively and can be proved; larger ones are sampled with
          several schedulers. Each passing test case gets a verdict: <code>proved</code>, <code>search settled</code>,
          or <code>still finding new behavior</code>, which means it needs more time or a smaller test. Without{" "}
          <code>--timeout</code> the test cases share a short time budget; set <code>--timeout</code> before trusting a
          pass on a large model.
        </p>
        <p>
          When every test case passes, <code>check</code> then runs them against mutants: copies of the model with
          one small change, such as a removed <code>send</code> or an <code>if</code> forced one way. A change no test
          case detects fails the check and is listed with its line: the code ran, but no spec or assertion depends on
          what it does. Add a spec, an assertion or a test case that would fail with the change, or suppress it with a
          reason. See <a href={`${BLOB}/docs/checking-models.md#mutation-testing`}>mutation testing</a>.
        </p>
        <p>
          <code>snappea check lint</code> runs only the model lints, without exploring schedules.{" "}
          <code>snappea check mutation</code> runs only mutation testing, with more time, e.g. as a CI gate.
        </p>
        <p>
          A failing test case writes its <code>bug.txt</code> (read this first), <code>bug.schedule</code> for{" "}
          <code>snappea replay</code> and <code>bug.trace.json</code> to <code>PCheckerOutput/&lt;TEST&gt;/</code>.{" "}
          <code>snappea check --help</code> lists every option and default; the{" "}
          <a href={`${BLOB}/docs/checking-models.md`}>checking reference</a> states the exact behavior.
        </p>
      </section>

      <section>
        <h2 id="replay">snappea replay</h2>
        <Code lang="text">{`
snappea replay <FILE.schedule> [SOURCE...] [--test <NAME>] [-v]
`}</Code>
        <p>
          Runs a schedule written by <code>snappea check</code> again, so you can debug it (<code>-v</code> logs
          each step) and, after fixing the model or the design, confirm the bug is gone. Exit status 1 means the bug
          was reproduced. Copy the command from the <code>Replay:</code> line that <code>check</code> prints.
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
          no event ran. The <a href={`${BLOB}/docs/observe.md`}>observe reference</a> covers its options, log formats
          and live streams.
        </p>
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
