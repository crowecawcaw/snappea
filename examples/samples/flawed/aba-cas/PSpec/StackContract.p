// The stack's contract, stated over what the driver plans and what memory holds at each
// successful compare-and-swap. The spec replays the committed operations on an abstract
// stack (a sequence, top first) and compares it with the list memory actually links up.
// It never reads a thread's local view, so it holds for any algorithm.

enum Operation { Pop, Push }
// One thread's script: the operations it runs in order, and the free nodes it owns
// before it starts. A pop adds the node to the thread's owned nodes; a push takes the
// oldest node the thread owns, so popping X and later pushing reuses node X.
type ThreadPlan = (ops: seq[Operation], owned: seq[int]);

// From the driver, before any thread starts: the initial stack and each thread's plan.
event StackStarted: (stack: seq[int], plans: map[int, ThreadPlan]);
// From memory, inside the compare-and-swap that succeeded: which thread swapped the
// top, and memory's whole list (top and every next pointer) right after it.
event TopSwapped: (tid: int, top: int, next: map[int, int]);
// From a thread, when its operation returns: the node it popped or pushed.
event OpDone: (tid: int, op: Operation, node: int);

// Safety and progress: every successful swap commits the thread's next planned
// operation, the list memory links up afterwards is exactly the stack the history of
// committed operations gives, each operation returns the node it committed, and every
// planned operation finishes.
spec StackIsLinearizable observes StackStarted, TopSwapped, OpDone {
  var stack: seq[int];
  var plans: map[int, ThreadPlan];
  // tid -> index of its current operation
  var step: map[int, int];
  // tid -> node its current operation committed (absent until the swap succeeds)
  var committed: map[int, int];
  var unfinished: int;

  start state Idle {
    on StackStarted do (p: (stack: seq[int], plans: map[int, ThreadPlan])) {
      var tid: int;
      stack = p.stack;
      plans = p.plans;
      foreach (tid in keys(plans)) {
        step[tid] = 0;
        unfinished = unfinished + sizeof(plans[tid].ops);
      }
      assert unfinished > 0, "planned no operations";
      goto Running;
    }
    on TopSwapped do { assert false, "top swapped before the stack started"; }
    on OpDone do { assert false, "operation finished before the stack started"; }
  }

  hot state Running {
    on StackStarted do { assert false, "stack started twice"; }
    on TopSwapped do (p: (tid: int, top: int, next: map[int, int])) {
      var op: Operation;
      var node: int;
      assert p.tid in plans, format("unknown thread {0} swapped the top", p.tid);
      assert step[p.tid] < sizeof(plans[p.tid].ops), format("thread {0} swapped after its last operation", p.tid);
      assert !(p.tid in committed), format("thread {0} committed one operation twice", p.tid);
      op = plans[p.tid].ops[step[p.tid]];
      if (op == Pop) {
        assert sizeof(stack) > 0, format("thread {0} popped an empty stack", p.tid);
        node = stack[0];
        stack -= (0);
        plans[p.tid].owned += (sizeof(plans[p.tid].owned), node);
      } else {
        assert sizeof(plans[p.tid].owned) > 0, format("thread {0} pushed with no free node", p.tid);
        node = plans[p.tid].owned[0];
        plans[p.tid].owned -= (0);
        stack += (0, node);
      }
      committed[p.tid] = node;
      assert Linked(p.top, p.next) == stack,
        format("after thread {0} committed node {1}, memory links up the stack {2}, but the committed operations give {3}",
          p.tid, node, Linked(p.top, p.next), stack);
    }
    on OpDone do (p: (tid: int, op: Operation, node: int)) {
      assert p.tid in committed, format("thread {0}'s operation returned before it committed", p.tid);
      assert p.op == plans[p.tid].ops[step[p.tid]],
        format("thread {0} ran an operation its plan does not have next", p.tid);
      assert p.node == committed[p.tid],
        format("thread {0}'s operation returned node {1}, but it committed node {2}", p.tid, p.node, committed[p.tid]);
      committed -= (p.tid);
      step[p.tid] = step[p.tid] + 1;
      unfinished = unfinished - 1;
      if (unfinished == 0) { goto Finished; }
    }
  }

  state Finished {
    on StackStarted do { assert false, "stack started twice"; }
    on TopSwapped do { assert false, "top swapped after every planned operation finished"; }
    on OpDone do { assert false, "operation finished after every planned operation finished"; }
  }

  // Follow next pointers until the null node. Reject a cycle explicitly, so a
  // malformed stack produces a useful counterexample instead of hanging the spec.
  fun Linked(top: int, next: map[int, int]) : seq[int] {
    var list: seq[int];
    var visited: set[int];
    var p: int;
    p = top;
    while (p != 0) {
      assert !(p in visited), format("cycle in stack links at node {0}", p);
      visited += (p);
      list += (sizeof(list), p);
      p = next[p];
    }
    return list;
  }
}
