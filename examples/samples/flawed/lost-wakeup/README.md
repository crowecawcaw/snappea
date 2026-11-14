---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - What is a lost wake-up, and how does a P model find it?
  - Why must "check for work" and "register to wait" be one atomic step?
  - How does a progress spec turn a consumer that sleeps forever into a failing test?
---

# Lost wake-up in a blocking queue

A blocking queue hands items from producers to consumers. A consumer that finds the
queue empty registers as a waiter and parks; a producer that puts an item wakes one
waiter. This is the condition-variable pattern, modeled as messages to the queue.

**Difficulty: Medium.** The code reads correctly line by line; the bug is in where the
lock is released.

**The flaw in one sentence:** the consumer checks for an item and registers as a
waiter in two separate critical sections, so a put that runs between them wakes nobody
and the consumer then parks beside an item.

```text
if (queue.tryTake() == null) {   // critical section 1: empty
    waiters.add(me);             // critical section 2: a put already ran in between
    park();                      // sleeps forever
}
```

**Contract** (in [`PSpec/BlockingQueue.p`](PSpec/BlockingQueue.p), the same for both designs)

- `TakeBlocksOnlyWhenEmpty` (safety): items come out in the order they went in, each
  once, only to a consumer that wants one and is awake.
- `TakeBlocksOnlyWhenEmpty` (progress, hot state `ItemsOutstanding`): every planned
  item is eventually taken, so a take blocks only while the queue is empty.

**Atomic primitives** (one critical section, one handler each): take-if-available,
register as a waiter, put-and-wake-one. In the fixed design, take-or-register is one
critical section.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/BlockingQueue.p`](PSpec/BlockingQueue.p) | `TakeBlocksOnlyWhenEmpty` and the events it observes |
| [`PSrc/Producer.p`](PSrc/Producer.p) | The producer, shared by both designs |
| [`PSrc/RacyQueue.p`](PSrc/RacyQueue.p) | Flawed: check and register in separate critical sections |
| [`PSrc/WaitQueue.p`](PSrc/WaitQueue.p) | Fixed: take-or-register in one critical section |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers, the `PutWakesSleeper` scenario, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcBugCheckThenWait` | Flawed queue, one consumer, one item | The lost wake-up itself | Expected bug: `Deadlock detected.` (consumer 1 asleep with item 10 queued) |
| `tcFixedOneConsumerTwoItems` | One consumer takes two items from one producer | Take from a non-empty queue, sleep on an empty one, be woken by a put | Passes |
| `tcFixedTwoConsumers` | Two consumers, one item each | Both asleep at once, a put wakes only one, a woken consumer finds its item stolen and sleeps again | Passes |

`PutWakesSleeper` confirms the fixed tests reach the interesting history: a consumer
asleep, then a put, then its wake-up.

## The counterexample

From `tcBugCheckThenWait` (one consumer, one item):

1. The consumer sends `TryTake`; the queue is empty, so it answers "no item".
2. The producer puts item 10. The queue has no waiters, so it wakes nobody.
3. The consumer, acting on step 1's answer, registers as a waiter and parks.
4. Item 10 sits in the queue, the consumer waits for a wake-up that already happened,
   and nothing else can run.

`snappea check` reports:

```text
Deadlock detected. RacyConsumer(3) is waiting to receive an event, but no other controlled tasks are enabled, while spec TakeBlocksOnlyWhenEmpty is in hot state 'ItemsOutstanding' (wants = {1: 1}, queued = [10], sleeping = {1}, outstanding = 1).
```

**How a hang is reported.** The consumer waits in a blocking `receive`. When a
machine is blocked in `receive` and no other machine can run, the checker reports
`Deadlock detected`, with or without a spec, and names any spec left in a hot state.
A machine that waits in a state handler instead is simply idle: the run ends quietly,
and only a hot-state spec turns the hang into a failure (see
[lock-order-deadlock](../lock-order-deadlock/)). The expected bug pins the spec's
state, consumer 1 asleep while item 10 is queued, so a different hang cannot match it.
In a system that keeps running (more producers, timers), the same history ends in the
hot state instead: an item never taken.

## Why ordinary tests miss it

The window is a few instructions wide. In a test with a steady producer, the next put
wakes the stuck consumer, so the bug shows up as an occasional latency spike, not a
failure. It hangs for good only when the last item lands in the window, which a load
test rarely arranges. The checker tries that order directly.

## The fix

`WaitQueue` makes take-or-register one critical section (`TakeOrWait`): the queue
either hands back an item or records the consumer as a waiter while it still holds the
lock. A put then either finds the item gone or finds the waiter and wakes it. This is
what `while (queue.isEmpty()) cond.wait(lock);` gives you, since `wait` releases the
lock and parks atomically. A woken consumer loops and tries again, because another
consumer may have taken the item first. Re-checking the queue after registering, before
parking, also fixes it.

## Run

```sh
snappea check LostWakeup.pproj            # everything: the fix, the negative controls, coverage, mutation testing
snappea check LostWakeup.pproj 'tcBug*'   # only the negative controls: each passes only when its declared bug is found
```

Each negative control carries a native `// snappea: expect-bug` declaration: a matching
failure passes the test; no failure or a different failure fails it. Describe a
search as exhaustive only when its report says `search_complete`.

## Bounds and omissions

One producer, one or two consumers, at most two items. Wake-ups are never spurious and
never lost in transit; the queue is unbounded, so producers never block. Not modeled:
timed waits, `notifyAll`, a bounded queue with a second condition for "not full" (where
the same flaw makes producers hang).

## Testing the implementation

Emit `Queued`, `Taken`, `Sleeping` and `Woken` from inside the queue's critical
sections. To force the race, add a test hook between the consumer's empty check and its
registration, block the consumer there, put the last item, then release the consumer:
with the flaw the take never returns (use a timeout in the test).
