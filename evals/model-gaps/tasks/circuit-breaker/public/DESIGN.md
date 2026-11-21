# Circuit breaker in front of a flaky dependency

A client-side circuit breaker protects a dependency that sometimes fails. Callers send
their calls to the breaker; it either passes a call on to the dependency or rejects it
at once.

- **Closed** (the initial state): every call goes to the dependency. K consecutive
  failures open the breaker; a success resets the count. K is the `threshold` the
  breaker is created with.
- **Open**: every call is rejected at once. On opening, the breaker asks the timer
  service for a cooldown; when that cooldown is over the breaker becomes half-open.
- **Half-open**: exactly one trial call goes to the dependency; other calls are
  rejected while it is in flight. If the trial succeeds the breaker closes; if it
  fails the breaker opens again, with a new cooldown.

Only the breaker (`PSrc/Breaker.p`) is the design. The dependency, the timer service
and the callers are its environment. `PSrc/Events.p` fixes the messages between them
and the events each side announces, with when to announce each.

## Contract

- While open, no call reaches the dependency. While half-open, at most one call (the
  trial) is sent to the dependency. Calls sent while closed may still be in flight
  after the breaker has opened.
- A call is rejected only while the breaker is open, or half-open with the trial in
  flight.
- The breaker opens from closed exactly when K consecutive results of the current
  closed period failed: not earlier, not later, and in the same step as the Kth
  failure. A success resets the count. Every closed period counts from zero.
- A *late result*, one for a call that was sent before the breaker's last change of
  state, counts for nothing: not as a failure, not as a success, not as the trial's
  result. It still answers its caller.
- The breaker becomes half-open only from open, and only after the cooldown of the
  current open period is over. Then the trial's result, and nothing else, decides:
  closed on success, open on failure, in the same step as that result.
- The breaker does not stay open: once the cooldown is over it becomes half-open.
- Every call gets exactly one answer: `Rejected` if and only if the breaker
  short-circuited it, otherwise the dependency's answer (`Succeeded` or `Failed`).
  Every caller finishes.

## Atomic steps

The breaker handles one event at a time (one lock in code): a call request, a
dependency result, or a cooldown. Each is one atomic step. Between steps, calls,
dependency results and timer firings interleave freely.

## What can happen

- One to four callers run concurrently. A caller makes its calls one after another,
  each with a new unique call id, and may retry a rejected call at once (with a new
  id). New callers can start at any time, for instance while the breaker is open or
  half-open.
- The dependency answers every call, with success or failure, in any pattern. Its
  answers usually come back in order, but an answer can be slow: answers to later
  calls can overtake it, and it can arrive arbitrarily late, for instance after the
  breaker has opened, gone half-open and closed again, and after further calls were
  answered.
- The timer service fires every cooldown it is asked for, whenever it gets to it. It
  delivers at least once: a firing can be delivered a second time, arbitrarily later,
  for instance during the trial, after the breaker closed, or in a later open period.
- K is 1 or more.

Not in scope: call timeouts (a dependency call that never returns), real time,
failure rates over a sliding window, more than one trial in half-open, losing
messages, crashes of the breaker.
