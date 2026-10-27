package io.github.crowecawcaw.snappea;

import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.time.Instant;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.Executors;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;
import org.opentest4j.AssertionFailedError;
import static org.junit.jupiter.api.Assertions.*;

class ObserverTest {
    @TempDir Path temporary;
    static Path spec() { return Path.of("src/test/resources/spec.p"); }
    private SnappeaObserver.Builder builder(String... specs) {
        var builder = SnappeaObserver.builder().config(SnappeaConfig.NONE).specFiles(spec())
                .artifacts(temporary.resolve("artifacts with spaces"));
        return specs.length == 0 ? builder : builder.specs(specs);
    }

    @Test void independentRecordingsAndRelevantSpecSubsets() {
        var observer = builder("Counter", "Other").build();
        for (int i = 0; i < 2; i++) {
            try (var events = observer.recording("case " + i)) {
                events.emit("eAdd", "counter", 3);
                events.emit("eTake", "counter", 2);
                events.emit("eOther", "other");
            }
        }
    }

    @Test void specsDefaultToAllAndStrictFollowsThem() {
        var all = builder().build();
        assertFalse(all.strict());
        try (var events = all.recording("only Counter events")) { events.emit("eAdd", "sku", 1); }
        var excluded = builder().exclude("Counter").build().recording("excluded spec");
        excluded.emit("eTake", "sku", 5);
        assertDoesNotThrow(excluded::close);
        assertTrue(builder("Counter").build().strict());
    }

    @Test void strictRejectsEmptyAndIrrelevantButCanBeDisabled() {
        var empty = builder("Counter").build().recording("empty");
        var failure = assertThrows(SetupError.class, empty::close);
        assertEquals(SetupError.VALIDATION, failure.kind());
        assertTrue(Files.exists(failure.artifacts().resolve("report.json")));
        var irrelevant = builder("Counter", "Other").build().recording("missing other");
        irrelevant.emit("eAdd", "counter", 1);
        assertEquals(SetupError.VALIDATION, assertThrows(SetupError.class, irrelevant::close).kind());
        try (var ignored = builder("Counter").strict(false).build().recording("explicit empty opt out")) {}
    }

    @Test void emptyNonStrictRecordingPassesWithoutRunningTheChecker() {
        try (var ignored = builder().binary(temporary.resolve("no snappea here").toString()).build().recording("nothing")) {}
    }

    @Test void violationsAreStructuredAndPrintedLikePython() throws Exception {
        var recording = builder("Counter").build().recording("overspend");
        recording.emit("eAdd", "sku", 1);
        recording.emitAt(Instant.parse("2100-01-01T00:00:00Z"), "eTake", "sku", 2);
        var violation = assertThrows(SpecViolation.class, recording::close);
        assertInstanceOf(AssertionFailedError.class, violation);
        assertEquals("Counter", violation.spec());
        assertEquals("sku", violation.key());
        assertEquals("counter below zero", violation.message());
        assertFalse(violation.liveness());
        assertTrue(violation.location().endsWith("spec.p:14"), violation.location());
        assertEquals(2, violation.events().size());
        var last = violation.events().get(1);
        assertEquals("eTake", last.event());
        assertEquals(2, last.payload());
        assertTrue(last.src().startsWith("io/github/crowecawcaw/snappea/ObserverTest.java:"), last.src());
        assertTrue(last.at());
        var text = violation.getMessage();
        assertTrue(text.startsWith("P spec Counter was broken: counter below zero\n\n  Events (key \"sku\"):\n"), text);
        assertTrue(text.contains("  <- broke the spec here"), text);
        assertTrue(text.contains("(time set with emitAt)"), text);
        assertTrue(text.contains("  Spec: "), text);
        assertTrue(text.contains("  snappea output: " + violation.artifacts()), text);
        assertTrue(Files.exists(violation.artifacts().resolve("events.jsonl")));
        assertEquals("overspend", Json.get(Json.get(Json.parse(violation.report().orElseThrow().json()), "source"), "label"));
        assertDoesNotThrow(recording::close);
        assertThrows(IllegalStateException.class, () -> recording.emit("eOther", "late"));
    }

