// Paxos: proposers seek promises, then acceptance from a majority. Learners observe the
// chosen value; the agreement rule rejects conflicting decisions.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

type Ballot = int;
type Value = int;

type PrepareRequestData = (proposer: Proposer, ballotNumber: Ballot, value: Value);
event PrepareRequest: PrepareRequestData;
type PrepareResponseData = (acceptor: Acceptor, promised: Ballot, acceptedValue: Value, acceptedBallot: Ballot);
event PrepareResponse: PrepareResponseData;
type AcceptRequestData = (proposer: Proposer, ballotNumber: Ballot, value: Value);
event AcceptRequest: AcceptRequestData;
type AcceptResponseData = (acceptor: Acceptor, accepted: Ballot);
event AcceptResponse: AcceptResponseData;

// The acceptor role in Paxos (see proposer.p for more details).
machine Acceptor {
  var n_prepared: Ballot;

  var acceptedValue: Value;
  var acceptedBallot: Ballot;

  start state Init {
    entry {
      n_prepared = -1;
      acceptedValue = -1;
      acceptedBallot = -1;
      goto Accept;
    }
  }

  state Accept {
    // When we get a prepare request, we promise not to accept any prepare requests with a lower ballot number, and return any
    //  proposed value we've accepted, if any.
    on PrepareRequest do (request: PrepareRequestData) {
      if (request.ballotNumber > n_prepared) {
        send request.proposer, PrepareResponse, (acceptor = this, promised = request.ballotNumber, acceptedValue = acceptedValue, acceptedBallot = acceptedBallot);
        n_prepared = request.ballotNumber;
        // print format("{0}: ({1}:{2}, {3})", this, acceptedBallot, acceptedValue, n_prepared);
      }

      // As an optimization, we could send a NACK here to let the proposer know we got its message, allowing it to tell the difference
      //  between losing it's proposal and packet loss. It's just an optimization, so we don't do it yet.
    }

    // Once a proposer has been made the 'leader', it sends us a proposed value. To avoid accepting values from old leaders, we simply
    //  discard any messages with ballot numbers lower than the one we prepared.
    on AcceptRequest do (request: AcceptRequestData) {
      if (request.ballotNumber >= n_prepared) {
        acceptedValue = request.value;
        acceptedBallot = request.ballotNumber;
        // Treat accepting as "prepare, accept" the way that Lamport does in Part Time Parliament (but not in Paxos Made Simple)
        //  See https://brooker.co.za/blog/2021/11/16/paxos.html and https://stackoverflow.com/questions/29880949/contradiction-in-lamports-paxos-made-simple-paper
        n_prepared = request.ballotNumber;

        send request.proposer, AcceptResponse, (acceptor = this, accepted = request.ballotNumber);
      }

      // Same story with NACKs here.
    }
  }
}
