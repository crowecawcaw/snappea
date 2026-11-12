// The stack's contract, stated over what the driver plans and what memory holds at each
// successful compare-and-swap. The spec replays the committed operations on an abstract
// stack (a sequence, top first) and compares it with the list memory actually links up.
// It never reads a thread's local view, so it holds for any algorithm.

enum tOp { Pop, Push }
// One thread's script: the operations it runs in order, and the free nodes it owns
// before it starts. A pop adds the node to the thread's owned nodes; a push takes the
// oldest node the thread owns, so popping X and later pushing reuses node X.
type tPlan = (ops: seq[tOp], owned: seq[int]);

// From the driver, before any thread starts: the initial stack and each thread's plan.
event eStackStarted: (stack: seq[int], plans: map[int, tPlan]);
// From memory, inside the compare-and-swap that succeeded: which thread swapped the
// top, and memory's whole list (top and every next pointer) right after it.
event eTopSwapped: (tid: int, top: int, next: map[int, int]);
// From a thread, when its operation returns: the node it popped or pushed.
event eOpDone: (tid: int, op: tOp, node: int);

// Safety and progress: every successful swap commits the thread's next planned
// operation, the list memory links up afterwards is exactly the stack the history of
// committed operations gives, each operation returns the node it committed, and every
// planned operation finishes.
spec StackIsLinearizable observes eStackStarted, eTopSwapped, eOpDone {
  var stack: seq[int];
  var plans: map[int, tPlan];
  // tid -> index of its current operation
  var step: map[int, int];
  // tid -> node its current operation committed (absent until the swap succeeds)
  var committed: map[int, int];
  var unfinished: int;

  start state Idle {
    on eStackStarted do (p: (stack: seq[int], plans: map[int, tPlan])) {
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
    on eTopSwapped do { assert false, "top swapped before the stack started"; }
    on eOpDone do { assert false, "operation finished before the stack started"; }
  }

  hot state Running {
    on eStackStarted do { assert false, "stack started twice"; }
    on eTopSwapped do (p: (tid: int, top: int, next: map[int, int])) {
      var op: tOp;
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
    on eOpDone do (p: (tid: int, op: tOp, node: int)) {
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
    on eStackStarted do { assert false, "stack started twice"; }
    on eTopSwapped do { assert false, "top swapped after every planned operation finished"; }
    on eOpDone do { assert false, "operation finished after every planned operation finished"; }
  }

  // The list memory holds, top first: follow next pointers from top until 0. Stops after
  // as many nodes as memory has, so a cycle shows up as a too-long list instead of a hang.
  // snappea-lint: allow coverage -- the bound only stops a cycle, which only a broken design links up
  fun Linked(top: int, next: map[int, int]) : seq[int] {
    var list: seq[int];
    var p: int;
    p = top;
    while (p != 0 && sizeof(list) <= sizeof(next)) {
      list += (sizeof(list), p);
      p = next[p];
    }
    return list;
  }
}