    @Test void deprecatedObservationFailureStillCatchesBoth() {
        var recording = builder("Counter").build().recording("old API");
        recording.emit("eTake", "sku", 1);
        @SuppressWarnings("deprecation")
        var failure = assertThrows(ObservationFailure.class, recording::close);
        assertEquals("spec_violation", failure.report().orElseThrow().errors().get(0).kind());
    }

    @Test void mutablePayloadIsSnapshottedAndEmissionErrorsStaySticky() {
        var payload = new HashMap<String, Object>(Map.of("units", 1));
        try (var events = builder("Snapshot").build().recording("snapshot")) {
            events.emit("eTuple", "sku", payload);
            payload.put("units", -1);
        }
        var events = builder("Counter").build().recording("bad payload");
        events.emit("eAdd", "sku", 1);
        assertThrows(IllegalArgumentException.class, () -> events.emit("eAdd", "sku", new Object()));
        var failure = assertThrows(SetupError.class, events::close);
        assertEquals(SetupError.CONFIG, failure.kind());
        assertInstanceOf(IllegalArgumentException.class, failure.getCause());
    }

    @Test void tryWithResourcesPreservesBodyFailure() {
        var primary = assertThrows(IllegalStateException.class, () -> {
            try (var events = builder("Counter").build().recording("body and observation fail")) {
                events.emit("eTake", "sku", 1);
                throw new IllegalStateException("body failed");
            }
        });
        assertEquals("body failed", primary.getMessage());
        assertInstanceOf(SpecViolation.class, primary.getSuppressed()[0]);
    }

    @Test void timestampsAreIsoNanosAndConcurrentCaptureKeepsEveryEvent() throws Exception {
        var recording = builder("Counter").build().recording("parallel emitters");
        var pool = Executors.newFixedThreadPool(4);
        try {
            var tasks = new java.util.ArrayList<java.util.concurrent.Callable<Void>>();
            for (int i = 0; i < 100; i++) tasks.add(() -> { recording.emit("eAdd", "sku", 1); return null; });
            for (var future : pool.invokeAll(tasks)) future.get();
        } finally { pool.shutdownNow(); }
        recording.emit("eTake", "sku", 101); // fails, so the events are kept to inspect
        var failure = assertThrows(SpecViolation.class, recording::close);
        var lines = Files.readAllLines(failure.artifacts().resolve("events.jsonl"));
        assertEquals(101, lines.size());
        var first = Json.parse(lines.get(0));
        assertTrue(Json.text(Json.get(first, "timestamp")).matches("\\d{4}-\\d\\d-\\d\\dT\\d\\d:\\d\\d:\\d\\d\\.\\d{9}Z"));
        assertTrue(Json.integer(Json.get(first, "seq")) > 0);
        assertEquals(ProcessHandle.current().pid(), Json.integer(Json.get(first, "pid")));
        var exact = builder("Counter").build().recording("timestamp");
        exact.emitAt(Instant.parse("2026-10-19T00:00:00.123456789Z"), "eTake", "sku", 1);
        var exactFailure = assertThrows(SpecViolation.class, exact::close);
        assertEquals("2026-10-19T00:00:00.123456789Z", exactFailure.events().get(0).timestamp());
    }

    @Test void checkHotIsIndependentOfStrict() {
        try (var events = builder("Completion").build().recording("unfinished permitted")) {
            events.emit("eBegin", "flow");
        }
        var events = builder("Completion").strict(false).checkHot(true).build().recording("unfinished rejected");
        events.emit("eBegin", "flow");
        var violation = assertThrows(SpecViolation.class, events::close);
        assertTrue(violation.liveness());
        assertEquals("it ended in hot state 'Pending'", violation.message());
        assertTrue(violation.getMessage().contains("<- events ended here"), violation.getMessage());
    }

