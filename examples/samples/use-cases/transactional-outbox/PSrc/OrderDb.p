// The order service's database. Placing an order writes the order row and its outbox
// row in one transaction, so either both exist or neither does.
//
// Real atomic primitives (one transaction each):
//   place:    insert order + outbox row (unsent), or abort and write nothing
//   fetch:    read the unsent outbox rows; if there are none, register the relay to be
//             woken by the next commit (LISTEN/NOTIFY in Postgres). A poll interval
//             works the same way in production; in the model a busy poll loop would
//             never let a test end, while a sleeping relay does.
//   markSent: set one outbox row to sent

event PlaceOrder: (id: int, commits: bool);
event Fetch: machine;
event FetchResp: seq[int];
event MarkSent: int;
event Wake;

machine OrderDb {
  // order id -> has its outbox row been marked sent?
  var outbox: map[int, bool];
  var sleepers: set[machine];

  start state Serving {
    on PlaceOrder do (o: (id: int, commits: bool)) {
      if (!o.commits) {
        // The transaction failed (validation, conflict): neither row is written.
        announce OrderAborted, o.id;
        return;
      }
      outbox[o.id] = false;
      announce OrderCommitted, o.id;
      WakeSleepers();
    }
    on Fetch do (relay: machine) {
      var rows: seq[int];
      var id: int;
      foreach (id in keys(outbox)) {
        if (!outbox[id]) { rows += (sizeof(rows), id); }
      }
      if (sizeof(rows) == 0) { sleepers += (relay); }
      send relay, FetchResp, rows;
    }
    on MarkSent do (id: int) {
      outbox[id] = true;
      announce MarkedSent, id;
    }
  }

  fun WakeSleepers() {
    var s: machine;
    foreach (s in sleepers) { send s, Wake; }
    sleepers = default(set[machine]);
  }
}
