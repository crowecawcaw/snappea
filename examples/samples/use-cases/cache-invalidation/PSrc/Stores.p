// A look-aside cache in front of a database, for one key.
//
// Real atomic primitives:
//   database: read (returns the committed value), write (commits a new value)
//   cache:    get (hit, or miss plus a lease), set-with-lease, delete
// Each is one handler. A reader's miss -> database read -> set spans three of them,
// so a writer's write and delete can land in between.

event DatabaseRead: (client: machine);
event DatabaseReadResponse: (value: int);
event DatabaseWrite: (client: machine, value: int);
event DatabaseWriteResponse;

event CacheGet: (client: machine);
// On a miss, `lease` is the token the reader must present to fill the cache.
event CacheGetResponse: (hit: bool, value: int, lease: int);
event CacheSet: (client: machine, lease: int, value: int);
// The reader does not care whether its fill was stored, so the reply carries nothing.
event CacheSetResponse;
event CacheDelete: (client: machine);
event CacheDeleteResponse;

machine Database {
  // Committed value; 0 is the initial row.
  var value: int;

  start state Serving {
    on DatabaseRead do (r: (client: machine)) {
      send r.client, DatabaseReadResponse, (value = value,);
    }
    on DatabaseWrite do (w: (client: machine, value: int)) {
      value = w.value;
      announce DatabaseCommitted, (value = value,);
      send w.client, DatabaseWriteResponse;
    }
  }
}

// The fixed design (memcache-style leases). A miss hands out a fresh lease, which
// replaces any earlier one. Delete drops the entry and revokes the outstanding lease.
// A set stores its value only if its lease is still the outstanding one, so a reader
// that read the database before a writer's delete cannot fill the cache afterwards.
machine LeaseCache {
  var present: bool;
  var value: int;
  // Outstanding lease, 0 when none. Leases are numbered from 1.
  var lease: int;
  var nextLease: int;

  start state Serving {
    on CacheGet do (g: (client: machine)) {
      if (present) {
        announce CacheHit, (value = value,);
        send g.client, CacheGetResponse, (hit = true, value = value, lease = 0);
        return;
      }
      // snappea-lint: allow mutation:swap-operator -- leases only need to be distinct and non-zero; counting down keeps them so
      nextLease = nextLease + 1;
      lease = nextLease;
      announce CacheMiss;
      send g.client, CacheGetResponse, (hit = false, value = 0, lease = lease);
    }
    on CacheSet do (s: (client: machine, lease: int, value: int)) {
      // After a fill `present` is true, so no further miss or set can use this lease
      // until a delete revokes it; there is no need to consume it here.
      if (s.lease == lease) {
        present = true;
        value = s.value;
        announce CacheFilled, (value = value,);
      }
      // A refused fill is silent: the reader returns the value it read either way.
      send s.client, CacheSetResponse;
    }
    on CacheDelete do (d: (client: machine)) {
      present = false;
      lease = 0;
      announce Invalidated;
      send d.client, CacheDeleteResponse;
    }
  }
}
