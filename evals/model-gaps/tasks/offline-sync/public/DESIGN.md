# Offline note sync with base versions

A mobile notes app lets users edit offline. When a client comes online it pushes its
user's change with the version the user edited (the base). The server accepts an update
only if its base is the current version; otherwise it answers with a conflict that
carries the server's copy. An editing client then merges (the server's copy plus its own
content) and retries on the server's version. A deleting client drops its delete,
because the note changed after its user last saw it. If the note was deleted meanwhile,
a merging edit restores it, so the user's work is kept.

There is one note. Its content is modeled as the set of edit ids it contains, so merge
is union and "edit 11 was lost" is a checkable statement. A deleted note has no content.
Each client makes one change in its lifetime: an edit, which adds its edit id to the
content, or a delete.

## Contract

- No edit is silently lost. Every accepted update is based on the current version,
  becomes the next version, and keeps all the content of the version it replaces. Only
  a delete removes content, and only from the version its user saw: an accepted delete
  is based on the copy its client started with and leaves no content.
- No request is applied twice. A request the server already applied is answered again
  with the version it became then, whatever happened since, and changes nothing.
- A conflict is real: the server refuses an update only when its base is not the
  current version, reports the current version, and never refuses a request it already
  applied. A resend after a lost reply must not conflict with itself.
- A client pushes only what it has reason to push. Its first push is based on the
  version its user edited. After that it pushes once per timeout, the same request
  again, or on a newer version that a conflict reported to it. It never pushes again on
  a version it already used or an older one: a reply to an earlier attempt, arriving
  late, is not news.
- Every user's change is resolved exactly once, with the outcome its updates justify.
  An edit is always applied. A delete is applied, or dropped after a conflict; a delete
  that was applied is not reported dropped. A client that reports its change applied
  reports the version its own update became. A client that drops its delete holds the
  copy a conflict sent it, and reports that version.
- Everything converges. Once every client has resolved its change and pulled, each
  holds exactly the server's note, and the server has seen every resend.

## Atomic steps

The server's update is one conditional write on the note's row that also records the
request id: a known request id returns the version recorded for it; otherwise a current
base writes version + 1 and records the request id; otherwise it conflicts. A fetch
returns the current copy. On the client, the offline change, each push, handling one
reply, handling a timeout and the pull are separate steps that interleave with the other
clients.

## Messages and announcements

The events are declared in `PSrc/Sync.p` and fixed.

A client is created with `ClientInit`: its owner, the server (or a network link in front
of it; treat it as the server), its id, the copy its user last saw, the id of the user's
change and whether it is a delete. It pushes at once.

- `Update` carries `client = this` (the reply goes there), the client's id, `req` = the
  id of the user's change in every attempt, the base, the full new content (empty for a
  delete) and `deleted`.
- The server answers `UpdateOk` with the version the request became, or
  `UpdateConflict` with its current copy.
- `SyncTimeout` tells a client that the request in flight timed out. It resends the
  same request.
- When its change is resolved the client sends `ClientIdle` to its owner, once. Later
  the owner sends `Pull`; the client sends `Fetch` with itself as payload to the server
  and takes the `FetchResp` as its copy.

The design announces these, for the specs:

| Event | Who, when | Payload |
| --- | --- | --- |
| `Accepted` | server, when it applies an update | the request's client id, request id and base; the new version, content and deleted flag |
| `Replayed` | server, every time it answers a request it had already applied | the request's client id, request id and base; the version the request became when applied |
| `Conflicted` | server, every time it refuses an update | the request's client id, request id and base; the current version |
| `OpDone` | client, once, when its change is resolved (before `ClientIdle`) | `applied = true` and the version from `UpdateOk`; or `applied = false` and the version of the copy the conflict returned |
| `Synced` | client, when it takes a `FetchResp` | its copy |

The test environment announces `SyncPlanned` (the initial note and the number of
clients), `OpPlanned` (each user's change) and `RequestTimedOut` (the network took a
reply away and sent that client a `SyncTimeout`).

## What can happen

Two or three clients, or one alone, start from the same copy; any mix of edits and
deletes. Their pushes reach the server in any order, and a client's merged retry can
conflict again. The reply to a client's first update can be lost, or delayed: either
way the client gets a `SyncTimeout` and resends, and a delayed reply then also arrives,
before or after the reply to the resend, before or after the client's next push, or
after its change is resolved. So a client can get two replies for one attempt, and the
late one can be an `UpdateOk` or an `UpdateConflict`. Other clients can write before,
between and after a client's push and its resend. Several clients can have a faulty
reply in the same run. Requests are not lost or duplicated by the network. Each client
is asked to pull once, after every client has resolved its change.

Not in scope: several notes, field-level merge, server crashes, a client going offline
again mid-retry, more than one timeout per client.
