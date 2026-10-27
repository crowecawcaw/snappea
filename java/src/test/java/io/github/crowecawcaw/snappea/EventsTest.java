package io.github.crowecawcaw.snappea;

import io.github.crowecawcaw.snappea.emit.SnappeaEmit;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.List;
import java.util.Map;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;
import org.opentest4j.AssertionFailedError;
import static org.junit.jupiter.api.Assertions.*;

/** Event inputs: SnappeaEmit routing, child processes, added folders and logs, expectViolation. */
class EventsTest {
    @TempDir Path temporary;

    private SnappeaObserver observer(String... specs) {
        return SnappeaObserver.builder().config(SnappeaConfig.NONE).specFiles(ObserverTest.spec()).specs(specs)
                .artifacts(temporary.resolve("artifacts")).build();
    }

    @Test void staticEmitGoesToTheRecordingsOfThisThreadAndItsChildren() throws Exception {
        SnappeaEmit.emit("eTake", "sku", 1); // no recording: dropped
        var outer = observer("Counter").recording("outer");
        try (var inner = observer("Counter").recording("inner")) {
            assertTrue(SnappeaEmit.enabled());
            SnappeaEmit.emit("eAdd", "sku", 2);
            var worker = new Thread(() -> SnappeaEmit.emit("eTake", "sku", 1));
            worker.start();
            worker.join();
        }
        SnappeaEmit.emit("eTake", "sku", 5); // only the outer recording is still active
        var violation = assertThrows(SpecViolation.class, outer::close);
        assertEquals(3, violation.events().size());
        assertTrue(violation.events().get(0).src().startsWith("io/github/crowecawcaw/snappea/EventsTest.java:"));
    }

    @Test void unencodablePayloadFailsTheCheckWithoutThrowingFromEmit() {
        var recording = observer("Counter").recording("bad payload");
        long dropped = SnappeaEmit.dropped();
        assertDoesNotThrow(() -> SnappeaEmit.emit("eAdd", "sku", Double.NaN)); // the code under test keeps running
        SnappeaEmit.emit("eAdd", "sku", 1);
        assertEquals(dropped + 1, SnappeaEmit.dropped());
        var failure = assertThrows(SetupError.class, recording::close);
        assertTrue(failure.getMessage().contains("cannot encode event eAdd at io/github/crowecawcaw/snappea/EventsTest.java:"),
                failure.getMessage());
        assertTrue(failure.getMessage().contains("not finite"), failure.getMessage());
    }

    @Test void emitAfterTheCheckFails() throws Exception {
        var recording = observer("Counter").recording("finished");
        SnappeaEmit.emit("eAdd", "sku", 1);
        var late = new java.util.concurrent.atomic.AtomicReference<Throwable>();
        var worker = new Thread(() -> {
            try { Thread.sleep(200); SnappeaEmit.emit("eAdd", "sku", 1); }
            catch (Throwable error) { late.set(error); }
        });
        worker.start();
        recording.close();
        worker.join();
        assertInstanceOf(IllegalStateException.class, late.get());
    }

    @Test void childProcessesWriteToTheEventsFolder() throws Exception {
        var recording = observer("Counter").recording("child process");
        recording.emit("eAdd", "sku", 3);
        var java = ProcessHandle.current().info().command().orElse("java");
        var child = new ProcessBuilder(java, "-cp", System.getProperty("java.class.path"),
                "-Dsnappea.name=worker", ChildEmitter.class.getName(), "2", "2").inheritIO();
        child.environment().putAll(recording.environment());
        var process = child.start();
        assertTrue(process.waitFor(60, TimeUnit.SECONDS));
        assertEquals(0, process.exitValue());
        try (var files = Files.list(recording.eventsDir())) {
            var name = files.findFirst().orElseThrow().getFileName().toString();
            assertTrue(name.matches("worker-\\d+-[0-9a-f]+\\.jsonl"), name);
        }
        var violation = assertThrows(SpecViolation.class, recording::close);
        assertEquals("io/github/crowecawcaw/snappea/ChildEmitter.java:8", violation.events().get(2).src());
        assertEquals(recording.eventsDir().toString(), recording.environment().get("SNAPPEA_EVENTS_DIR"));
    }

