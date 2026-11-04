// Shared environment model: timers and crash injection make nondeterministic choices so
// the checker can explore different delivery and failure orders.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

/* Create the timer module which consists of only the timer machine */
module Timer = {
  Timer
}
;
