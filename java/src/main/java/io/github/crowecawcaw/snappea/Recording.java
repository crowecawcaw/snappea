package io.github.crowecawcaw.snappea;

import io.github.crowecawcaw.snappea.emit.EmitHook;
import java.io.BufferedWriter;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.regex.Pattern;

/**
 * One check: collects events while active, then checks them with {@code snappea observe}.
 *
 * <p>Events come from {@link #emit} on this object, {@code SnappeaEmit.emit} on the thread that
 * started it (and threads it starts), child processes writing to {@link #eventsDir()}, and
 * {@link #addEventsDir} / {@link #addLog} inputs. Closing (try-with-resources) checks once:
 * it throws {@link SpecViolation} or {@link SetupError}. Close after application cleanup,
 * and stop or join child processes and threads first.
 */
public final class Recording implements AutoCloseable {
    private static final Pattern ASSERTION = Pattern.compile("(?s)Assertion failure: (.+?:\\d+):\\d+ (.*)");
    private static final Pattern HOT = Pattern.compile("(?s).*detected liveness bug in hot state '(.*)' at the end of program execution\\.?");

    private enum State { ACTIVE, STOPPED, CHECKED }

    private record Log(Path path, LogEvent.Parser parser) {}

    private final SnappeaObserver observer;
    private final CoverageCollector coverage;
    private final String label;
    private final Path directory;
    private final Path eventsDir;
    private final Path ownFile;
    private final BufferedWriter writer;
    private final boolean inert;
    private final List<Path> addedDirs = new ArrayList<>();
    private final List<Log> logs = new ArrayList<>();
    private final Set<String> ignored = ConcurrentHashMap.newKeySet();
    private final Routing.Node node;
    private volatile State state = State.ACTIVE;
    private long emitted;
    private Throwable emissionError;
    private String routingError;
    private ObservationReport report;

    Recording(SnappeaObserver observer, String label, CoverageCollector coverage) {
        this(observer, label, coverage, false);
    }

    private Recording(SnappeaObserver observer, String label, CoverageCollector coverage, boolean inert) {
        this.observer = observer;
        this.label = SnappeaObserver.nonempty(label, "label");
        this.coverage = coverage;
        this.inert = inert;
        Path root = inert ? Path.of(System.getProperty("java.io.tmpdir")) : observer.artifacts();
        Path created = root;
        try {
            Files.createDirectories(root);
            created = Files.createTempDirectory(root, "recording-");
            directory = created;
            eventsDir = Files.createDirectory(directory.resolve("events"));
            ownFile = directory.resolve("events.jsonl");
            writer = Files.newBufferedWriter(ownFile);
        } catch (IOException error) {
            throw new SetupError(SetupError.CONFIG, "Cannot start recording " + label + " in " + root + ": " + error,
                    created, null, error);
        }
        node = inert ? null : Routing.start(this);
    }

    /** A recording that discards its events and never checks: for tests the extension skips. */
    static Recording inert(String label) {
        return new Recording(null, label, null, true);
    }

    /** Records {@code event} on stream {@code key} now. Same as {@code SnappeaEmit.emit}, but for this recording only. */
    public void emit(String event, String key, Object payload) {
        accept(lineOf(null, event, key, payload));
    }

    /** Records an event without a payload. */
    public void emit(String event, String key) { emit(event, key, null); }

    /** Records an event at a time the caller chose (an application timestamp or a logical clock). */
    public void emitAt(Instant at, String event, String key, Object payload) {
        accept(lineOf(Objects.requireNonNull(at, "at"), event, key, payload));
    }

    /** Records an event without a payload at a time the caller chose. */
    public void emitAt(Instant at, String event, String key) { emitAt(at, event, key, null); }

    private String lineOf(Instant at, String event, String key, Object payload) {
        if (state == State.CHECKED) throw new IllegalStateException("Recording '" + label + "' was already checked");
        try {
            return EmitHook.line(at, event, key, payload, EmitHook.callSite());
        } catch (RuntimeException error) {
            synchronized (this) {
                if (emissionError == null) emissionError = error;
            }
            throw error;
        }
    }

