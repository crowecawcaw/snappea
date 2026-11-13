# P syntax and commands

## Small syntax reference

Two fresh additions, one sequential client; no retries or failures. This checks actual results and completion, not just input validity.

```p
event eAdd: (client: Client, id: int, units: int);
event eReply: (id: int, total: int);
event eRequested: (id: int, units: int);
event eFinished;

// Contract: each fresh request adds its units; replies expose the stored total.
// Expectations come from client inputs, not Service's chosen result.
spec Contract observes eRequested, eReply, eFinished {
  var requests: map[int, int];
  var replied: set[int];
  var expected: int;
  start hot state Waiting {
    on eRequested do (p: (id: int, units: int)) {
      assert !(p.id in requests), "this example uses fresh request IDs";
      requests[p.id] = p.units;
    }
    on eReply do (p: (id: int, total: int)) {
      assert p.id in requests;
      assert !(p.id in replied);
      expected = expected + requests[p.id];
      assert p.total == expected, "reply disagrees with requested additions";
      replied += (p.id);
    }
    on eFinished do {
      assert sizeof(replied) == 2, "driver must complete both requests";
      goto Complete;
    }
  }
  state Complete { }
}

machine Service {
  var total: int;
  start state Serving {
    on eAdd do (p: (client: Client, id: int, units: int)) {
      // This add is the assumed atomic primitive. A load/CAS algorithm needs
      // separate load and cas messages; do not make its whole API atomic.
      total = total + p.units;
      send p.client, eReply, (id = p.id, total = total);
    }
  }
}

machine Client {
  start state Run {
    entry (s: Service) {
      var step: int;               // all block locals precede statements
      var units: int;
      while (step < 2) {
        units = choose(2) + 1;     // generated inputs: 1 or 2, fresh IDs 0/1
        announce eRequested, (id = step, units = units);
        send s, eAdd, (client = this, id = step, units = units);
        receive { case eReply: (reply: (id: int, total: int)) {
          assert reply.id == step;
        } }
        step = step + 1;
      }
      announce eFinished;          // hot monitor catches an unfinished driver
    }
  }
}

machine Main {
  start state Init {
    entry {
      var s: Service;
      s = new Service();
      new Client(s);
    }
  }
}

// Select the spec explicitly. The set contains machines, never the spec itself.
test tcRequest [main=Main]: assert Contract in { Main, Service, Client };
```

- `int` is a type; `1` is an integer expression. A singleton tuple type is `(id: int)`; its expression uses a trailing comma, `(id = 1,)`. `choose(n)` explores integers from `0` through `n - 1`; `$` is a nondeterministic Boolean.
- Named composition: `module Model = { Main, Service, Client };` then `test tcRequest [main=Main]: assert Contract in Model;`. A machine name is not a module; specs go after `assert`, not in the machine set.
- Sequences: `s += (sizeof(s), value);` appends; `s -= (index);` removes. Sets: `items += (value);`. These are distinct operations.
- Maps use `map[keyType, valueType]`, membership (`k in m`), indexing (`m[k]`), and `keys(m)`. `foreach (item in collection)` iterates a set, sequence, or `keys(map)`; declare `item` as a local before the loop. Avoid using a machine field as the iterator.
- Put handler/function locals at the start of that block, before executable statements. P keywords are reserved; using one as a variable, field, function or type name is a parse error, so rename it (`vals`, not `values`). Reserved words that look like names: `values`, `keys`, `to`, `as`, `in`, `default`, `format`, `choose`, `sizeof`, `data`, `any`, `string`, `map`, `set`, `seq`, `start`, `state`, `event`, `type`, `test`, `main`, `module`, `param`, `entry`, `exit`, `this`, `new`, `send`, `receive`, `print`, `receives`, `sends`, `creates`, `safe`, `union`, `rename`, `compose`; and the PVerifier words `is`, `sent`, `targets`, `inflight`, `pure`, `assume`, `requires`, `ensures`, `invariant`, `using`, `except`, `forall`, `exists`, `prove`, `axiom`.
- `goto S, v;` enters state `S` and passes `v` to its `entry (x: T)`. A state whose entry takes a parameter needs one on every `goto`. To re-enter it from places that have no value, keep the value in a machine variable instead: set it once in a separate start state (`start state Init { entry (cfg: Config) { config = cfg; goto Idle; } }`) and give `Idle` a plain `entry { ... }`.
- P has no `sender()`. Put the sender in the payload (`event eReq: (client: machine, id: int);`, `send s, eReq, (client = this, id = 1);`) and reply to `p.client`.
- A state must handle every event that reaches it: `on E do`, `on E goto S`, `defer E;` (keep it for a later state) or `ignore E;` (drop it). A `receive` takes only its own cases, and only while it waits; other events stay queued for the state.
- `send target, event, payload` delivers to a machine; `receive { case event: (p: Type) { ... } }` waits; `announce event, payload` records a monitor-visible event. `send`, `receive`, and `new` are scheduling points; assignments do not yield.

## Messages, transitions and bindings

- `$$` is a fair nondeterministic Boolean: under fair exploration, a choice made
  repeatedly cannot keep selecting only one outcome forever. `$` has no such
  fairness promise. Use explicit finite bounds when a test needs bounded losses;
  see `snappea guide faults`.
- A module can bind an interface to an implementation with `Machine -> Interface`
  inside its machine set. Creation through that interface uses the bound machine;
  this lets tests replace an implementation while keeping its clients.
- An `event` variable can supply the event in `send target, e, payload;`. Its
  runtime payload must match the selected event's declared type.
- Specs observe `send` as well as `announce`. Observe the real send when available;
  use `announce` for facts nobody receives. Announcing and sending the same event
  makes the spec observe it twice.
- `receive` selects its listed cases. An unlisted event can remain queued when the
  current state has a handler for it; an event the state cannot handle fails the
  run. Do not assume a receive silently accepts every unrelated event.
- In a receive loop, include an operation ID in replies and check it against the
  current request. A late reply from a previous iteration is not a fresh answer.
- `goto` inside a called function ends the current handler too: statements after
  that call do not run. `raise E, payload;` also ends the current action and handles
  the event in the current machine. Prefer `raise` when the current state's event
  handler should choose the next action; use `goto S, payload;` when the destination
  state is known and its entry action should consume the payload.

For independent specs, completion patterns and timeout tests, see
`snappea guide spec-patterns`.

## Checking

```sh
snappea check --timeout 5s                # check, coverage, then mutations
snappea check 'tcRetry*' --timeout 5s
snappea check --no-mutation --timeout 5s # quick model iteration
snappea check mutation tcRetry --mutation-timeout 5s
```

- Check the top-level `summary` first: assertion failures, missing required coverage, and incomplete search are separate outcomes. Mutation limits alone do not fail the base check.
- Completion means the search finished within its budget; a timeout is incomplete evidence. Exhaustive completion covers only finite modeled bounds.
- Coverage shows which statements, branches, and handlers ran; it does not show that assertions enforce the contract. Mutation checking perturbs executed model code and reports changes surviving the test cases. Inspect survivors and add a discriminating scenario or independent assertion.
