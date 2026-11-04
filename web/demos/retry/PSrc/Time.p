// Retry budget: a client makes at most three attempts. The server may drop requests;
// timers trigger retries. The rule requires success or failure within one second.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

// The entire time "extension": plain P foreign functions. Times are int microseconds. The
// checker implements them natively (causal clocks, see ../../PROPOSAL.md).

// The machine's local clock: worst / best case. Use for absolute deadlines.
fun Now(): int;
fun NowBest(): int;

// A stopwatch. Mark() captures the current clock; Elapsed(m) is the worst
// (ElapsedBest the best) case of now - mark, computed with the correlation
// kept, so shared history cancels. Use these for budgets, not Now() - t0.
fun Mark(): int;
fun Elapsed(m: int): int;
fun ElapsedBest(m: int): int;

// Local work taking between lo and hi microseconds.
fun Elapse(lo: int, hi: int);

fun ms(n: int): int {
  return n * 1000;
}
