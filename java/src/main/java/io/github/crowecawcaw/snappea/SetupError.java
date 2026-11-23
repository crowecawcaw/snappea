package io.github.crowecawcaw.snappea;

import java.nio.file.Path;
import java.util.Optional;
import org.opentest4j.AssertionFailedError;

/**
 * The check could not run or its input is unusable; not a spec violation. Assert on
 * {@link #kind()}, never on the message text.
 */
public final class SetupError extends AssertionFailedError {
    private static final long serialVersionUID = 1L;
    /** No checker binary; the message says how to install one. */
    public static final String BINARY_MISSING = "binary_missing";
    /** Bad options or config file, missing spec files, P compile errors, unknown spec name. */
    public static final String CONFIG = "config";
    /** An event line or payload the core cannot read (including truncated lines). */
    public static final String PARSER = "parser";
    /** {@code strict} found a selected spec with no relevant event. */
    public static final String VALIDATION = "validation";
    /** An event name no loaded P file declares, under {@code unknownEvents = ERROR}. */
    public static final String UNKNOWN_EVENT = "unknown_event";
    /** Live input out of order. */
    public static final String OUT_OF_ORDER = "out_of_order";
    /** The checker exceeded the timeout. */
    public static final String TIMEOUT = "timeout";

    private final String kind;
    private final transient Path artifacts;
    private final transient ObservationReport report;

    SetupError(String kind, String message, Path artifacts, ObservationReport report, Throwable cause) {
        super(message, cause);
        this.kind = kind;
        this.artifacts = artifacts;
        this.report = report;
    }

    static SetupError config(String message) {
        return new SetupError(CONFIG, message, null, null, null);
    }

    /** One of the constants in this class, or another kind copied from the core report. */
    public String kind() { return kind; }

    /** The folder holding the events, snappea's console output and its report; null when none was made. */
    public Path artifacts() { return artifacts; }

    /** The parsed JSON report, when snappea wrote a valid one. */
    public Optional<ObservationReport> report() { return Optional.ofNullable(report); }
}
