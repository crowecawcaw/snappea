---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model Raft-style leader election with crashes in P?
  - How do I state "some leader is eventually elected" so a bounded checker can test it?
  - What must a node persist before it answers a vote request?
---

# Term-based leader election

Three nodes (four in one test, two in focused restart tests) elect a leader the way
Raft does, without the log: a node whose election timer fires increments its term,
votes for itself and asks the others for votes; a node grants at most one vote per
term; a candidate with a majority of the cluster becomes leader. A higher term in any
request or reply makes a node adopt it and step down. Only the leader accepts client
requests. Nodes can crash and restart, and messages are delayed arbitrarily.

**Contract**

- Safety (`ElectionSafety`): a node never votes twice in a term (its own candidacy
  counts); its term never goes back and it never grants a vote for an older term; a
  leader has a majority of votes in its term, counted from the answers; at most one
  leader per term; a node never reports losing a term it won, or one where a majority
  voted for it and it saw no higher term; after a loss it next stands above every term
  it was told of; a node accepts a client request only as leader of its current term,
  so a leader that saw a higher term has stepped down.
- Progress (`SomeLeaderElected`, bounded): once elections start, some node becomes
  leader. Assumptions: at most one instantaneous crash/restart, messages are delayed
  but not lost, and if every candidate of a round loses, one node's timer eventually
  fires alone after the earlier messages were handled (what Raft's randomized
  timeouts provide).
- Progress (`CandidaciesConclude`): every candidacy hears back from every peer,
  unless a newer candidacy supersedes it or a crash discards it. Replies already in
  flight can still arrive after cancellation, but cannot revive the old tally.
- Recovery (`LeaderAfterRestart`): the planned node acknowledges its restart and
  becomes leader in a fresh term. Winning before the crash does not satisfy recovery.
- Progress (`ProbeAnswered`): a client request is accepted or refused.

**Atomic primitives** (one handler each): timeout (term + 1 and self-vote, persisted,
then send requests); answer a RequestVote (adopt a higher term, grant if this node has
not voted in that term, persist, reply); handle a reply; answer a client request.
`term` and `votedTerm` are durable; the whole campaign record is volatile and is
discarded on restart.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `ElectionSafety`, election/reply progress, and `LeaderAfterRestart` |
| [`PSrc/Node.p`](PSrc/Node.p) | The node: follower, candidate, leader |
| [`PSrc/ForgetfulNode.p`](PSrc/ForgetfulNode.p) | Negative control: the vote is lost in a crash |
| [`PTst/Tests.p`](PTst/Tests.p) | The `Election` driver (timers, crash, uncontested round), the `Prober` client and the test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoCandidates` | Nodes 1 and 2 stand at once; node 3 decides | The core race: either candidate may see the other's request first and stand in a higher term. All schedules | Passes |
| `tcSplitVote` | Node 3 restarts having voted for itself in term 1; nodes 1 and 2 stand in term 1 | Split vote: nobody wins term 1, both report the loss, the uncontested round must elect. All schedules | Passes |
| `tcHigherTermReply` | Node 3 (and sometimes node 2) restarts at term 5; node 1 stands in term 1 | A candidate or leader gets a reply with a higher term; a loser must next stand above it. All schedules | Passes |
| `tcStaleVoteReply` | Node 1's timer fires twice in a row | Replies to its term-1 requests arrive during its term-2 candidacy and must not count. All schedules | Passes |
| `tcOneNodeDown` | Node 3 never answers | A majority, not unanimity: one grant plus the self-vote must win. All schedules | Passes |
| `tcFourNodesOneDown` | Four nodes, one never answers; node 1 stands | Exactly at the boundary: two votes of four are half, not a majority, and must not win; three must. All schedules | Passes |
| `tcDeposedLeader` | Node 1 stands in term 1, node 3 (restarted at term 5) in term 6; a client request to node 1 at any moment | A leader that sees a higher term in a request or a reply must stop accepting requests. Sampled | Passes |
| `tcGeneratedElection` | One or two candidates, maybe a crash and restart of a non-candidate | Crashes at any moment, including between granting one candidate and seeing the other. Sampled | Passes |
| `tcCandidateRestart` | A timeout followed by a crash, while replies race with the crash | The old campaign is discarded; scenarios require crashes both before and after winning, followed by refusal of a client request before reelection. All schedules | Passes |
| `tcSelfVoteSurvivesRestart` | Two self-votes in term 1; the loser restarts before the competitor's delayed request | A persisted self-vote must prevent a second vote in the same term; the next term can succeed. All schedules | Passes |
| `tcRequestStepDownRestart` | A controlled peer's higher-term request makes the candidate step down, then it crashes | The higher term and granted vote survive a former-candidate restart. All schedules | Passes |
| `tcReplyStepDownRestart` | A controlled peer rejects the campaign with a higher term, then the target crashes | The higher term learned from a reply survives restart; recovery advances beyond it. All schedules | Passes |
| `tcBugVoteNotPersisted` | `ForgetfulNode`, two candidates, node 3 crashes | The restarted voter forgot its vote and grants again | Expected bug: `node 3 voted twice in term 1: for 2 and for 1` |

## What building it taught

- The safety spec found a real design bug in a first draft: the candidate concluded
  its round ("heard every reply, lost") before counting the grant in that same last
  reply, so it reported a loss and then became leader. The check "never report losing a
  term you won" exists because the driver announces what it was told
  (`LossReported`).
- Safety alone left 30 mutants alive. Announcing every answer (`VoteAnswered`, not only
  grants) let `CandidaciesConclude` check that "heard all replies" is true, and
  announcing each node's starting disk state (`Disk`) made restored terms checkable.
- `tcOneNodeDown` and `tcFourNodesOneDown` separate majority from unanimity.
- An earlier version suppressed the leader's step-down mutants as unobservable
  ("voting only"). They were not equivalent: a deposed leader kept accepting work.
  Giving the leader one observable action (`ClientAnswered`) and a scenario that
  deposes it (`tcDeposedLeader`) replaced six suppressions with assertions. A
  hard-coded `votes >= 2` needed a coverage suppression; the general majority rule
  with a four-node test checks the even-size boundary instead.

## Run

```sh
snappea check tcTwoCandidates tcSplitVote tcHigherTermReply tcStaleVoteReply tcOneNodeDown \
  tcFourNodesOneDown tcDeposedLeader tcGeneratedElection tcCandidateRestart \
  tcSelfVoteSurvivesRestart tcRequestStepDownRestart tcReplyStepDownRestart \
  --deny mutation
