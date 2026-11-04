// Failure detection: the detector sends heartbeats and suspects silent nodes. Delayed
// replies can look like crashes; the rule checks reported failures against actual crashes.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

// Node machine sends a pong message on receiving a ping
machine Node {
  start state WaitForPing {
    on Ping do (request: (detector: FailureDetector, trial: int)) {
      UnReliableSend(request.detector, Pong, (node = this, trial = request.trial));
    }

    on ShutDown do {
      raise halt;
    }
  }
}
