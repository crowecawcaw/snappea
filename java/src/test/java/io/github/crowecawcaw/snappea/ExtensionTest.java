package io.github.crowecawcaw.snappea;

import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Order;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.extension.BeforeEachCallback;
import org.junit.jupiter.api.extension.RegisterExtension;
import org.junit.jupiter.api.io.TempDir;
import org.junit.platform.launcher.core.LauncherDiscoveryRequestBuilder;
import org.junit.platform.launcher.core.LauncherFactory;
import org.junit.platform.launcher.listeners.SummaryGeneratingListener;
import org.junit.platform.launcher.listeners.TestExecutionSummary;
import static org.junit.jupiter.api.Assertions.*;
import static org.junit.platform.engine.discovery.DiscoverySelectors.selectClass;

class ExtensionTest {
    @TempDir Path temporary;
    private static Path root;
    private static CountDownLatch simultaneous;

    private static SnappeaExtension.Builder builder(String... specs) {
        return SnappeaExtension.builder().specFiles(Path.of("src/test/resources/spec.p"))
                .specs(specs).artifacts(root.resolve("snappea"));
    }

    private TestExecutionSummary run(boolean parallel, Class<?>... classes) {
        return run(parallel, false, classes);
    }

    private TestExecutionSummary run(boolean parallel, boolean autodetect, Class<?>... classes) {
        root = temporary;
        var request = LauncherDiscoveryRequestBuilder.request();
        request.configurationParameter("junit.jupiter.extensions.autodetection.enabled", Boolean.toString(autodetect));
        for (var type : classes) request.selectors(selectClass(type));
        request.configurationParameter("junit.jupiter.execution.parallel.enabled", Boolean.toString(parallel));
        request.configurationParameter("junit.jupiter.execution.parallel.mode.default", "concurrent");
        request.configurationParameter("junit.jupiter.execution.parallel.mode.classes.default", "concurrent");
        request.configurationParameter("junit.jupiter.execution.parallel.config.strategy", "fixed");
        request.configurationParameter("junit.jupiter.execution.parallel.config.fixed.parallelism", "4");
        var listener = new SummaryGeneratingListener();
        LauncherFactory.create().execute(request.build(), listener);
        return listener.getSummary();
    }

    @Test void capturesFixturesAndParallelInvocationsWithOneSuiteUnion() throws Exception {
        simultaneous = new CountDownLatch(2);
        var summary = run(true, ParallelFixture.class, AnotherClassFixture.class);
        assertEquals(3, summary.getTestsSucceededCount(), summary.getFailures().toString());
        assertEquals(0, summary.getTotalFailureCount(), summary.getFailures().toString());
        var json = Files.readString(temporary.resolve("coverage/monitor.json"));
        assertTrue(json.contains("observed_monitor_suite"));
        assertTrue(json.contains("first("));
        assertTrue(json.contains("second("));
        assertTrue(json.contains("another("));
    }

    @Test void primaryBodyFailureKeepsMonitorFailureSuppressedAndCoverage() throws Exception {
        var summary = run(false, BodyFailureFixture.class);
        var error = summary.getFailures().get(0).getException();
        assertEquals("body failed", error.getMessage());
        assertSuppressedObservation(error);
        assertTrue(Files.readString(temporary.resolve("failed-coverage.json")).contains("spec_violation"));
    }

    @Test void setupFailureKeepsObservationSuppressed() {
        var summary = run(false, SetupFailureFixture.class);
        var error = summary.getFailures().get(0).getException();
        assertEquals("setup failed", error.getMessage());
        assertSuppressedObservation(error);
    }

    @Test void teardownFailureKeepsObservationSuppressed() {
        var summary = run(false, TeardownFailureFixture.class);
        var error = summary.getFailures().get(0).getException();
        assertEquals("teardown failed", error.getMessage());
        assertSuppressedObservation(error);
    }

    @Test void anotherCallbacksPrimaryFailureIsPreserved() {
        var summary = run(false, CallbackFailureFixture.class);
        var error = summary.getFailures().get(0).getException();
        assertEquals("another callback failed", error.getMessage());
        assertSuppressedObservation(error);
    }

