// FLAWED: the service commits the change to its database, then publishes an event to
// the broker, then answers the client:
//
//     db.commit(change);          // durable
//     broker.publish(change);     // the process can die before this line
//     return ok;
//
// A crash after the commit and before the publish leaves a committed change that no
// event will ever announce. Nothing durable records that a publish is owed, so the
// restarted process cannot know.
//
// Real atomic primitives: the database commit (one transaction), the broker accepting a
// message. The service's in-memory request state is lost on a crash.

event Commit: (from: machine, epoch: int, change: Change);
event CommitResult: int;

machine OrdersDb {

  start state Serving {
    on Commit do (r: (from: machine, epoch: int, change: Change)) {
      announce Committed, r.change;
      send r.from, CommitResult, r.epoch;
    }
  }
}

machine DualWriteService {
  var db: machine;
  var broker: machine;
  // Counts restarts. Replies carry the epoch of the request, so a reply addressed to the
  // process before a crash is recognised and dropped, as a new connection would.
  var epoch: int;
  var client: machine;
  var change: Change;

  start state Init {
    entry (p: (db: machine, broker: machine)) {
      db = p.db;
      broker = p.broker;
      goto Ready;
    }
  }

  state Ready {
    on Update goto Committing with (r: (client: machine, change: Change)) {
      client = r.client;
      change = r.change;
      send db, Commit, (from = this, epoch = epoch, change = change);
    }
    // A crash between requests loses nothing.
    // Replies addressed to the process before a crash.
    ignore Crash, CommitResult, PublishAck;
  }

  state Committing {
    defer Update;
    on CommitResult do (e: int) {
      if (e == epoch) {
        // BUG: the commit is durable, the publish below is not yet.
        send broker, Publish, (from = this, epoch = epoch, change = change);
        goto Publishing;
      }
    }
    on Crash do { Crash(); }
  }

  state Publishing {
    defer Update;
    // One publish is in flight per epoch; the epoch identifies its response.
    // The broker also returns id for the outbox relay, which can publish many rows.
    on PublishAck do (a: (epoch: int, id: int)) {
      if (a.epoch == epoch) {
        send client, UpdateResp, change.id;
        goto Ready;
      }
    }
    on Crash do { Crash(); }
  }

  fun Crash() {
    send client, Reset, change.id;
    epoch = epoch + 1;
    goto Ready;
  }
}
