package io.github.crowecawcaw.snappea.emit;

import java.time.Instant;
import java.time.LocalDateTime;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;
import java.util.Set;
import java.util.function.Consumer;

/**
 * The connection between {@link SnappeaEmit} and the test helpers (package {@code io.github.crowecawcaw.snappea}).
 * Not application API: application code calls {@link SnappeaEmit}.
 */
public final class EmitHook {
    /** Routes events emitted in this JVM to the recordings of a running test. */
    public interface Router {
        /** Where an event emitted on the current thread goes, or null when no recording takes it. */
        Consumer<String> target();

        /**
         * An event for the recordings {@link #target()} named could not be encoded: they fail
         * their check with {@code error} instead of {@code emit} throwing into application code.
         */
        void failed(RuntimeException error);
    }

    private static final Object LOCK = new Object();
    private static final DateTimeFormatter SECONDS = DateTimeFormatter.ofPattern("uuuu-MM-dd'T'HH:mm:ss");
    private static final Set<String> SKIPPED = Set.of(SnappeaEmit.class.getName(), EmitHook.class.getName(),
            "io.github.crowecawcaw.snappea.Recording");
    private static final StackWalker WALKER = StackWalker.getInstance();
    private static final long PID = ProcessHandle.current().pid();
    private static long seq;
    private static Instant last = Instant.EPOCH;

    private EmitHook() {}

    /** Installs the router of the test helpers; null removes it. */
    public static void install(Router router) {
        SnappeaEmit.router = router;
    }

    /**
     * One event as a JSON line in the events folder format (without the newline). Assigns
     * the process's next {@code seq}; with {@code at == null} the time is the wall clock,
     * never earlier than the previous event's.
     */
    public static String line(Instant at, String event, String key, Object payload, String src) {
        if (event == null || event.isBlank()) throw new IllegalArgumentException("event must not be blank");
        var body = new StringBuilder(128);
        Json.write(body, payload); // a snapshot: later changes to the payload do not matter
        Instant time;
        long n;
        synchronized (LOCK) {
            n = ++seq;
            if (at == null) {
                var now = Instant.now();
                if (now.isBefore(last)) now = last;
                last = now;
                time = now;
            } else {
                time = at;
            }
        }
        var out = new StringBuilder(body.length() + 160);
        out.append("{\"key\":");
        Json.string(out, key == null ? "" : key);
        out.append(",\"timestamp\":\"").append(timestamp(time)).append("\",\"event\":");
        Json.string(out, event);
        out.append(",\"payload\":").append(body);
        if (src != null) {
            out.append(",\"src\":");
            Json.string(out, src);
        }
        out.append(",\"seq\":").append(n).append(",\"pid\":").append(PID);
        if (at != null) out.append(",\"at\":true");
        return out.append('}').toString();
    }

    /** ISO-8601 UTC with nanoseconds, which snappea orders exactly. */
    public static String timestamp(Instant time) {
        var seconds = LocalDateTime.ofEpochSecond(time.getEpochSecond(), 0, ZoneOffset.UTC);
        return SECONDS.format(seconds) + String.format(".%09dZ", time.getNano());
    }

    /** {@code package/File.java:line} of the first caller outside the emit classes, or null. */
    public static String callSite() {
        return WALKER.walk(frames -> frames.filter(f -> !SKIPPED.contains(f.getClassName())).findFirst()
                .map(EmitHook::format).orElse(null));
    }

    private static String format(StackWalker.StackFrame frame) {
        String file = frame.getFileName();
        String type = frame.getClassName();
        int dot = type.lastIndexOf('.');
        if (file == null) file = type.substring(dot + 1) + ".java";
        String folder = dot < 0 ? "" : type.substring(0, dot).replace('.', '/') + "/";
        return frame.getLineNumber() > 0 ? folder + file + ":" + frame.getLineNumber() : folder + file;
    }
}
