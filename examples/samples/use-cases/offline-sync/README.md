---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model offline edits that sync later with a base version?
  - How do I keep a retried sync from conflicting with itself?
  - How do I check that no edit is silently lost?
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
- Every user's change is resolved: edits are always applied; a delete is applied or
  dropped after a real conflict. After every client stops editing and pulls, each
  holds exactly the server's note.

**Atomic primitive**: one conditional write on the note's row that also records the
request id: `update(req, base, content)` returns the recorded version for a known
`req`, writes version + 1 when `base` is current, and conflicts otherwise. The offline
edit, merge, retry and pull are separate steps that interleave.

Note content is modeled as the set of edit ids it contains, so "edit 11 was lost" is
a checkable statement.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `NoLostUpdates` (safety) and `SyncCompletes` (progress, hot state; also checks the base each client pushes) |
| [`PSrc/NoteServer.p`](PSrc/NoteServer.p) | The server and its conditional update |
| [`PSrc/SyncClient.p`](PSrc/SyncClient.p) | The client: push, merge or drop on conflict, resend on timeout, pull |
| [`PSrc/LastWriterWins.p`](PSrc/LastWriterWins.p) | Negative controls: last-writer-wins server; server without request ids |
| [`PTst/Tests.p`](PTst/Tests.p) | `LossyLink` (drops one reply), the driver, a scenario, test cases |

## Test cases

| Test | Drives | Why it matters | Expected |
| --- | --- | --- | --- |
| `tcTwoEditorsSameBase` | Two clients edit version 1 offline | The core conflict: the second must merge, not overwrite | pass, exhaustive |
| `tcLostReplyRetry` | Client 1's reply is lost and it resends; client 2 writes meanwhile | The retry is recognised by its edit id, not judged by its stale base | pass, exhaustive |
| `tcEditVsDelete` | One client deletes, the other edits, same base | Delete first: the edit restores the note. Edit first: the delete is dropped | pass, exhaustive |
| `tcGeneratedSync` | Three clients, each edits or deletes, any may lose a reply | Mixed histories | pass, sampled |
| `tcBugLastWriterWins` | Server ignores the base | **Expected to fail**: `update 21 from client 2 based on version 1 overwrote version 2` | fail |
| `tcBugNoRequestIds` | Server without request ids, one lost reply | **Expected to fail**: `request 11 conflicted with itself: it was already applied` | fail |

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
- The edit id is the idempotency key across every attempt. A fresh id per attempt was
  unobservable to this contract, but only because the model treats every timeout as a
  lost reply (see bounds). With slow replies, the client needs to tell which attempt a
  reply answers; the edit id alone cannot.
- Only the lossy client talks through the link: each forwarding hop multiplies the
  schedules, and the two-client tests stay exhaustive this way.

## Run

```sh
snappea check tcTwoEditorsSameBase tcLostReplyRetry tcEditVsDelete tcGeneratedSync --timeout 30s --deny mutation
snappea check tcBugLastWriterWins     # exits 1: ... overwrote version 2
snappea check tcBugNoRequestIds       # exits 1: ... conflicted with itself
```

## Bounds and omissions

One note, two or three clients, one change per client, at most one lost reply per
client. Requests are not lost or duplicated by the network (a lost request is the same
as a slower one); the final pull happens after every client stopped editing, which is
how convergence is stated. Not modeled: several notes, field-level merge (content is a
set, so merge is union), server crashes, clients going offline again mid-retry.

**A timeout is modeled only as a lost reply, never as a slow one.** In a real client
the timeout can fire while the reply is still on its way, so after the resend the
client gets two replies. The modeled client would then fail: in `Idle` it does not
handle a late `eUpdateOk`, and in `Pushing` a late conflict for an earlier attempt
looks like a fresh one and makes it push again, possibly based on an older version. An implementation must
drop replies that do not answer the attempt in flight: ignore any reply once the
change is resolved, and ignore a conflict whose server version is not newer than the
attempt's base (a fresh conflict always reports a newer version). To model it, let
`LossyLink` deliver the reply after the timeout instead of dropping it, and relax
`SyncCompletes` to accept any base a conflict reported to that client.

## Testing the implementation

Emit `eAccepted` and `eConflicted` from the server inside the update transaction, and
`eOpDone`/`eSynced` from the client. To force the races: hold client B's push until
client A's update commits (conflict); drop the HTTP response of an accepted update and
let the client's timeout resend it while another client writes (self-conflict); delete
from one device while the other is offline with an edit. Also delay (not drop) a
conflict response past the client's timeout, so the late reply arrives after the
merged retry was sent: the client must ignore it (see bounds).
