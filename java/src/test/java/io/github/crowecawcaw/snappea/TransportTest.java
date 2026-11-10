package io.github.crowecawcaw.snappea;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.condition.EnabledOnOs;
import org.junit.jupiter.api.condition.OS;
import org.junit.jupiter.api.io.TempDir;
import static org.junit.jupiter.api.Assertions.*;

@SuppressWarnings("deprecation")
class TransportTest {
    @TempDir Path temporary;

    private Path executable(String body) throws IOException {
        var path = temporary.resolve("fake snappea");
        Files.writeString(path, "#!/bin/sh\n" + body);
        assertTrue(path.toFile().setExecutable(true));
        return path;
    }

    private Recording recording(Path executable) {
        return SnappeaObserver.builder().config(SnappeaConfig.NONE).binary(executable.toString()).specFiles(ObserverTest.spec())
                .specs("Spec").artifacts(temporary.resolve("artifacts")).build().recording("fake transport");
    }

    @Test @EnabledOnOs(OS.LINUX)
    void missingAndMalformedSuccessReportsFailClosed() throws Exception {
        for (String json : List.of("", "{}", "{\"exit_code\":0}", "{\"exit_code\":0,\"specs\":[]}",
                "{\"exit_code\":0,\"specs\":[{\"spec\":\"Spec\"}]}",
                "{\"exit_code\":0,\"specs\":[{\"spec\":\"Wrong\",\"errors\":[]}]}",
                "{\"exit_code\":0,\"specs\":[{\"spec\":\"Spec\",\"errors\":null}]}",
                "{\"exit_code\":\"0\",\"specs\":[{\"spec\":\"Spec\",\"errors\":[]}]}")) {
            var binary = executable("report=''\nwhile [ \"$#\" -gt 0 ]; do\n"
                    + "  if [ \"$1\" = '--json-report' ]; then shift; report=$1; fi\n  shift\ndone\n"
                    + "printf '%s' '" + json + "' > \"$report\"\nexit 0\n");
            var events = recording(binary);
            events.emit("event", "key");
            var failure = assertThrows(ObservationFailure.class, events::close, json);
            assertTrue(Files.exists(failure.artifacts().resolve("events.jsonl")));
        }
        var events = recording(executable("exit 0\n"));
        assertThrows(ObservationFailure.class, events::close);
    }

    @Test @EnabledOnOs({OS.LINUX, OS.MAC})
    void commandLineErrorsNameTheMismatchedBinary() throws Exception {
        var unknown = recording(executable("if [ \"$1\" = --version ]; then echo 'snappea 0.0.1'; exit 0; fi\n"
                + "echo \"error: unknown option '--json-report'\" >&2\n"
                + "echo 'Run `snappea observe --help` for usage.' >&2\nexit 2\n"));
        unknown.emit("event", "key");
        var failure = assertThrows(SetupError.class, unknown::close);
        assertEquals(SetupError.CONFIG, failure.kind());
        assertTrue(failure.getMessage().contains("This snappea (snappea 0.0.1) does not support --json-report"), failure.getMessage());
        assertFalse(failure.getMessage().contains("--help"), failure.getMessage());

        var old = recording(executable("echo 'Failed parsing ::'\necho \"Unknown option '-l'\"\n"
                + "echo 'Usage: snappea observe [options]'\nexit 2\n"));
        old.emit("event", "key");
        failure = assertThrows(SetupError.class, old::close);
        assertTrue(failure.getMessage().contains("old PObserve-style"), failure.getMessage());

        var missing = recording(executable("echo 'error: no spec named Spec. Specs: Other' >&2\nexit 2\n"));
        missing.emit("event", "key");
        failure = assertThrows(SetupError.class, missing::close);
        assertTrue(failure.getMessage().contains("A spec in [Spec] is not declared"), failure.getMessage());

        // snappea writes {"exit_code": 2} when it fails before checking; the console says why.
        var early = recording(executable("report=''\nwhile [ \"$#\" -gt 0 ]; do\n"
                + "  if [ \"$1\" = '--json-report' ]; then shift; report=$1; fi\n  shift\ndone\n"
                + "echo 'error: cannot read spec.p' >&2\nprintf '{\"exit_code\": 2}' > \"$report\"\nexit 2\n"));
        early.emit("event", "key");
        failure = assertThrows(SetupError.class, early::close);
        assertEquals(SetupError.CONFIG, failure.kind());
        assertTrue(failure.getMessage().contains("error: cannot read spec.p"), failure.getMessage());
        assertFalse(failure.getMessage().contains("unusable report"), failure.getMessage());
    }

