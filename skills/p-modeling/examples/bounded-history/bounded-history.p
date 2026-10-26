type Request = (requester: machine, id: string, replenish: bool, units: int);
type Decision = (id: string, replenish: bool, units: int, accepted: bool);

event eTry: Request;
event eResult: Decision;
event eDecision: Decision;

// Oracle state starts from the contract's initial stock, independently of Store.
spec CapacityOracle observes eDecision {
  var remaining: int;
  start state Watching {
    entry { remaining = 2; }
    on eDecision do (d: Decision) {
      assert d.units > 0, "request units must be positive";
      if (d.replenish) {
        assert d.accepted, format("replenishment {0} was rejected", d.id);
        remaining = remaining + d.units;
      } else {
        assert d.accepted == (d.units <= remaining),
          format("purchase {0} of {1} units had accepted={2} with {3} remaining",
            d.id, d.units, d.accepted, remaining);
        if (d.accepted) { remaining = remaining - d.units; }
      }
    }
  }
}

machine Store {
  var remaining: int;
  var broken: bool;

  start state Serving {
    entry (input: (broken: bool)) {
      remaining = 2;
      broken = input.broken;
    }
    on eTry do (r: Request) {
      var accepted: bool;
      accepted = false;
      if (r.replenish) {
        accepted = true;
        remaining = remaining + r.units;
      } else {
        if (r.units <= remaining || broken) {
          accepted = true;
          remaining = remaining - r.units;
        }
      }
      announce eDecision, (id = r.id, replenish = r.replenish,
        units = r.units, accepted = accepted);
      send r.requester, eResult, (id = r.id, replenish = r.replenish,
        units = r.units, accepted = accepted);
    }
  }
}

machine Buyer {
  start state Start {
    entry (input: (store: Store, id: string, units: int)) {
      send input.store, eTry,
        (requester = this, id = input.id, replenish = false, units = input.units);
      receive { case eResult: (result: Decision) { } }
    }
  }
}

machine FixedHistory {
  start state Start {
    entry (input: (store: Store)) {
      send input.store, eTry, (requester = this, id = "a", replenish = false, units = 1);
      receive { case eResult: (first: Decision) { } }
      send input.store, eTry, (requester = this, id = "b", replenish = false, units = 1);
      receive { case eResult: (second: Decision) { } }
      send input.store, eTry, (requester = this, id = "c", replenish = false, units = 1);
      receive { case eResult: (third: Decision) { } }
    }
  }
}

// Generate exactly three sequential actions. Inputs vary; their order is the
// generated history, while the checker still explores enabled scheduler steps.
machine GeneratedHistory {
  start state Start {
    entry (input: (store: Store)) {
      var step: int;
      var replenish: bool;
      var units: int;
      var id: string;
      while (step < 3) {
        replenish = $;
        units = choose(2) + 1;
        id = format("step-{0}", step);
        send input.store, eTry,
          (requester = this, id = id, replenish = replenish, units = units);
        receive { case eResult: (result: Decision) { } }
        step = step + 1;
      }
    }
  }
}

machine ScheduledMain {
  start state Init {
    entry {
      var store: Store;
      store = new Store((broken = false,));
      new Buyer((store = store, id = "a", units = 1));
      new Buyer((store = store, id = "b", units = 1));
      new Buyer((store = store, id = "c", units = 1));
    }
  }
}

machine FixedMain {
  start state Init {
    entry {
      var store: Store;
      store = new Store((broken = false,));
      new FixedHistory((store = store,));
    }
  }
}

machine BrokenMain {
  start state Init {
    entry {
      var store: Store;
      store = new Store((broken = true,));
      new FixedHistory((store = store,));
    }
  }
}

machine GeneratedMain {
  start state Init {
    entry {
      var store: Store;
      store = new Store((broken = false,));
      new GeneratedHistory((store = store,));
    }
  }
}

machine BrokenGeneratedMain {
  start state Init {
    entry {
      var store: Store;
      store = new Store((broken = true,));
      new GeneratedHistory((store = store,));
    }
  }
}

test tcSchedulerChoosesInterleaving [main=ScheduledMain]:
  assert CapacityOracle in { ScheduledMain, Store, Buyer };

test tcFixedInputHistory [main=FixedMain]:
  assert CapacityOracle in { FixedMain, Store, FixedHistory };

test tcBrokenInputHistory [main=BrokenMain]:
  assert CapacityOracle in { BrokenMain, Store, FixedHistory };

test tcGeneratedHistory [main=GeneratedMain]:
  assert CapacityOracle in { GeneratedMain, Store, GeneratedHistory };

test tcBrokenGeneratedHistory [main=BrokenGeneratedMain]:
  assert CapacityOracle in { BrokenGeneratedMain, Store, GeneratedHistory };