    @Test void addedFoldersAndLogsAreChecked() throws Exception {
        var folder = Files.createDirectories(temporary.resolve("service"));
        Files.writeString(folder.resolve("svc-1-a.jsonl"),
                "{\"event\":\"eAdd\",\"key\":\"sku\",\"timestamp\":\"2026-01-01T00:00:00.000000001Z\",\"payload\":1}\n");
        var json = temporary.resolve("app.jsonl");
        Files.writeString(json, "{\"event\":\"eAdd\",\"key\":\"sku\",\"timestamp\":\"2026-01-01T00:00:00.000000002Z\",\"payload\":1}\n");
        var text = temporary.resolve("server.log");
        Files.writeString(text, "boot\n2026-01-01T00:00:00.000000003Z TAKE 3\n");
        var recording = observer("Counter").recording("inputs");
        recording.addEventsDir(folder);
        recording.addLog(json);
        recording.addLog(text, line -> {
            var parts = line.split(" ");
            return parts.length < 3 ? null : new LogEvent(Instant.parse(parts[0]), "eTake", "sku", Integer.parseInt(parts[2]));
        });
        var violation = assertThrows(SpecViolation.class, recording::close);
        assertEquals(List.of("eAdd", "eAdd", "eTake"), violation.events().stream().map(SpecViolation.Event::event).toList());
        assertEquals("server.log:2", violation.events().get(2).src());
    }

    @Test void expectViolationChecksSpecAndMessage() {
        var counter = observer("Counter");
        var caught = Recording.expectViolation("Counter", "below zero", () -> {
            try (var events = counter.recording("negative control")) { events.emit("eTake", "sku", 1); }
        });
        assertEquals("sku", caught.key());
        var passed = assertThrows(AssertionFailedError.class, () -> Recording.expectViolation("Counter", "x", () -> {
            try (var events = counter.recording("passes")) { events.emit("eAdd", "sku", 1); }
        }));
        assertTrue(passed.getMessage().contains("the check passed"));
        assertThrows(AssertionFailedError.class, () -> Recording.expectViolation("Other", ".*", () -> {
            try (var events = counter.recording("wrong spec")) { events.emit("eTake", "sku", 1); }
        }));
        assertThrows(AssertionFailedError.class, () -> Recording.expectViolation("Counter", "^nothing like it$", () -> {
            try (var events = counter.recording("wrong message")) { events.emit("eTake", "sku", 1); }
        }));
    }

    @Test void enclosingRecordingsIgnoreTheExpectedSpec() {
        try (var outer = observer("Counter").recording("whole test")) {
            Recording.expectViolation("Counter", "below zero", () -> {
                try (var inner = observer("Counter").recording("deliberate")) { SnappeaEmit.emit("eTake", "sku", 1); }
            });
        }
    }

    @Test void configFileResolvesPathsAgainstItsFolder() throws Exception {
        var project = Files.createDirectories(temporary.resolve("project/sub"));
        Files.createDirectories(temporary.resolve("project/specs"));
        Files.copy(ObserverTest.spec(), temporary.resolve("project/specs/spec.p"));
        Files.writeString(temporary.resolve("project/snappea.properties"),
                "spec_files = specs/*.p\nspecs = Counter\nunknown_events = warn\ntimeout = 5s\nartifacts = out\n");
        var config = SnappeaConfig.load(project, null);
        var observer = SnappeaObserver.builder().config(config).environment(Map.of()).build();
        assertEquals(List.of(temporary.resolve("project/specs/spec.p").toAbsolutePath()), observer.files());
        assertEquals(List.of("Counter"), observer.specs());
        assertTrue(observer.strict());
        assertEquals(UnknownEvents.WARN, observer.unknownEvents());
        assertEquals(java.time.Duration.ofSeconds(5), observer.timeout());
        assertEquals(temporary.resolve("project/out").toAbsolutePath(), observer.artifacts());
        var env = SnappeaObserver.builder().config(config).environment(Map.of(
                "SNAPPEA_SPEC_FILES", ObserverTest.spec().toString(), "SNAPPEA_ARTIFACTS", "elsewhere", "SNAPPEA_MODE", "marked")).build();
        assertEquals(List.of(ObserverTest.spec().toAbsolutePath()), env.files());
        assertEquals(Path.of("elsewhere").toAbsolutePath(), env.artifacts());
        assertEquals(Mode.MARKED, env.mode());
        Files.writeString(temporary.resolve("project/snappea.properties"), "spec_file = specs\n");
        var unknown = assertThrows(SetupError.class, () -> SnappeaConfig.load(project, null));
        assertEquals(SetupError.CONFIG, unknown.kind());
    }
}
