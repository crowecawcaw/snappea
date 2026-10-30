/*
 * File map (search these names):
 * CapacityOracle = oracle; Store = atomic primitive; HistoryClient = protocol;
 * *Main = fixed/generated and sequential/concurrent workloads;
 * test tc* = positive/negative selections; adjacent JSONL = oracle-only probes.
 *
 * Bounded histories: inputs, interleavings, independent state and completion.
 *
 * Contract: stock starts at 2; positive purchase succeeds iff affordable; replenish
 * adds units; rejection changes nothing. IDs are fresh correlation IDs, NOT retries.
 * CapacityOracle records driver inputs; derives decisions/stock independently;
 * matches actual replies by ID and actual public snapshots. It never reads Store
 * fields. The negative state control returns false correctly but changes stock.
 *
 * Store check/update is an ACTUAL atomic primitive, not split load/CAS. Observations
 * at Store decisions/reads precede reply delivery; reply receipt is not commit
 * order. Other clients may commit before inspection: compare against all intervening
 * decisions, not the stock immediately after the original operation. Sequential
 * immediate inspections catch rejection damage before later work masks it.
 *
 * Drivers share HistoryClient: fixed three purchases; three competing fixed inputs;
 * three generated sequential purchase/replenish choices of 1/2; or two concurrent
 * clients each choosing purchase/replenish of 2. Input choice and schedule choice
 * are different coverage dimensions. Bounds preserve depletion/exact-fit/excess/
 * replenishment while keeping complete exploration quick, not arbitrary histories.
 * Hot completion requires a declared nonzero count and one decision, checked reply,
 * inspection per ID plus every client done; dropping work cannot pass vacuously.
 *
 * Run positive tests TOGETHER to cover shared code (each narrow case can lack coverage):
 *   snappea check bounded-history.p tcFixedInputHistory tcSchedulerChoosesInterleaving tcGeneratedHistory tcConcurrentGeneratedHistory --no-mutation --timeout 5s
 * Negative controls (each must fail its intended property, not compile/coverage):
 *   snappea check bounded-history.p tcBrokenInputHistory --no-mutation
 *   snappea check bounded-history.p tcBrokenGeneratedHistory --no-mutation
 *   snappea check bounded-history.p tcRejectedWriteChangesState --no-mutation
 * First two catch excess acceptance; last catches "public stock 1, expected 0".
 *   snappea observe bounded-history.p valid-concurrent.jsonl --spec CapacityOracle --strict --check-hot
 * valid.jsonl also passes; invalid*.jsonl, incomplete.jsonl and zero-work.jsonl must
 * fail decision/state/reply/progress/nonzero checks. These hand-calculated traces
 * validate only the oracle; see test_storage.py in `snappea guide example reservation` for real capture.
 *
 * Limits: finite lengths above, positive units, fresh IDs, one Store, reliable
 * messages and live clients. No retries, persistence, crashes, external storage
 * failures or unbounded fairness. Schedule counts are not semantic coverage.
 */

// --- Contract declarations and independent oracle ---

type Request = (requester: machine, id: string, replenish: bool, units: int);
type Decision = (id: string, replenish: bool, units: int, accepted: bool);
type Inspection = (id: string, remaining: int);

event eBegin: (clients: int, commands: int);
event eRequested: (id: string, replenish: bool, units: int);
event eTry: Request;
event eResult: Decision;
event eDecision: Decision;
event eReturned: Decision;
event eInspect: (requester: machine, id: string);
event eSnapshot: Inspection;
event eSnapshotResult: Inspection;
event eClientFinished: string;

