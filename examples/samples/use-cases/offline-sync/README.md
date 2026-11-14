---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model offline edits that sync later with a base version?
  - How do I keep a retried sync from conflicting with itself?
  - How do I check that no edit is silently lost?
  - How should a client handle a reply that arrives after its timeout?
---

# Offline note sync with base versions

A mobile notes app lets users edit offline. When a client comes online it pushes its
change with the version it edited (the base). The server accepts the update only if
the base is the current version; otherwise it returns a conflict with the server copy.
An editing client merges (server copy plus its own content) and retries on the server's
version; a deleting client drops its delete, because the note changed after the user
last saw it. If the server already deleted the note, a merging edit restores it, so the
user's work is kept.

**Contract**

- No edit is silently lost: every accepted update is based on the current version and
  keeps all of its content. Only a delete supersedes content, and only the version its
  user saw.
- A conflict is reported only when the base really is stale, and never for a request
  the server already applied (a retry after a lost reply must not conflict with itself).
- A timeout does not mean the reply was lost: it may arrive after the resend, so a
  client can get two replies for one attempt. Each push is the first push on the
  edited version, one resend per timeout, or a push on a newer version a conflict
  reported. The client ignores every reply once its change is resolved, and a conflict
  whose version is not newer than its attempt's base.
- Every user's change is resolved: edits are always applied; a delete is applied or
  dropped after a real conflict. After every client stops editing and pulls, each
  holds exactly the server's note.

**Atomic primitive**: one conditional write on the note's row that also records the
request id: `update(req, base, content)` returns the recorded version for a known
`req` (announced as a replay), writes version + 1 when `base` is current, and
conflicts otherwise. The offline
edit, merge, retry and pull are separate steps that interleave.

