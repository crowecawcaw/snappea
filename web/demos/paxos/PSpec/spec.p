// Paxos: proposers seek promises, then acceptance from a majority. Learners observe the
// chosen value; the agreement rule rejects conflicting decisions.
// This observer checks a rule; it does not implement the system. Hot states require
// eventual progress.

// This safety spec ensures that the value that was taught never changes (though it can be
// taught multiple times)
spec OneValueTaught observes Learn {
  var decided: int;

  start state Init {
    entry {
      decided = -1;
    }

    on Learn do (payload: (ballot: Ballot, value: Value)) {
      assert(payload.value != -1);
      if (decided != -1) {
        assert(decided == payload.value);
      }

      decided = payload.value;
    }
  }
}