    /** Appends one JSON line (from this recording or from {@code SnappeaEmit}). */
    synchronized void accept(String line) {
        if (state == State.CHECKED) throw new IllegalStateException("Recording '" + label + "' was already checked");
        if (inert) return;
        try {
            writer.write(line);
            writer.write('\n');
            emitted++;
        } catch (IOException error) {
            // the check fails with it; emit does not throw into the code under test
            if (emissionError == null) emissionError = error;
        }
    }

    /** An event for this recording could not be encoded: the check fails with {@code error}. */
    synchronized void failed(Throwable error) {
        if (state != State.CHECKED && emissionError == null) emissionError = error;
    }

    synchronized void fail(String message) {
        if (routingError == null) routingError = message;
    }

    /** The folder child processes write events to; pass it as {@code SNAPPEA_EVENTS_DIR}, e.g. with {@link #environment()}. */
    public Path eventsDir() { return eventsDir; }

    /**
     * This process's environment plus {@code SNAPPEA_EVENTS_DIR}, for a child process:
     * {@code processBuilder.environment().putAll(recording.environment())}.
     */
    public Map<String, String> environment() {
        var env = new HashMap<>(System.getenv());
        env.put("SNAPPEA_EVENTS_DIR", eventsDir.toString());
        return Collections.unmodifiableMap(env);
    }

    /** Also checks the files in a folder this recording did not create (a container volume, a service). */
    public synchronized void addEventsDir(Path folder) {
        requireUnchecked();
        addedDirs.add(folder.toAbsolutePath());
    }

    /** Also checks a JSON-lines log in the events format {@code snappea observe} reads. */
    public synchronized void addLog(Path log) {
        requireUnchecked();
        logs.add(new Log(log.toAbsolutePath(), null));
    }

    /** Also checks an existing log: {@code parse} turns each line into an event, or null to skip it. */
    public synchronized void addLog(Path log, LogEvent.Parser parse) {
        requireUnchecked();
        logs.add(new Log(log.toAbsolutePath(), Objects.requireNonNull(parse)));
    }

    private void requireUnchecked() {
        if (state == State.CHECKED) throw new IllegalStateException("Recording '" + label + "' was already checked");
    }

    /** Stops collecting {@code SnappeaEmit} events; {@link #check()} still reads everything recorded so far. */
    public void stop() {
        if (state == State.ACTIVE) {
            state = State.STOPPED;
            if (node != null) Routing.stop(this);
        }
    }

    /** Stops and checks once; later calls do nothing. Throws {@link SpecViolation} or {@link SetupError}. */
    public synchronized void check() {
        if (state == State.CHECKED) return;
        stop();
        state = State.CHECKED;
        try {
            writer.close();
        } catch (IOException error) {
            if (emissionError == null) emissionError = error;
        }
        if (inert) {
            deleteQuietly(directory);
            return;
        }
        if (emissionError != null)
            throw new SetupError(SetupError.CONFIG, "Cannot record an event for " + label + ": " + emissionError
                    + "\n\n  snappea output: " + directory, directory, null, emissionError);
        if (routingError != null)
            throw new SetupError(SetupError.CONFIG, routingError + "\n\n  snappea output: " + directory, directory, null, null);
        var failure = run();
        if (failure != null) throw failure;
        deleteQuietly(directory);
    }

    /** Same as {@link #check()}. */
    @Override public void close() { check(); }

    boolean checked() { return state == State.CHECKED; }
    Routing.Node node() { return node; }
    void ignore(String spec) { ignored.add(spec); }

    public String label() { return label; }
    /** The folder with this recording's events, snappea's console output and report; deleted when the check passes. */
    public Path artifacts() { return directory; }
    /** The parsed report of the check, once it ran. */
    public synchronized Optional<ObservationReport> report() { return Optional.ofNullable(report); }

