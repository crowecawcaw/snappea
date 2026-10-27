package io.github.crowecawcaw.snappea;

import java.io.File;
import java.io.IOException;
import java.nio.file.FileSystems;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Duration;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Immutable check configuration. Each {@link #recording} is an independent check.
 *
 * <p>Every setting is taken from, in order: the builder, the environment variable
 * ({@code SNAPPEA_SPEC_FILES}, {@code SNAPPEA_BIN}, {@code SNAPPEA_ARTIFACTS}, {@code SNAPPEA_COVERAGE},
 * {@code SNAPPEA_MODE}), the config file ({@code snappea.properties}, or {@code snappea.*} keys in
 * {@code junit-platform.properties}), then the default.
 */
public final class SnappeaObserver {
    static final String INSTALL = "Install snappea with `cargo install snappea` "
            + "(or `cargo build -p snappea` in a checkout), then set SNAPPEA_BIN to the binary or put snappea on PATH.";
    static final String NATIVES_DOC = "https://github.com/crowecawcaw/snappea/tree/main/java#install";
    private final List<Path> files;
    private final List<String> specs;
    private final List<String> exclude;
    private final String binary;
    private final Path artifacts;
    private final Duration timeout;
    private final boolean strict;
    private final boolean checkHot;
    private final UnknownEvents unknownEvents;
    private final Path coverage;
    private final boolean coverageOff;
    private final Mode mode;
    private final Map<String, String> environment;

    private SnappeaObserver(Builder builder) {
        var env = builder.environment != null ? builder.environment : System.getenv();
        var config = builder.config != null ? builder.config : SnappeaConfig.load();
        var cwd = Path.of("").toAbsolutePath();
        environment = env;
        if (!builder.files.isEmpty()) {
            files = expand(builder.files, cwd, "specFiles");
        } else if (nonblank(env.get("SNAPPEA_SPEC_FILES"))) {
            files = expand(Arrays.stream(env.get("SNAPPEA_SPEC_FILES").split(Pattern.quote(File.pathSeparator)))
                    .filter(s -> !s.isBlank()).toList(), cwd, "SNAPPEA_SPEC_FILES");
        } else if (config.list("spec_files") != null) {
            files = expand(config.list("spec_files"), config.base(), config.where() + " spec_files");
        } else {
            throw SetupError.config("No P spec files: set spec_files in snappea.properties, SNAPPEA_SPEC_FILES, "
                    + "or .specFiles(...) on the builder.");
        }
        specs = names(builder.specs != null ? builder.specs : orEmpty(config.list("specs")), "spec");
        exclude = names(builder.exclude != null ? builder.exclude : orEmpty(config.list("exclude")), "exclude");
        if (!specs.isEmpty() && !exclude.isEmpty())
            throw SetupError.config("exclude applies only when specs is not set (every spec is checked)");
        var configStrict = config.bool("strict");
        strict = builder.strict != null ? builder.strict : configStrict != null ? configStrict : !specs.isEmpty();
        var configHot = config.bool("check_hot");
        checkHot = builder.checkHot != null ? builder.checkHot : configHot != null && configHot;
        unknownEvents = builder.unknownEvents != null ? builder.unknownEvents
                : config.get("unknown_events") != null ? UnknownEvents.parse(config.get("unknown_events"), config.where())
                : UnknownEvents.ERROR;
        binary = builder.binary != null ? builder.binary : nonblank(env.get("SNAPPEA_BIN")) ? env.get("SNAPPEA_BIN") : null;
        if (builder.artifacts != null) artifacts = builder.artifacts.toAbsolutePath().normalize();
        else if (nonblank(env.get("SNAPPEA_ARTIFACTS"))) artifacts = cwd.resolve(env.get("SNAPPEA_ARTIFACTS")).normalize();
        else if (config.get("artifacts") != null) artifacts = config.base().resolve(config.get("artifacts")).normalize();
        else artifacts = Path.of(System.getProperty("java.io.tmpdir"), "snappea").toAbsolutePath();
        if (builder.timeout != null) timeout = builder.timeout;
        else if (config.get("timeout") != null) timeout = duration(config.get("timeout"), config.where());
        else timeout = Duration.ofSeconds(30);
        if (timeout.isNegative() || timeout.toMillis() < 1)
            throw SetupError.config("timeout must be at least one millisecond, not " + timeout);
        // "off" (environment or config file) turns suite coverage off instead of naming a path.
        var coverageSetting = builder.coverage != null ? null
                : nonblank(env.get("SNAPPEA_COVERAGE")) ? env.get("SNAPPEA_COVERAGE") : config.get("coverage");
        coverageOff = "off".equals(coverageSetting);
        if (builder.coverage != null) coverage = builder.coverage.toAbsolutePath().normalize();
        else if (coverageOff) coverage = null;
        else if (nonblank(env.get("SNAPPEA_COVERAGE"))) coverage = cwd.resolve(env.get("SNAPPEA_COVERAGE")).normalize();
        else if (config.get("coverage") != null) coverage = config.base().resolve(config.get("coverage")).normalize();
        else coverage = null;
        if (builder.mode != null) mode = builder.mode;
        else if (nonblank(env.get("SNAPPEA_MODE"))) mode = Mode.parse(env.get("SNAPPEA_MODE"), "SNAPPEA_MODE");
        else if (config.get("mode") != null) mode = Mode.parse(config.get("mode"), config.where());
        else mode = Mode.AUTO;
    }

    private SnappeaObserver(SnappeaObserver base, List<String> specs, List<String> exclude, boolean checkHot) {
        files = base.files;
        this.specs = specs;
        this.exclude = exclude;
        binary = base.binary;
        artifacts = base.artifacts;
        timeout = base.timeout;
        strict = base.strict;
        this.checkHot = checkHot;
        unknownEvents = base.unknownEvents;
        coverage = base.coverage;
        coverageOff = base.coverageOff;
        mode = base.mode;
        environment = base.environment;
    }

    /**
     * This configuration without the {@code skip} specs and with {@code checkHot} turned on
     * when asked; empty when nothing is left to check. Used for per-test options.
     */
    Optional<SnappeaObserver> narrowed(List<String> skip, boolean hot) {
        if (skip.isEmpty() && (!hot || checkHot)) return Optional.of(this);
        if (!specs.isEmpty()) {
            var left = specs.stream().filter(s -> !skip.contains(s)).toList();
            return left.isEmpty() ? Optional.empty() : Optional.of(new SnappeaObserver(this, left, exclude, checkHot || hot));
        }
        var more = new ArrayList<>(exclude);
        skip.stream().filter(s -> !more.contains(s)).forEach(more::add);
        return Optional.of(new SnappeaObserver(this, specs, List.copyOf(more), checkHot || hot));
    }

    private static boolean nonblank(String value) { return value != null && !value.isBlank(); }
    private static List<String> orEmpty(List<String> value) { return value == null ? List.of() : value; }

    private static List<String> names(List<String> values, String what) {
        for (var value : values)
            if (value == null || value.isBlank()) throw SetupError.config(what + " names must not be blank");
        return values.stream().distinct().toList();
    }

    static String nonempty(String value, String name) {
        if (value == null || value.isBlank()) throw new IllegalArgumentException(name + " must not be blank");
        return value;
    }

    static Duration duration(String text, String where) {
        var value = text.trim();
        try {
            if (value.startsWith("P") || value.startsWith("p")) return Duration.parse(value);
            Matcher m = Pattern.compile("(\\d+(?:\\.\\d+)?)\\s*(ms|s|m)?").matcher(value);
            if (m.matches()) {
                double number = Double.parseDouble(m.group(1));
                String unit = m.group(2) == null ? "s" : m.group(2);
                double millis = switch (unit) { case "ms" -> number; case "m" -> number * 60_000; default -> number * 1000; };
                return Duration.ofMillis(Math.round(millis));
            }
        } catch (RuntimeException ignored) {
            // reported below
        }
        throw SetupError.config(where + ": timeout must be like 30s, 500ms, 2m or PT30S, not '" + text + "'");
    }

    /** Paths, folders (every .p file in them) and globs ({@code specs/*.p}, {@code **}) to P files. */
    static List<Path> expand(List<?> entries, Path base, String where) {
        var out = new LinkedHashSet<Path>();
        for (var entry : entries) {
            String text = entry.toString();
            if (text.isBlank()) continue;
            var matched = new ArrayList<Path>();
            if (entry instanceof String && text.matches(".*[*?\\[{].*")) {
                var pattern = base.resolve(text).normalize().toString();
                var root = base.resolve(text.substring(0, globStart(text))).normalize();
                while (!Files.isDirectory(root) && root.getParent() != null) root = root.getParent();
                var matcher = FileSystems.getDefault().getPathMatcher("glob:" + pattern.replace("\\", "\\\\"));
                try (var paths = Files.walk(root)) {
                    paths.filter(matcher::matches).sorted().forEach(matched::add);
                } catch (IOException error) {
                    throw new SetupError(SetupError.CONFIG, where + ": cannot list " + root + ": " + error, null, null, error);
                }
            } else {
                var path = base.resolve(text).normalize();
                if (Files.exists(path)) matched.add(path);
            }
            if (matched.isEmpty())
                throw SetupError.config(where + ": '" + text + "' matches no files (relative to " + base + ")");
            for (var path : matched) {
                if (Files.isDirectory(path)) {
                    try (var paths = Files.walk(path)) {
                        paths.filter(p -> p.toString().endsWith(".p") && Files.isRegularFile(p)).sorted().forEach(out::add);
                    } catch (IOException error) {
                        throw new SetupError(SetupError.CONFIG, where + ": cannot list " + path + ": " + error, null, null, error);
                    }
                } else {
                    out.add(path.toAbsolutePath());
                }
            }
        }
        if (out.isEmpty()) throw SetupError.config(where + ": no .p files found");
        return List.copyOf(out);
    }

    private static int globStart(String text) {
        int first = text.length();
        for (char c : new char[] {'*', '?', '[', '{'}) {
            int at = text.indexOf(c);
            if (at >= 0) first = Math.min(first, at);
        }
        int slash = Math.max(text.lastIndexOf('/', first), text.lastIndexOf(File.separatorChar, first));
        return slash < 0 ? 0 : slash;
    }

    public static Builder builder() { return new Builder(); }

    /** Starts a recording: try-with-resources checks it when the block ends. */
    public Recording recording(String label) { return new Recording(this, label, null); }

    /** Starts a recording whose coverage goes to {@code coverage}. */
    public Recording recording(String label, CoverageCollector coverage) {
        Objects.requireNonNull(coverage).requireCompatible(this);
        return new Recording(this, label, coverage);
    }

    /** Same as {@link #recording(String)}: the manual form, with {@code stop()} and {@code check()}. */
    public Recording start(String label) { return recording(label); }

    /** A suite coverage collector writing to {@code destination} when closed. */
    public CoverageCollector coverage(Path destination) {
        return new CoverageCollector(this, destination, false);
    }

    /**
     * A suite coverage collector for the configured destination ({@code SNAPPEA_COVERAGE} or the
     * config file's {@code coverage}). With none configured, closing it only prints the coverage
     * summary and writes no file; with {@code SNAPPEA_COVERAGE=off} (or {@code coverage = off}) it
     * does nothing. Close it once, after the last recording, for example in {@code @AfterAll}.
     */
    public CoverageCollector coverage() {
        if (coverage != null) return new CoverageCollector(this, coverage, false);
        return new CoverageCollector(this, null, !coverageOff);
    }

    /** Used to reject collectors sharing a destination but using different CLI settings. */
    String coverageCompatibilityKey() {
        return binary + "\n" + timeout + "\n" + artifacts;
    }

    List<String> command(String executable, List<Path> logs, Path report) {
        var args = new ArrayList<>(List.of(executable, "observe"));
        files.forEach(p -> args.add(p.toString())); // positional .p sources (absolute paths)
        if (!specs.isEmpty()) {
            for (var spec : specs) args.addAll(List.of("--spec", spec));
        } else {
            args.add("--all-specs");
            for (var spec : exclude) args.addAll(List.of("--exclude-specs", spec));
        }
        // One -l per log, so a log path is never taken for a source (.p/.pproj) or an option.
        logs.forEach(p -> args.addAll(List.of("-l", p.toString())));
        // Text on the console (kept in console.txt for people), whatever CI or SNAPPEA_FORMAT say;
        // the result is read from the JSON report.
        args.addAll(List.of("--format", "text", "--json-report", report.toString(),
                "--unknown-events", unknownEvents.option()));
        if (strict) args.add("--strict");
        if (checkHot) args.add("--check-hot");
        return args;
    }

    /**
     * The checker binary: explicit, then {@code SNAPPEA_BIN}, then the {@code snappea} natives jar for
     * this platform on the classpath, then {@code snappea} on {@code PATH}.
     */
    String executable() {
        String configured = binary;
        if (configured != null && (configured.contains("/") || configured.contains(File.separator))) {
            var path = Path.of(configured);
            if (Files.isRegularFile(path) && Files.isExecutable(path)) return path.toAbsolutePath().toString();
            throw new SetupError(SetupError.BINARY_MISSING, "The snappea binary " + configured
                    + (configured.equals(environment.get("SNAPPEA_BIN")) ? " (from SNAPPEA_BIN)" : "")
                    + " does not exist or is not executable. " + INSTALL, null, null, null);
        }
        String name = configured == null ? "snappea" : configured;
        var classifier = BundledBinary.classifier();
        if (configured == null && classifier != null) {
            var bundled = bundled(classifier);
            if (bundled != null) return bundled.toString();
        }
        var path = environment.getOrDefault("PATH", "");
        var extensions = File.separatorChar == '\\' ? List.of("", ".exe", ".cmd", ".bat") : List.of("");
        for (var folder : path.split(Pattern.quote(File.pathSeparator))) {
            if (folder.isBlank()) continue;
            for (var extension : extensions) {
                var candidate = Path.of(folder, name + extension);
                if (Files.isRegularFile(candidate) && Files.isExecutable(candidate)) return candidate.toString();
            }
        }
        if (configured != null) throw new SetupError(SetupError.BINARY_MISSING, "No snappea binary: '" + name
                + "' is not on PATH. " + INSTALL, null, null, null);
        if (classifier == null) throw new SetupError(SetupError.BINARY_MISSING, "No snappea binary: SNAPPEA_BIN is not set, "
                + "snappea has no natives jar for " + System.getProperty("os.name") + " " + System.getProperty("os.arch")
                + ", and 'snappea' is not on PATH. " + INSTALL, null, null, null);
        throw new SetupError(SetupError.BINARY_MISSING, "No snappea binary: SNAPPEA_BIN is not set, the natives jar for "
                + "this platform is not on the classpath, and 'snappea' is not on PATH. Add the test dependency "
                + "io.github.crowecawcaw:snappea:" + BundledBinary.version() + ":" + classifier + " (see " + NATIVES_DOC
                + "), or " + Character.toLowerCase(INSTALL.charAt(0)) + INSTALL.substring(1), null, null, null);
    }

    /** The binary from the natives jar on the context or this class's class loader, or null. */
    private Path bundled(String classifier) {
        var root = BundledBinary.cacheRoot(environment);
        var version = BundledBinary.version();
        var loaders = new LinkedHashSet<ClassLoader>();
        if (Thread.currentThread().getContextClassLoader() != null) loaders.add(Thread.currentThread().getContextClassLoader());
        if (SnappeaObserver.class.getClassLoader() != null) loaders.add(SnappeaObserver.class.getClassLoader());
        for (var loader : loaders) {
            try {
                var found = BundledBinary.find(loader, classifier, root, version);
                if (found != null) return found;
            } catch (IOException e) {
                throw new SetupError(SetupError.BINARY_MISSING, "The snappea binary in the snappea " + classifier
                        + " jar could not be used (cache " + root + "): " + e.getMessage()
                        + ". Set XDG_CACHE_HOME to a writable folder, or SNAPPEA_BIN to a snappea binary.", null, null, e);
            }
        }
        return null;
    }

    Path artifacts() { return artifacts; }
    Duration timeout() { return timeout; }
    boolean strict() { return strict; }
    boolean checkHot() { return checkHot; }
    /** The named specs; empty means every spec in the spec files. */
    List<String> specs() { return specs; }
    List<String> exclude() { return exclude; }
    List<Path> files() { return files; }
    UnknownEvents unknownEvents() { return unknownEvents; }
    Optional<Path> coverageDestination() { return Optional.ofNullable(coverage); }
    /** True when SNAPPEA_COVERAGE or the config file's coverage is {@code off}. */
    boolean coverageOff() { return coverageOff; }
    Mode mode() { return mode; }

    public static final class Builder {
        private final List<Object> files = new ArrayList<>();
        private List<String> specs;
        private List<String> exclude;
        private String binary;
        private Path artifacts;
        private Duration timeout;
        private Boolean strict;
        private Boolean checkHot;
        private UnknownEvents unknownEvents;
        private Path coverage;
        private Mode mode;
        private Map<String, String> environment;
        private SnappeaConfig config;

        private Builder() {}
        /** P files or folders (every .p file in them); relative paths resolve against the working directory. */
        public Builder specFiles(Path... values) {
            Arrays.stream(values).map(Objects::requireNonNull).forEach(files::add);
            return this;
        }
        /** Paths, folders or globs such as {@code specs/*.p}; relative to the working directory. */
        public Builder specFiles(String... values) {
            Arrays.stream(values).map(Objects::requireNonNull).forEach(files::add);
            return this;
        }
        /** Specs to check. Default: every spec in the spec files. */
        public Builder specs(String... values) { specs = List.of(values); return this; }
        /** Specs left out when {@code specs} is not set. */
        public Builder exclude(String... values) { exclude = List.of(values); return this; }
        /** Executable name on PATH, or a path; never a shell command. Default: {@code SNAPPEA_BIN}, then {@code snappea}. */
        public Builder binary(String value) { binary = Objects.requireNonNull(value); return this; }
        /** Where failed checks keep their events and reports. Default: a {@code snappea} folder in the temp folder. */
        public Builder artifacts(Path value) { artifacts = Objects.requireNonNull(value); return this; }
        /** Per checker run. Default: 30 seconds. */
        public Builder timeout(Duration value) { timeout = Objects.requireNonNull(value); return this; }
        /** Every selected spec must check an event. Default: true when {@code specs} is set. */
        public Builder strict(boolean value) { strict = value; return this; }
        /** A key whose spec ends in a hot state is a violation. Default: false. */
        public Builder checkHot(boolean value) { checkHot = value; return this; }
        /** Event names no P file declares. Default: {@link UnknownEvents#ERROR}. */
        public Builder unknownEvents(UnknownEvents value) { unknownEvents = Objects.requireNonNull(value); return this; }
        /**
         * Suite coverage JSON, for {@link SnappeaObserver#coverage()} and the JUnit extension.
         * Default: {@code SNAPPEA_COVERAGE}, then the config file's {@code coverage}; without
         * either, the suite coverage is printed but not written.
         */
        public Builder coverage(Path value) { coverage = Objects.requireNonNull(value); return this; }
        /** Which tests the JUnit extension records. Default: {@link Mode#AUTO}. */
        public Builder mode(Mode value) { mode = Objects.requireNonNull(value); return this; }
        Builder environment(Map<String, String> value) { environment = Map.copyOf(value); return this; }
        Builder config(SnappeaConfig value) { config = value; return this; }
        /** Throws {@link SetupError} of kind {@code config} for bad settings. */
        public SnappeaObserver build() { return new SnappeaObserver(this); }
    }
}
