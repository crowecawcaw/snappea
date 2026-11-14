---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - Why does a quorum of n/2 allow two leaders in a 4-node cluster?
  - What does split brain look like in a P counterexample?
  - How do I model a network partition in P?
---

# Minority quorum: n/2 votes elect two leaders

**Difficulty: Medium.**

A four-node cluster elects a leader Raft-style. When a node's election timer fires,
it starts a new term (one past the newest term it has voted in), votes for itself
and asks every other node for its vote, one RPC at a time. A node grants at most one
vote per term, to the first candidate that asks. A candidate with a quorum of votes
is leader for that term.

**The flaw in one sentence:** the quorum is computed as `n / 2`, which for 4 nodes is
2, so two disjoint pairs of nodes, {1, 2} and {3, 4}, can each elect a leader in the
same term.

The code is plausible: `n / 2` reads as "half the votes". With 3 nodes, `3 / 2 = 1`
lets every candidate elect itself, which shows up fast; a team that only ever runs 4
nodes sees elections that work.

**Contract** (in [`PSpec/Contract.p`](PSpec/Contract.p), the same for both designs)

- `OneLeaderPerTerm` (safety): a node votes only in a term newer than any it voted in
  (so at most once per term), only for a node standing in that term, and denies a
  vote only if it already voted in that term or later; a candidate stands only in a
  term newer than its last vote; **a term has at most one leader**.
- `ElectionsEnd` (progress, hot state): every node whose timer fired ends its
  election, having voted for itself; each node answers a candidate once (an
  unreachable node counts as a failed request); a candidate gives up only after
  every node answered and never with a majority of votes.

The specs know the cluster size from the driver and count votes themselves; neither
reads the nodes' quorum setting.

**Atomic primitives** (one handler each): a node's vote decision (compare terms,
record the vote, reply), and a candidate counting one reply. A candidate's request
to one node is an RPC it waits on before asking the next.

**Modeling the partition.** Each candidate gets the cluster membership when its
timer fires. In the partition tests, the driver gives each side a view in which the
nodes on the other side are an `Unreachable` machine: a request to it fails, as an
RPC to an unreachable host does after its timeout. The node code is the same in
every test; only the network differs.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | The two specs and the events they observe |
| [`PSrc/Election.p`](PSrc/Election.p) | The protocol's messages and primitives |
| [`PSrc/HalfQuorum.p`](PSrc/HalfQuorum.p) | The flawed node: quorum `n / 2` |
| [`PSrc/Majority.p`](PSrc/Majority.p) | The fix: quorum `n / 2 + 1` |
| [`PTst/Tests.p`](PTst/Tests.p) | Drivers, the partitioned network, a scenario, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcBugPartition` | 4 nodes partitioned 2-2, candidates 1 and 3, quorum `n / 2` | Split brain from a partition | Expected bug: `split brain: term 1 has two leaders (node 1 and node 3)` |
| `tcBugSplitVote` | 4 connected nodes, candidates 1 and 3, quorum `n / 2` | Split brain from message order alone | Expected bug: same message |
| `tcFixedPartition` | The partition, majority quorum | Neither side elects a leader, and both give up | Passes |
| `tcFixedSplitVote` | The split vote, majority quorum | One leader with 3 votes, or nobody | Passes |
| `tcFixedOddCluster` | One candidate among 3 nodes | Boundary: quorum 2 of 3 must elect it | Passes |

## The counterexamples

Both bug tests fail with:

```text
Assertion Failed: PSpec/Contract.p:51:7 split brain: term 1 has two leaders (node 1 and node 3)
```

`tcBugPartition`, from `PCheckerOutput/tcBugPartition/bug.txt`: the network splits
{1, 2} from {3, 4}, and the timers of nodes 1 and 3 fire.

1. Node 3 starts term 1 and votes for itself. Its request to node 1 fails.
2. Node 1 starts term 1 and votes for itself. Node 2 grants it a vote: 2 votes.
3. Node 1 has `4 / 2 = 2` votes and becomes leader of term 1.
4. Node 3's request to node 2 fails. Node 4 grants node 3 a vote: 2 votes.
5. Node 3 becomes leader of term 1 too. `OneLeaderPerTerm` fails.

`tcBugSplitVote` needs no partition at all, only message order. Nodes 1 and 3 time
out together, and each asks its next neighbour first:

1. Nodes 3 and 1 both start term 1 and vote for themselves.
2. Node 2 receives node 1's request first and grants it. Node 1 has 2 votes and wins.
3. Node 1 denies node 3 (it voted for itself), and so does node 2.
4. Node 4 receives node 3's request and grants it. Node 3 has 2 votes and wins.

## Why ordinary tests miss it

Election tests usually start one candidate in a healthy cluster: it gets every vote
and wins, with any quorum from 1 to 4. Split brain needs two candidates in the same
term and a vote split exactly 2-2, which happens only when two timers fire within one
round trip of each other or the network splits evenly. Randomized election timeouts
make it rare, not impossible; a partition makes it certain.

## The fix

Quorum = `n / 2 + 1`, a strict majority. Any two majorities of the same cluster share
at least one node, and that node votes once per term, so two candidates cannot both
reach a quorum in the same term. The cost: with 4 nodes the quorum is 3, so a 2-2
split elects nobody this term (the `SplitVoteNobodyWins` scenario confirms the checker
reaches that outcome), and a later term with randomized timeouts is needed. A 4-node
cluster tolerates one failure, like a 3-node one; clusters usually have an odd size.

## Run

```sh
snappea check MinorityQuorum.pproj            # everything: the fix, the negative controls, coverage, mutation testing
snappea check MinorityQuorum.pproj 'tcBug*'   # only the negative controls: each passes only when its declared bug is found
```

Each negative control carries a native `// snappea: expect-bug` declaration: a matching
failure passes the test; no failure or a different failure fails it. Describe a
search as exhaustive only when its report says `search_complete`.

## Bounds and omissions

Three or four nodes, one election per candidate, one term each. A candidate asks one
node at a time: parallel requests add orderings but no new vote outcomes for this
property, and with them the split-vote search grows by orders of magnitude and no
longer finishes in a CI budget. Not modeled: retrying in a later term,
election timeouts, log comparison in the vote rule, leaders stepping down on a newer
term, membership changes, crashes and durable vote storage.

## Testing the implementation

Emit `Candidacy`, `VoteGranted`/`VoteDenied` and `ElectionEnded` from the
election code. To force the race, run four nodes in-process with a fault-injecting
transport, drop traffic between {1, 2} and {3, 4}, and fire the election timers of
nodes 1 and 3; assert at most one leader per term. Also unit-test the quorum function
itself for every cluster size from 1 to 7.
