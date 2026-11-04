// Failure detection: the detector sends heartbeats and suspects silent nodes. Delayed
// replies can look like crashes; the rule checks reported failures against actual crashes.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

// module to represent the Failure Detector
module FailureDetector = (union {
  FailureDetector, Node, Client
}
, Timer);
