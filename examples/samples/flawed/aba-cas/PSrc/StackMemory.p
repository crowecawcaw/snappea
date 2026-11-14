// A Treiber stack over a fixed pool of nodes (1, 2, 3; 0 is null). Nodes are reused:
// a thread that pops a node owns it and may push it again later, as an allocator's free
// list or an object pool does. That reuse is what makes ABA possible.
//
// Real atomic primitives, one memory operation each (one handler each):
//   load top, load node.next, store node.next, compare-and-swap top.
// Everything between them interleaves with other threads.
//
// FLAWED: the compare-and-swap compares only the pointer. If the top went X -> Y -> X
// while a thread was between its loads and its swap, the swap still succeeds and
// installs the next pointer the thread read before the change.

event LoadNext: (client: machine, node: int);
event NextValue: int;
event StoreNext: (node: int, next: int);

event LoadTop: machine;
event TopValue: int;
event CasTop: (client: machine, tid: int, expected: int, desired: int);
event CasResult: bool;
// Tells whoever started the thread (the test driver) that its operations are over.
event ThreadDone: int;

// Builds the list from the initial stack (top first). Shared by both memory variants.
fun LinkStack(stack: seq[int], next: map[int, int]) : map[int, int] {
  var i: int;
  while (i < sizeof(stack)) {
    if (i + 1 < sizeof(stack)) {
      next[stack[i]] = stack[i + 1];
    } else {
      next[stack[i]] = 0;
    }
    i = i + 1;
  }
  return next;
}

machine StackMemory {
  var top: int;
  var next: map[int, int];

  start state Serving {
    entry (stack: seq[int]) {
      next = LinkStack(stack, next);
      top = stack[0];
    }
    on LoadTop do (client: machine) { send client, TopValue, top; }
    on LoadNext do (r: (client: machine, node: int)) { send r.client, NextValue, next[r.node]; }
    on StoreNext do (w: (node: int, next: int)) { next[w.node] = w.next; }
    on CasTop do (c: (client: machine, tid: int, expected: int, desired: int)) {
      if (top == c.expected) {
        top = c.desired;
        announce TopSwapped, (tid = c.tid, top = top, next = next);
        send c.client, CasResult, true;
      } else {
        send c.client, CasResult, false;
      }
    }
  }
}

// The textbook algorithm. Push: read top, point the node at it, swap top to the node.
// Pop: read top, read top.next, swap top to that next. Retry on a failed swap.
machine StackThread {
  var tid: int;
  var mem: machine;
  var owner: machine;
  var owned: seq[int];

  start state Run {
    entry (input: (owner: machine, tid: int, mem: machine, plan: ThreadPlan)) {
      var op: Operation;
      tid = input.tid;
      mem = input.mem;
      owner = input.owner;
      owned = input.plan.owned;
      foreach (op in input.plan.ops) {
        if (op == Pop) { Pop(); } else { Push(); }
      }
      send owner, ThreadDone, tid;
    }
  }

  fun Pop() {
    var top: int;
    var next: int;
    var ok: bool;
    while (!ok) {
      send mem, LoadTop, this;
      receive { case TopValue: (v: int) { top = v; } }
      send mem, LoadNext, (client = this, node = top);
      receive { case NextValue: (v: int) { next = v; } }
      // BUG: between the loads above and this swap, other threads can pop this node,
      // pop its successor, and push this node back. The swap sees the same pointer and
      // installs `next`, a node that is no longer on the stack.
      send mem, CasTop, (client = this, tid = tid, expected = top, desired = next);
      receive { case CasResult: (r: bool) { ok = r; } }
    }
    owned += (sizeof(owned), top);
    announce OpDone, (tid = tid, op = Pop, node = top);
  }

  fun Push() {
    var node: int;
    var top: int;
    var ok: bool;
    node = owned[0];
    owned -= (0);
    while (!ok) {
      send mem, LoadTop, this;
      receive { case TopValue: (v: int) { top = v; } }
      send mem, StoreNext, (node = node, next = top);
      send mem, CasTop, (client = this, tid = tid, expected = top, desired = node);
      receive { case CasResult: (r: bool) { ok = r; } }
    }
    announce OpDone, (tid = tid, op = Push, node = node);
  }
}
