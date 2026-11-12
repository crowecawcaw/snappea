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

event eCommit: (from: machine, epoch: int, change: tChange);
event eCommitResult: int;

machine OrdersDb {

  start state Serving {
    on eCommit do (r: (from: machine, epoch: int, change: tChange)) {
      announce eCommitted, r.change;
      send r.from, eCommitResult, r.epoch;
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
  var change: tChange;

  start state Init {
    entry (p: (db: machine, broker: machine)) {
      db = p.db;
      broker = p.broker;
      goto Ready;
    }
  }

  state Ready {
    on eUpdate goto Committing with (r: (client: machine, change: tChange)) {
      client = r.client;
      change = r.change;
      send db, eCommit, (from = this, epoch = epoch, change = change);
    }
    on eCrash do { epoch = epoch + 1; }
    // Replies addressed to the process before a crash.
    ignore eCommitResult, ePublishAck;
  }

  state Committing {
    defer eUpdate;
    on eCommitResult do (e: int) {
      if (e == epoch) {
        // BUG: the commit is durable, the publish below is not yet.
        send broker, ePublish, (from = this, epoch = epoch, change = change);
        goto Publishing;
      }
    }
    on eCrash do { Crash(); }
  }

  state Publishing {
    defer eUpdate;
    on ePublishAck do (a: (epoch: int, id: int)) {
      if (a.epoch == epoch) {
        send client, eUpdateResp, change.id;
        goto Ready;
      }
    }
    on eCrash do { Crash(); }
  }

  fun Crash() {
    send client, eReset, change.id;
    epoch = epoch + 1;
    goto Ready;
  }
}
