/*
 * File map (search these names):
 * RetryContract = oracle; DurableService = atomic durable design;
 * SplitCommitService/ReapplyReplayService = negative designs;
 * *Driver and ConcurrentCaller = workloads; test tc* = selected properties.
 *
 * Durable retry: effects and outcome memo must survive together.
 *
 * Contract: apply each request ID once; retry returns its original result without
 * reverting newer state. DurableService co-commits total + memo and retains both
 * across restart. Adjacent assignments model an ACTUAL atomic durable primitive;
 * send/receive/create can yield. SplitCommitService loses its volatile memo after
 * persisting the effect; ReapplyReplayService returns the old reply but reapplies.
 *
 * Oracle: requests create obligations, not assumed effects; unique commits advance
 * an independent expected total. Actual eReply sends must preserve original result
 * AND current total. Observe at reply creation, before delayed delivery; receipt
 * order across clients is not commit order. Drivers validate pending IDs and wait
 * for real replies. Hot completion requires nonzero work and every ID committed
 * and returned; safety alone must not accept an abandoned or silent workload.
 *
 * Scenarios: RetryDriver loses reply 7, restarts and retries; CompletedReplayDriver
 * retries old 7 after 8 advanced state; ConcurrentRetryDriver has two independent
 * clients, stable IDs 7/8 and one retry each. Concurrent case has no crashes;
 * fixed crash cases have one lost reply and at most two restarts. Keep both.
 *
 * Run from this directory:
 *   snappea check DurableRetry.p tcDurableRetry tcCompletedReplay tcConcurrentRetries
 *   snappea check DurableRetry.p tcSplitCommitRetry
 *   snappea check DurableRetry.p tcReappliedReplay
 * Last two must fail: "one request caused more than one effect". Unfiltered check
 * includes broken tests. Default mutation feedback is additional evidence.
 *   snappea observe DurableRetry.p valid-concurrent.jsonl --spec RetryContract --strict --check-hot
 * Other invalid-*.jsonl files must fail duplicate effect, incomplete/premature/
 * zero-work completion, or a reply-created current-state regression. These manual
 * traces validate the oracle, not implementation capture.
 *   python -m unittest -v
 * The regression corrupts only real eReply payloads and requires all three correct
 * workloads to reject them; a parallel audit event cannot hide a bad response.
 *
 * Limits: one service, at most two clients/IDs, increment effects; no concurrent
 * crash/recovery, other crash points, conflicting payloads, expiry, arbitrary loss,
 * storage failure or database isolation model. Finite liveness is not production
 * fairness or unbounded recovery correctness. Real capture: reservation example.
 */

// --- Independent contract and observable events ---

// Public history reported by the driver and service. The contract does not
// inspect service variables. Requests create obligations; only unique commits
// advance the effect total. Several requests may be outstanding at once.
event eRequest: int;
event eCommit: (id: int, total: int);
event eReply: (id: int, result: int, currentTotal: int);
event eScenarioDone;

spec RetryContract observes eRequest, eCommit, eReply, eScenarioDone {
  var requested: map[int, bool];
  var commitResults: map[int, int];
  var returned: map[int, bool];
  var requestedIds: int;
  var expectedTotal: int;
  var returnedIds: int;

  start hot state AwaitingOutcome {
    on eRequest do (id: int) {
      if (!(id in requested)) {
        requested[id] = true;
        requestedIds = requestedIds + 1;
      }
    }
    on eCommit do (commit: (id: int, total: int)) {
      assert commit.id in requested, "commit without a request";
      assert !(commit.id in commitResults), "one request caused more than one effect";
      expectedTotal = expectedTotal + 1;
      assert commit.total == expectedTotal,
        format("expected effect total {0} at commit, got {1}", expectedTotal, commit.total);
      commitResults[commit.id] = commit.total;
    }
    on eReply do (outcome: (id: int, result: int, currentTotal: int)) {
      assert outcome.id in requested, "reply without a request";
      assert outcome.id in commitResults, "successful outcome without a commit";
      assert outcome.result == commitResults[outcome.id],
        format("request {0} replayed result {1}, expected original result {2}",
          outcome.id, outcome.result, commitResults[outcome.id]);
      // Selected specs observe the actual eReply send before client delivery.
      // Receipt order at different clients would not identify this snapshot.
      assert outcome.currentTotal == expectedTotal,
        format("expected current effect total {0}, got {1}",
          expectedTotal, outcome.currentTotal);
      if (!(outcome.id in returned)) {
        returned[outcome.id] = true;
        returnedIds = returnedIds + 1;
      }
    }
    on eScenarioDone do {
      assert requestedIds > 0, "scenario completed without any requests";
      assert expectedTotal == requestedIds, "some requested effects never committed";
      assert returnedIds == requestedIds,
        format("expected outcomes for {0} operations, got {1}", requestedIds, returnedIds);
      goto Complete;
    }
  }

  state Complete {}
}

