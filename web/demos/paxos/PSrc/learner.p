// Paxos: proposers seek promises, then acceptance from a majority. Learners observe the
// chosen value; the agreement rule rejects conflicting decisions.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

event Learn: (ballot: Ballot, value: Value);

// The learner role in Paxos (see proposer.p for more details).
machine Learner {
  var learned_value: Value;

  start state Init {
    entry {
      learned_value = -1;
      goto Learn;
    }
  }

  state Learn {
    on Learn do (payload: (ballot: Ballot, value: Value)) {
      assert(payload.value != -1);
      // This check is a belt-and-braces with the spec, but it's a useful piece of sanity
      assert((learned_value == -1) || (learned_value == payload.value));
      learned_value = payload.value;
    }
  }
}
