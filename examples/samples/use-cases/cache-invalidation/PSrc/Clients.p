// The application code: read-through on a miss, and write-then-delete.

// Tells whoever started a client (the test driver) that it has finished.
event eClientDone;

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
      send input.owner, eClientDone;
    }
  }

  fun ReadOnce(cache: machine, db: Database, id: int) {
    var result: int;
    var fromCache: bool;
    announce eReadStarted, (reader = id,);
    send cache, eCacheGet, (client = this,);
    receive {
      case eCacheGetResp: (g: (hit: bool, value: int, lease: int)) {
        if (g.hit) {
          result = g.value;
          fromCache = true;
        } else {
          send db, eDbRead, (client = this,);
          receive {
            case eDbReadResp: (r: (value: int)) { result = r.value; }
          }
          send cache, eCacheSet, (client = this, lease = g.lease, value = result);
          // The read's answer does not depend on whether the fill was stored.
          receive { case eCacheSetResp: { } }
        }
      }
    }
    announce eReadReturned, (reader = id, value = result, fromCache = fromCache);
  }
}

// Writes each value in turn: commit to the database, then delete the cache key.
machine Writer {
  start state Writing {
    entry (input: (owner: machine, cache: machine, db: Database, writes: seq[int])) {
      var v: int;
      foreach (v in input.writes) {
        send input.db, eDbWrite, (client = this, value = v);
        receive { case eDbWriteResp: { } }
        send input.cache, eCacheDelete, (client = this,);
        receive { case eCacheDeleteResp: { } }
        announce eWriteDone, (value = v,);
      }
      send input.owner, eClientDone;
    }
  }
}
