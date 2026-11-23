package io.github.crowecawcaw.snappea;

import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import org.opentest4j.AssertionFailedError;

/**
 * A P spec was broken by the recorded events. An {@code AssertionFailedError}, so test
 * frameworks and property-test shrinkers treat it as an ordinary assertion failure.
 */
public final class SpecViolation extends AssertionFailedError {
    private static final long serialVersionUID = 1L;
    /** Events shown in the failure message, counting back from the one that broke the spec. */
    static final int SHOWN = 20;

    /** One event of the violating key: as emitted, with where it was emitted. */
    public record Event(String event, String key, Object payload, String src, String timestamp, boolean at) {}

    private final String spec;
    private final String key;
    private final String message;
    private final String location;
    private final transient List<Event> events;
    private final boolean liveness;
    private final transient List<SpecViolation> others;
    private final transient Path artifacts;
    private final transient ObservationReport report;

    SpecViolation(String spec, String key, String message, String location, List<Event> events, boolean liveness,
                  List<SpecViolation> others, Path artifacts, ObservationReport report) {
        super(render(spec, key, message, location, events, liveness, others, artifacts));
        this.artifacts = artifacts;
        this.report = report;
        this.spec = spec;
        this.key = key;
        this.message = message;
        this.location = location;
        this.events = List.copyOf(events);
        this.liveness = liveness;
        this.others = List.copyOf(others);
    }

    SpecViolation withOthers(List<SpecViolation> more) {
        return new SpecViolation(spec, key, message, location, events, liveness, more, artifacts(), report().orElse(null));
    }

    /** The broken spec. */
    public String spec() { return spec; }
    /** The key (stream) the violation happened on; "" for the shared stream. */
    public String key() { return key; }
    /** The spec's assert message, or for liveness which hot state it ended in. */
    public String message() { return message; }
    /** {@code file:line} of the failed assert in the P sources, when known. */
    public String location() { return location; }
    /** The key's events up to and including the violating one (the last ones). */
    public List<Event> events() { return events; }
    /** True when the key ended in a hot state under {@code checkHot}. */
    public boolean liveness() { return liveness; }
    /** Further violations found by the same check. */
    public List<SpecViolation> others() { return others; }

    /** The folder holding the events, snappea's console output and its report; null when none was made. */
    public Path artifacts() { return artifacts; }

    /** The parsed JSON report, when snappea wrote a valid one. */
    public Optional<ObservationReport> report() { return Optional.ofNullable(report); }

    private static String render(String spec, String key, String message, String location, List<Event> events,
                                 boolean liveness, List<SpecViolation> others, Path artifacts) {
        var out = new StringBuilder();
        out.append(liveness ? "P spec " + spec + " was still waiting when the events ended: " + message
                : "P spec " + spec + " was broken: " + message).append("\n\n");
        out.append(key == null || key.isEmpty() ? "  Events:\n" : "  Events (key \"" + key + "\"):\n");
        int earlier = Math.max(0, events.size() - SHOWN);
        if (earlier > 0) out.append("     ... ").append(earlier).append(" earlier event(s)\n");
        var shown = events.subList(earlier, events.size());
        int eventWidth = 0;
        int payloadWidth = 0;
        for (var event : shown) {
            eventWidth = Math.max(eventWidth, event.event().length());
            payloadWidth = Math.max(payloadWidth, payloadText(event.payload()).length());
        }
        payloadWidth = Math.min(payloadWidth, 48);
        for (int i = 0; i < shown.size(); i++) {
            var event = shown.get(i);
            var row = new StringBuilder(String.format("    %3d  %-" + Math.max(1, eventWidth) + "s  %-"
                    + Math.max(1, payloadWidth) + "s  %s", earlier + i + 1, event.event(),
                    payloadText(event.payload()), event.src() == null ? "" : event.src()));
            if (event.at()) row.append("  (time set with emitAt)");
            if (i == shown.size() - 1) row.append(liveness ? "  <- events ended here" : "  <- broke the spec here");
            out.append(i < shown.size() - 1 ? row.toString().stripTrailing() : row).append('\n');
        }
        if (shown.isEmpty()) out.append("    (none)\n");
        out.append('\n');
        if (location != null) out.append("  Spec: ").append(location).append('\n');
        if (artifacts != null) out.append("  snappea output: ").append(artifacts).append('\n');
        if (!others.isEmpty()) {
            out.append("\n  ").append(others.size()).append(" more violation(s):\n");
            for (var other : others) {
                out.append("    ").append(other.spec);
                if (other.key != null && !other.key.isEmpty()) out.append(" (key \"").append(other.key).append("\")");
                out.append(": ").append(other.message).append('\n');
            }
        }
        return out.toString().stripTrailing();
    }

    static String payloadText(Object payload) {
        if (payload == null) return "";
        if (payload instanceof Map<?, ?> map) {
            var out = new StringBuilder();
            for (var entry : map.entrySet()) {
                if (out.length() > 0) out.append(' ');
                out.append(entry.getKey()).append('=').append(shortText(entry.getValue()));
            }
            return out.toString();
        }
        return shortText(payload);
    }

    private static String shortText(Object value) {
        String text;
        if (value instanceof String s) text = s;
        else {
            try { text = Json.write(value); }
            catch (RuntimeException error) { text = String.valueOf(value); }
        }
        return text.length() <= 40 ? text : text.substring(0, 37) + "...";
    }
}