    @Test @EnabledOnOs(OS.LINUX)
    void invalidMetricStatusAndErrorTypesCannotMasqueradeAsSuccess() throws Exception {
        var valid = "{\"exit_code\":0,\"specs\":[{\"spec\":\"Spec\","
                + "\"events_read\":1,\"partition_keys\":1,\"verified_keys\":1,\"errors\":[]}]}";
        for (String mutation : List.of("large status", "unknown status", "missing metric", "negative metric",
                "fractional metric", "too many verified", "unknown kind", "missing message", "process mismatch",
                "strict empty", "unverified keys", "partitions exceed events", "events without partitions", "one strict spec empty")) {
            @SuppressWarnings("unchecked") var value = (Map<String, Object>) Json.parse(valid);
            @SuppressWarnings("unchecked") var spec = (Map<String, Object>) Json.items(value.get("specs")).get(0);
            int exit = 0;
            switch (mutation) {
                case "large status" -> value.put("exit_code", 4_294_967_296L);
                case "unknown status" -> value.put("exit_code", 17);
                case "missing metric" -> spec.remove("events_read");
                case "negative metric" -> spec.put("events_read", -1);
                case "fractional metric" -> spec.put("events_read", 1.5);
                case "too many verified" -> spec.put("verified_keys", 2);
                case "unknown kind" -> spec.put("errors", List.of(Map.of("kind", "typo", "message", "bad")));
                case "missing message" -> spec.put("errors", List.of(Map.of("kind", "parser")));
                case "process mismatch" -> exit = 2;
                case "strict empty" -> {
                    spec.putAll(Map.of("events_read", 0, "partition_keys", 0, "verified_keys", 0));
                }
                case "unverified keys" -> spec.put("verified_keys", 0);
                case "partitions exceed events" -> spec.put("events_read", 0);
                case "events without partitions" -> spec.putAll(Map.of("partition_keys", 0, "verified_keys", 0));
                case "one strict spec empty" -> {
                    var empty = new java.util.LinkedHashMap<>(spec);
                    empty.putAll(Map.of("spec", "Another", "events_read", 0, "partition_keys", 0, "verified_keys", 0));
                    Json.items(value.get("specs")).add(empty);
                }
            }
            var json = Json.write(value);
            var binary = executable("report=''\nwhile [ \"$#\" -gt 0 ]; do\n"
                    + "  if [ \"$1\" = '--json-report' ]; then shift; report=$1; fi\n  shift\ndone\n"
                    + "printf '%s' '" + json + "' > \"$report\"\nexit " + exit + "\n");
            String[] specs = mutation.equals("one strict spec empty") ? new String[]{"Spec", "Another"} : new String[]{"Spec"};
            var events = SnappeaObserver.builder().config(SnappeaConfig.NONE).binary(binary.toString()).specFiles(ObserverTest.spec())
                    .specs(specs).artifacts(temporary.resolve("artifacts")).build().recording("malformed metrics");
            events.emit("event", "key");
            var failure = assertThrows(ObservationFailure.class, events::close, mutation);
            assertTrue(failure.report().isEmpty(), "Invalid report must not be collected as coverage: " + mutation);
        }
    }

    @Test @EnabledOnOs(OS.LINUX)
    void strictFalseAllowsAnExplicitlyEmptySuccessReport() throws Exception {
        String json = "{\"exit_code\":0,\"specs\":[{\"spec\":\"Spec\",\"events_read\":0,"
                + "\"partition_keys\":0,\"verified_keys\":0,\"errors\":[]}]}";
        var binary = executable("report=''\nwhile [ \"$#\" -gt 0 ]; do\n"
                + "  if [ \"$1\" = '--json-report' ]; then shift; report=$1; fi\n  shift\ndone\n"
                + "printf '%s' '" + json + "' > \"$report\"\nexit 0\n");
        var events = SnappeaObserver.builder().config(SnappeaConfig.NONE).binary(binary.toString()).specFiles(ObserverTest.spec())
                .specs("Spec").strict(false).artifacts(temporary.resolve("artifacts")).build().recording("empty opt out");
        events.emit("event", "key"); // with no events at all, the checker would not run
        assertDoesNotThrow(events::close);
        assertEquals(0, events.report().orElseThrow().exitCode());
    }

    @Test @EnabledOnOs(OS.LINUX)
    void timeoutTerminatesWrapperAndItsChildWithinBound() throws Exception {
        var pid = temporary.resolve("child.pid");
        var binary = executable("sleep 60 &\nchild=$!\nprintf '%s' \"$child\" > '" + pid + "'\nwait\n");
        long began = System.nanoTime();
        var error = assertThrows(IOException.class, () -> Processes.run(List.of(binary.toString()), null,
                temporary.resolve("console.txt"), Duration.ofMillis(200)));
        assertTrue(error.getMessage().contains("timeout"));
        assertTrue(Duration.ofNanos(System.nanoTime() - began).compareTo(Duration.ofSeconds(5)) < 0);
        long child = Long.parseLong(Files.readString(pid));
        // A killed child can briefly remain a zombie until init reaps it; a zombie
        // has no executable or open event stream and is already terminated.
        // destroyForcibly only sends SIGKILL, so allow the kernel a moment to act on it.
        var stat = Path.of("/proc", Long.toString(child), "stat");
        long deadline = System.nanoTime() + Duration.ofSeconds(2).toNanos();
        boolean ended;
        while (!(ended = terminated(child, stat)) && System.nanoTime() < deadline) Thread.sleep(20);
        assertTrue(ended);
    }

    private static boolean terminated(long pid, Path stat) throws IOException {
        if (ProcessHandle.of(pid).map(h -> !h.isAlive()).orElse(true)) return true;
        try {
            return Files.readString(stat).split(" ")[2].equals("Z");
        } catch (java.nio.file.NoSuchFileException gone) {
            return true;
        }
    }

    @Test @EnabledOnOs(OS.LINUX)
    void interruptionTerminatesProcessAndRestoresInterruptFlag() throws Exception {
        var binary = executable("sleep 60\n");
        var flag = new java.util.concurrent.atomic.AtomicBoolean();
        var failure = new java.util.concurrent.atomic.AtomicReference<Throwable>();
        var worker = new Thread(() -> {
            try {
                Processes.run(List.of(binary.toString()), null, temporary.resolve("interrupted.txt"), Duration.ofSeconds(30));
                failure.set(new AssertionError("process was not interrupted"));
            } catch (IOException expected) {
                flag.set(Thread.currentThread().isInterrupted());
            }
        });
        worker.start();
        worker.interrupt();
        worker.join(5000);
        assertFalse(worker.isAlive());
        assertNull(failure.get());
        assertTrue(flag.get());
    }
}
