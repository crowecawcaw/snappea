# Java: inventory reservations

Three JUnit test classes check one [inventory service](src/main/java/example/Inventory.java)
against the [P spec](specs/inventory.p) with the [Java helpers](../../java/README.md),
which run the `snappea observe` CLI. See the
[shared overview](../README.md#how-the-checks-work) for what the checks do and why.

## Run

From the repository root, with JDK 17+, Maven 3.9.9 and Rust (on another platform,
replace `natives-macos-aarch64` with yours, such as `natives-linux-x86_64`):

```sh
cargo build --locked -p snappea
mkdir -p java/natives/natives-macos-aarch64
cp target/debug/snappea java/natives/natives-macos-aarch64/
SNAPPEA_BIN="$PWD/target/debug/snappea" mvn --batch-mode --no-transfer-progress -f java/pom.xml -Pnatives install
mvn --batch-mode --no-transfer-progress -f examples/java/pom.xml test
```

The example builds against this checkout's `snappea` (a SNAPSHOT version), so the first
Maven command installs it locally, with a natives jar holding the checker. The [pom](pom.xml) depends on `snappea` and, in
test scope, on the natives jar its profiles pick for the platform; the tests then need
no `SNAPPEA_BIN`. From `examples/java`, run one class with
`mvn test -Dtest=GeneratedInventoryTest`. The
[CI workflow](../../.github/workflows/examples-java.yml) runs them with Java 17.

`Inventory` calls `SnappeaEmit.emit(...)` after each successful change. Outside a test
recording that does nothing; a production deployment calls
`SnappeaEmit.setSink(Sink.file(...))` at startup and checks the file with `snappea observe`
([Testing and production](../../docs/production.md)).
[`snappea.properties`](snappea.properties) holds the spec files, spec name, artifacts folder
and coverage path for every test class.

## 1. Explicit checks in selected tests

[ExplicitInventoryTest](src/test/java/example/ExplicitInventoryTest.java) opens a
recording in try-with-resources before building the inventory; closing it checks.
The negative control uses `Recording.expectViolation`. `workerProcess` starts
[a second JVM](src/main/java/example/Worker.java) with `recording.environment()`, so
its events land in the recording's events folder, and waits for it before the check.

## 2. Automatic checks with a JUnit extension

[AutomaticInventoryTest](src/test/java/example/AutomaticInventoryTest.java) uses
`@ExtendWith(SnappeaExtension.class)`. Each test's recording opens before `@BeforeEach`
stocks products and is checked after `@AfterEach` releases the fixture's reservation,
so both are recorded.

## 3. Generated sequences with jqwik

[GeneratedInventoryTest](src/test/java/example/GeneratedInventoryTest.java) generates
60 sequences of 1–40 reserve/release operations, including invalid ones, and checks
them against a reference ledger as well as the spec.

**Gotcha: Jupiter callbacks don't wrap jqwik tries.** Create the inventory, ledger
and recording inside the `@Property` method so every try and shrink attempt is
independent. Coverage goes to `OBSERVER.coverage()`, closed in `@AfterContainer`.

jqwik prints its seed and smallest failing list, and keeps replay state in
`.jqwik-database`. Add `seed = "<reported seed>"` to `@Property` to reproduce one.

## Coverage

The extension and the jqwik collector both merge into `target/coverage/monitor.json`
(from `snappea.properties`) and print a summary, with any spec code no test reached:

```
Coverage:
  InventoryCorrect: 12/12 statements, 0/0 branches, 3/3 event handlers
Spec coverage JSON: .../examples/java/target/coverage/monitor.json
```

Delete the file before a fresh run.

## Adapt to your project

Copy `snappea.properties` and point `spec_files` and `specs` at your specs. Emit from the
code that commits the change. Failed checks keep events, reports and replay logs
under `target/snappea`. See the [Java guide](https://snappea.dev/docs/java.html) for modes,
per-test options, other processes and production sinks.
