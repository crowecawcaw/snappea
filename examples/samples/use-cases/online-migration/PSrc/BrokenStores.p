// Two plausible broken new stores, kept as negative controls. Writers are unchanged;
// only the put the backfill relies on differs.

// The backfill copy is an unconditional put. A copy of a row read before a writer
// updated it lands after the writer's dual-write and overwrites the newer value.
// tcBugBlindBackfill shows it.
machine BlindCopyNewStore {
  var rows: map[int, Row];

  start state Serving {
    on NewWrite do (w: (client: machine, key: int, row: Row)) {
      if (w.row.lsn > RowOf(rows, w.key).lsn) {
        Put(w.key, w.row);
      }
      send w.client, NewWritten, (key = w.key,);
    }
    on Copy do (w: (client: machine, key: int, row: Row)) {
      // BUG: no version check.
      Put(w.key, w.row);
      send w.client, NewWritten, (key = w.key,);
    }
    on ReadRow do (r: (client: machine, reader: int, key: int)) {
      announce ReadServed, (table = NewTable, reader = r.reader, key = r.key);
      send r.client, RowRead, (key = r.key, row = RowOf(rows, r.key));
    }
  }

  fun Put(key: int, row: Row) {
    rows[key] = row;
    announce NewApplied, (key = key, row = row);
  }
}

// Insert-if-absent backfill with hard deletes: the usual first fix for the stale
// copy. It stops a copy from overwriting a live row, but a delete removes the row,
// so a copy of the pre-delete row read before the delete inserts it again.
// tcBugResurrectedDelete shows it.
machine InsertIfAbsentNewStore {
  var rows: map[int, Row];

  start state Serving {
    on NewWrite do (w: (client: machine, key: int, row: Row)) {
      if (w.row.deleted) {
        // Announced as the delete it applied; readers see no row either way.
        rows -= (w.key);
        announce NewApplied, (key = w.key, row = w.row);
      } else {
        rows[w.key] = w.row;
        announce NewApplied, (key = w.key, row = w.row);
      }
      send w.client, NewWritten, (key = w.key,);
    }
    on Copy do (w: (client: machine, key: int, row: Row)) {
      // BUG: "absent" cannot tell "never copied" from "deleted since".
      if (!(w.key in rows)) {
        rows[w.key] = w.row;
        announce NewApplied, (key = w.key, row = w.row);
      }
      send w.client, NewWritten, (key = w.key,);
    }
    on ReadRow do (r: (client: machine, reader: int, key: int)) {
      announce ReadServed, (table = NewTable, reader = r.reader, key = r.key);
      send r.client, RowRead, (key = r.key, row = RowOf(rows, r.key));
    }
  }
}

// A router that flips reads to new as soon as the backfill finishes, without draining.
// A write that committed to old but has not reached new yet is missing from new at
// the flip (a reader that saw it in old would see the key go back in time on its next
// read), and a read routed to old just before the flip is served by old after it.
// tcBugCutoverMidWrite and tcBugCutoverMidRead show one each.
machine EagerRouter {
  var oldStore: OldStore;
  var newStore: machine;
  var readsFromNew: bool;

  start state Live {
    entry (input: (oldStore: OldStore, newStore: machine)) {
      oldStore = input.oldStore;
      newStore = input.newStore;
    }
    on BeginWrite do (writer: machine) {
      send writer, WriteGranted;
    }
    ignore EndWrite, ReadDone;
    on Route do (r: (client: machine, reader: int, key: int)) {
      if (readsFromNew) {
        send newStore, ReadRow, r;
      } else {
        send oldStore, ReadRow, r;
      }
    }
    on CutoverReq do (m: machine) {
      // BUG: in-flight reads and writes are not drained first.
      readsFromNew = true;
      announce Cutover;
      send m, CutoverDone;
    }
  }
}
