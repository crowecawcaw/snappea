/*
 * Lease lock: a liveness spec for a lock that can be held forever.
 *
 * File map: OneHolderAtATime = safety spec; EveryWaiterAcquires, EveryHolderActs =
 * liveness specs; WokenHolderFenced = scenario (coverage only); LeaseLockServer +
 * Resource = fixed design; NoExpiryLockServer = broken design; Client = acquire,
 * write, release; ReleaseLink, StartClients, Start* = faults and workloads
 * (drivers); test tc* = selected properties.
 *
 * Two clients acquire one lock, write to a Resource under the lock's token, then
 * release. One fault per test: the first client stops while holding, for good
 * (tcCrashedHolder) or until it wakes (tcPausedHolder), or the first release is
 * lost (tcLostRelease); tcNoFault has none. NoExpiryLockServer then holds
 * the lock forever, the other client waits forever, and EveryWaiterAcquires stays
 * in its hot state. tcNoExpiry* declare that liveness bug with expect-bug, so they
 * pass only when the checker finds it.
 * LeaseLockServer lets a lease expire (LeaseExpired is `$`: any time, as the
 * environment chooses) and grants increasing tokens. Resource refuses a token older
 * than one it accepted, so a holder that wakes after expiry cannot act as holder.
 * tcCrashedHolder and tcLostRelease are fixed-design tests where expiry is the only
 * way forward: remove it and EveryWaiterAcquires fails.
 *
 * Clients retry until they get the lock. Do not give up after N attempts: a client
 * that gives up leaves the wait without the lock, and a spec that accepts that
 * outcome passes a design whose lock is stuck forever.
 *
 * Run from this directory:
 *   snappea check                    # all tests, with mutation testing; must pass
 *   snappea check tcNoFault tcPausedHolder tcCrashedHolder tcLostRelease --no-mutation
 *   snappea check 'tcNoExpiry*' --no-mutation   # pass by finding the liveness bug
 *   snappea check mutation           # deeper search per mutant; detects every one
 * Suppressed mutants are marked in the code: with expiry at any time, an early or
 * missing release only ends a lease sooner or later, which no spec can tell apart.
 *
 * Limits: one lock, two clients, at most one crash or one lost release; acquire,
 * grant and write messages are not lost. A lease may expire before its holder
 * writes: there is no clock, so this checks safety under any expiry timing, not that
 * a lease is long enough. Liveness assumes fair scheduling: a lease eventually
 * expires and a retry eventually runs.
 */

// --- Observable events and specs ---

event eLockPlan: int;                      // clients that will request the lock
event eWaiting: machine;                   // a client starts waiting for the lock
event eGranted: (client: machine, token: int);
event eLeaseEnded: int;                    // the server freed this token's lease
event eWritten: int;                       // Resource accepted work under this token
event eRejected: (client: machine, token: int);   // Resource refused a stale token

// Safety: one holder at a time. The server grants a lease only after the previous
// one ended (released or expired), with a larger token each time. The Resource,
// where holders act, never accepts work from an older holder once a newer one has
// acted, and refuses only such stale work. Holders are ordered by the grants this
// spec saw, not by the Resource's own comparison.
spec OneHolderAtATime observes eGranted, eLeaseEnded, eWritten, eRejected {
  var grantOrder: map[int, int];           // token -> position in grant order
  var newestToken: int;
  var live: bool;                          // the newest lease has not ended
  var newestActed: int;                    // grant position of the newest holder that acted
  start state Watching {
    on eGranted do (g: (client: machine, token: int)) {
      assert !live, format("two holders at once: granted token {0} while token {1} was live", g.token, newestToken);
      assert g.token > newestToken, format("token {0} is not larger than earlier token {1}", g.token, newestToken);
      grantOrder[g.token] = sizeof(grantOrder) + 1;
      newestToken = g.token;
      live = true;
    }
    on eLeaseEnded do (t: int) {
      assert live && t == newestToken, format("ended token {0}, which is not the live lease", t);
      live = false;
    }
    on eWritten do (t: int) {
      assert t in grantOrder, format("work accepted under token {0}, which was never granted", t);
      assert grantOrder[t] >= newestActed, format("two holders at once: token {0} acted after a newer holder", t);
      newestActed = grantOrder[t];
    }
    on eRejected do (r: (client: machine, token: int)) {
      assert r.token in grantOrder && grantOrder[r.token] < newestActed,
        format("rejected token {0}, but no newer holder had acted", r.token);
    }
  }
}

