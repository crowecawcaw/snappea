// Snap Pea's time functions: declaring them turns on time tracking. Times are int
// microseconds. Each machine has a clock; a send carries it to the receiver.

// Local work taking between lo and hi microseconds.
fun Elapse(lo: int, hi: int);
// A stopwatch: Mark() saves the current clock, Elapsed(m) is the worst case of
// now - mark. Use it for budgets rather than subtracting two Now() readings.
fun Mark(): int;
fun Elapsed(m: int): int;

fun ms(n: int): int { return n * 1000; }
