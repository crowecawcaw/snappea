package io.github.crowecawcaw.snappea;

import java.io.IOException;
import java.nio.file.Path;
import java.time.Duration;
import java.util.List;
import java.util.concurrent.TimeUnit;

final class Processes {
    private Processes() {}

    /** The command ran longer than its timeout and was stopped. */
    static final class Timeout extends IOException {
        private static final long serialVersionUID = 1L;
        Timeout(String message) { super(message); }
    }

    /** {@code snappea --version}, or "unknown version". */
    static String version(String executable) {
        try {
            var process = new ProcessBuilder(executable, "--version").redirectErrorStream(true).start();
            process.getOutputStream().close();
            if (!process.waitFor(5, TimeUnit.SECONDS)) {
                process.destroyForcibly();
                return "unknown version";
            }
            return new String(process.getInputStream().readAllBytes(), java.nio.charset.StandardCharsets.UTF_8).strip();
        } catch (IOException error) {
            return "unknown version";
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
            return "unknown version";
        }
    }

    static int run(List<String> command, Path input, Path console, Duration timeout) throws IOException {
        var builder = new ProcessBuilder(command).redirectErrorStream(true).redirectOutput(console.toFile());
        if (input != null) builder.redirectInput(input.toFile());
        var process = builder.start();
        try {
            // Batch commands have no interactive input; don't leave an inherited pipe open.
            if (input == null) process.getOutputStream().close();
            if (!process.waitFor(timeout.toMillis(), TimeUnit.MILLISECONDS)) {
                throw new Timeout("snappea exceeded timeout " + timeout);
            }
            return process.exitValue();
        } catch (InterruptedException error) {
            terminate(process);
            Thread.currentThread().interrupt();
            throw new IOException("Interrupted while waiting for snappea", error);
        } catch (IOException | RuntimeException error) {
            terminate(process);
            throw error;
        }
    }

    record Captured(int exit, String output) {}

    /** Runs command with input on stdin and returns its combined output, writing no file. */
    static Captured capture(List<String> command, String input, Duration timeout) throws IOException {
        var process = new ProcessBuilder(command).redirectErrorStream(true).start();
        try {
            var stdin = java.util.concurrent.CompletableFuture.runAsync(() -> {
                try (var in = process.getOutputStream()) {
                    in.write(input.getBytes(java.nio.charset.StandardCharsets.UTF_8));
                } catch (IOException ignored) { /* the exit code and output tell what went wrong */ }
            });
            var stdout = java.util.concurrent.CompletableFuture.supplyAsync(() -> {
                try { return new String(process.getInputStream().readAllBytes(), java.nio.charset.StandardCharsets.UTF_8); }
                catch (IOException error) { throw new java.io.UncheckedIOException(error); }
            });
            if (!process.waitFor(timeout.toMillis(), TimeUnit.MILLISECONDS)) {
                throw new Timeout("snappea exceeded timeout " + timeout);
            }
            stdin.join();
            return new Captured(process.exitValue(), stdout.get(5, TimeUnit.SECONDS));
        } catch (InterruptedException error) {
            terminate(process);
            Thread.currentThread().interrupt();
            throw new IOException("Interrupted while waiting for snappea", error);
        } catch (java.util.concurrent.ExecutionException | java.util.concurrent.TimeoutException error) {
            terminate(process);
            throw new IOException("Cannot read snappea output: " + error, error);
        } catch (IOException | RuntimeException error) {
            terminate(process);
            throw error;
        }
    }

    private static void terminate(Process process) {
        // The CLI normally has no children. Also terminate descendants if the configured
        // executable is a wrapper, without an unbounded waitFor after destroyForcibly.
        var children = process.descendants().toList();
        children.forEach(ProcessHandle::destroyForcibly);
        process.destroyForcibly();
        try {
            process.waitFor(2, TimeUnit.SECONDS);
        } catch (InterruptedException error) {
            Thread.currentThread().interrupt();
        }
    }

    static String tail(Path path) {
        try (var file = java.nio.channels.FileChannel.open(path)) {
            int size = (int) Math.min(file.size(), 32_768);
            var bytes = java.nio.ByteBuffer.allocate(size);
            file.position(Math.max(0, file.size() - size));
            while (bytes.hasRemaining() && file.read(bytes) >= 0) {}
            return new String(bytes.array(), 0, bytes.position(), java.nio.charset.StandardCharsets.UTF_8);
        } catch (IOException error) {
            return "(console unavailable: " + error + ")";
        }
    }
}
