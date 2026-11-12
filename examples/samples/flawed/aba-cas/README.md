---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - What is the ABA problem, and how does a P model find it?
  - Why does a compare-and-swap on a pointer alone corrupt a lock-free stack that reuses nodes?
  - How do I force a rare interleaving so mutation testing can rely on it?
---

# ABA in a lock-free stack

A Treiber stack (a lock-free linked stack) pushes and pops with a compare-and-swap on
the top pointer. Its nodes come from a fixed pool and are reused: a thread that pops a
node owns it and may push it again later, as an allocator's free list or an object pool
does.

**Difficulty: hard.** The algorithm is the textbook one, every unit test passes, and
the failing interleaving needs one thread to stall for three complete operations of
another.

**Contract.** The stack behaves as if each operation happened at one instant (its
successful swap): a pop returns the top element, a push puts its node on top, and the
list memory links up is exactly the stack those operations give. Every operation
finishes.

**Atomic primitives** (one handler each): load top, load `node.next`, store
`node.next`, compare-and-swap top. Everything between them interleaves.

## The flaw

The swap compares only the pointer, so it succeeds when the top changed and changed
back (X → Y → X) since the thread read it, and installs a `next` pointer read before
the change.

## The counterexample

From `tcBugPointerOnlyCas` (the stack starts as 1 → 2 → 3):

1. Thread 1 starts a pop: it loads top = 1, then loads `1.next` = 2. It is about to
   swap top from 1 to 2, and stalls.
2. Thread 2 pops 1 (top = 2) and pops 2 (top = 3). It now owns nodes 1 and 2.
3. Thread 2 pushes node 1 back: it stores `1.next = 3` and swaps top from 3 to 1.
   The stack is 1 → 3.
4. Thread 1 resumes. Its swap compares top (1) with what it read (1), succeeds, and
   sets top = 2.
5. Memory now links up 2 → 3. The committed operations say thread 1 popped 1, leaving
   3. Node 2, which thread 2 still owns, is back on the stack, and the next pop hands
   it out twice.

The check fails with:

```text
Assertion Failed: PSpec/StackContract.p:70:7 after thread 1 committed node 1, memory links up the stack (2, 3), but the committed operations give (3)
```

## Why ordinary tests miss it

Thread 1 must stall between its loads and its swap for the whole of three operations of
another thread. Stress tests almost never produce that, and in a garbage-collected
language the bug cannot happen at all unless nodes are pooled, so the code looks
correct in review and in most test environments. Without the forcing driver, the
checker needs a few hundred random schedules to hit it; a CI stress test runs on real
threads and might never.

## The fix

`TaggedStackMemory` keeps a tag next to the top pointer and bumps it on every successful
swap; the swap compares pointer and tag together (a double-width compare-and-swap, or a
counter packed into the pointer's spare bits). In step 4 the pointer matches but the
tag does not, so thread 1's swap fails, it reloads (top = 1, `1.next` = 3) and pops
correctly. Hazard pointers or epoch-based reclamation fix it the other way: by never
reusing a node another thread might still be reading.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/StackContract.p`](PSpec/StackContract.p) | `StackIsLinearizable`: replays committed operations on an abstract stack, compares memory's list, requires every operation to finish (hot state) |
| [`PSrc/StackMemory.p`](PSrc/StackMemory.p) | Flawed: memory with a pointer-only swap, and the textbook thread |
| [`PSrc/TaggedStackMemory.p`](PSrc/TaggedStackMemory.p) | Fixed: memory with a tagged top, and the same thread using the tag |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers, the `SwapHolder` that forces the stall, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcFixedAbaInterleaving` | Fixed design; thread 1 pops, thread 2 pops, pops, pushes back | Every interleaving of the ABA history, including the stall | passes, all schedules |
| `tcFixedForcedAba` | Fixed design; `SwapHolder` holds thread 1's swap until thread 2 is done | The ABA stall in one schedule, so mutation testing sees a missing tag at once | passes, all schedules |
| `tcFixedPushPopContention` | Fixed design; one-element stack, thread 1 pushes two nodes, thread 2 pops and pushes back | Push/pop swaps collide both ways; both retry loops run; a thread pushing twice uses two nodes | passes, all schedules |
| `tcBugPointerOnlyCas` | Flawed design, forced stall | **Expected to fail**: `but the committed operations give` | fails after 1 schedule |

The forced test exists because a mutant (say, the tag never bumped) gets at most about a
thousand schedules. The ABA schedule is one of a few thousand in
`tcFixedAbaInterleaving`, so mutation testing could miss it; forcing it makes the
verdict certain. Running the flawed design without the holder also fails, after a few
hundred schedules.

## Run

```sh
snappea check AbaCas.pproj 'tcFixed*' --deny mutation
snappea check AbaCas.pproj tcBugPointerOnlyCas   # exits 1 with the assertion above
```

## Bounds and omissions

Two threads, three nodes, at most four operations. The stack never runs empty in these
plans, so pop-on-empty is not modeled. Memory is sequentially consistent: no weak memory
ordering, no torn reads. Tags do not wrap around (a real 16-bit tag can, after 65536
swaps during one stall). Node reclamation (freeing memory) is not modeled; reuse is.

One suppression: `tag - 1` instead of `tag + 1` is equivalent, since any change of the
tag on every swap works.

## Testing the implementation

Emit `eTopSwapped` (thread, new top, and a snapshot of the list) right after each
successful swap, and `eOpDone` when an operation returns; the spec then checks real
runs. To force the race, do what `SwapHolder` does: put a hook between the loads and
the swap in the pop path, block thread 1 there, run thread 2's pop, pop, push to
completion, then release thread 1. With a pointer-only swap the next pop returns a
node another thread owns.
