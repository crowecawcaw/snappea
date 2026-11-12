// FIXED: a transactional outbox. The service commits the change and an outbox row in the
// same database transaction, then answers the client; it never talks to the broker. A
// relay reads unpublished outbox rows, publishes each, and marks it published once the
// broker accepted it. Every step the relay takes is driven by durable state, so a
// restarted relay simply reads the outbox again.
//
// The relay can crash after the broker accepted a row and before marking it: the row is
// published again after the restart. That is why the consumer skips changes it already
// applied (by change id); the outbox gives at-least-once delivery, not exactly-once.
//
// Real atomic primitives: commit change + outbox row (one transaction), read the
// unpublished rows, mark one row published, the broker accepting a message.

event eCommitWithOutbox: (from: machine, epoch: int, change: tChange);
event eSubscribe: machine;
event eOutboxChanged;
event eReadOutbox: (from: machine, epoch: int);
event eOutboxRows: (epoch: int, rows: seq[tChange]);
event eMarkPublished: int;

machine OutboxDb {
  // Unpublished outbox rows: change id -> value.
  var outbox: map[int, int];
  var relay: machine;

  start state Serving {
    on eSubscribe do (r: machine) { relay = r; }
    on eCommitWithOutbox do (r: (from: machine, epoch: int, change: tChange)) {
      outbox[r.change.id] = r.change.value;
      announce eCommitted, r.change;
      send r.from, eCommitResult, r.epoch;
      // Like LISTEN/NOTIFY or a change feed: wakes the relay instead of polling. Before
      // the relay subscribes there is no one to wake; its first read finds this row.
      if (relay != null) { send relay, eOutboxChanged; }
    }
    on eReadOutbox do (r: (from: machine, epoch: int)) {
      var rows: seq[tChange];
      var id: int;
      foreach (id in keys(outbox)) {
        rows += (sizeof(rows), (id = id, value = outbox[id]));
      }
      send r.from, eOutboxRows, (epoch = r.epoch, rows = rows);
    }
    on eMarkPublished do (id: int) { outbox -= (id); }
  }
}

machine OutboxService {
  var db: machine;
  var epoch: int;
  var client: machine;
  var change: tChange;

  start state Init {
    entry (d: machine) {
      db = d;
      goto Ready;
    }
  }

  state Ready {
    on eUpdate goto Committing with (r: (client: machine, change: tChange)) {
      client = r.client;
      change = r.change;
      send db, eCommitWithOutbox, (from = this, epoch = epoch, change = change);
    }
    on eCrash do { epoch = epoch + 1; }
    ignore eCommitResult;
  }

  state Committing {
    defer eUpdate;
    on eCommitResult do (e: int) {
      if (e == epoch) {
        // The outbox row committed with the change; the relay will publish it.
        send client, eUpdateResp, change.id;
        goto Ready;
      }
    }
    on eCrash do {
      send client, eReset, change.id;
      epoch = epoch + 1;
      goto Ready;
    }
  }
}

machine OutboxRelay {
  var db: machine;
  var broker: machine;
  var epoch: int;
  var pending: seq[tChange];

  start state Init {
    entry (p: (db: machine, broker: machine)) {
      db = p.db;
      broker = p.broker;
      send db, eSubscribe, this;
      ReadOutbox();
    }
  }

  state Reading {
    // A row committed while this read is in flight is handled by the next read.
    defer eOutboxChanged;
    on eOutboxRows do (r: (epoch: int, rows: seq[tChange])) {
      if (r.epoch == epoch) {
        pending = r.rows;
        PublishNext();
      }
    }
    on eCrash do { Restart(); }
    // An acknowledgement addressed to the process before a crash.
    ignore ePublishAck;
  }

  state Publishing {
    defer eOutboxChanged;
    on ePublishAck do (a: (epoch: int, id: int)) {
      if (a.epoch == epoch) {
        // The process can die here: published, not yet marked.
        send db, eMarkPublished, a.id;
        pending -= (0);
        PublishNext();
      }
    }
    on eCrash do { Restart(); }
  }

  state Idle {
    on eOutboxChanged do { ReadOutbox(); }
    on eCrash do { Restart(); }
    ignore eOutboxRows, ePublishAck;
  }

  fun ReadOutbox() {
    send db, eReadOutbox, (from = this, epoch = epoch);
    goto Reading;
  }

  fun PublishNext() {
    if (sizeof(pending) == 0) {
      goto Idle;
    } else {
      send broker, ePublish, (from = this, epoch = epoch, change = pending[0]);
      goto Publishing;
    }
  }

  // A new process: nothing from memory survives; read the outbox again.
  fun Restart() {
    epoch = epoch + 1;
    ReadOutbox();
  }
}