// Contract oracle, not a copy of Store's state. Inputs come from the driver;
// effects come from linearization-point observations, not reply receipt order.
spec CapacityOracle observes eBegin, eRequested, eDecision, eReturned, eSnapshot, eClientFinished {
  var remaining: int;
  var expectedClients: int;
  var expectedCommands: int;
  var requests: map[string, (replenish: bool, units: int)];
  var decisions: map[string, bool];
  var returned: set[string];
  var inspections: set[string];
  var finished: set[string];

  start hot state BeforeBegin {
    on eBegin do (b: (clients: int, commands: int)) {
      // A silent or zero-work driver must not pass by omitting all observations.
      assert b.clients > 0 && b.commands > 0, "scenario must perform real work";
      remaining = 2;
      expectedClients = b.clients;
      expectedCommands = b.commands;
      goto Working;
    }
  }

  hot state Working {
    on eRequested do (r: (id: string, replenish: bool, units: int)) {
      assert r.units > 0, "request units must be positive";
      // IDs correlate unique commands; this example does not promise idempotency.
      assert !(r.id in requests), "a command ID was reused";
      requests[r.id] = (replenish = r.replenish, units = r.units);
    }
    on eDecision do (d: Decision) {
      assert d.id in requests, "decision without a request";
      assert !(d.id in decisions), "one command produced two decisions";
      assert d.replenish == requests[d.id].replenish && d.units == requests[d.id].units,
        "decision changed the requested operation or quantity";
      if (d.replenish) {
        assert d.accepted, format("replenishment {0} was rejected", d.id);
        remaining = remaining + d.units;
      } else {
        // Check useful success as well as rejection. Always rejecting is wrong.
        assert d.accepted == (d.units <= remaining),
          format("purchase {0} of {1} units had accepted={2} with {3} remaining",
            d.id, d.units, d.accepted, remaining);
        if (d.accepted) { remaining = remaining - d.units; }
      }
      decisions[d.id] = d.accepted;
    }
    on eReturned do (d: Decision) {
      // Replies may arrive out of order; match their immutable decision by ID,
      // not against the stock at receipt time or the last reply seen globally.
      assert d.id in decisions, "reply without a committed decision";
      assert !(d.id in returned), "one command replied twice";
      assert d.replenish == requests[d.id].replenish && d.units == requests[d.id].units,
        "reply changed the requested operation or quantity";
      assert d.accepted == decisions[d.id], "reply disagrees with the committed decision";
      returned += (d.id);
    }
    on eSnapshot do (s: Inspection) {
      assert s.id in decisions, "inspection before the command's decision";
      assert !(s.id in inspections), "command inspected twice";
      // A correct return value can hide a bad write. Compare actual public state
      // with independently accumulated effects, before the next local command.
      assert s.remaining == remaining,
        format("public stock {0}, expected {1}", s.remaining, remaining);
      inspections += (s.id);
    }
    on eClientFinished do (id: string) {
      assert !(id in finished), "one client finished twice";
      finished += (id);
      if (sizeof(finished) == expectedClients) {
        assert sizeof(requests) == expectedCommands, "driver omitted commands";
        assert sizeof(decisions) == expectedCommands, "a command never produced a decision";
        assert sizeof(returned) == expectedCommands, "a command's reply was not checked";
        assert sizeof(inspections) == expectedCommands, "a command's state was not inspected";
        goto Complete;
      }
    }
  }

  state Complete { }
}

// --- Atomic storage primitive ---

machine Store {
  var remaining: int;
  var acceptTooMuch: bool;
  var rejectedWrite: int;

  start state Serving {
    entry (input: (acceptTooMuch: bool, rejectedWrite: int)) {
      remaining = 2;
      // Fault controls exist only to validate the oracle, not as design options.
      acceptTooMuch = input.acceptTooMuch;
      rejectedWrite = input.rejectedWrite;
    }
    on eTry do (r: Request) {
      var accepted: bool;
      accepted = false;
      // This whole check/update region is one *assumed storage primitive*.
      // Do not copy this abstraction if an implementation separates read/write;
      // use the reservation/publication examples for that interference instead.
      if (r.replenish) {
        accepted = true;
        remaining = remaining + r.units;
      } else {
        if (r.units <= remaining || acceptTooMuch) {
          accepted = true;
          remaining = remaining - r.units;
        } else {
          // Correct rejection leaves stock unchanged (0); the negative control
          // writes +1 but returns the right rejection, caught only by inspection.
          remaining = remaining + rejectedWrite;
        }
      }
      announce eDecision, (id = r.id, replenish = r.replenish,
        units = r.units, accepted = accepted);
      send r.requester, eResult, (id = r.id, replenish = r.replenish,
        units = r.units, accepted = accepted);
    }
    on eInspect do (r: (requester: machine, id: string)) {
      // Observed at the Store read, not when a delayed client receives its reply.
      // Other clients may commit before this read; the oracle sees those effects.
      announce eSnapshot, (id = r.id, remaining = remaining);
      send r.requester, eSnapshotResult, (id = r.id, remaining = remaining);
    }
  }
}

// The same protocol supports fixed and generated inputs. Every result is matched
// to its invocation; every command is followed by a real public read and reply.
// No driver reads Store fields or invents an expected successful result.
// --- Shared client protocol and bounded scenarios ---

