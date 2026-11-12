# Releasing Snap Pea

Releases are cut by the manually triggered **Release** workflow
(`.github/workflows/release.yml`). Nothing publishes on a tag push. One run releases
every package at one version:

| Package | Where |
| --- | --- |
| crates `snappea-syntax`, `snappea-lint`, and `snappea` | crates.io |
| `pysnappea` wheels (with `snappea`) and sdist | PyPI, and the GitHub release |
| `io.github.crowecawcaw:snappea` and its `natives-<os>-<arch>` jars | Maven Central |
| module `github.com/crowecawcaw/snappea` | the `vX.Y.Z` tag (Go needs nothing else) |
| `snappea-vX.Y.Z-<target>.tar.gz` / `.zip` and `SHA256SUMS` | the GitHub release |

`crates/snappea-wasm` (the playground's WebAssembly build) is not published.

## Cut a release

Actions → **Release** → Run workflow, on `main`, and pick a level:

| level   | effect                                        |
| ------- | --------------------------------------------- |
| patch   | 0.1.0 → 0.1.1                                  |
| minor   | 0.1.0 → 0.2.0                                  |
| major   | 0.1.0 → 1.0.0                                  |
| release | publish the version already in `Cargo.toml`    |

The version is `[workspace.package] version` in the root `Cargo.toml`.
`scripts/release.py` computes it and writes it into the other packages.

Or from a terminal: `gh workflow run release.yml --ref main -f level=patch`.

The workflow:

1. **Preflight**: fmt, clippy, tests and `cargo publish --dry-run` on `main`.
2. **Version**: the next version, and whether its tag already exists.
3. **Build**: `snappea` for `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`
   (both glibc 2.17+, via cargo-zigbuild), `x86_64-apple-darwin`,
   `aarch64-apple-darwin` and `x86_64-pc-windows-msvc` (static CRT), from the run's
   commit with the version applied, so `snappea --version` reports the release. Each is
   archived as `snappea-vX.Y.Z-<target>.tar.gz` (Windows: `.zip` with `snappea.exe`).
   The three targets that run on their runner (x86_64 Linux, Apple silicon, Windows)
   use profile-guided optimization, about 10-15% faster checking: an instrumented build
   checks the tutorial models for about 30 seconds (`scripts/pgo.py`), and the release
   build uses that profile. The two cross-built targets ship without it.
4. **Commit and tag**: writes `SHA256SUMS` for those exact archives, then commits
   `Release Snap Pea vX.Y.Z` with every package at the version: `Cargo.toml` and the lock
   files, `java/pom.xml` (and `examples/java`), and Go's
   `go/internal/snappeabin/version.go` and `SHA256SUMS` (checked by
   `go test ./go/internal/snappeabin`). It tags that commit `vX.Y.Z`, then commits
   `Release Snap Pea vX.Y.Z: back to development`, and pushes both commits and the tag
   atomically. If `main` moved since the run started, the push fails and nothing is
   published.
5. In parallel, from the tag:
   - **crates.io**: publishes `snappea-syntax`, `snappea-lint`, and `snappea` in dependency order.
   - **GitHub release**: created at once with the archives and `SHA256SUMS` (Go's
     `snappeatest` downloads them, verified against the checksums in the module), then
     Claude replaces GitHub's generated notes with notes written from the merged PRs,
     following `.github/release-notes-prompt.md`. If that fails (for example an
     expired token), the generated notes stay and the run still succeeds.
   - **Python** (after the release exists): wheels with the `snappea` binary for the same
     five platforms, plus an sdist, built from the tag, attached to the release and
     published to PyPI.
   - **Maven Central**: natives jars made from the archives' binaries, the Java tests
     against the Linux binary, then a signed upload through the Central Portal that
     waits until the version is published. Skipped with a notice until the Maven
     secrets below exist.

### Versions between releases

The tagged commit has the release version everywhere. Afterwards `main` keeps it in
`Cargo.toml` (as before: the next release bumps it), while:

- Go goes back to `Version = "dev"` with an empty `SHA256SUMS`, so a checkout or
  pseudo-version never downloads a binary built from other sources.
- Java goes to the next patch `-SNAPSHOT` (0.1.1 → 0.1.2-SNAPSHOT), so `mvn install`
  from a checkout never overwrites the released version in `~/.m2`.

`python3 scripts/release.py set X.Y.Z` and `python3 scripts/release.py dev` make these
edits; the workflow is the only caller in normal use.

### Re-runs

Every publish step skips a version that is already there (crates.io, PyPI, Maven
Central, existing release assets), so a failed run can be finished:

- **Re-run failed jobs** of the same run. It reuses the run's archives, so this is the
  way to recover when the tag was pushed but the GitHub release lacks its assets.
- Or start a new run with `level=release`. When the tag exists, the build jobs
  download the archives from its GitHub release and check them against the tag's
  committed `SHA256SUMS` instead of rebuilding (a rebuilt archive would not match), and
  nothing is committed.

## Fix a release's notes

Actions → **Update release notes** → enter the tag. It regenerates the notes with
Claude and replaces them on the release. Tick *dry run* to only print them.

## Requirements

- The `github-actions[bot]` must be able to push to `main`. If `main` gets branch
  protection, allow GitHub Actions to bypass it (or the commit step will fail).
- crates.io trusted publishing for each Rust crate (`snappea-syntax`, `snappea-lint`,
  and `snappea`) must trust:
  - Repository: `crowecawcaw/snappea`
  - Workflow: `release.yml`
  - Environment: `release`

  The `release` environment must allow the `main` branch. No crates.io token is
  stored in GitHub; the auth action gets a short-lived token and revokes it after.
  Configure publishers for the new crates before their first release, including any
  initial crate registration required by crates.io. The existing `snappea` crate
  requires trusted publishing for new versions, so API tokens are rejected.
- PyPI trusted publishing for `pysnappea` (set up once on pypi.org, as a pending
  publisher before the first release) trusts:
  - Repository: `crowecawcaw/snappea`
  - Workflow: `release.yml` (the upload runs there, not in the reusable
    `python.yml`: PyPI checks attestations against the top-level workflow)
  - Environment: `pypi`

  Create the `pypi` environment in the repository settings.
- The `CLAUDE_CODE_OAUTH_TOKEN` repository secret, for release notes. Create it
  with `claude setup-token`; it lasts about a year.
- Maven Central (one-time; until it is done, releases skip Maven Central):
  1. Sign in at <https://central.sonatype.com> with the `crowecawcaw` GitHub
     account. Signing in with GitHub verifies the namespace `io.github.crowecawcaw`
     (check it under **Namespaces**; otherwise add it there and follow the
     verification steps, which create a temporary public repository).
  2. **Account** → **Generate User Token**. The token's username and password become
     the `CENTRAL_TOKEN_USERNAME` and `CENTRAL_TOKEN_PASSWORD` secrets. They are not
     the account's login.
  3. Create a signing key and publish its public half, which Central uses to check
     the signatures:

     ```sh
     gpg --quick-gen-key "Snap Pea releases <you@example.com>" ed25519 sign 3y
     gpg --list-secret-keys --keyid-format long          # note the key id
     gpg --keyserver keyserver.ubuntu.com --send-keys KEYID
     gpg --keyserver keys.openpgp.org --send-keys KEYID  # then confirm the email it sends
     gpg --armor --export-secret-keys KEYID              # the MAVEN_GPG_PRIVATE_KEY secret
     ```

  4. Add four repository secrets (Settings → Secrets and variables → Actions):
     `MAVEN_GPG_PRIVATE_KEY` (the armored private key, including the BEGIN/END
     lines), `MAVEN_GPG_PASSPHRASE`, `CENTRAL_TOKEN_USERNAME`, `CENTRAL_TOKEN_PASSWORD`.

  If a release already ran without them, re-run its **Publish to Maven Central** job
  (or start a new run with `level=release`) to publish that version.
