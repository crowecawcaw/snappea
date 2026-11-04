// Two-phase commit: clients request writes; the coordinator asks every participant to
// prepare before committing. Timeouts and crashes can interrupt this agreement.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

// the two phase commit module
module TwoPhaseCommit = union {
  Coordinator, Participant
}
, Timer;
