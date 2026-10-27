package io.github.crowecawcaw.snappea;

import io.github.crowecawcaw.snappea.emit.EmitHook;
import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import java.util.function.Consumer;

/**
 * Sends {@code SnappeaEmit.emit} calls to recordings. A recording is bound to the thread that
 * started it and to threads that thread starts; nested recordings all receive the event.
 * A thread with no binding (a pool created earlier) uses the one active recording, and
 * fails the check when independent recordings (parallel tests) make the target ambiguous.
 */
final class Routing implements EmitHook.Router {
    record Node(Recording recording, Node parent, Thread thread) {}

    private static final Routing INSTANCE = new Routing();
    private static final InheritableThreadLocal<Node> CURRENT = new InheritableThreadLocal<>();
    private static final Set<Recording> ACTIVE = ConcurrentHashMap.newKeySet();
    /** Binds a thread to no recording: events from a test that is not recorded go nowhere. */
    private static final Node NOTHING = new Node(null, null, null);

    static Node start(Recording recording) {
        var node = new Node(recording, prune(CURRENT.get()), Thread.currentThread());
        CURRENT.set(node);
        ACTIVE.add(recording);
        EmitHook.install(INSTANCE);
        return node;
    }

    static void stop(Recording recording) {
        ACTIVE.remove(recording);
        var chain = CURRENT.get();
        if (chain != null && contains(chain, recording)) CURRENT.set(without(chain, recording));
    }

    private static boolean contains(Node node, Recording recording) {
        for (; node != null; node = node.parent) if (node.recording == recording && recording != null) return true;
        return false;
    }

    private static Node without(Node node, Recording recording) {
        if (node == null) return null;
        if (node.recording == recording) return node.parent;
        return new Node(node.recording, without(node.parent, recording), node.thread);
    }

    /**
     * The chain a new recording on this thread nests in: active recordings started on this
     * thread. Bindings inherited from another test's thread (a pool thread created while
     * that test ran, then reused for this one) are dropped.
     */
    private static Node prune(Node node) {
        if (node == null || node == NOTHING) return null;
        var parent = prune(node.parent);
        if (!ACTIVE.contains(node.recording) || node.thread != Thread.currentThread()) return parent;
        return parent == node.parent ? node : new Node(node.recording, parent, node.thread);
    }

    /** Unbinds the current thread (for a test that is not recorded); returns the binding for {@link #restore}. */
    static Object detach() {
        var chain = CURRENT.get();
        CURRENT.set(NOTHING);
        return chain;
    }

    static void restore(Object chain) {
        if (chain == null) CURRENT.remove();
        else CURRENT.set((Node) chain);
    }

    /** The active recordings enclosing the current thread, innermost first. */
    static List<Recording> enclosing() {
        var out = new ArrayList<Recording>();
        for (var node = CURRENT.get(); node != null && node != NOTHING; node = node.parent)
            if (ACTIVE.contains(node.recording)) out.add(node.recording);
        return out;
    }

    @Override public void failed(RuntimeException error) {
        // the recordings target() sent the event to
        var mine = enclosing();
        (mine.isEmpty() ? List.copyOf(ACTIVE) : mine).forEach(r -> r.failed(error));
    }

    @Override public Consumer<String> target() {
        var mine = enclosing();
        if (!mine.isEmpty()) return line -> mine.forEach(r -> r.accept(line));
        if (CURRENT.get() == NOTHING) return null;
        var all = List.copyOf(ACTIVE);
        if (all.isEmpty()) {
            for (var node = CURRENT.get(); node != null && node != NOTHING; node = node.parent) {
                if (node.recording.checked())
                    throw new IllegalStateException("SnappeaEmit.emit after recording '" + node.recording.label()
                            + "' was checked; stop or join threads that emit before the recording ends");
            }
            return null;
        }
        var roots = all.stream().filter(r -> {
            for (var node = r.node().parent; node != null; node = node.parent)
                if (ACTIVE.contains(node.recording)) return false;
            return true;
        }).toList();
        if (roots.size() == 1) return line -> all.forEach(r -> r.accept(line));
        var message = "SnappeaEmit.emit on thread '" + Thread.currentThread().getName() + "', which no recording started, while "
                + roots.size() + " independent recordings are active (parallel tests?). Emit from the test's thread or "
                + "threads it starts, or pass the Recording to the code under test and call recording.emit(...)";
        all.forEach(r -> r.fail(message));
        throw new IllegalStateException(message);
    }
}
