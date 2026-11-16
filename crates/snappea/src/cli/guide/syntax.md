---
diataxis: reference
personas: [application-developer, coding-agent, p-user]
questions:
  - What does a complete small P model look like?
  - How do messages, transitions, receive and payload bindings behave?
  - Which syntax mistakes are common?
---

# P syntax

A complete model: one client makes two fresh additions; a spec checks each reply and
that both complete. Check it with `snappea check Example.p`.

```p
event eAdd: (client: Client, id: int, units: int);
event eReply: (id: int, total: int);
event eRequested: (id: int, units: int);
event eFinished;

// Expectations come from client inputs, not from Service's result.
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
      assert p.id in requests && !(p.id in replied);
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
      total = total + p.units;     // the assumed atomic primitive
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
        units = choose(2) + 1;     // generated input: 1 or 2
        announce eRequested, (id = step, units = units);
        send s, eAdd, (client = this, id = step, units = units);
        receive { case eReply: (reply: (id: int, total: int)) { assert reply.id == step; } }
        step = step + 1;
      }
      announce eFinished;          // the hot spec catches an unfinished driver
    }
  }
}

machine Main {
  start state Init { entry { new Client(new Service()); } }
}

// Select the spec explicitly. The set contains machines, never the spec itself.
test tcRequest [main=Main]: assert Contract in { Main, Service, Client };
```

## Rules that trip people up

- Locals go first in their block. Reserved words cannot be names (`vals`, not
  `values`): `values`, `keys`, `to`, `as`, `in`, `default`, `format`, `choose`,
  `sizeof`, `data`, `any`, `string`, `map`, `set`, `seq`, `start`, `state`, `event`,
  `type`, `test`, `main`, `module`, `param`, `entry`, `exit`, `this`, `new`, `send`,
  `receive`, `print`, `receives`, `sends`, `creates`, `safe`, `union`, `rename`,
  `compose`, `is`, `sent`, `targets`, `inflight`, `pure`, `assume`, `requires`,
  `ensures`, `invariant`, `using`, `except`, `forall`, `exists`, `prove`, `axiom`.
- Singleton tuple value: `(id = 1,)`. `choose(n)` yields `0` to `n - 1`; `$` is a
  Boolean. Both are fair for liveness, so bound faults (`snappea guide faults`).
- `s += (sizeof(s), v);` appends to a seq, `s -= (i);` removes; sets `s += (v);`;
  maps `k in m`, `m[k]`, `keys(m)`. Declare a `foreach` variable as a local.
- `module M = { Main, Service };` then `test t [main=Main]: assert Contract in M;`.
- No `sender()`: put `client = this` in the payload and reply to `p.client`.
- A state must handle (`on`, `defer`, `ignore`) every event that reaches it. A
  `receive` takes only its cases; others stay queued for the state after it.
- `goto S, v;` passes `v` to `entry (x: T)`; then every `goto S` needs a value.
- `goto` in a called function also ends the caller's handler. `raise E, v;` ends the
  action and handles `E` in this machine.
- Specs observe `send` and `announce`; doing both makes the spec see an event twice.
- Scheduling points: `snappea guide modeling`. Specs: `snappea guide spec-patterns`.