    /**
     * Runs {@code body}, which must end with a check that violates {@code spec} with a
     * message matching {@code match} (a regular expression, searched). Returns that
     * violation. Fails when the check passes or another spec is violated. Recordings
     * enclosing this call ignore {@code spec} from now on, so the deliberate violation is
     * not reported twice.
     */
    public static SpecViolation expectViolation(String spec, String match, Body body) {
        Objects.requireNonNull(spec, "spec");
        var pattern = Pattern.compile(Objects.requireNonNull(match, "match"));
        for (var enclosing : Routing.enclosing()) enclosing.ignore(spec);
        try {
            body.run();
        } catch (SpecViolation violation) {
            var all = new ArrayList<SpecViolation>();
            all.add(violation);
            all.addAll(violation.others());
            for (var each : all) {
                if (!each.spec().equals(spec))
                    throw new org.opentest4j.AssertionFailedError("Expected a violation of " + spec + ", but "
                            + each.spec() + " was violated", violation);
            }
            if (!pattern.matcher(violation.message()).find())
                throw new org.opentest4j.AssertionFailedError("Expected " + spec + " to be violated with a message matching /"
                        + match + "/, got: " + violation.message(), violation);
            return violation;
        } catch (Error | RuntimeException error) {
            throw error;
        } catch (Throwable error) {
            throw new IllegalStateException(error);
        }
        throw new org.opentest4j.AssertionFailedError("Expected " + spec + " to be violated, but the check passed");
    }

    /** The code under {@link #expectViolation}. */
    @FunctionalInterface
    public interface Body {
        void run() throws Throwable;
    }

    // ---- running snappea ---------------------------------------------------------------

    private ObservationFailure run() {
        boolean children;
        try (var files = Files.list(eventsDir)) {
            children = files.findAny().isPresent();
        } catch (IOException error) {
            return new SetupError(SetupError.CONFIG, "Cannot read " + eventsDir + ": " + error, directory, null, error);
        }
        if (emitted == 0 && !children && addedDirs.isEmpty() && logs.isEmpty() && !observer.strict()) return null;
        var console = directory.resolve("console.txt");
        var reportPath = directory.resolve("report.json");
        String executable;
        List<Path> inputs;
        try {
            executable = observer.executable();
            inputs = inputs();
        } catch (SetupError error) {
            return new SetupError(error.kind(), error.getMessage() + "\n\n  snappea output: " + directory, directory, null, error);
        }
        var command = observer.command(executable, inputs, reportPath);
        int exit;
        try {
            Files.writeString(directory.resolve("command.txt"), String.join(" ", command) + "\n");
            exit = Processes.run(command, null, console, observer.timeout());
        } catch (Processes.Timeout error) {
            return new SetupError(SetupError.TIMEOUT, "snappea did not finish checking " + label + " within "
                    + observer.timeout() + "\n\n  snappea output: " + directory, directory, null, error);
        } catch (IOException error) {
            return new SetupError(SetupError.CONFIG, "Cannot run " + executable + " for " + label + ": " + error
                    + "\n\n  snappea output: " + directory, directory, null, error);
        }
        if (!Files.isRegularFile(reportPath)) return commandError(executable, console);
        Map<String, Object> full;
        try {
            var value = Json.read(reportPath);
            // snappea failed before checking (bad sources, unknown spec...): {"exit_code": 2} and
            // no specs; the console has what it reported.
            if (exit == 2 && value instanceof Map && !Json.has(value, "specs")
                    && Integer.valueOf(2).equals(Json.get(value, "exit_code"))) return commandError(executable, console);
            validateReport(value, exit);
            @SuppressWarnings("unchecked") var object = (Map<String, Object>) value;
            full = object;
        } catch (IOException | RuntimeException error) {
            return new SetupError(SetupError.CONFIG, "snappea wrote an unusable report for " + label + ": " + error + "\n"
                    + Processes.tail(console) + "\n\n  snappea output: " + directory, directory, null, error);
        }
        var source = new LinkedHashMap<String, Object>();
        source.put("label", label);
        source.put("recording_id", directory.getFileName().toString());
        full.put("source", source);
        try {
            Files.writeString(reportPath, Json.pretty(full));
        } catch (IOException error) {
            return new SetupError(SetupError.CONFIG, "Cannot write " + reportPath + ": " + error, directory, null, error);
        }
        report = new ObservationReport(full);
        ObservationFailure failure = Outcomes.of(report, Set.copyOf(ignored), directory);
        if (coverage != null && coverage.enabled()) {
            try {
                coverage.add(report);
            } catch (ObservationFailure error) {
                if (failure == null) return error;
                failure.addSuppressed(error);
            }
        }
        return failure;
    }