Note content is modeled as the set of edit ids it contains, so "edit 11 was lost" is
a checkable statement.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `NoLostUpdates` (safety) and `SyncCompletes` (progress, hot state; also checks the base of every push, resend and replay) |
| [`PSrc/NoteServer.p`](PSrc/NoteServer.p) | The server and its conditional update |
| [`PSrc/SyncClient.p`](PSrc/SyncClient.p) | The client: push, merge or drop on conflict, resend on timeout, ignore late replies, pull |
| [`PSrc/LastWriterWins.p`](PSrc/LastWriterWins.p) | Negative controls: last-writer-wins server; server without request ids; client that takes every conflict as fresh |
| [`PTst/Tests.p`](PTst/Tests.p) | `LossyLink` (drops or delays one reply), imported `BBDelivery` (delivers the late reply), the driver, a scenario, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoEditorsSameBase` | Two clients edit version 1 offline | The core conflict: the second must merge, not overwrite | pass, bounded search |
| `tcLostReplyRetry` | Client 1's reply is lost and it resends; client 2 writes meanwhile | The retry is recognised by its edit id, not judged by its stale base | pass, bounded search |
| `tcSlowReply` | Client 1's first reply is late: it times out, resends, and gets both replies in any order; client 2 writes meanwhile | Late `UpdateOk` after the change is resolved; late conflict after the merged retry was sent | pass, bounded search |
| `tcEditVsDelete` | One client deletes, the other edits, same base | Delete first: the edit restores the note. Edit first: the delete is dropped | pass, bounded search |
| `tcGeneratedSync` | Three clients, each edits or deletes, any may lose a reply or get it late | Mixed histories | pass, sampled |
| `tcBugLastWriterWins` | Server ignores the base | **Expected bug**: `update 21 from client 2 based on version 1 overwrote version 2` | matched bug |
| `tcBugNoRequestIds` | Server without request ids, one lost reply | **Expected bug**: `request 11 conflicted with itself: it was already applied` | matched bug |
| `tcBugStaleReply` | `tcSlowReply` with a client that merges every conflict | **Expected bug**: `client 1 pushed base 2, not newer than its last push on base 2: it took a late reply as fresh` | matched bug |

The `EditRestoresDeletedNote` scenario confirms the checks reach a delete followed by
a restoring edit.

## What building it taught

- The spec must check what each client *sends*, not only what the server decides. A
  client that pushed base 0 instead of the version its user edited simply conflicted,
  merged and succeeded, and nothing failed until `SyncCompletes` required the first
  push to carry the edited base and every later push the base the conflict reported.
- Put fault injection in the test network (`LossyLink`), not in the client. Removing a
  fault can never fail a spec, so in the client it produced undetectable mutants; in a
  driver machine it is not mutated, and the client's timeout handler becomes checked.
- The edit id is the idempotency key across every attempt, so it cannot tell which
  attempt a reply answers. The base can: a fresh conflict always reports a version
  newer than the attempt's base, so the client drops any conflict that does not.
- A late reply needs its own sender. P delivers one sender's messages in order, so the
  link hands the late reply to the imported `BBDelivery`, which can deliver it before or after
  anything the link sends next.
- The naive client's duplicate push was invisible at first: the server answered it as
  a replay without announcing anything. Announcing replays, and requiring each
  timeout's resend to be seen before the client pulls, made it checkable with two
  clients, without requiring a larger workload.
- Only the lossy client talks through the link: each forwarding hop multiplies the
  schedules without adding a new fault to the other client.
- Negative controls still complete the normal protocol on schedules that do not
  expose their flaw. `BuggyStaleReply` waits for both edits to finish and asks both
  clients to pull, just like the correct driver. Omitting those pulls creates an
  unrelated convergence failure that can hide the intended stale-reply assertion.

## Run

```sh
snappea check tcTwoEditorsSameBase tcLostReplyRetry tcSlowReply tcEditVsDelete tcGeneratedSync --timeout 30s --deny mutation
snappea check tcBugLastWriterWins     # passes on the expected bug: ... overwrote version 2
snappea check tcBugNoRequestIds       # passes on the expected bug: ... conflicted with itself
snappea check tcBugStaleReply         # passes on the expected bug: ... took a late reply as fresh
```

## Bounds and omissions

The `--timeout 30s` is a search budget, not a completeness guarantee.

One note, two or three clients, one change per client, at most one timeout per
client, on its first reply: that reply is either lost or delivered late, after the
resend. Requests are not lost or duplicated by the network (a lost request is the same
as a slower one); the final pull happens after every client stopped editing, which is
how convergence is stated. Not modeled: several notes, field-level merge (content is a
set, so merge is union), server crashes, clients going offline again mid-retry, a
timeout on a later attempt (the same rule applies: compare the conflict's version with
the attempt's base).

## Testing the implementation

Emit `Accepted` and `Conflicted` from the server inside the update transaction, and
`Replayed` when it answers a request it already applied, and `OpDone`/`Synced` from
the client. To force the races: hold client B's push until
client A's update commits (conflict); drop the HTTP response of an accepted update and
let the client's timeout resend it while another client writes (self-conflict); delete
from one device while the other is offline with an edit. For the slow reply, delay
(not drop) a response past the client's timeout and release it after the resend, both
for an accepted update (a late success after the change is resolved) and for a
conflict (a late conflict after the merged retry was sent): `SyncCompletes` fails if
the client pushes again on a version it already pushed.

## Reuse the delivery building block

The project imports `Network.pproj` from Snappea's guide sources and uses
`BBDelivery` for independently scheduled delivery. It does not add loss or duplicate
messages by itself. When copying this sample outside the repository, run
`snappea guide example building-blocks --out vendor`, commit the exported files,
and point the project's `IncludeProject` at `vendor/building-blocks/Network.pproj`.

Negative controls declare `// snappea: expect-bug "message"`. A matching bug
passes the test; a different failure or no discovered bug fails it. Check their
failure identity across seeds with `snappea check 'tcBug*' --no-mutation
--expect-bug-seeds 1,2,3 --workers 1 --timeout 10s`. Passing negative controls do
not fill coverage gaps or detect mutants in the correct design.

Search is complete only when this run reports `all N schedules` (or JSON
`search_complete: true`). A timeout, seed, or historical schedule count does not
guarantee completeness. An incomplete passing run is sampled evidence.

Each client models one offline edit for its entire lifetime. Its reply request ID
therefore cannot name another operation; the design-payload advisory is retained
for review. Reusing a client for later edits requires matching reply IDs before
applying them, in addition to the version and late-reply checks shown here.
