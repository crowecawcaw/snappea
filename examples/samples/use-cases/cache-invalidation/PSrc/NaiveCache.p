// A plausible broken design, kept as a negative control: the cache stores every set,
// with no lease check. A reader that read the old value before a writer's write and
// delete fills the cache with it afterwards, and the stale value stays until the next
// write. tcBugNaiveCache shows the spec catching it.
machine NaiveCache {
  var present: bool;
  var value: int;

  start state Serving {
    on eCacheGet do (g: (client: machine)) {
      if (present) {
        announce eCacheHit, (value = value,);
        send g.client, eCacheGetResp, (hit = true, value = value, lease = 0);
        return;
      }
      announce eCacheMiss;
      send g.client, eCacheGetResp, (hit = false, value = 0, lease = 0);
    }
    on eCacheSet do (s: (client: machine, lease: int, value: int)) {
      // BUG: no lease, so a set computed before the last delete still lands.
      present = true;
      value = s.value;
      announce eCacheFilled, (value = value,);
      send s.client, eCacheSetResp;
    }
    on eCacheDelete do (d: (client: machine)) {
      present = false;
      announce eInvalidated;
      send d.client, eCacheDeleteResp;
    }
  }
}
