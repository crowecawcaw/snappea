// Paxos: proposers seek promises, then acceptance from a majority. Learners observe the
// chosen value; the agreement rule rejects conflicting decisions.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

type ProposerConfig = (acceptors: set[Acceptor], learners: set[Learner], valueToPropose: int, proposerId: int);

machine Proposer {
  var acceptors: set[Acceptor];
  var learners: set[Learner];

  // The ballot number we use for the prepares and accepts that this proposer sends out
  var ballotNumber: Ballot;
  var highestAcceptedBallot: Ballot;
  var valueToPropose: Value;

  // A number equal to a quorum: more than half the acceptors
  var majority: int;
  // Acceptors we have received prepare ACKs from. This is a set rather than a counter to deal with the
  //  fact that messages can be delivered multiple times in our network model.
  var prepareAcknowledgements: set[Acceptor];
  // Acceptors we have received accept ACKs from.
  var acceptAcknowledgements: set[Acceptor];

  start state Init {
    entry (config : ProposerConfig) {
      acceptors = config.acceptors;
      learners = config.learners;
      // For now, use our id as a ballot number
      ballotNumber = config.proposerId;
      valueToPropose = config.valueToPropose;
      majority = sizeof(acceptors) / 2 + 1;
      goto Prepare;
    }
  }

  // Phase 1, prepare
  state Prepare {
    entry {
      var acceptor: Acceptor;
      highestAcceptedBallot = -1;
      // Step 1a is to fire our proposal at every acceptor
      // ReliableBroadcastMajority(acceptors, PrepareRequest, (proposer = this, ballotNumber = ballotNumber, v = valueToPropose));
      foreach (acceptor in acceptors) {
        send acceptor, PrepareRequest, (proposer = this, ballotNumber = ballotNumber, value = valueToPropose);
      }
    }

    on PrepareResponse do (response: PrepareResponseData) {
      // The acceptors then get back to us, saying whether they have accepted our proposal
      if (response.promised == ballotNumber) {
        // If this acceptor has already accepted a proposal, we drop the one we're proposing and drive that forward instead.
        if (response.acceptedBallot > highestAcceptedBallot) {
          highestAcceptedBallot = response.acceptedBallot;
          valueToPropose = response.acceptedValue;
        }

        // Add this acceptor to the set of acceptors who have ACKed our proposal
        prepareAcknowledgements += (response.acceptor);
        // If more than half the acceptors have ACKed our proposal, we've been elected 'leader' and its time to move to phase 2
        if (sizeof(prepareAcknowledgements) >= majority) {
          goto Accept;
        }
      }
    }
  }

  // Phase 2, accept
  state Accept {
    entry {
      // We start the accept phase by firing off our accept requests at the acceptors
      // ReliableBroadcastMajority(acceptors, AcceptRequest, (proposer = this, ballotNumber = ballotNumber, v = valueToPropose));
      var acceptor: Acceptor;
      foreach (acceptor in acceptors) {
        send acceptor, AcceptRequest, (proposer = this, ballotNumber = ballotNumber, value = valueToPropose);
      }
    }

    // Then get responses from the acceptors
    on AcceptResponse do (response: AcceptResponseData) {
      if (response.accepted == ballotNumber) {
        // Add this acceptor to the set that have accepted our value
        acceptAcknowledgements += (response.acceptor);
        // And when more than half agree, it's been decided and it's time to teach
        if (sizeof(acceptAcknowledgements) >= majority) {
          goto Teach;
        }
      }
    }

    // Stale prepares can be safely ignored
    ignore PrepareResponse;
  }

  // Phase 3, teaching.
  // We diverge a little from the paper here, which puts the acceptors in the teacher role. Doing it here adds a round-trip,
  //  but makes the learners much simpler (they just accept whatever is thrown at them).
  state Teach {
    entry {
      // ReliableBroadcast(learners, Learn, (v=valueToPropose,));
      var learner: Learner;
      foreach (learner in learners) {
        send learner, Learn, (ballot=ballotNumber, value=valueToPropose);
      }
    }

    // At this point it's safe to ignore every message
    ignore AcceptResponse;
    ignore PrepareResponse;
  }
}

module Paxos = {
  Proposer, Acceptor, Learner
}
;