// --- Correct co-commit and deliberately broken services ---

// One service, a small bounded set of operations, and alternative storage designs.
// Only adjacent local updates before a communication scheduling point model
// one storage primitive. A handler can yield at send/receive/create; this does
// not make the whole handler atomic. The real primitive must co-commit both
// durable updates in the fixed variant.
event eInvoke: (client: machine, id: int, dropReply: bool);
event eCrash;
event eRestart;
event eRemember: (client: machine, id: int);
event eCommitted: int;

machine DurableService {
  var durableTotal: int;
  var durableOutcomes: map[int, int];

  start state Serving {
    on eInvoke do (call: (client: machine, id: int, dropReply: bool)) {
      if (call.id in durableOutcomes) {

        send call.client, eReply,
          (id = call.id, result = durableOutcomes[call.id], currentTotal = durableTotal);
      } else {
        // This non-yielding update region is the modeled durable primitive.
        durableTotal = durableTotal + 1;
        durableOutcomes[call.id] = durableTotal;
        announce eCommit, (id = call.id, total = durableTotal);
        if (call.dropReply) {
          send call.client, eCommitted, call.id;
        } else {

          send call.client, eReply,
            (id = call.id, result = durableOutcomes[call.id], currentTotal = durableTotal);
        }
      }
    }

    on eCrash do { goto Stopped; }
  }

  state Stopped {
    on eRestart do { goto Serving; }
  }
}

// Deliberately broken: the effect survives in durableTotal, while the
// outcome memo lives in volatileMemo and is written in a later transition.
machine SplitCommitService {
  var durableTotal: int;
  var volatileMemo: map[int, int];

  start state Serving {
    on eInvoke do (call: (client: machine, id: int, dropReply: bool)) {
      if (call.id in volatileMemo) {

        send call.client, eReply,
          (id = call.id, result = volatileMemo[call.id], currentTotal = durableTotal);
      } else {
        durableTotal = durableTotal + 1;
        announce eCommit, (id = call.id, total = durableTotal);
        send call.client, eCommitted, call.id;
        goto MemoPending;
      }
    }

    on eCrash do {
      volatileMemo = default(map[int, int]);
      goto Stopped;
    }
  }

  state MemoPending {
    on eRemember do (call: (client: machine, id: int)) {
      volatileMemo[call.id] = durableTotal;

      send call.client, eReply,
        (id = call.id, result = durableTotal, currentTotal = durableTotal);
      goto Serving;
    }
    on eCrash do {
      volatileMemo = default(map[int, int]);
      goto Stopped;
    }
  }

  state Stopped {
    on eRestart do { goto Serving; }
  }
}

// Deliberately broken replay: it returns the saved result but applies that
// request's effect again after another ID has changed the current total.
machine ReapplyReplayService {
  var durableTotal: int;
  var durableOutcomes: map[int, int];

  start state Serving {
    on eInvoke do (call: (client: machine, id: int, dropReply: bool)) {
      if (call.id in durableOutcomes) {
        durableTotal = durableTotal + 1;
        announce eCommit, (id = call.id, total = durableTotal);
        if (call.dropReply) {
          send call.client, eCommitted, call.id;
        } else {

          send call.client, eReply,
            (id = call.id, result = durableOutcomes[call.id], currentTotal = durableTotal);
        }
      } else {
        durableTotal = durableTotal + 1;
        durableOutcomes[call.id] = durableTotal;
        announce eCommit, (id = call.id, total = durableTotal);
        if (call.dropReply) {
          send call.client, eCommitted, call.id;
        } else {

          send call.client, eReply,
            (id = call.id, result = durableOutcomes[call.id], currentTotal = durableTotal);
        }
      }
    }

    on eCrash do { goto Stopped; }
  }

  state Stopped {
    on eRestart do { goto Serving; }
  }
}

// --- Lost reply, delayed replay, concurrent client drivers ---

// Drivers wait for actual replies before declaring completion. RetryContract
// observes their real send payloads, before reordered client receipt.
// The original crash case drops the response after request 7 commits, then
// restarts and retries ID 7.
machine RetryDriver {
  var service: DurableService;

  start state Init {
    entry {
      service = new DurableService();
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = true);
    }
    on eCommitted do (id: int) {
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
      goto WaitingForReply;
    }
  }

  state WaitingForReply {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eScenarioDone;
      goto Done;
    }
  }

  state Done {}
}