// Coverage, not a check: a holder paused past its lease resumed its write and was
// fenced off. Its token was written twice (before the pause and on waking) and
// then rejected after a newer holder acted.
scenario WokenHolderFenced observes eWrite, eRejected {
  var writes: map[int, int];
  start hot state Watching {
    on eWrite do (w: (client: machine, token: int)) {
      if (!(w.token in writes)) { writes[w.token] = 0; }
      writes[w.token] = writes[w.token] + 1;
    }
    on eRejected do (r: (client: machine, token: int)) {
      if (r.token in writes && writes[r.token] == 2) { goto Fenced; }
    }
  }
  cold state Fenced { }
}

// Liveness: a client waiting for the lock eventually gets it. Hot while any
// planned client has not acquired it yet; cold once all of them have.
spec EveryWaiterAcquires observes eLockPlan, eWaiting, eGranted {
  var planned: int;
  var waiting: set[machine];
  var acquired: set[machine];
  start hot state AwaitingPlan {
    on eLockPlan goto ClientWaitingForLock with (n: int) {
      assert n > 0, "a lock test must plan at least one client";
      planned = n;
    }
  }
  hot state ClientWaitingForLock {
    on eWaiting do (c: machine) {
      assert !(c in waiting) && !(c in acquired), "a client started waiting twice";
      waiting += (c);
    }
    // snappea-lint: allow payload-field-checked-by-other-spec -- token order is OneHolderAtATime's job
    on eGranted do (g: (client: machine, token: int)) {
      assert g.client in waiting, "lock granted to a client that was not waiting";
      waiting -= (g.client);
      acquired += (g.client);
      if (sizeof(acquired) == planned) { goto AllAcquired; }
    }
  }
  cold state AllAcquired {
    on eWaiting do { assert false, "more clients waited than the plan declared"; }
    on eGranted do { assert false, "lock granted again after every client acquired it"; }
  }
}

// Liveness: every holder acts. Each granted token is written at least once, and
// the Resource answers every write (accepted or rejected), crashed holder or not.
spec EveryHolderActs observes eGranted, eWrite, eWritten, eRejected {
  var unwritten: set[int];                 // granted tokens not written yet
  var unanswered: map[int, int];           // token -> writes without an answer
  start cold state Settled {
    on eGranted goto HolderActing with Granted;
    on eWrite goto HolderActing with Wrote;
    on eWritten do { assert false, "a write was answered while none was waiting"; }
    on eRejected do { assert false, "a write was rejected while none was waiting"; }
  }
  hot state HolderActing {
    on eGranted do Granted;
    on eWrite do Wrote;
    on eWritten do (t: int) { Answered(t); if (Idle()) { goto Settled; } }
    on eRejected do (r: (client: machine, token: int)) { Answered(r.token); if (Idle()) { goto Settled; } }
  }
  fun Granted(g: (client: machine, token: int)) { unwritten += (g.token); }
  fun Wrote(w: (client: machine, token: int)) {
    unwritten -= (w.token);
    if (!(w.token in unanswered)) { unanswered[w.token] = 0; }
    unanswered[w.token] = unanswered[w.token] + 1;
  }
  fun Answered(t: int) {
    assert t in unanswered && unanswered[t] > 0, format("answer for token {0}, which has no write waiting", t);
    unanswered[t] = unanswered[t] - 1;
    if (unanswered[t] == 0) { unanswered -= (t); }
  }
  fun Idle(): bool { return sizeof(unwritten) == 0 && sizeof(unanswered) == 0; }
}

// --- Protocol messages ---