    @Test void multipleExtensionsHaveIndependentQualifiedRecordings() {
        var summary = run(false, MultipleFixture.class);
        assertEquals(1, summary.getTestsSucceededCount(), summary.getFailures().toString());
    }

    /** Runs the launcher with System.out and System.err captured: {stdout, stderr}. */
    private String[] captured(Class<?>... classes) {
        var out = new java.io.ByteArrayOutputStream();
        var err = new java.io.ByteArrayOutputStream();
        var originalOut = System.out;
        var originalErr = System.err;
        System.setOut(new java.io.PrintStream(out, true));
        System.setErr(new java.io.PrintStream(err, true));
        try {
            var summary = run(false, classes);
            assertEquals(0, summary.getTotalFailureCount(), summary.getFailures().toString());
        } finally {
            System.setOut(originalOut);
            System.setErr(originalErr);
        }
        return new String[] {out.toString(), err.toString()};
    }

    @Test void suiteCoverageIsPrintedByDefaultWithoutWritingAFile() throws Exception {
        var output = captured(DefaultCoverageFixture.class, AnotherDefaultCoverageFixture.class);
        assertEquals("", output[1]);
        var printed = output[0];
        assertEquals(1, printed.split(java.util.regex.Pattern.quote(CoverageCollector.HEADING), -1).length - 1, printed);
        assertTrue(printed.contains("Counter: 1/1 states"), printed);
        assertTrue(printed.contains("Counter.Ready, on eTake: never handled"), printed);
        assertTrue(printed.contains("Completion.Pending: state never reached"), printed);
        try (var files = Files.walk(temporary)) {
            assertEquals(java.util.List.of(), files.filter(Files::isRegularFile).toList(), "default coverage wrote a file");
        }
    }

    @Test void suiteCoverageOffPrintsNothing() {
        var output = captured(CoverageOffFixture.class);
        assertEquals("", output[0] + output[1]);
    }

    @Test void defaultSuiteCoverageFailureIsAWarning() {
        var output = captured(DefaultCoverageBadBinaryFixture.class);
        assertEquals("", output[0]);
        assertTrue(output[1].startsWith("snappea: warning: no suite spec coverage report: "), output[1]);
        assertEquals(1, output[1].strip().lines().count(), output[1]);
    }

    @Test void sameCoverageDestinationRejectsConflictingConfiguration() {
        var summary = run(false, ConflictingFixture.class);
        assertEquals(1, summary.getTestsFailedCount());
        assertTrue(summary.getFailures().get(0).getException().getMessage().contains("Conflicting"));
    }

    @Test void rootCoverageMergeFailureFailsLauncherAndPreservesDestination() throws Exception {
        Files.writeString(temporary.resolve("changed.p"), Files.readString(Path.of("src/test/resources/spec.p"))
                + "\n// incompatible fingerprint\n");
        var destination = temporary.resolve("incompatible.json");
        Files.writeString(destination, "previous destination");
        var summary = run(false, IncompatibleFixture.class);
        assertTrue(summary.getTotalFailureCount() > 0, "root merge failure must fail the launcher");
        assertEquals("previous destination", Files.readString(destination));
    }

    @Test void abortedTestCannotHideObservationFailure() {
        var summary = run(false, AbortedFixture.class);
        assertEquals(1, summary.getTestsFailedCount());
        assertEquals(0, summary.getTestsAbortedCount());
    }

    @Test void markedModeChecksOnlyAnnotatedTests() {
        var summary = run(false, MarkedFixture.class);
        assertEquals(1, summary.getTestsFailedCount(), summary.getFailures().toString());
        assertEquals(2, summary.getTestsSucceededCount());
        var failure = summary.getFailures().get(0);
        assertTrue(failure.getTestIdentifier().getDisplayName().startsWith("marked("));
        assertInstanceOf(SpecViolation.class, failure.getException());
    }

    @Test void perTestSkipAndCheckHot() {
        var summary = run(false, OptionsFixture.class);
        assertEquals(3, summary.getTestsSucceededCount(), summary.getFailures().toString());
        assertEquals(1, summary.getTestsFailedCount());
        var violation = assertInstanceOf(SpecViolation.class, summary.getFailures().get(0).getException());
        assertTrue(violation.liveness());
    }