    @Test void unknownEventsAreErrorsByDefault() {
        var typo = builder("Counter").build().recording("typo");
        typo.emit("eAdd", "sku", 1);
        typo.emit("eAddd", "sku", 1);
        var failure = assertThrows(SetupError.class, typo::close);
        assertEquals(SetupError.UNKNOWN_EVENT, failure.kind());
        assertTrue(failure.getMessage().contains("did you mean eAdd?"), failure.getMessage());
        assertTrue(failure.getMessage().contains("emitted at io/github/crowecawcaw/snappea/ObserverTest.java:"), failure.getMessage());
        try (var ignored = builder("Counter").unknownEvents(UnknownEvents.IGNORE).build().recording("ignored typo")) {
            ignored.emit("eAdd", "sku", 1);
            ignored.emit("eAddd", "sku", 1);
        }
    }

    @Test void parserAndMissingSpecFailuresNeverPass() {
        var wrongPayload = builder("Counter").build().recording("parser error");
        wrongPayload.emit("eAdd", "sku", "not an integer");
        var failure = assertThrows(SetupError.class, wrongPayload::close);
        assertEquals(SetupError.PARSER, failure.kind());
        assertTrue(failure.getMessage().contains("eAdd (io/github/crowecawcaw/snappea/ObserverTest.java:"), failure.getMessage());
        var missing = builder("Typo").build().recording("missing spec");
        missing.emit("eOther", "sku");
        var config = assertThrows(SetupError.class, missing::close);
        assertEquals(SetupError.CONFIG, config.kind());
        assertTrue(config.getMessage().contains("not declared"), config.getMessage());
    }

    @Test void missingBinaryAndInvalidConfigurationFailClearly() {
        var events = builder("Counter").binary(temporary.resolve("does not exist").toString()).build().recording("binary");
        events.emit("eAdd", "sku", 1);
        var failure = assertThrows(SetupError.class, events::close);
        assertEquals(SetupError.BINARY_MISSING, failure.kind());
        assertTrue(failure.getMessage().contains("cargo install"), failure.getMessage());
        var onPath = builder("Counter").environment(Map.of("PATH", temporary.toString())).build().recording("path");
        onPath.emit("eAdd", "sku", 1);
        assertEquals(SetupError.BINARY_MISSING, assertThrows(SetupError.class, onPath::close).kind());
        assertEquals(SetupError.CONFIG, assertThrows(SetupError.class, () -> builder("Counter").timeout(Duration.ZERO).build()).kind());
        assertEquals(SetupError.CONFIG, assertThrows(SetupError.class,
                () -> SnappeaObserver.builder().config(SnappeaConfig.NONE).environment(Map.of()).build()).kind());
        assertEquals(SetupError.CONFIG, assertThrows(SetupError.class,
                () -> builder().specFiles(temporary.resolve("missing.p")).build()).kind());
    }

    @Test void timeoutIsASetupError() throws Exception {
        var slow = temporary.resolve("slow snappea");
        Files.writeString(slow, "#!/bin/sh\nsleep 10\n");
        assertTrue(slow.toFile().setExecutable(true));
        var events = builder("Counter").binary(slow.toString()).timeout(Duration.ofMillis(200)).build().recording("slow");
        events.emit("eAdd", "sku", 1);
        assertEquals(SetupError.TIMEOUT, assertThrows(SetupError.class, events::close).kind());
    }

