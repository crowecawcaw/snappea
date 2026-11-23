# Java helpers

Check the events your Java code emits against P specs, in tests and in production.
Java 17+. The **[Java guide](https://snappea.dev/docs/java.html)** covers install (Maven and Gradle),
the JUnit 5 extension, explicit checks, emitting, other processes, failures and every
setting; the API reference is on [javadoc.io](https://javadoc.io/doc/io.github.crowecawcaw/snappea).
The helpers run the `snappea observe` CLI; the rules every language helper follows are in
[docs/language-support.md](../docs/language-support.md).

One artifact, `io.github.crowecawcaw:snappea`, holds both halves:

| Package | Use it for | Needs |
| --- | --- | --- |
| `io.github.crowecawcaw.snappea.emit` | `SnappeaEmit.emit(...)` in application code; production sinks | the JDK only |
| `io.github.crowecawcaw.snappea` | Recordings and checks from any test framework; the JUnit 5 extension | opentest4j and the JUnit Jupiter API, from your test framework |

`snappea` has no runtime dependencies: JUnit and opentest4j are optional, so production
code depending on `snappea` pulls nothing else and loads no test classes. The `snappea` checker comes in a classifier
jar of the same artifact, `natives-<os>-<arch>`: `natives-linux-x86_64`,
`natives-linux-aarch64`, `natives-macos-x86_64`, `natives-macos-aarch64`,
`natives-windows-x86_64`.

## Example

```java
@ExtendWith(SnappeaExtension.class)          // spec_files from snappea.properties
class InventoryTest {
    @Test
    void reserve() {
        new Inventory().reserve("coffee", "order-1", 2);   // emits with SnappeaEmit.emit
    }                                       // the events are checked after each test
}
```

[Install snippets](https://snappea.dev/docs/java.html#install) cover the `snappea` dependency and its
natives jar.

## From this repository

To use a checkout instead of a release (version `X.Y.Z-SNAPSHOT` in
[pom.xml](pom.xml)): from the repository root, with JDK 17+, Maven 3.9 and Rust, build
the checker and install `snappea` with a natives jar for your platform (here macOS on
Apple silicon):

```sh
cargo build --locked -p snappea
mkdir -p java/natives/natives-macos-aarch64
cp target/debug/snappea java/natives/natives-macos-aarch64/
SNAPPEA_BIN="$PWD/target/debug/snappea" mvn -f java/pom.xml -Pnatives install
```

The `natives` profile packages every `java/natives/<classifier>/snappea` (or `snappea.exe`) it
finds, or the folders under `-Dsnappea.natives=DIR`, as classifier jars. Or skip the
natives jar and set `SNAPPEA_BIN`, or put `snappea` on `PATH` with `cargo install --path crates/snappea`.

## Upgrading from 0.1

- `snappea-emit`, `snappea-test-core` and `snappea-junit5` are one artifact, `snappea`; packages and
  class names are unchanged. Add the natives jar to drop `SNAPPEA_BIN`.
- `specs` is optional (default: every spec), so `strict` defaults to false without
  named specs; it stays true with them.
- Undeclared event names now fail the check (`unknownEvents`, default `ERROR`).
- Catch `SpecViolation` / `SetupError` instead of `ObservationFailure`.
- Builders throw `SetupError` (kind `config`) for bad settings.
- Artifacts default to `<tmp>/snappea`; set `artifacts` to keep them under `target/`.
- Timestamps are ISO-8601 with nanoseconds, and lines carry `src`, `seq` and `pid`.
- Coverage merges into an existing file instead of replacing it.
