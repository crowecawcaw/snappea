package io.github.crowecawcaw.snappea;

import java.lang.reflect.AnnotatedElement;
import java.nio.file.Path;
import java.time.Duration;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.extension.AfterEachCallback;
import org.junit.jupiter.api.extension.BeforeEachCallback;
import org.junit.jupiter.api.extension.ExtensionContext;
import org.junit.jupiter.api.extension.LifecycleMethodExecutionExceptionHandler;
import org.junit.jupiter.api.extension.ParameterContext;
import org.junit.jupiter.api.extension.ParameterResolutionException;
import org.junit.jupiter.api.extension.ParameterResolver;
import org.junit.jupiter.api.extension.RegisterExtension;
import org.junit.jupiter.api.extension.TestExecutionExceptionHandler;

/**
 * Checks each Jupiter test, including its {@code @BeforeEach} and {@code @AfterEach}: a
 * recording starts before them and is checked after them.
 *
 * <p>Register it with {@code @RegisterExtension} and {@link #builder()}, with
 * {@code @ExtendWith(SnappeaExtension.class)}, or for every test class with
 * {@code junit.jupiter.extensions.autodetection.enabled=true}. The last two read their
 * settings from {@code snappea.properties} (or {@code snappea.*} keys in
 * {@code junit-platform.properties}) and the {@code SNAPPEA_*} environment variables.
 */
