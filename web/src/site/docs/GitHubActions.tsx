// Docs: checking P models, and tests with P specs, in GitHub Actions.
import { REPO, links } from "../SiteHeader";
import { Code } from "./Code";
import { INSTALL_PS1, INSTALL_SH } from "./Cli";

const BLOB = `${REPO}/blob/main`;

export function GitHubActions({ root }: { root: string }) {
  const l = links(root);
  return (
    <>
      <section className="hero">
        <p className="eyebrow">Docs</p>
        <h1>GitHub Actions</h1>
        <p className="subtitle">Check your P model on every pull request, so a design change can't quietly break it.</p>
        <p>
          A model that is checked once and then forgotten drifts from the design. Run it in CI like any other test: a
          job installs <code>snappea</code> in about a second, runs the model's test cases, and fails the pull request
          when one finds a bug. This works the same for a change to the model made by a person or by an agent.
        </p>
      </section>

      <section>
        <h2 id="workflow">A workflow</h2>
        <p>
          Save this as <code>.github/workflows/p-model.yml</code>, with <code>working-directory</code> set to the
          folder that has your <code>.pproj</code> file (or your <code>.p</code> files):
        </p>
        <Code lang="yaml" title=".github/workflows/p-model.yml">{`
name: P model

on:
  pull_request:
  push:
    branches: [main]

permissions:
  contents: read

jobs:
  check:
    runs-on: ubuntu-latest
    timeout-minutes: 15
    defaults:
      run:
        working-directory: model
    steps:
      - uses: actions/checkout@v7

      - name: Install snappea
        run: curl -fsSL ${INSTALL_SH} | sh -s -- --version 0.1.0

      - name: Check the model
        run: snappea check --timeout 2m

      - uses: actions/upload-artifact@v7
        if: failure()
        with:
          name: snappea-output
          path: model/PCheckerOutput
`}</Code>
        <ul>
          <li>
            <b>Install.</b> The <a href={`${l.cli}#install`}>install script</a> downloads the release, checks its
            checksum, and adds <code>snappea</code> to the PATH of later steps. Pin a version so a new release can't
            change your results without a commit; leave out <code>-s -- --version ...</code> for the latest.
          </li>
          <li>
            <b>Check.</b> <code>snappea check</code> with no arguments finds the sources and checks every test case.
            The job fails on exit status 1 (a bug) or 2 (an error, such as a type error).
          </li>
          <li>
            <b>Output.</b> On a failure, the artifact has the failing schedule's log (<code>bug.txt</code>) and a{" "}
            <code>bug.schedule</code> file. Download it from the run page and run{" "}
            <code>snappea replay bug.schedule</code> locally to step through the same bug.
          </li>
        </ul>
        <p>
          The <a href={`${REPO}/tree/main/examples/model`}>example model</a> has a copy of this workflow:{" "}
          <a href={`${BLOB}/examples/model/github-workflow.yml`}>github-workflow.yml</a>. This repository runs the
          same steps with its own build of snappea in{" "}
          <a href={`${BLOB}/.github/workflows/check-model.yml`}>check-model.yml</a>.
        </p>
      </section>

      <section>
        <h2 id="budget">How long to explore</h2>
        <p>
          Search strategies are selected automatically. The default is ten seconds per test; choose a larger timeout for CI when needed. Exhaustive completion ends a check early:
        </p>
        <table className="compare ref">
          <tbody>
            <tr>
              <th>
                <code>--timeout 2m</code>
              </th>
              <td>Explore for a fixed time per test case. The simplest choice for CI.</td>
            </tr>
          </tbody>
        </table>
        <p>
          The checker uses every CPU core by default. A slower nightly job with a larger budget (
          <code>on: schedule</code>) is a good complement to a quick check on each pull request.
        </p>
      </section>

      <section>
        <h2 id="expected-bugs">Test cases that should fail</h2>
        <p>
          A model often keeps a test case for a design you rejected, to show why. Name the test cases that should pass
          (globs work: <code>snappea check 'tcSafe*'</code>), or check that the rejected one still finds its bug, so it
          keeps proving the point:
        </p>
        <Code lang="yaml">{`
      - name: The check-then-reserve design oversells
        run: |
          status=0
          snappea check tcCheckThenReserve || status=$?
          test "$status" -eq 1
`}</Code>
      </section>

      <section>
        <h2 id="matrix">Many test cases</h2>
        <p>
          To spread a large model over several runners, list the test cases with <code>snappea tests</code> and check
          one per job:
        </p>
        <Code lang="yaml">{`
jobs:
  list:
    runs-on: ubuntu-latest
    outputs:
      tests: \${{ steps.list.outputs.tests }}
    steps:
      - uses: actions/checkout@v7
      - run: curl -fsSL ${INSTALL_SH} | sh -s -- --version 0.1.0
      - id: list
        working-directory: model
        run: echo "tests=$(snappea tests | jq -R . | jq -sc .)" >> "$GITHUB_OUTPUT"

  check:
    needs: list
    strategy:
      fail-fast: false
      matrix:
        test: \${{ fromJSON(needs.list.outputs.tests) }}
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v7
      - run: curl -fsSL ${INSTALL_SH} | sh -s -- --version 0.1.0
      - working-directory: model
        run: snappea check "\${{ matrix.test }}" --timeout 5m
`}</Code>
      </section>

      <section>
        <h2 id="other-runners">macOS and Windows runners</h2>
        <p>
          The script works on <code>macos-latest</code> and, in a <code>bash</code> step, on{" "}
          <code>windows-latest</code>. For a PowerShell step on Windows:
        </p>
        <Code lang="yaml">{`
      - name: Install snappea
        shell: pwsh
        env:
          SNAPPEA_VERSION: 0.1.0
        run: irm ${INSTALL_PS1} | iex
`}</Code>
      </section>

      <section>
        <h2 id="language-tests">Specs in your language's tests</h2>
        <p>
          The language helpers check P specs inside your ordinary unit tests and bring the checker with them, so CI
          needs no install step: see <a href={l.python}>Python</a>, <a href={l.rust}>Rust</a>,{" "}
          <a href={l.go}>Go</a> and <a href={l.java}>Java</a>. Run <code>pytest</code>, <code>go test</code>, <code>cargo test</code> or your Maven or
          Gradle build as usual.
        </p>
      </section>

      <section className="next">
        <h2>More</h2>
        <p>
          Every command and option is on the <a href={l.cli}>command line</a> page.
        </p>
      </section>
    </>
  );
}
