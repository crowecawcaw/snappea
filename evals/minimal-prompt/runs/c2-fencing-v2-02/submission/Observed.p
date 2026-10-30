// Sequential-history contract for the real implementation (ttl = 2).
// Events are recorded by the tests with actual arguments and results.
// token -1 in eAcquired means None.
event eAcquired: (client: string, now: int, token: int);
event eReleased: (client: string, token: int, now: int, ok: bool);
event eWritten: (res: string, token: int, ok: bool);

spec ObservedContract observes eAcquired, eReleased, eWritten {
  var last: int;
  var holder: string;
  var held: bool;
  var token: int;
  var exp: int;
  var high: map[string, int];
  start state Watching {
    on eAcquired do (p: (client: string, now: int, token: int)) {
      var live: bool;
      live = held && p.now < exp;
      if (live) {
        if (holder == p.client) { assert p.token == token, "holder must get its own token back"; }
        else { assert p.token == -1, "acquire granted while lease is live"; }
      } else {
        assert p.token == last + 1, "token must be fresh and strictly increasing";
        last = p.token; holder = p.client; held = true; token = p.token; exp = p.now + 2;
      }
    }
    on eReleased do (p: (client: string, token: int, now: int, ok: bool)) {
      var should: bool;
      should = held && holder == p.client && token == p.token && p.now < exp;
      assert p.ok == should, "release result disagrees with contract";
      if (should) { held = false; }
    }
    on eWritten do (p: (res: string, token: int, ok: bool)) {
      var should: bool;
      should = p.token >= 1 && p.token <= last;
      if (should && (p.res in high)) { should = p.token >= high[p.res]; }
      assert p.ok == should, "write result disagrees with fencing contract";
      if (should) { high[p.res] = p.token; }
    }
  }
}