    @Test void suiteUnionIncludesFailuresAndRetainsDistinctSources() throws Exception {
        var observer = builder("Counter").build();
        var target = temporary.resolve("coverage/monitor.json");
        try (var coverage = observer.coverage(target)) {
            try (var events = observer.recording("successful", coverage)) { events.emit("eAdd", "sku", 2); }
            var failed = observer.recording("failed", coverage);
            failed.emit("eTake", "sku", 1);
            assertThrows(SpecViolation.class, failed::close);
        }
        var merged = Json.read(target);
        assertEquals("observed_monitor_suite", Json.get(merged, "scope"));
        assertEquals(2, Json.items(Json.get(merged, "observations")).size());
        assertTrue(Json.write(Json.get(merged, "observations")).contains("spec_violation"));
        var handlers = Json.get(Json.get(Json.items(Json.get(merged, "specs")).get(0), "coverage"), "handlers");
        assertEquals(2, Json.get(handlers, "covered"));
        // A second collector (another JVM fork) merges into the file instead of replacing it.
        try (var coverage = observer.coverage(target)) {
            try (var events = observer.recording("another fork", coverage)) { events.emit("eAdd", "sku", 2); }
        }
        assertEquals(3, Json.items(Json.get(Json.read(target), "observations")).size());
    }

    private static String stdout(Runnable body) {
        var out = new java.io.ByteArrayOutputStream();
        var original = System.out;
        System.setOut(new java.io.PrintStream(out, true));
        try { body.run(); } finally { System.setOut(original); }
        return out.toString();
    }

    @Test void configuredCoverageCollectorPrintsOnlyWithoutADestinationAndOffIsInert() throws Exception {
        var observer = builder("Counter").build();
        var printed = stdout(() -> {
            try (var coverage = observer.coverage()) {
                assertNull(coverage.destination());
                try (var events = observer.recording("no coverage file", coverage)) { events.emit("eAdd", "sku", 2); }
            }
        });
        assertTrue(printed.contains(CoverageCollector.HEADING) && printed.contains("Counter.Ready, on eTake: never handled"), printed);
        var env = Map.of("SNAPPEA_BIN", System.getenv().getOrDefault("SNAPPEA_BIN", "snappea"), "PATH", System.getenv("PATH"));
        var off = builder("Counter").environment(new HashMap<>(env) {{ put("SNAPPEA_COVERAGE", "off"); }}).build();
        assertEquals("", stdout(() -> {
            try (var coverage = off.coverage()) {
                assertNull(coverage.destination());
                try (var events = off.recording("coverage off", coverage)) { events.emit("eAdd", "sku", 2); }
            }
        }));
        assertFalse(Files.exists(Path.of("off")), "off was taken as a path");
        var target = temporary.resolve("env.json");
        var fromEnv = builder("Counter").environment(Map.of("SNAPPEA_COVERAGE", target.toString(),
                "SNAPPEA_BIN", System.getenv().getOrDefault("SNAPPEA_BIN", "snappea"), "PATH", System.getenv("PATH"))).build();
        try (var coverage = fromEnv.coverage()) {
            try (var events = fromEnv.recording("env coverage", coverage)) { events.emit("eAdd", "sku", 2); }
        }
        assertTrue(Files.exists(target));
    }

    @Test void incompatibleSourceMergePreservesExistingDestination() throws Exception {
        var sourceCopy = temporary.resolve("changed.p");
        Files.writeString(sourceCopy, Files.readString(spec()) + "\n// different compilation sources\n");
        var first = builder("Counter").build();
        var changed = SnappeaObserver.builder().config(SnappeaConfig.NONE).specFiles(sourceCopy).specs("Counter")
                .artifacts(temporary.resolve("artifacts with spaces")).build();
        var target = temporary.resolve("coverage.json");
        var coverage = first.coverage(target);
        try (var events = first.recording("first", coverage)) { events.emit("eAdd", "sku", 1); }
        try (var events = changed.recording("changed", coverage)) { events.emit("eAdd", "sku", 1); }
        var failure = assertThrows(SetupError.class, coverage::close);
        assertTrue(failure.getMessage().contains("different P sources"), failure.getMessage());
        assertFalse(Files.exists(target));
    }
}
