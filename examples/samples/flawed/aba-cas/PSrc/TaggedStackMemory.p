// FIXED: the top is a tagged pointer, (pointer, tag), and every successful swap bumps
// the tag. The swap compares both words at once (a double-width compare-and-swap, or a
// pointer with a counter packed into its spare bits). After X -> Y -> X the pointer is
// the same but the tag is not, so a thread holding the old view fails and retries with
// a fresh read. Same nodes, same loads; only the top word and its swap change.

event LoadTaggedTop: machine;
event TaggedTopValue: (ptr: int, tag: int);
event CasTaggedTop: (client: machine, tid: int, expected: (ptr: int, tag: int), desired: int);

machine TaggedStackMemory {
  var top: int;
  var tag: int;
  var next: map[int, int];

  start state Serving {
    entry (stack: seq[int]) {
      next = LinkStack(stack, next);
      top = stack[0];
    }
    on LoadTaggedTop do (client: machine) { send client, TaggedTopValue, (ptr = top, tag = tag); }
    on LoadNext do (r: (client: machine, node: int)) { send r.client, NextValue, next[r.node]; }
    on StoreNext do (w: (node: int, next: int)) { next[w.node] = w.next; }
    on CasTaggedTop do (c: (client: machine, tid: int, expected: (ptr: int, tag: int), desired: int)) {
      if ((ptr = top, tag = tag) == c.expected) {
        top = c.desired;
        // snappea-lint: allow mutation:swap-operator -- counting down is equally correct: any change of tag on every swap works
        tag = tag + 1;
        announce TopSwapped, (tid = c.tid, top = top, next = next);
        send c.client, CasResult, true;
      } else {
        send c.client, CasResult, false;
      }
    }
  }
}

machine TaggedStackThread {
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
    var top: (ptr: int, tag: int);
    var next: int;
    var ok: bool;
    while (!ok) {
      send mem, LoadTaggedTop, this;
      receive { case TaggedTopValue: (v: (ptr: int, tag: int)) { top = v; } }
      send mem, LoadNext, (client = this, node = top.ptr);
      receive { case NextValue: (v: int) { next = v; } }
      // The tag read with the pointer makes the swap fail if the top changed at all
      // since that read, even if it changed back to the same node.
      send mem, CasTaggedTop, (client = this, tid = tid, expected = top, desired = next);
      receive { case CasResult: (r: bool) { ok = r; } }
    }
    owned += (sizeof(owned), top.ptr);
    announce OpDone, (tid = tid, op = Pop, node = top.ptr);
  }

  fun Push() {
    var node: int;
    var top: (ptr: int, tag: int);
    var ok: bool;
    node = owned[0];
    owned -= (0);
    while (!ok) {
      send mem, LoadTaggedTop, this;
      receive { case TaggedTopValue: (v: (ptr: int, tag: int)) { top = v; } }
      send mem, StoreNext, (node = node, next = top.ptr);
      send mem, CasTaggedTop, (client = this, tid = tid, expected = top, desired = node);
      receive { case CasResult: (r: bool) { ok = r; } }
    }
    announce OpDone, (tid = tid, op = Push, node = node);
  }
}
