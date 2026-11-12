// Drivers. Threads run the stack algorithm and are part of the design under test; only
// the machines in this file are drivers. Each driver announces the initial stack and
// every thread's plan before starting anything, so the spec knows what must happen.

fun Ops(a: seq[tOp], op: tOp) : seq[tOp] {
  a += (sizeof(a), op);
  return a;
}

fun Nodes(a: seq[int], n: int) : seq[int] {
  a += (sizeof(a), n);
  return a;
}

// Fixed design: memory with a tagged top, and its threads. Thread 1 talks to `mem1`,
// which is the memory itself or a SwapHolder in front of it.
fun TaggedThreads(owner: machine, mem: machine, mem1: machine, plans: map[int, tPlan], startSecond: bool) {
  new TaggedStackThread((owner = owner, tid = 1, mem = mem1, plan = plans[1]));
  if (startSecond) { new TaggedStackThread((owner = owner, tid = 2, mem = mem, plan = plans[2])); }
}

// The ABA history: stack 1 -> 2 -> 3. Thread 1 pops once; thread 2 pops twice and pushes
// back the first node it popped. If thread 1 reads top = 1 and 1.next = 2, then thread 2
// runs all three operations, top is 1 again but 1.next is now 3 and node 2 is thread 2's.
fun AbaStack() : seq[int] {
  var s: seq[int];
  return Nodes(Nodes(Nodes(s, 1), 2), 3);
}

fun AbaPlans() : map[int, tPlan] {
  var none: seq[int];
  var ops: seq[tOp];
  var plans: map[int, tPlan];
  plans[1] = (ops = Ops(ops, Pop), owned = none);
  plans[2] = (ops = Ops(Ops(Ops(ops, Pop), Pop), Push), owned = none);
  return plans;
}

// Every interleaving of the ABA history: the checker decides when thread 1 is delayed.
machine ConcurrentAba {
  start state Init {
    entry {
      var mem: machine;
      announce eStackStarted, (stack = AbaStack(), plans = AbaPlans());
      mem = new TaggedStackMemory(AbaStack());
      TaggedThreads(this, mem, mem, AbaPlans(), true);
    }
    ignore eThreadDone;
  }
}

event eSwapHeld;
event eReleaseSwap;

// The ABA history forced, as an implementation test would force it: a SwapHolder in
// front of memory holds thread 1's first swap (its loads are done); thread 2 runs all
// its operations; then the swap goes through. One schedule, so mutation testing finds
// a design that does not catch the stale swap at once.
machine ForcedTaggedAba {
  var mem: machine;
  var holder: machine;

  start state Init {
    entry {
      announce eStackStarted, (stack = AbaStack(), plans = AbaPlans());
      mem = new TaggedStackMemory(AbaStack());
      holder = new SwapHolder((mem = mem, driver = this));
      TaggedThreads(this, mem, holder, AbaPlans(), false);
    }
    on eSwapHeld goto SecondThreadRuns;
  }
  state SecondThreadRuns {
    entry {
      new TaggedStackThread((owner = this, tid = 2, mem = mem, plan = AbaPlans()[2]));
    }
    on eThreadDone goto Released with (tid: int) {
      assert tid == 2, "thread 1 finished while its swap was held";
      send holder, eReleaseSwap;
    }
  }
  state Released {
    ignore eThreadDone;
  }
}

// The same forced history against the flawed design.
machine ForcedPointerAba {
  var mem: machine;
  var holder: machine;

  start state Init {
    entry {
      announce eStackStarted, (stack = AbaStack(), plans = AbaPlans());
      mem = new StackMemory(AbaStack());
      holder = new SwapHolder((mem = mem, driver = this));
      new StackThread((owner = this, tid = 1, mem = holder, plan = AbaPlans()[1]));
    }
    on eSwapHeld goto SecondThreadRuns;
  }
  state SecondThreadRuns {
    entry {
      new StackThread((owner = this, tid = 2, mem = mem, plan = AbaPlans()[2]));
    }
    on eThreadDone goto Released with (tid: int) {
      assert tid == 2, "thread 1 finished while its swap was held";
      send holder, eReleaseSwap;
    }
  }
  state Released {
    ignore eThreadDone;
  }
}

