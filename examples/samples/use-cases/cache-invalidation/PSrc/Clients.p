// The application code: read-through on a miss, and write-then-delete.

// Tells whoever started a client (the test driver) that it has finished.
event ClientDone;

// Reads the key `reads` times, one after another. On a miss it reads the database and
// tries to fill the cache with the lease from the miss; the fill may be refused.
machine Reader {
  start state Reading {
    entry (input: (owner: machine, cache: machine, db: Database, id: int, reads: int)) {
      var i: int;
      while (i < input.reads) {
        ReadOnce(input.cache, input.db, input.id);
        i = i + 1;
      }
      send input.owner, ClientDone;
    }
  }

  fun ReadOnce(cache: machine, db: Database, id: int) {
    var result: int;
    var fromCache: bool;
    announce ReadStarted, (reader = id,);
    send cache, CacheGet, (client = this,);
    receive {
      case CacheGetResponse: (g: (hit: bool, value: int, lease: int)) {
        if (g.hit) {
          result = g.value;
          fromCache = true;
        } else {
          send db, DatabaseRead, (client = this,);
          receive {
            case DatabaseReadResponse: (r: (value: int)) { result = r.value; }
          }
          send cache, CacheSet, (client = this, lease = g.lease, value = result);
          // The read's answer does not depend on whether the fill was stored.
          receive { case CacheSetResponse: { } }
        }
      }
    }
    announce ReadReturned, (reader = id, value = result, fromCache = fromCache);
  }
}

// Writes each value in turn: commit to the database, then delete the cache key.
machine Writer {
  start state Writing {
    entry (input: (owner: machine, cache: machine, db: Database, writes: seq[int])) {
      var v: int;
      foreach (v in input.writes) {
        send input.db, DatabaseWrite, (client = this, value = v);
        receive { case DatabaseWriteResponse: { } }
        send input.cache, CacheDelete, (client = this,);
        receive { case CacheDeleteResponse: { } }
        announce WriteDone, (value = v,);
      }
      send input.owner, ClientDone;
    }
  }
}