machine HistoryClient {
  start state Start {
    entry (input: (store: Store, name: string, steps: int, generated: bool, chooseUnits: bool)) {
      var step: int;
      var replenish: bool;
      var units: int;
      var id: string;
      while (step < input.steps) {
        replenish = false;
        units = 1;
        if (input.generated) {
          replenish = $;
          units = 2;
          if (input.chooseUnits) { units = choose(2) + 1; }
        }
        id = format("{0}-{1}", input.name, step);
        announce eRequested, (id = id, replenish = replenish, units = units);
        send input.store, eTry,
          (requester = this, id = id, replenish = replenish, units = units);
        receive { case eResult: (result: Decision) {
          assert result.id == id && result.replenish == replenish && result.units == units,
            "reply did not match the invocation";
          announce eReturned, result;
        } }
        // Especially important after rejection: a later successful command must
        // not overwrite evidence before the rejected command's state is read.
        send input.store, eInspect, (requester = this, id = id);
        receive { case eSnapshotResult: (snapshot: Inspection) {
          assert snapshot.id == id, "inspection reply did not match the command";
        } }
        step = step + 1;
      }
      announce eClientFinished, input.name;
    }
  }
}

machine ScheduledMain {
  start state Init {
    entry {
      var store: Store;
      announce eBegin, (clients = 3, commands = 3);
      store = new Store((acceptTooMuch = false, rejectedWrite = 0));
      new HistoryClient((store = store, name = "a", steps = 1, generated = false, chooseUnits = false));
      new HistoryClient((store = store, name = "b", steps = 1, generated = false, chooseUnits = false));
      new HistoryClient((store = store, name = "c", steps = 1, generated = false, chooseUnits = false));
    }
  }
}

machine FixedMain {
  start state Init {
    entry {
      var store: Store;
      announce eBegin, (clients = 1, commands = 3);
      store = new Store((acceptTooMuch = false, rejectedWrite = 0));
      new HistoryClient((store = store, name = "fixed", steps = 3, generated = false, chooseUnits = false));
    }
  }
}

machine BrokenMain {
  start state Init {
    entry {
      var store: Store;
      announce eBegin, (clients = 1, commands = 3);
      store = new Store((acceptTooMuch = true, rejectedWrite = 0));
      new HistoryClient((store = store, name = "fixed", steps = 3, generated = false, chooseUnits = false));
    }
  }
}

machine GeneratedMain {
  start state Init {
    entry {
      var store: Store;
      announce eBegin, (clients = 1, commands = 3);
      store = new Store((acceptTooMuch = false, rejectedWrite = 0));
      new HistoryClient((store = store, name = "generated", steps = 3, generated = true, chooseUnits = true));
    }
  }
}

machine BrokenGeneratedMain {
  start state Init {
    entry {
      var store: Store;
      announce eBegin, (clients = 1, commands = 3);
      store = new Store((acceptTooMuch = true, rejectedWrite = 0));
      new HistoryClient((store = store, name = "generated", steps = 3, generated = true, chooseUnits = true));
    }
  }
}

machine ConcurrentGeneratedMain {
  start state Init {
    entry {
      var store: Store;
      announce eBegin, (clients = 2, commands = 2);
      store = new Store((acceptTooMuch = false, rejectedWrite = 0));
      new HistoryClient((store = store, name = "left", steps = 1, generated = true, chooseUnits = false));
      new HistoryClient((store = store, name = "right", steps = 1, generated = true, chooseUnits = false));
    }
  }
}

machine RejectedWriteMain {
  start state Init {
    entry {
      var store: Store;
      announce eBegin, (clients = 1, commands = 3);
      store = new Store((acceptTooMuch = false, rejectedWrite = 1));
      new HistoryClient((store = store, name = "fixed", steps = 3, generated = false, chooseUnits = false));
    }
  }
}

// --- Selected positive and negative tests ---

test tcSchedulerChoosesInterleaving [main=ScheduledMain]:
  assert CapacityOracle in { ScheduledMain, Store, HistoryClient };

test tcFixedInputHistory [main=FixedMain]:
  assert CapacityOracle in { FixedMain, Store, HistoryClient };

test tcBrokenInputHistory [main=BrokenMain]:
  assert CapacityOracle in { BrokenMain, Store, HistoryClient };

test tcGeneratedHistory [main=GeneratedMain]:
  assert CapacityOracle in { GeneratedMain, Store, HistoryClient };

test tcBrokenGeneratedHistory [main=BrokenGeneratedMain]:
  assert CapacityOracle in { BrokenGeneratedMain, Store, HistoryClient };

test tcConcurrentGeneratedHistory [main=ConcurrentGeneratedMain]:
  assert CapacityOracle in { ConcurrentGeneratedMain, Store, HistoryClient };

test tcRejectedWriteChangesState [main=RejectedWriteMain]:
  assert CapacityOracle in { RejectedWriteMain, Store, HistoryClient };