// Forwards a thread's memory operations, but holds its first compare-and-swap until the
// driver releases it: the delay a preempted or slow thread has in the real system. The
// held thread only pops, so it never stores a next pointer.
machine SwapHolder {
  var mem: machine;
  var driver: machine;
  var heldEvent: event;
  var heldPayload: any;

  start state Forwarding {
    entry (p: (mem: machine, driver: machine)) {
      mem = p.mem;
      driver = p.driver;
    }
    // snappea-lint: allow coverage -- the flawed design's loads, used only by tcBugPointerOnlyCas
    on eLoadTop do (c: machine) { send mem, eLoadTop, c; }
    on eLoadTaggedTop do (c: machine) { send mem, eLoadTaggedTop, c; }
    on eLoadNext do (r: (client: machine, node: int)) { send mem, eLoadNext, r; }
    // snappea-lint: allow coverage -- the flawed design's swap, used only by tcBugPointerOnlyCas
    on eCasTop do (c: (client: machine, tid: int, expected: int, desired: int)) {
      Hold(eCasTop, c);
    }
    on eCasTaggedTop do (c: (client: machine, tid: int, expected: (ptr: int, tag: int), desired: int)) {
      Hold(eCasTaggedTop, c);
    }
    on eReleaseSwap do { assert false, "released a swap that was never held"; }
  }

  // After the release, only the fixed design's retry passes through: a pop's loads and
  // swap (the held thread only pops).
  state Passing {
    on eLoadTaggedTop do (c: machine) { send mem, eLoadTaggedTop, c; }
    on eLoadNext do (r: (client: machine, node: int)) { send mem, eLoadNext, r; }
    on eCasTaggedTop do (c: (client: machine, tid: int, expected: (ptr: int, tag: int), desired: int)) {
      send mem, eCasTaggedTop, c;
    }
  }

  fun Hold(e: event, payload: any) {
    heldEvent = e;
    heldPayload = payload;
    send driver, eSwapHeld;
    receive { case eReleaseSwap: { } }
    send mem, heldEvent, heldPayload;
    goto Passing;
  }
}

// Push against pop on a one-element stack: thread 1 pushes its free nodes 2 and 3;
// thread 2 pops, then pushes back what it popped. Swaps on the top collide in both
// directions, so both retry paths run, and each push must link to the top it swapped on.
machine PushPopContention {
  start state Init {
    entry {
      var mem: machine;
      var plans: map[int, tPlan];
      var none: seq[int];
      var ops: seq[tOp];
      plans[1] = (ops = Ops(Ops(ops, Push), Push), owned = Nodes(Nodes(none, 2), 3));
      plans[2] = (ops = Ops(Ops(ops, Pop), Push), owned = none);
      announce eStackStarted, (stack = Nodes(none, 1), plans = plans);
      mem = new TaggedStackMemory(Nodes(none, 1));
      TaggedThreads(this, mem, mem, plans, true);
    }
    ignore eThreadDone;
  }
}

test tcFixedAbaInterleaving [main=ConcurrentAba]:
  assert StackIsLinearizable in { ConcurrentAba, TaggedStackMemory, TaggedStackThread };

test tcFixedForcedAba [main=ForcedTaggedAba]:
  assert StackIsLinearizable in { ForcedTaggedAba, SwapHolder, TaggedStackMemory, TaggedStackThread };

test tcFixedPushPopContention [main=PushPopContention]:
  assert StackIsLinearizable in { PushPopContention, TaggedStackMemory, TaggedStackThread };

// expect-bug: but the committed operations give
test tcBugPointerOnlyCas [main=ForcedPointerAba]:
  assert StackIsLinearizable in { ForcedPointerAba, SwapHolder, StackMemory, StackThread };
