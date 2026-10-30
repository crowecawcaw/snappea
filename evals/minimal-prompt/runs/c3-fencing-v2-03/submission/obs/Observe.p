// Sequential oracle for observed real calls (ttl = 3). Events carry actual args and results.
event eAcquire: (client: string, now: int, result: int);   // result 0 = None
event eRelease: (client: string, token: int, now: int, ok: bool);
event eWrite: (token: int, ok: bool);

spec Observed observes eAcquire, eRelease, eWrite {
  var holder: string;
  var held: bool;
  var token: int;
  var expiry: int;
  var last: int;
  var maxUsed: int;
  start state Run {
    on eAcquire do (a: (client: string, now: int, result: int)) {
      if (held && a.now < expiry) {
        if (holder == a.client) { assert a.result == token, "holder must get its token back"; }
        else { assert a.result == 0, "acquire must fail while leased"; }
      } else {
        assert a.result > last, "token must exceed every earlier token";
        held = true; holder = a.client; token = a.result; expiry = a.now + 3; last = a.result;
      }
    }
    on eRelease do (r: (client: string, token: int, now: int, ok: bool)) {
      var valid: bool;
      valid = held && holder == r.client && token == r.token && r.now < expiry;
      assert r.ok == valid, "release result wrong";
      if (valid) { held = false; }
    }
    on eWrite do (w: (token: int, ok: bool)) {
      var valid: bool;
      valid = w.token >= 1 && w.token <= last && w.token >= maxUsed;
      assert w.ok == valid, "write result wrong";
      if (valid) { maxUsed = w.token; }
    }
  }
}
