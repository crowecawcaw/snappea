# Term-based leader election

Nodes elect a leader the way Raft does, without the log. A node whose election timer
fires increments its term, votes for itself and asks the others for votes; a node
grants at most one vote per term; a candidate with a majority of the cluster becomes
leader. A higher term in any request or reply makes a node adopt it and step down.
Only the leader accepts client requests. Nodes can crash and restart, and messages are
delayed arbitrarily.

## Contract

- A node never votes twice in a term (its own candidacy counts). Its term never goes
  back and it never grants a vote for an older term.
- A leader has a majority of the cluster's votes in its term, counted from the
  answers; there is at most one leader per term.
- A candidate that heard a reply from every peer and did not win tells the timer
  service (`RoundLost`). It never reports losing a term it won, or one where a
  majority voted for it and it saw no higher term. After a loss it next stands above
  every term it was told of.
- A node accepts a client request only as leader of its current term, so a leader
  that saw a higher term has stepped down. Every client request is answered: accepted
  or refused.
- Once elections start, some node becomes leader. Every candidacy hears back from
  every peer, unless a newer candidacy of that node supersedes it or a crash discards
  it; replies still in flight then must not revive the old tally.
- A node that restarts acknowledges it (`Restarted`) and can become leader again, in
  a fresh term. Winning before the crash does not count after it.

## Atomic steps

One handler each: timeout (term + 1 and self-vote, persisted, then send requests);
answer a RequestVote (adopt a higher term, decide, persist, reply); handle a reply;
answer a client request; crash and restart. `term` and the vote are durable.
Everything else is volatile and is lost in a restart.

## What can happen

Clusters have two or more nodes, of even or odd size. Some nodes may never answer. A
node can start from any durable state (a term and a vote from an earlier life). Any
node's timer can fire at any time, also twice in a row, and several can fire together.
The timer service hands a candidate its peers and whom to tell if it loses; when every
candidate of a round has lost, it lets one timer fire alone. A node can crash and
restart at any moment, in any role: one atomic step, after which it acknowledges the
restart to whoever crashed it. Messages are delayed arbitrarily and reordered between
senders, but not lost: a message sent before a crash can arrive after the restart. A
peer can already be at a much higher term. A client request can arrive at any node at
any time.

Not in scope: the log, heartbeats, pre-vote, retransmission.
