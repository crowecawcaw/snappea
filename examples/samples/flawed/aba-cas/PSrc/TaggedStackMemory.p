// FIXED: the top is a tagged pointer, (pointer, tag), and every successful swap bumps
// the tag. The swap compares both words at once (a double-width compare-and-swap, or a
// pointer with a counter packed into its spare bits). After X -> Y -> X the pointer is
// the same but the tag is not, so a thread holding the old view fails and retries with
// a fresh read. Same nodes, same loads; only the top word and its swap change.

event eLoadTaggedTop: machine;
event eTaggedTopValue: (ptr: int, tag: int);
event eCasTaggedTop: (client: machine, tid: int, expected: (ptr: int, tag: int), desired: int);

machine TaggedStackMemory {
  var top: int;
  var tag: int;
  var next: map[int, int];

  start state Serving {
    entry (stack: seq[int]) {
      next = LinkStack(stack, next);
      top = stack[0];
    }
    on eLoadTaggedTop do (client: machine) { send client, eTaggedTopValue, (ptr = top, tag = tag); }
    on eLoadNext do (r: (client: machine, node: int)) { send r.client, eNextValue, next[r.node]; }
    on eStoreNext do (w: (node: int, next: int)) { next[w.node] = w.next; }
    on eCasTaggedTop do (c: (client: machine, tid: int, expected: (ptr: int, tag: int), desired: int)) {
      if (top == c.expected.ptr && tag == c.expected.tag) {
        top = c.desired;
        // snappea-lint: allow mutation:swap-operator -- counting down is equally correct: any change of tag on every swap works
        tag = tag + 1;
        announce eTopSwapped, (tid = c.tid, top = top, next = next);
        send c.client, eCasResult, true;
      } else {
        send c.client, eCasResult, false;
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
    entry (input: (owner: machine, tid: int, mem: machine, plan: tPlan)) {
      var op: tOp;
      tid = input.tid;
      mem = input.mem;
      owner = input.owner;
      owned = input.plan.owned;
      foreach (op in input.plan.ops) {
        if (op == Pop) { Pop(); } else { Push(); }
      }
      send owner, eThreadDone, tid;
    }
  }

  fun Pop() {
    var top: (ptr: int, tag: int);
    var next: int;
    var ok: bool;
    while (!ok) {
      send mem, eLoadTaggedTop, this;
      receive { case eTaggedTopValue: (v: (ptr: int, tag: int)) { top = v; } }
      send mem, eLoadNext, (client = this, node = top.ptr);
      receive { case eNextValue: (v: int) { next = v; } }
      // The tag read with the pointer makes the swap fail if the top changed at all
      // since that read, even if it changed back to the same node.
      send mem, eCasTaggedTop, (client = this, tid = tid, expected = top, desired = next);
      receive { case eCasResult: (r: bool) { ok = r; } }
    }
    owned += (sizeof(owned), top.ptr);
    announce eOpDone, (tid = tid, op = Pop, node = top.ptr);
  }

  fun Push() {
    var node: int;
    var top: (ptr: int, tag: int);
    var ok: bool;
    node = owned[0];
    owned -= (0);
    while (!ok) {
      send mem, eLoadTaggedTop, this;
      receive { case eTaggedTopValue: (v: (ptr: int, tag: int)) { top = v; } }
      send mem, eStoreNext, (node = node, next = top.ptr);
      send mem, eCasTaggedTop, (client = this, tid = tid, expected = top, desired = node);
      receive { case eCasResult: (r: bool) { ok = r; } }
    }
    announce eOpDone, (tid = tid, op = Push, node = node);
  }
}
