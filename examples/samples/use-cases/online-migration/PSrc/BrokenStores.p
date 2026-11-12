// Two plausible broken new stores, kept as negative controls. Writers are unchanged;
// only the put the backfill relies on differs.

// The backfill copy is an unconditional put. A copy of a row read before a writer
// updated it lands after the writer's dual-write and overwrites the newer value.
// tcBugBlindBackfill shows it.
machine BlindCopyNewStore {
  var rows: map[int, tRow];

  start state Serving {
    on eNewWrite do (w: (client: machine, key: int, row: tRow)) {
      if (w.row.lsn > RowOf(rows, w.key).lsn) {
        Put(w.key, w.row);
      }
      send w.client, eNewWritten, (key = w.key,);
    }
    on eCopy do (w: (client: machine, key: int, row: tRow)) {
      // BUG: no version check.
      Put(w.key, w.row);
      send w.client, eNewWritten, (key = w.key,);
    }
    on eReadRow do (r: (client: machine, key: int)) {
      send r.client, eRow, (key = r.key, row = RowOf(rows, r.key));
    }
  }

  fun Put(key: int, row: tRow) {
    rows[key] = row;
    announce eNewApplied, (key = key, row = row);
  }
}

// Insert-if-absent backfill with hard deletes: the usual first fix for the stale
// copy. It stops a copy from overwriting a live row, but a delete removes the row,
// so a copy of the pre-delete row read before the delete inserts it again.
// tcBugResurrectedDelete shows it.
machine InsertIfAbsentNewStore {
  var rows: map[int, tRow];

  start state Serving {
    on eNewWrite do (w: (client: machine, key: int, row: tRow)) {
      if (w.row.deleted) {
        // Announced as the delete it applied; readers see no row either way.
        rows -= (w.key);
        announce eNewApplied, (key = w.key, row = w.row);
      } else {
        rows[w.key] = w.row;
        announce eNewApplied, (key = w.key, row = w.row);
      }
      send w.client, eNewWritten, (key = w.key,);
    }
    on eCopy do (w: (client: machine, key: int, row: tRow)) {
      // BUG: "absent" cannot tell "never copied" from "deleted since".
      if (!(w.key in rows)) {
        rows[w.key] = w.row;
        announce eNewApplied, (key = w.key, row = w.row);
      }
      send w.client, eNewWritten, (key = w.key,);
    }
    on eReadRow do (r: (client: machine, key: int)) {
      send r.client, eRow, (key = r.key, row = RowOf(rows, r.key));
    }
  }
}

// A router that flips reads to new as soon as the backfill finishes, without draining.
// A write that committed to old but has not reached new yet is missing from new at
// the flip, so a reader that saw it in old sees the key go back in time when its next
// read comes from new. tcBugCutoverWithoutDrain shows it.
machine EagerRouter {
  var oldStore: OldStore;
  var newStore: machine;
  var readsFromNew: bool;

  start state Live {
    entry (input: (oldStore: OldStore, newStore: machine)) {
      oldStore = input.oldStore;
      newStore = input.newStore;
    }
    on eBeginWrite do (writer: machine) {
      send writer, eWriteGranted;
    }
    ignore eEndWrite, eReadDone;
    on eRoute do (r: (client: machine, key: int)) {
      if (readsFromNew) {
        send newStore, eReadRow, r;
      } else {
        send oldStore, eReadRow, r;
      }
    }
    on eCutoverReq do (m: machine) {
      // BUG: in-flight reads and writes are not drained first.
      readsFromNew = true;
      announce eCutover;
      send m, eCutoverDone;
    }
  }
}