event eAcquire: machine;
event eBusy;
event eRelease: int;
event eWrite: (client: machine, token: int);

// --- Fixed design: leases expire; tokens fence off expired holders ---

machine LeaseLockServer {
  var token: int;                          // token of the latest grant
  var held: bool;
  start state Serving {
    on eAcquire do (c: machine) {
      // A lease ends at release or expiry; the server reads its clock when asked.
      // snappea-lint: allow mutation:drop-conjunct -- a lease may expire at any time, including at once
      if (held && LeaseExpired()) { EndLease(); }
      if (held) { send c, eBusy; return; }
      held = true;
      token = token + 1;
      send c, eGranted, (client = c, token = token);
    }
    // A release under an older token (its lease expired) must not free a newer lease.
    // Without release a lease still ends at expiry, which may come at any time: no
    // spec can tell a slow lock from a released one, so these mutants survive.
    // snappea-lint: allow mutation-nested:remove-statement,condition-false,swap-operator -- release only ends a lease sooner than expiry
    on eRelease do (t: int) { if (t == token) { EndLease(); } }
  }
  fun EndLease() { held = false; announce eLeaseEnded, token; }
  // The real clock check (now > grantTime + leaseLength): true at some point the
  // environment chooses, not decided by the design.
  fun LeaseExpired(): bool { return $; }
}

// The protected resource: accepts work only from the newest token it has seen.
machine Resource {
  var newest: int;
  start state Serving {
    on eWrite do (w: (client: machine, token: int)) {
      if (w.token < newest) { send w.client, eRejected, w; return; }
      newest = w.token;
      send w.client, eWritten, w.token;
    }
  }
}

// --- Broken design: no expiry, so a lock whose holder never releases is held forever ---

machine NoExpiryLockServer {
  var token: int;
  var held: bool;
  start state Serving {
    on eAcquire do (c: machine) {
      if (held) { send c, eBusy; return; }
      held = true;
      token = token + 1;
      send c, eGranted, (client = c, token = token);
    }
    on eRelease do (t: int) { if (t == token) { held = false; announce eLeaseEnded, token; } }
  }
}

// --- Client: wait (retrying until granted), use, release ---

event eStart: (server: machine, resource: Resource, link: ReleaseLink);
event eCrash;
event eWake;
event eForwardRelease: int;

machine Client {
  var server: machine;
  var resource: Resource;
  var link: ReleaseLink;
  var token: int;
  // Faults are sent before eStart and kept until the client holds the lock.
  start state Init {
    // snappea-lint: allow mutation:defer-to-ignore -- an ignored fault is a run without that fault
    defer eCrash, eWake;
    on eStart goto Waiting with (cfg: (server: machine, resource: Resource, link: ReleaseLink)) {
      server = cfg.server;
      resource = cfg.resource;
      link = cfg.link;
    }
  }
  state Waiting {
    // snappea-lint: allow mutation:defer-to-ignore -- an ignored fault is a run without that fault
    defer eCrash, eWake;
    entry {
      announce eWaiting, this;
      send server, eAcquire, this;
    }
    on eBusy do { send server, eAcquire, this; }   // retry until granted; never give up
    on eGranted goto Holding with (g: (client: machine, token: int)) {
      token = g.token;
      send resource, eWrite, (client = this, token = token);
    }
  }
  state Holding {
    // snappea-lint: allow mutation-nested:remove-statement -- a missing release is a lost release; expiry recovers
    on eWritten goto Done with { send link, eForwardRelease, token; }
    on eRejected goto Done;                // fenced off: its lease had expired
    on eCrash goto Crashed;                // never releases
  }
  // Stopped while holding; the reply to its write is lost with it. eWake means it
  // was only paused: it retries the write as if it still held the lock, and only
  // the token stops it once the lease has expired and another client acted.
  state Crashed {
    ignore eWritten, eRejected;
    // Scenario WokenHolderFenced, not a spec, needs this write: resuming is optional.
    // snappea-lint: allow mutation-nested:remove-statement -- a paused holder need not resume its work
    on eWake goto Holding with { send resource, eWrite, (client = this, token = token); }
  }
  state Done { ignore eWritten, eRejected; }   // the second reply after a resumed write
}

