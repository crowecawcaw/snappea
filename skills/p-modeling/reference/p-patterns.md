# P syntax and commands

## Small syntax reference

```p
event eRequestToService: (client: Client, id: int);
event eReply: (ok: bool);
event eAudit: (id: int);

spec Contract observes eAudit {
  start state Ready {
    on eAudit do (p: (id: int)) { assert p.id >= 0; }
  }
}

machine Service {
  var counts: map[string, int];

  start state Serving {
    on eRequestToService do (p: (client: Client, id: int)) {
      var key: string;             // block locals go before statements
      key = format("{0}", p.id);
      counts[key] = choose(3);     // nondeterministic integer: 0, 1, or 2
      announce eAudit, (id = p.id,);
      send p.client, eReply, (ok = counts[key] > 0,);
    }
  }
}

machine Client {
  start state Run {
    entry (s: Service) {
      send s, eRequestToService, (client = this, id = choose(2)); // send can interleave
      receive { case eReply: (reply: (ok: bool)) { } }
    }
  }
}

machine Main {
  start state Init {
    entry {
      var m: map[string, int];
      var key: string;             // foreach iterator is a declared variable
      var s: Service;
      m = default(map[string, int]);
      m["example"] = 1;
      foreach (key in keys(m)) { print key; }
      s = new Service();
      new Client(s);
    }
  }
}

test tcRequest [main=Main]: assert Contract in { Main, Service, Client };
```

- `int` is a type; `1` is an integer expression. A singleton tuple type is `(id: int)`; its expression uses a trailing comma, `(id = 1,)`. `choose(n)` explores integers from `0` through `n - 1`; `$` is a nondeterministic Boolean.
- Maps use `map[keyType, valueType]`, membership (`k in m`), indexing (`m[k]`), and `keys(m)`. `foreach (item in collection)` iterates a set, sequence, or `keys(map)`; declare `item` as a local before the loop. Avoid using a machine field as the iterator.
- Put handler/function locals at the start of that block, before executable statements. P keywords such as `start`, `state`, and `event` are reserved; using one as a name causes a parse error, so rename it.
- `send target, event, payload` delivers to a machine; `receive { case event: (p: Type) { ... } }` waits; `announce event, payload` records a monitor-visible event. `send`, `receive`, and `new` are scheduling points; assignments do not yield.

## Checking

```sh
snappea check events.p monitor.p model.p tests.p --timeout 5s  # check, coverage, then mutations
snappea check 'tcRetry*' --timeout 5s
snappea check --no-mutation --timeout 5s # quick model iteration
snappea check mutation tcRetry --mutation-timeout 5s
```

- Check the top-level `summary` first: assertion failures, missing required coverage, and incomplete search are separate outcomes. Mutation limits alone do not fail the base check.
- Completion means the search finished within its budget; a timeout is incomplete evidence. Exhaustive completion covers only finite modeled bounds.
- Coverage shows which statements, branches, and handlers ran; it does not show that assertions enforce the contract. Mutation checking perturbs executed model code and reports changes surviving the test cases. Inspect survivors and add a discriminating scenario or independent assertion.
- For implementation histories, see [capture and observation](../guides/implementation.md). For a compact input-history generator, see [bounded history](../examples/bounded-history/README.md).
- Related examples: [reservation](../examples/reservation/README.md), [durable retry](../examples/durable-retry/README.md), and [versioned publication](../examples/versioned-publication/README.md).
