// Black-box spec for observed real calls (see test_solution.py). Events are
// recorded one at a time, so the history is a sequential linearization.
event eObsAcquire: (cid: int, now: int, result: int);
event eObsRelease: (cid: int, token: int, now: int, result: bool);
event eObsWrite: (res: int, token: int, ok: bool);

// snappea-lint: allow unselected-spec -- observation-only spec, checked by test_solution.py via check_events
spec ObservedContract observes eObsAcquire, eObsRelease, eObsWrite {
  var last: int;
  var holder: int;
  var token: int;
  var expires: int;
  var maxFor: map[int, int];
  start state Watching {
    entry { holder = -1; }
    on eObsAcquire do (p: (cid: int, now: int, result: int)) {
      var held: bool;
      held = holder != -1 && p.now < expires;
      if (held) {
        if (holder == p.cid) { assert p.result == token, "re-acquire must return the same token"; }
        else { assert p.result == -1, "acquire must fail while another client holds the lease"; }
      } else {
        assert p.result == last + 1, "tokens must strictly increase by grant";
        last = p.result; holder = p.cid; token = p.result; expires = p.now + 2;
      }
    }
    on eObsRelease do (p: (cid: int, token: int, now: int, result: bool)) {
      var ok: bool;
      ok = holder == p.cid && token == p.token && p.now < expires;
      assert p.result == ok, "release result disagrees with the lease";
      if (ok) { holder = -1; }
    }
    on eObsWrite do (p: (res: int, token: int, ok: bool)) {
      var cur: int;
      var expect: bool;
      cur = 0;
      if (p.res in maxFor) { cur = maxFor[p.res]; }
      expect = p.token >= 1 && p.token <= last && p.token >= cur;
      assert p.ok == expect, "fenced write result disagrees with the contract";
      if (expect) { maxFor[p.res] = p.token; }
    }
  }
}