// --- Faults and workloads (drivers: the environment, not the design) ---

// Carries releases to the server, losing the first `drops` of them.
// snappea: driver
machine ReleaseLink {
  var server: machine;
  var drops: int;
  start state Up {
    entry (cfg: (server: machine, drops: int)) { server = cfg.server; drops = cfg.drops; }
    on eForwardRelease do (t: int) {
      if (drops > 0) { drops = drops - 1; return; }
      send server, eRelease, t;
    }
  }
}

enum tFaults { NoFault, HolderPauses, HolderCrashes, ReleaseLost }

// Two clients. The first: HolderPauses = stops while holding, then wakes;
// HolderCrashes = stops while holding for good; ReleaseLost = the first release
// sent is lost. One fault class per test keeps each search small and complete.
fun StartClients(server: machine, faults: tFaults) {
  var resource: Resource;
  var link: ReleaseLink;
  var first: Client;
  var drops: int;
  if (faults == ReleaseLost) { drops = 1; }
  resource = new Resource();
  link = new ReleaseLink((server = server, drops = drops));
  announce eLockPlan, 2;
  first = new Client();
  if (faults == HolderPauses || faults == HolderCrashes) { send first, eCrash; }
  if (faults == HolderPauses) { send first, eWake; }
  send first, eStart, (server = server, resource = resource, link = link);
  send new Client(), eStart, (server = server, resource = resource, link = link);
}

// snappea: driver
machine StartNoFault { start state S { entry { StartClients(new LeaseLockServer(), NoFault); } } }
// snappea: driver
machine StartPausedHolder { start state S { entry { StartClients(new LeaseLockServer(), HolderPauses); } } }
// snappea: driver
machine StartCrashedHolder { start state S { entry { StartClients(new LeaseLockServer(), HolderCrashes); } } }
// snappea: driver
machine StartLostRelease { start state S { entry { StartClients(new LeaseLockServer(), ReleaseLost); } } }
// snappea: driver
machine StartNoExpiryCrash { start state S { entry { StartClients(new NoExpiryLockServer(), HolderCrashes); } } }
// snappea: driver
machine StartNoExpiryLostRelease { start state S { entry { StartClients(new NoExpiryLockServer(), ReleaseLost); } } }

module LeaseFaults = { ReleaseLink, Resource, Client };

test tcNoFault [main=StartNoFault]:
  assert OneHolderAtATime, EveryWaiterAcquires, EveryHolderActs in (union { StartNoFault, LeaseLockServer }, LeaseFaults);
test tcPausedHolder [main=StartPausedHolder]:
  assert OneHolderAtATime, EveryWaiterAcquires, EveryHolderActs in (union { StartPausedHolder, LeaseLockServer }, LeaseFaults);
test tcCrashedHolder [main=StartCrashedHolder]:
  assert OneHolderAtATime, EveryWaiterAcquires, EveryHolderActs in (union { StartCrashedHolder, LeaseLockServer }, LeaseFaults);
test tcLostRelease [main=StartLostRelease]:
  assert OneHolderAtATime, EveryWaiterAcquires, EveryHolderActs in (union { StartLostRelease, LeaseLockServer }, LeaseFaults);

// snappea: expect-bug "EveryWaiterAcquires detected liveness bug in hot state 'ClientWaitingForLock'"
test tcNoExpiryCrash [main=StartNoExpiryCrash]:
  assert OneHolderAtATime, EveryWaiterAcquires, EveryHolderActs in (union { StartNoExpiryCrash, NoExpiryLockServer }, LeaseFaults);
// snappea: expect-bug "EveryWaiterAcquires detected liveness bug in hot state 'ClientWaitingForLock'"
test tcNoExpiryLostRelease [main=StartNoExpiryLostRelease]:
  assert OneHolderAtATime, EveryWaiterAcquires, EveryHolderActs in (union { StartNoExpiryLostRelease, NoExpiryLockServer }, LeaseFaults);
