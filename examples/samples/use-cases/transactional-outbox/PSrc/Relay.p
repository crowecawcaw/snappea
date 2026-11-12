// The outbox relay. It fetches unsent rows, publishes each to the broker, waits for the
// broker's ack, then marks the row sent. It keeps nothing durable: after a crash a new
// relay starts from the outbox table, so a row published but not yet marked is
// published again. Two relays may run at once (a rolling deploy, or a restart while
// the old process is still alive); they publish some rows twice. Duplicates are
// expected, and the consumer removes them.

event ePublish: (relay: machine, id: int);
event eAck: int;
// To the supervisor (the test driver): this relay process died.
event eRelayCrashed;

machine Relay {
  var db: OrderDb;
  var broker: machine;
  var supervisor: machine;
  var crashAfterPublish: bool;

  start state Polling {
    entry (input: (db: OrderDb, broker: machine, supervisor: machine, crashAfterPublish: bool)) {
      db = input.db;
      broker = input.broker;
      supervisor = input.supervisor;
      crashAfterPublish = input.crashAfterPublish;
      goto Fetching;
    }
  }

  state Fetching {
    entry { send db, eFetch, this; }
    on eFetchResp do (rows: seq[int]) {
      var id: int;
      if (sizeof(rows) == 0) { goto Sleeping; }
      foreach (id in rows) {
        send broker, ePublish, (relay = this, id = id);
        receive { case eAck: (acked: int) { } }
        if (crashAfterPublish) {
          // Fault injection for the test: the process dies here, published but not
          // marked, so the row is still unsent in the table.
          send supervisor, eRelayCrashed;
          // snappea-lint: allow mutation -- without it the "dead" relay keeps running beside its replacement: the two-relay case, which the contract allows
          goto Crashed;
        }
        send db, eMarkSent, id;
      }
      goto Fetching;
    }
  }

  state Sleeping {
    on eWake goto Fetching;
  }

  state Crashed {
    ignore eWake;
  }
}