    /** In order: this process's events, the events folder, added folders, logs (equal times keep this order). */
    private List<Path> inputs() {
        var out = new ArrayList<Path>();
        out.add(ownFile);
        out.add(eventsDir);
        for (var folder : addedDirs) {
            if (!Files.isDirectory(folder)) throw SetupError.config("addEventsDir: " + folder + " is not a folder");
            out.add(folder);
        }
        int n = 0;
        for (var log : logs) {
            if (!Files.isRegularFile(log.path)) throw SetupError.config("addLog: " + log.path + " does not exist");
            if (log.parser == null) {
                out.add(log.path);
                continue;
            }
            var parsed = directory.resolve("log-" + (n++) + "-" + log.path.getFileName() + ".jsonl");
            try (var lines = Files.newBufferedReader(log.path); var outLines = Files.newBufferedWriter(parsed)) {
                String line;
                int number = 0;
                while ((line = lines.readLine()) != null) {
                    number++;
                    LogEvent event;
                    try {
                        event = log.parser.parse(line);
                    } catch (Exception error) {
                        throw new SetupError(SetupError.PARSER, "addLog: the parser of " + log.path + " failed on line "
                                + number + ": " + error, directory, null, error);
                    }
                    if (event == null) continue;
                    outLines.write(EmitHook.line(event.at(), event.event(), event.key(), event.payload(),
                            log.path.getFileName() + ":" + number));
                    outLines.write('\n');
                }
            } catch (IOException error) {
                throw new SetupError(SetupError.CONFIG, "addLog: cannot read " + log.path + ": " + error, directory, null, error);
            }
            out.add(parsed);
        }
        return out;
    }

    private SetupError commandError(String executable, Path console) {
        String output = Processes.tail(console).strip();
        var lines = output.lines().toList();
        String text;
        if (lines.stream().anyMatch(line -> line.startsWith("Failed parsing"))) {
            // A snappea from before `snappea observe` had its own command line: PObserve-style options and errors.
            text = "This snappea (" + Processes.version(executable) + ") has the old PObserve-style `snappea observe`"
                    + " options, but snappea " + BundledBinary.version() + " needs a newer snappea. " + SnappeaObserver.INSTALL
                    + "\n" + output;
        } else {
            // Errors are "error: <message>" lines (on stderr, merged into the console), command line
            // errors followed by a "Run `snappea observe --help` for usage." hint. Keep the rest, such as
            // compiler diagnostics before "error: The P specification does not type-check."
            text = lines.stream().filter(line -> !line.startsWith("Run `snappea observe --help`"))
                    .collect(java.util.stream.Collectors.joining("\n")).strip();
            if (text.contains("no spec named")) {
                var named = observer.specs().isEmpty() ? observer.exclude() : observer.specs();
                text = "A spec in " + named + " is not declared in the spec files " + observer.files()
                        + " (snappea: " + text + ")";
            }
            var option = Pattern.compile("^error: unknown option '(-[^'=\\s]*)", Pattern.MULTILINE).matcher(text);
            if (option.find()) {
                text = "This snappea (" + Processes.version(executable) + ") does not support " + option.group(1)
                        + ", which snappea " + BundledBinary.version() + " passes: the snappea binary does not match this"
                        + " helper. Use snappea " + BundledBinary.version() + ". " + SnappeaObserver.INSTALL + "\n" + text;
            }
        }
        return new SetupError(SetupError.CONFIG, "snappea could not check the events of " + label + ":\n" + text
                + "\n\n  snappea output: " + directory, directory, null, null);
    }

