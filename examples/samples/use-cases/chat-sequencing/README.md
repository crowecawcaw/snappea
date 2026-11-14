---
diataxis: explanation
personas: [application-developer, coding-agent]
questions:
  - How do I model a chat room with sequence numbers, dropped connections and resume?
  - How do I check "no gaps, no duplicates, and everyone catches up"?
  - Why must the client order by seq after a resume?
---

# Chat room sequencing with resume

A chat server gives each message in a room the next sequence number, logs it, and pushes
it to every connected client. A connection can drop at any moment, losing whatever it
was carrying. The client reconnects at once and subscribes with "resume after the last
seq I displayed". The server adds the new connection to the live set first, so nothing
posted later is missed, and then replays the log from that point as a separate step.
Messages posted in between go out both live and in the replay, and a live one can
overtake the replay. The client displays only the next expected seq and skips anything
else.

**Contract**

- Safety (`InOrderNoGaps`): the server numbers posted messages 1, 2, 3, ... once each;
  each client displays the room's messages in seq order with no gaps and no
  duplicates, each with the message the server gave that seq.
- Progress (`EveryoneCatchesUp`): once the senders stop, every client eventually
  displays every message, including after a dropped connection.

**Atomic primitives** (server handlers): post (seq + 1, log, push live), subscribe (join
the live set), replay (read the log after N, send it). Client: handle one delivery.
The network (`Connection`, in PTst) is the environment.

## Files

| File | What it holds |
| --- | --- |
| [`PSpec/Contract.p`](PSpec/Contract.p) | `InOrderNoGaps` (safety), `EveryoneCatchesUp` (progress, hot state) |
| [`PSrc/Server.p`](PSrc/Server.p) | The server: log, live push, subscribe then replay |
| [`PSrc/Client.p`](PSrc/Client.p) | The client (display by seq, reconnect) and the sender |
| [`PSrc/NoReplayServer.p`](PSrc/NoReplayServer.p) | Negative control: no log, no replay |
| [`PTst/Network.p`](PTst/Network.p) | `Network` and `Connection`: in-order delivery, drop budget |
| [`PTst/Tests.p`](PTst/Tests.p) | The `Room` driver and the test cases |

## Test cases

| Test | Drives | Why it matters |
| --- | --- | --- |
| `tcDropAndResume` | Two senders, one client whose first connection may drop | Drop with a message in flight; a post during the resume handshake; duplicates and overtaking live messages after the resume |
| `tcTwoClients` | Two senders, two clients, one connection may drop | The other client keeps receiving live while one catches up; sampled |
| `tcBugNoReplay` | `NoReplayServer` in the core race | **Expected bug**: `EveryoneCatchesUp detected liveness bug in hot state 'CatchingUp'` (a client never displays a message posted before it subscribed or lost on the dropped connection) |

`tcDropAndResume` isolates the reconnect race. Inspect `search_complete` for each
run; a passing run can still have sampled the bounded schedule space.

## Run

```sh
snappea check tcDropAndResume tcTwoClients --deny mutation
snappea check tcBugNoReplay      # passes only when the declared failure is found
```

## Bounds and omissions

One room, two messages from two senders, one or two clients, at most one dropped
connection. A dropped connection drops when a message reaches it; the client learns
of it at once and reconnects. Senders never lose a post. Not modeled: a server restart
(the log would need to be durable), log truncation (a resume older than the log), sender
retries and their deduplication, client restarts that lose the displayed seq.

One suppression: replaying from the start instead of from the resume point only sends
duplicates, which the client skips; the contract says nothing about bandwidth.

## What building it taught

- The first server replayed and subscribed in one step, so duplicates could not happen
  and the client's "skip" branch was dead code. Real servers often subscribe first
  and replay from storage after, and that order is what makes ordering by seq on the
  client necessary.
- The progress spec is what fails the negative control: without replay, no safety rule
  breaks; a client just stops short.

## Testing the implementation

Emit `Sequenced` when the server commits a message to the log, `Displayed` from the
client's render path, and `Posted` from the sender. To force the handshake race,
hold the server's replay read until a new message has been posted and pushed live on
the new connection; to force the drop, close the socket right after the server writes
a message to it.