    @Test void offModeIsInert() {
        var summary = run(false, OffFixture.class);
        assertEquals(1, summary.getTestsSucceededCount(), summary.getFailures().toString());
    }

    @Test void autodetectedExtensionUsesTheConfigFileAndStepsAsideForExplicitOnes() {
        var summary = run(false, true, AutodetectedFixture.class, MultipleFixture.class);
        assertEquals(2, summary.getTestsSucceededCount(), summary.getFailures().toString());
        assertEquals(1, summary.getTestsFailedCount());
        assertInstanceOf(SpecViolation.class, summary.getFailures().get(0).getException());
    }

    @Test void expectViolationInsideACheckedTest() {
        var summary = run(false, NegativeControlFixture.class);
        assertEquals(1, summary.getTestsSucceededCount(), summary.getFailures().toString());
    }

    private static void assertSuppressedObservation(Throwable error) {
        assertTrue(java.util.Arrays.stream(error.getSuppressed()).anyMatch(e -> e instanceof SpecViolation || e instanceof SetupError),
                () -> "Expected suppressed observation: " + java.util.Arrays.toString(error.getSuppressed()));
    }

    static class ParallelFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter", "Other")
                .coverage(root.resolve("coverage/monitor.json")).build();
        @BeforeEach void setup(Recording events) throws Exception {
            events.emit("eAdd", "sku", 2);
            simultaneous.countDown();
            assertTrue(simultaneous.await(5, TimeUnit.SECONDS), "tests did not overlap");
        }
        @Test void first(Recording events) { events.emit("eTake", "sku", 1); }
        @Test void second(Recording events) { events.emit("eTake", "sku", 2); }
        @AfterEach void teardown(Recording events) { events.emit("eOther", "other"); }
    }

    static class AnotherClassFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Other")
                .coverage(root.resolve("coverage/monitor.json")).build();
        @Test void another(Recording events) { events.emit("eOther", "other"); }
    }

    static class BodyFailureFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter")
                .coverage(root.resolve("failed-coverage.json")).build();
        @Test void fails(Recording events) {
            events.emit("eTake", "sku", 1);
            throw new IllegalStateException("body failed");
        }
    }

    static class SetupFailureFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter").build();
        @BeforeEach void setup(Recording events) {
            events.emit("eTake", "sku", 1);
            throw new IllegalStateException("setup failed");
        }
        @Test void neverReached() { fail("body executed after setup failure"); }
    }

    static class TeardownFailureFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter").build();
        @Test void body(Recording events) { events.emit("eAdd", "sku", 1); }
        @AfterEach void teardown(Recording events) {
            events.emit("eTake", "sku", 2);
            throw new IllegalStateException("teardown failed");
        }
    }

    static class CallbackFailureFixture {
        @RegisterExtension @Order(0) static final SnappeaExtension SNAPPEA = builder("Counter").build();
        @RegisterExtension @Order(1) static final BeforeEachCallback FAILURE = context -> {
            throw new IllegalStateException("another callback failed");
        };
        @Test void neverReached() { fail("body executed after callback failure"); }
    }

    static class MultipleFixture {
        @RegisterExtension static final SnappeaExtension FIRST = builder("Counter").name("first").build();
        @RegisterExtension static final SnappeaExtension SECOND = builder("Other").name("second").build();
        @Test void separate(@SnappeaRecording("first") Recording first, @SnappeaRecording("second") Recording second) {
            first.emit("eAdd", "sku", 1);
            second.emit("eOther", "sku");
        }
    }

    static class ConflictingFixture {
        @RegisterExtension @Order(0) static final SnappeaExtension FIRST = builder("Counter").name("first")
                .coverage(root.resolve("conflict.json")).build();
        @RegisterExtension @Order(1) static final SnappeaExtension SECOND = builder("Counter").name("second")
                .timeout(Duration.ofSeconds(3)).coverage(root.resolve("conflict.json")).build();
        @Test void neverReached() { fail("conflicting destination accepted"); }
    }
    static class IncompatibleFixture {
        @RegisterExtension static final SnappeaExtension FIRST = builder("Counter").name("first")
                .coverage(root.resolve("incompatible.json")).build();
        @RegisterExtension static final SnappeaExtension SECOND = SnappeaExtension.builder().name("second")
                .specFiles(root.resolve("changed.p")).specs("Counter").artifacts(root.resolve("snappea"))
                .coverage(root.resolve("incompatible.json")).build();
        @Test void sources(@SnappeaRecording("first") Recording first, @SnappeaRecording("second") Recording second) {
            first.emit("eAdd", "sku", 1);
            second.emit("eAdd", "sku", 1);
        }
    }

    static class AbortedFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter").build();
        @Test void abort(Recording events) {
            events.emit("eTake", "sku", 1);
            org.junit.jupiter.api.Assumptions.assumeTrue(false, "abort after invalid event");
        }
    }

    static class MarkedFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter").mode(Mode.MARKED).build();
        @Test void unmarked(Recording events) { events.emit("eTake", "sku", 1); }
        @Test @Snappea void marked(Recording events) { events.emit("eTake", "sku", 1); }
        @Test void unmarkedStatic() { io.github.crowecawcaw.snappea.emit.SnappeaEmit.emit("eTake", "sku", 1); }
    }

    static class OptionsFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter", "Completion").build();
        @Test @Snappea(skip = Snappea.ALL) void skipAll(Recording events) { events.emit("eTake", "sku", 1); }
        @Test @Snappea(skip = "Counter") void skipOne(Recording events) {
            events.emit("eTake", "sku", 1);
            events.emit("eBegin", "flow");
        }
        @Test void hotIsUnchecked(Recording events) {
            events.emit("eAdd", "sku", 1);
            events.emit("eBegin", "flow");
        }
        @Test @Snappea(checkHot = true) void hotChecked(Recording events) {
            events.emit("eAdd", "sku", 1);
            events.emit("eBegin", "flow");
        }
    }

    static class DefaultCoverageFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter", "Completion").strict(false).build();
        @Test void adds(Recording events) { events.emit("eAdd", "sku", 2); }
    }

    static class AnotherDefaultCoverageFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter").timeout(Duration.ofSeconds(9)).build();
        @Test void addsAgain(Recording events) { events.emit("eAdd", "sku", 1); }
    }

    static class CoverageOffFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter")
                .config(new SnappeaConfig(null, java.util.Map.of("coverage", "off"))).build();
        @Test void adds(Recording events) { events.emit("eAdd", "sku", 2); }
    }

    /** The checks pass with a stub binary whose coverage merge fails. */
    static class DefaultCoverageBadBinaryFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter").binary(stub().toString()).build();
        @Test void adds(Recording events) { events.emit("eAdd", "sku", 2); }
        static Path stub() {
            try {
                var real = SnappeaObserver.builder().config(SnappeaConfig.NONE)
                        .specFiles(Path.of("src/test/resources/spec.p")).build().executable();
                var stub = root.resolve("stub-snappea");
                Files.writeString(stub, "#!/bin/sh\nif [ \"$1\" = coverage ]; then printf 'merge broke\\n  badly\\n' >&2; exit 2; fi\n"
                        + "exec '" + real + "' \"$@\"\n");
                assertTrue(stub.toFile().setExecutable(true));
                return stub;
            } catch (java.io.IOException error) {
                throw new java.io.UncheckedIOException(error);
            }
        }
    }

    static class OffFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter").mode(Mode.OFF).build();
        @Test void inert(Recording events) { events.emit("eTake", "sku", 1); }
    }

    static class AutodetectedFixture {
        @Test void violates(Recording events) { events.emit("eTake", "sku", 1); }
        @Test void passes() { io.github.crowecawcaw.snappea.emit.SnappeaEmit.emit("eAdd", "sku", 1); }
    }

    static class NegativeControlFixture {
        @RegisterExtension static final SnappeaExtension SNAPPEA = builder("Counter").build();
        @Test void deliberate() {
            var observer = SnappeaObserver.builder().specFiles(Path.of("src/test/resources/spec.p")).specs("Counter")
                    .artifacts(root.resolve("snappea")).build();
            var violation = Recording.expectViolation("Counter", "below zero", () -> {
                try (var inner = observer.recording("negative control")) {
                    io.github.crowecawcaw.snappea.emit.SnappeaEmit.emit("eTake", "sku", 1);
                }
            });
            assertEquals("sku", violation.key());
        }
    }
}