    private void validateReport(Object value, int processExit) throws IOException {
        if (!(value instanceof Map) || !(Json.get(value, "exit_code") instanceof Integer exitCode)
                || !Set.of(0, 1, 2).contains(exitCode)
                || !(Json.get(value, "specs") instanceof List)) throw new IOException("Malformed observation report");
        boolean success = exitCode == 0;
        boolean hasErrors = false;
        var names = new java.util.HashSet<String>();
        for (var spec : Json.items(Json.get(value, "specs"))) {
            var name = Json.text(Json.get(spec, "spec"));
            if (name == null || !names.add(name) || !(Json.get(spec, "errors") instanceof List))
                throw new IOException("Malformed spec report");
            for (String metric : List.of("events_read", "partition_keys", "verified_keys")) {
                var number = Json.integer(Json.get(spec, metric));
                if (number == null || number < 0) throw new IOException("Malformed observation metric: " + metric);
            }
            long events = Json.integer(Json.get(spec, "events_read"));
            long partitions = Json.integer(Json.get(spec, "partition_keys"));
            long verified = Json.integer(Json.get(spec, "verified_keys"));
            if (events < partitions) throw new IOException("partition_keys exceeds events_read");
            if ((events == 0) != (partitions == 0))
                throw new IOException("events_read and partition_keys disagree on empty observation");
            if (verified > partitions) throw new IOException("verified_keys exceeds partition_keys");
            if (success && verified != partitions) throw new IOException("Successful report contains unverified keys");
            if (success && observer.strict() && events == 0)
                throw new IOException("Strict observation reported success without relevant events for " + name);
            for (var error : Json.items(Json.get(spec, "errors"))) {
                hasErrors = true;
                var kind = Json.text(Json.get(error, "kind"));
                if (kind == null || !Set.of("spec_violation", "parser", "out_of_order", "unknown",
                        "validation", "unknown_event").contains(kind)
                        || Json.text(Json.get(error, "message")) == null)
                    throw new IOException("Malformed observation error");
                for (String field : List.of("key", "cause")) {
                    var optional = Json.get(error, field);
                    if (!Json.has(error, field) || (optional != null && !(optional instanceof String)))
                        throw new IOException("Malformed observation error " + field);
                }
                if (!(Json.get(error, "events") instanceof List)) throw new IOException("Malformed error replay events");
                for (var replay : Json.items(Json.get(error, "events"))) {
                    for (String field : List.of("key", "timestamp", "event", "log_line")) {
                        if (Json.text(Json.get(replay, field)) == null) throw new IOException("Malformed replay " + field);
                    }
                }
            }
        }
        if (!observer.specs().isEmpty() && !names.equals(new java.util.HashSet<>(observer.specs())))
            throw new IOException("Report spec set differs from requested specifications");
        if (observer.specs().isEmpty() && observer.exclude().stream().anyMatch(names::contains))
            throw new IOException("Report contains an excluded spec");
        // exit_code is snappea's process status (0 no error, 1 a spec violation, 2 another error);
        // outcomes are classified by error kind, not by this code.
        if (success == hasErrors || exitCode != processExit)
            throw new IOException("Inconsistent process status, report status and errors");
    }

    static void deleteTree(Path directory) throws IOException {
        try (var paths = Files.walk(directory)) {
            for (var path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) Files.delete(path);
        }
    }

    private static void deleteQuietly(Path directory) {
        try {
            deleteTree(directory);
        } catch (IOException ignored) {
            // a leftover temp folder is harmless
        }
    }

    /** Turns a report into the exception a test sees. */
    static final class Outcomes {
        private Outcomes() {}

        static ObservationFailure of(ObservationReport report, Set<String> ignored, Path directory) {
            var input = new ArrayList<ObservationReport.Error>();
            var validation = new ArrayList<ObservationReport.Error>();
            var violations = new ArrayList<SpecViolation>();
            for (var error : report.errors()) {
                switch (error.kind()) {
                    case "spec_violation" -> {
                        if (!ignored.contains(error.spec())) violations.add(violation(error, directory, report));
                    }
                    case "validation" -> validation.add(error);
                    default -> input.add(error);
                }
            }
            if (!input.isEmpty()) {
                var setup = setupError(input, directory, report);
                violations.forEach(setup::addSuppressed);
                return setup;
            }
            if (!violations.isEmpty()) return violations.get(0).withOthers(violations.subList(1, violations.size()));
            if (!validation.isEmpty()) {
                var text = new StringBuilder();
                validation.forEach(e -> text.append(e.message()).append('\n'));
                return new SetupError(SetupError.VALIDATION, text + "\n  snappea output: " + directory, directory, report, null);
            }
            return null;
        }