snappea check tcBugVoteNotPersisted      # passes when it finds the double vote above
```

The `CrashBeforeWinning` and `RestartAfterWinning` scenarios require evidence that
the candidate-restart test exercised both roles.

## Bounds and omissions

Three nodes (four in `tcFourNodesOneDown`, two in the controlled-peer restart
tests), at most two contested candidates, and at most one crash per test. A crash and
its restart are one atomic step. Candidate, follower, and leader roles all restart
as followers. In-flight messages survive and are delayed; packet loss, an interval
of unavailability, retransmissions, the log, heartbeats, and pre-vote are not modeled.

The two-node tests use an ordinary driver as a controlled peer to isolate a
higher-term request, a higher-term reply, and a delayed competing vote request. It
announces its persisted term/votes and actual replies so the same independent
safety and progress specs check them. The timer starts a fresh election after the
restart acknowledgment. This is the recovery assumption, not spontaneous progress
from a stopped timer.

The spec retains historical winners to check one leader per term, but retires a
node's authority when it restarts. A client request sent after the restart and before
the fresh timer must be refused even if that node won the old term.

Two suppressions, both equivalent mutants: removing a candidate's `goto Follower`
after it sees a higher term. Both roles refuse clients, handle vote requests and
replies the same way, and discard the whole campaign on crash; the higher-term guard
keeps old replies from winning, and only a new timeout starts a candidacy. The
leader's step-down is not suppressed: it is observable through client acceptance.

Raft persists `votedFor` so a repeated request from the same candidate can be granted
again. These tests send each candidate's request once; the whole voting rule here is
“at most one vote per term.” Restart does not clear that durable vote.

The uncontested round is the progress assumption made concrete: a candidate that hears
every reply without winning tells the driver (`RoundLost`), and once every candidate of
the round has, the driver fires any one node's timer alone. If that node loses (it was
behind on terms), its replies told it the highest term, so the driver fires the same
node again and it wins. Without such an assumption a bounded checker can always find
split votes that repeat until the bound.

## Testing the implementation

Emit `ElectionStarted` after the candidate persists its term and self-vote,
`VoteAnswered` after a voter persists and before it replies, `BecameLeader` on the role change, and
`ClientAnswered` where a node accepts or redirects a client write. To force
the persistence race: let node 3 grant node 1, crash and restart node 3 before node 2's
request reaches it (hold that request in a test transport), then deliver it. The
restarted node must refuse. To force a deposed leader: once node 1 leads, deliver a
RequestVote with a higher term to it, then send it a client write; it must redirect.
For restart tests, crash after persisting a self-vote or observing a higher term,
retain queued replies, and restart from disk. Before firing another timer, send a
client request (must refuse) or a competing request in the already-voted term (must
not receive another vote). Then let one new election run without another crash.