public final class SnappeaExtension implements BeforeEachCallback, AfterEachCallback, ParameterResolver,
        TestExecutionExceptionHandler, LifecycleMethodExecutionExceptionHandler {
    private static final ExtensionContext.Namespace ROOT = ExtensionContext.Namespace.create(SnappeaExtension.class, "coverage");
    private final ExtensionContext.Namespace namespace = ExtensionContext.Namespace.create(SnappeaExtension.class, this);
    private final boolean explicit;
    private final String name;
    private volatile SnappeaObserver observer;

    /** Settings from the config file and environment; used by {@code @ExtendWith} and autodetection. */
    public SnappeaExtension() {
        explicit = false;
        name = "snappea";
    }

    private SnappeaExtension(Builder builder) {
        explicit = true;
        observer = builder.observer.build();
        name = SnappeaObserver.nonempty(builder.name, "extension name");
    }

    public static Builder builder() { return new Builder(); }

    private SnappeaObserver observer() {
        var current = observer;
        if (current == null) {
            synchronized (this) {
                if (observer == null) observer = SnappeaObserver.builder().build();
                current = observer;
            }
        }
        return current;
    }

    /** A configured instance on the test class replaces this one (autodetected or {@code @ExtendWith}). */
    private boolean steppingAside(ExtensionContext context) {
        if (explicit) return false;
        for (Class<?> type = context.getRequiredTestClass(); type != null; type = type.getEnclosingClass()) {
            for (Class<?> c = type; c != null && c != Object.class; c = c.getSuperclass()) {
                for (var field : c.getDeclaredFields()) {
                    if (SnappeaExtension.class.isAssignableFrom(field.getType()) && field.isAnnotationPresent(RegisterExtension.class))
                        return true;
                }
            }
        }
        return false;
    }

    private static Snappea annotation(ExtensionContext context) {
        var method = context.getTestMethod().map(m -> m.getAnnotation(Snappea.class)).orElse(null);
        if (method != null) return method;
        for (Class<?> type = context.getRequiredTestClass(); type != null; type = type.getEnclosingClass()) {
            var found = ((AnnotatedElement) type).getAnnotation(Snappea.class);
            if (found != null) return found;
        }
        return null;
    }

    @Override public void beforeEach(ExtensionContext context) {
        if (steppingAside(context)) return;
        var config = observer();
        var options = annotation(context);
        String label = context.getUniqueId() + " [" + name + "] " + context.getDisplayName();
        boolean recorded = switch (config.mode()) {
            case AUTO -> true;
            case MARKED -> options != null;
            case OFF -> false;
        };
        List<String> skip = options == null ? List.of() : Arrays.asList(options.skip());
        var effective = recorded && !skip.contains(Snappea.ALL)
                ? config.narrowed(skip, options != null && options.checkHot()).orElse(null) : null;
        if (effective == null) {
            var state = new State(null, label);
            state.binding = Routing.detach(); // SnappeaEmit in this test goes to no other test's recording
            context.getStore(namespace).put("state", state);
            return;
        }
        CoverageCollector coverage = null;
        var destination = effective.coverageDestination();
        if (destination.isPresent() || !effective.coverageOff()) {
            // Closed when the test run ends: writes each destination, or only prints the
            // coverage of the checks without one.
            var registry = context.getRoot().getStore(ROOT).getOrComputeIfAbsent(
                    "registry", ignored -> new CoverageRegistry(), CoverageRegistry.class);
            coverage = destination.isPresent() ? registry.collector(effective, destination.get()) : registry.printed(effective);
        }
        var recording = coverage == null ? effective.recording(label) : effective.recording(label, coverage);
        context.getStore(namespace).put("state", new State(recording, label));
    }

    @Override public void afterEach(ExtensionContext context) {
        var state = context.getStore(namespace).remove("state", State.class);
        if (state == null) return; // A preceding callback may have failed before our setup.
        if (state.recording == null) {
            Routing.restore(state.binding);
            if (state.inert != null) state.inert.close();
            return;
        }
        try {
            state.recording.close();
        } catch (SpecViolation | SetupError failure) {
            var primary = state.primary != null ? state.primary : context.getExecutionException().orElse(null);
            if (primary != null && !(primary instanceof org.opentest4j.TestAbortedException)) {
                primary.addSuppressed(failure); // the test already failed: attach, never replace
            } else {
                if (primary != null) failure.addSuppressed(primary);
                throw failure;
            }
        }
    }

    @Override public boolean supportsParameter(ParameterContext parameter, ExtensionContext context) {
        if (parameter.getParameter().getType() != Recording.class) return false;
        if (context.getTestClass().isPresent() && steppingAside(context)) return false;
        var qualifier = parameter.findAnnotation(SnappeaRecording.class);
        return qualifier.isEmpty() || qualifier.get().value().equals(name);
    }

    @Override public Object resolveParameter(ParameterContext parameter, ExtensionContext context) {
        var state = context.getStore(namespace).get("state", State.class);
        if (state == null) throw new ParameterResolutionException("Recording injection is available only in @BeforeEach, "
                + "test methods and @AfterEach; use explicit observer.recording(...) in constructors/@BeforeAll");
        if (state.recording != null) return state.recording;
        if (state.inert == null) state.inert = Recording.inert(state.label); // not checked: off, unmarked or skipped
        return state.inert;
    }

    private void remember(ExtensionContext context, Throwable error) throws Throwable {
        var state = context.getStore(namespace).get("state", State.class);
        if (state != null && state.primary == null) state.primary = error;
        throw error;
    }

    @Override public void handleTestExecutionException(ExtensionContext context, Throwable error) throws Throwable {
        remember(context, error);
    }
    @Override public void handleBeforeEachMethodExecutionException(ExtensionContext context, Throwable error) throws Throwable {
        remember(context, error);
    }
    @Override public void handleAfterEachMethodExecutionException(ExtensionContext context, Throwable error) throws Throwable {
        remember(context, error);
    }

    private static final class State {
        private final Recording recording;
        private final String label;
        private Recording inert;
        private Object binding;
        private Throwable primary;
        private State(Recording recording, String label) {
            this.recording = recording;
            this.label = label;
        }
    }

    private static final class CoverageRegistry implements ExtensionContext.Store.CloseableResource {
        private record Entry(String configuration, CoverageCollector collector) {}
        private final Map<Path, Entry> entries = new LinkedHashMap<>();
        private CoverageCollector printed;

        /** The print-only collector for checks without a destination; the first check's binary merges. */
        synchronized CoverageCollector printed(SnappeaObserver observer) {
            if (printed == null) printed = observer.coverage();
            return printed;
        }

        synchronized CoverageCollector collector(SnappeaObserver observer, Path destination) {
            var existing = entries.get(destination);
            if (existing != null) {
                if (!existing.configuration.equals(observer.coverageCompatibilityKey()))
                    throw new IllegalArgumentException("Conflicting binary, timeout or artifact settings for coverage " + destination);
                return existing.collector;
            }
            var collector = observer.coverage(destination);
            entries.put(destination, new Entry(observer.coverageCompatibilityKey(), collector));
            return collector;
        }

        @Override public synchronized void close() {
            RuntimeException failure = null;
            for (var entry : entries.values()) {
                try { entry.collector.close(); }
                catch (RuntimeException error) {
                    if (failure == null) failure = error;
                    else failure.addSuppressed(error);
                }
            }
            if (printed != null) printed.close(); // never throws: failures are warnings
            if (failure != null) throw failure;
        }
    }

    public static final class Builder {
        private final SnappeaObserver.Builder observer = SnappeaObserver.builder();
        private String name = "snappea";

        private Builder() {}
        public Builder specFiles(Path... values) { observer.specFiles(values); return this; }
        public Builder specFiles(String... values) { observer.specFiles(values); return this; }
        public Builder specs(String... values) { observer.specs(values); return this; }
        public Builder exclude(String... values) { observer.exclude(values); return this; }
        public Builder binary(String value) { observer.binary(value); return this; }
        public Builder artifacts(Path value) { observer.artifacts(value); return this; }
        public Builder timeout(Duration value) { observer.timeout(value); return this; }
        public Builder strict(boolean value) { observer.strict(value); return this; }
        public Builder checkHot(boolean value) { observer.checkHot(value); return this; }
        public Builder unknownEvents(UnknownEvents value) { observer.unknownEvents(value); return this; }
        /**
         * Suite coverage JSON, written once when the test run ends. Default: {@code SNAPPEA_COVERAGE},
         * then the config file's {@code coverage}; without either, the run's suite coverage is only
         * printed. {@code SNAPPEA_COVERAGE=off} (or {@code coverage = off}) turns it off.
         */
        public Builder coverage(Path value) { observer.coverage(value); return this; }
        /** Default: {@code SNAPPEA_MODE}, then the config file, then {@link Mode#AUTO}. */
        public Builder mode(Mode value) { observer.mode(value); return this; }
        /** Distinguishes several extensions on one class; qualify parameters with {@code @SnappeaRecording(name)}. */
        public Builder name(String value) { name = value; return this; }
        Builder config(SnappeaConfig value) { observer.config(value); return this; }
        public SnappeaExtension build() { return new SnappeaExtension(this); }
    }
}