        private static SetupError setupError(List<ObservationReport.Error> errors, Path directory, ObservationReport report) {
            var kind = errors.get(0).kind();
            var text = new StringBuilder();
            if (kind.equals("parser")) {
                text.append("Some events do not match their declaration in the P spec files:\n");
                var seen = new java.util.LinkedHashSet<String>();
                for (var error : errors) {
                    if (!error.kind().equals("parser") || !seen.add(error.message())) continue;
                    if (seen.size() > 10) {
                        text.append("  ...\n");
                        break;
                    }
                    var line = parse(error.message());
                    if (line == null) {
                        text.append("  unreadable line: ").append(error.message()).append('\n');
                        if (error.cause() != null) text.append("    ").append(error.cause()).append('\n');
                    } else {
                        var src = Json.text(Json.get(line, "src"));
                        var event = Json.text(Json.get(line, "event"));
                        text.append("  ").append(event == null ? "?" : event).append(src == null ? "" : " (" + src + ")")
                                .append(": ").append(error.cause() == null ? "payload does not match" : error.cause())
                                .append('\n');
                        text.append("    payload: ").append(Json.has(line, "payload") ? Json.write(Json.get(line, "payload")) : "")
                                .append('\n');
                    }
                }
            } else {
                var seen = new java.util.LinkedHashSet<String>();
                for (var error : errors) {
                    if (!seen.add(error.kind() + error.message())) continue;
                    text.append(error.message());
                    if (error.cause() != null) text.append(": ").append(error.cause());
                    var src = sourceOf(error.message());
                    if (src != null) text.append("\n    emitted at ").append(src);
                    text.append('\n');
                }
            }
            return new SetupError(kind, text + "\n  snappea output: " + directory, directory, report, null);
        }

        /** The {@code src} of the line an unknown-event message points at ({@code first at file:line}). */
        private static String sourceOf(String message) {
            var m = Pattern.compile("first at (.+):(\\d+)\\)").matcher(message);
            if (!m.find()) return null;
            try (var lines = Files.lines(Path.of(m.group(1)))) {
                var line = lines.skip(Long.parseLong(m.group(2)) - 1).findFirst().map(Outcomes::parse).orElse(null);
                return Json.text(Json.get(line, "src"));
            } catch (IOException | RuntimeException error) {
                return null;
            }
        }

        /** One event line as an object, or null when it is not one. */
        private static Object parse(String line) {
            try {
                var value = Json.parse(line);
                return value instanceof Map ? value : null;
            } catch (IOException error) {
                return null;
            }
        }

        static SpecViolation violation(ObservationReport.Error error, Path directory, ObservationReport report) {
            String message = error.message() == null ? "" : error.message();
            String location = null;
            boolean liveness = false;
            var assertion = ASSERTION.matcher(message);
            var hot = HOT.matcher(message);
            if (assertion.matches()) {
                location = relative(assertion.group(1));
                message = assertion.group(2);
            } else if (hot.matches()) {
                liveness = true;
                message = "it ended in hot state '" + hot.group(1) + "'";
            } else if (message.isEmpty() || message.equals("null")) {
                message = error.kind();
            }
            var events = new ArrayList<SpecViolation.Event>();
            for (var replay : error.events()) {
                var line = parse(replay.logLine());
                Object payload = null;
                String src = null;
                boolean at = false;
                if (line != null) {
                    payload = Json.get(line, "payload");
                    src = Json.text(Json.get(line, "src"));
                    at = Boolean.TRUE.equals(Json.get(line, "at"));
                }
                events.add(new SpecViolation.Event(replay.event(), replay.key(), payload, src, replay.timestamp(), at));
            }
            return new SpecViolation(error.spec(), error.key() == null ? "" : error.key(), message, location, events,
                    liveness, List.of(), directory, report);
        }

        private static String relative(String location) {
            int colon = location.lastIndexOf(':');
            var path = Path.of(location.substring(0, colon));
            try {
                var cwd = Path.of("").toAbsolutePath();
                var absolute = path.toAbsolutePath().normalize();
                if (absolute.startsWith(cwd)) path = cwd.relativize(absolute);
            } catch (RuntimeException ignored) {
                // keep it as snappea printed it
            }
            return path + location.substring(colon);
        }
    }
}
