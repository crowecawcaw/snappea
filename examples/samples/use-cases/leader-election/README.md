---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model Raft-style leader election with crashes in P?
  - How do I state "some leader is eventually elected" so a bounded checker can test it?
  - What must a node persist before it answers a vote request?
---

# Term-based leader election

Three nodes (four in one test) elect a leader the way Raft does, without the log: a
node whose election timer fires increments its term, votes for itself and asks the
others for votes; a node grants at most one vote per term; a candidate with a majority
of the cluster becomes leader. A higher term in any request or reply makes a node adopt
it and step down. Only the leader accepts client requests.
Nodes can crash and restart, and messages are delayed arbitrarily.

**Contract**

- Safety (`ElectionSafety`): a node never votes twice in a term (its own candidacy
  counts); its term never goes back and it never grants a vote for an older term; a
  leader has a majority of votes in its term, counted from the answers; at most one
  leader per term; a node never reports losing a term it won, or one where a majority
  voted for it and it saw no higher term; after a loss it next stands above every term
  it was told of; a node accepts a client request only as leader of its current term,
  so a leader that saw a higher term has stepped down.
- Progress (`SomeLeaderElected`, bounded): once elections start, some node becomes
  leader. Assumptions: at most one crash (of a node that is not standing), every
  request is eventually answered (RequestVote is retried), and if every candidate of a
  round loses, one node's timer eventually fires alone after the earlier messages were
  handled (what Raft's randomized timeouts provide).
- Progress (`CandidaciesConclude`): every candidacy hears back from every peer.
- Progress (`ProbeAnswered`): a client request is accepted or refused.

**Atomic primitives** (one handler each): timeout (term + 1 and self-vote, persisted,
then send requests); answer a RequestVote (adopt a higher term, grant if this node has
not voted in that term, persist, reply); handle a reply; answer a client request. `term` and `votedTerm` are
durable; everything else is volatile.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `ElectionSafety`, `SomeLeaderElected`, `CandidaciesConclude` and `ProbeAnswered` (hot states) |
| [`PSrc/Node.p`](PSrc/Node.p) | The node: follower, candidate, leader |
| [`PSrc/ForgetfulNode.p`](PSrc/ForgetfulNode.p) | Negative control: the vote is lost in a crash |
| [`PTst/Tests.p`](PTst/Tests.p) | The `Election` driver (timers, crash, uncontested round), the `Prober` client and the test cases |

## Test cases

| Test | Drives | Why it matters |
| --- | --- | --- |
| `tcTwoCandidates` | Nodes 1 and 2 stand at once; node 3 decides | The core race: either candidate may see the other's request first and stand in a higher term |
| `tcSplitVote` | Node 3 restarts having voted for itself in term 1; nodes 1 and 2 stand in term 1 | Split vote: nobody wins term 1, both report the loss, the uncontested round must elect |
| `tcHigherTermReply` | Node 3 (and sometimes node 2) restarts at term 5; node 1 stands in term 1 | A candidate or leader gets a reply with a higher term; a loser must next stand above it |
| `tcStaleVoteReply` | Node 1's timer fires twice in a row | Replies to its term-1 requests arrive during its term-2 candidacy and must not count |
| `tcOneNodeDown` | Node 3 never answers | A majority, not unanimity: one grant plus the self-vote must win |
| `tcFourNodesOneDown` | Four nodes, one never answers; node 1 stands | Exactly at the boundary: two votes of four are half, not a majority, and must not win; three must |
| `tcDeposedLeader` | Node 1 stands in term 1, node 3 (restarted at term 5) in term 6; a client request to node 1 at any moment | A leader that sees a higher term in a request or a reply must stop accepting requests |
| `tcGeneratedElection` | One or two candidates, maybe a crash and restart of a non-candidate | Crashes at any moment, including between granting one candidate and seeing the other |
| `tcBugVoteNotPersisted` | `ForgetfulNode`, two candidates, node 3 crashes | **Expected bug**: `node 3 voted twice in term 1: for 2 and for 1` |

## Run

```sh
snappea check tcTwoCandidates tcSplitVote tcHigherTermReply tcStaleVoteReply tcOneNodeDown \
  tcFourNodesOneDown tcDeposedLeader tcGeneratedElection --timeout 30s --deny mutation
snappea check tcBugVoteNotPersisted      # passes when it finds the double vote above
```

`tcTwoCandidates` (8k schedules), `tcStaleVoteReply`, `tcOneNodeDown` and
`tcFourNodesOneDown` explore every schedule within a few seconds. `tcDeposedLeader`
(80k) needs about a minute to finish (`--timeout 2m`); every step-down mutant fails it
within 60 schedules. `tcSplitVote`, `tcHigherTermReply` and `tcGeneratedElection` are
sampled: the uncontested rounds pick any node, so their search does not finish in 30 s.

## Bounds and omissions

Three nodes (four in `tcFourNodesOneDown`), at most two contested candidates, at most
one crash. A crash and its restart are one step; only a node that never stands
for election crashes. Requests
to a crashed node are retried, so they are modeled as delayed, not lost. Not modeled:
the log, heartbeats, pre-vote, and message loss without retry. A leader's only action
is accepting a client request, which is how `tcDeposedLeader` observes a missed
step-down. Within this fault boundary, a candidate that misses its step-down has
the same outcomes as a follower: it refuses clients, counts replies for its own term,
and stands again on timeout. Two `goto Follower` suppressions cover that bounded
equivalence. The tool still warns that `Crash` is handled differently in these states;
that warning matters if the environment is extended to crash former candidates.
Raft persists `votedFor` so a retried request from the same candidate is granted again;
with no retries, "voted in this term" is the whole rule here.

The uncontested round is the progress assumption made concrete: a candidate that hears
every reply without winning tells the driver (`RoundLost`), and once every candidate of
the round has, the driver fires one node's timer alone. Without such an assumption a
bounded checker can always find split votes that repeat until the bound.

## What building it taught

- The safety spec found a real design bug in a first draft: the candidate concluded
  its round ("heard every reply, lost") before counting the grant in that same last
  reply, so it reported a loss and then became leader. The check "never report losing a
  term you won" exists because the driver announces what it was told
  (`LossReported`).
- Safety alone left 30 mutants alive. Announcing every answer (`VoteAnswered`, not only
  grants) let `CandidaciesConclude` check that "heard all replies" is true, and
  announcing each node's starting disk state (`Disk`) made restored terms checkable.
- `tcOneNodeDown` is the only test where unanimity and majority differ.
- An earlier version suppressed the leader's step-down mutants as unobservable
  ("voting only"). They were not equivalent: a deposed leader kept accepting work.
  Giving the leader one observable action (`ClientAnswered`) and a scenario that
  deposes it (`tcDeposedLeader`) replaced six suppressions with assertions. A
  hard-coded `votes >= 2` needed a coverage suppression; the general majority rule
  with a four-node test checks the even-size boundary instead.

## Testing the implementation

Emit `ElectionStarted` after the candidate persists its term and self-vote, `VoteAnswered` after a
voter persists and before it replies, `BecameLeader` on the role change, and
`ClientAnswered` where a node accepts or redirects a client write. To force
the persistence race: let node 3 grant node 1, crash and restart node 3 before node 2's
request reaches it (hold that request in a test transport), then deliver it. The
restarted node must refuse. To force a deposed leader: once node 1 leads, deliver a
RequestVote with a higher term to it, then send it a client write; it must redirect.

Negative controls declare `// snappea: expect-bug "message"`. A matching bug
passes the test; a different failure or no discovered bug fails it. Check their
failure identity across seeds with `snappea check 'tcBug*' --no-mutation
--expect-bug-seeds 1,2,3 --workers 1 --timeout 10s`. Passing negative controls do
not fill coverage gaps or detect mutants in the correct design.

Search is complete only when this run reports `all N schedules` (or JSON
`search_complete: true`). A timeout, seed, or historical schedule count does not
guarantee completeness. An incomplete passing run is sampled evidence.