machine SplitRetryDriver {
  var service: SplitCommitService;

  start state Init {
    entry {
      service = new SplitCommitService();
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = true);
    }
    on eCommitted do (id: int) {
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
      goto WaitingForRetryCommit;
    }
  }

  state WaitingForRetryCommit {
    on eCommitted do (id: int) {
      send service, eRemember, (client = this, id = id);
      goto WaitingForReply;
    }
  }

  state WaitingForReply {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eScenarioDone;
      goto Done;
    }
  }

  state Done {}
}

// This history completes ID 7, commits ID 8 without its reply, retries ID 8
// after restart, and then retries old ID 7. Its original result must stay 1
// while the current total stays 2.
machine CompletedReplayDriver {
  var service: DurableService;

  start state Init {
    entry {
      service = new DurableService();
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
    }
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eRequest, 8;
      send service, eInvoke, (client = this, id = 8, dropReply = true);
      goto WaitingForSecondCommit;
    }
  }

  state WaitingForSecondCommit {
    on eCommitted do (id: int) {
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 8;
      send service, eInvoke, (client = this, id = 8, dropReply = false);
      goto WaitingForSecondReply;
    }
  }

  state WaitingForSecondReply {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 8, "reply ID does not match the pending request";
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
      goto WaitingForOldReplay;
    }
  }

  state WaitingForOldReplay {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eScenarioDone;
      goto Done;
    }
  }

  state Done {}
}

machine ReappliedReplayDriver {
  var service: ReapplyReplayService;

  start state Init {
    entry {
      service = new ReapplyReplayService();
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
    }
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eRequest, 8;
      send service, eInvoke, (client = this, id = 8, dropReply = true);
      goto WaitingForSecondCommit;
    }
  }

  state WaitingForSecondCommit {
    on eCommitted do (id: int) {
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 8;
      send service, eInvoke, (client = this, id = 8, dropReply = false);
      goto WaitingForSecondReply;
    }
  }

  state WaitingForSecondReply {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 8, "reply ID does not match the pending request";
      send service, eCrash;
      send service, eRestart;
      announce eRequest, 7;
      send service, eInvoke, (client = this, id = 7, dropReply = false);
      goto WaitingForOldReplay;
    }
  }

  state WaitingForOldReplay {
    on eReply do (reply: (id: int, result: int, currentTotal: int)) {
      assert reply.id == 7, "reply ID does not match the pending request";
      announce eScenarioDone;
      goto Done;
    }
  }

  state Done {}
}

// Each caller has a stable logical ID; its retry never becomes a new request.
// Independent callers permit both requests to be pending before either commits.
event eCallerFinished;
machine ConcurrentCaller {
  start state Run {
    entry (p: (service: DurableService, coordinator: machine, id: int)) {
      announce eRequest, p.id;
      send p.service, eInvoke, (client = this, id = p.id, dropReply = false);
      receive { case eReply: (first: (id: int, result: int, currentTotal: int)) {
        assert first.id == p.id, "reply ID does not match the pending request";
      } }
      announce eRequest, p.id;
      send p.service, eInvoke, (client = this, id = p.id, dropReply = false);
      receive { case eReply: (retry: (id: int, result: int, currentTotal: int)) {
        assert retry.id == p.id, "retry ID does not match the pending request";
      } }
      send p.coordinator, eCallerFinished;
    }
  }
}
machine ConcurrentRetryDriver {
  var completed: int;
  start state Run {
    entry {
      var service: DurableService;
      service = new DurableService();
      new ConcurrentCaller((service = service, coordinator = this, id = 7));
      new ConcurrentCaller((service = service, coordinator = this, id = 8));
    }
    on eCallerFinished do {
      completed = completed + 1;
      if (completed == 2) { announce eScenarioDone; goto Done; }
    }
  }
  state Done { }
}

// --- Selected positive and negative tests ---

test tcDurableRetry [main=RetryDriver]:
  assert RetryContract in { RetryDriver, DurableService };

test tcSplitCommitRetry [main=SplitRetryDriver]:
  assert RetryContract in { SplitRetryDriver, SplitCommitService };

test tcCompletedReplay [main=CompletedReplayDriver]:
  assert RetryContract in { CompletedReplayDriver, DurableService };

test tcReappliedReplay [main=ReappliedReplayDriver]:
  assert RetryContract in { ReappliedReplayDriver, ReapplyReplayService };

// Selected properties apply to every generated scheduler interleaving.
test tcConcurrentRetries [main=ConcurrentRetryDriver]:
  assert RetryContract in { ConcurrentRetryDriver, ConcurrentCaller, DurableService };
