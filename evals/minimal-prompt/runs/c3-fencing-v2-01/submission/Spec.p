// Specs shared by the model and the Python observation tests.
// SeqContract: exact oracle for one-call-at-a-time histories (eAcquired/eReleased/eWritten
//   are the returned results of real calls).
// SafetyContract: commit-point checks that hold under any interleaving
//   (eGrant/eRevoke/eAccept are announced when the CAS commits).
event eAcquired: (client: int, now: int, token: int);   // token 0 = None
event eReleased: (client: int, token: int, now: int, ok: bool);
event eWritten: (token: int, ok: bool);
event eGrant: (client: int, token: int, now: int);
event eRevoke: (client: int, token: int, now: int);
event eAccept: (token: int);

spec SeqContract observes eAcquired, eReleased, eWritten {
  var counter: int;
  var active: bool;
  var holder: int;
  var token: int;
  var expires: int;
  var maxUsed: int;
  start state Tracking {
    entry { maxUsed = 0; }
    on eAcquired do (p: (client: int, now: int, token: int)) {
      if (active && p.now < expires) {
        if (holder == p.client) { assert p.token == token, "re-acquire must return the held token"; }
        else { assert p.token == 0, "acquire while another holds an unexpired lease must be None"; }
      } else {
        assert p.token == counter + 1, "new lease must get the next token";
        counter = counter + 1; active = true; holder = p.client; token = p.token; expires = p.now + 2;
      }
    }
    on eReleased do (p: (client: int, token: int, now: int, ok: bool)) {
      var expected: bool;
      expected = active && holder == p.client && token == p.token && p.now < expires;
      assert p.ok == expected, "release result wrong";
      if (expected) { active = false; }
    }
    on eWritten do (p: (token: int, ok: bool)) {
      var expected: bool;
      expected = p.token >= 1 && p.token <= counter && p.token >= maxUsed;
      assert p.ok == expected, "write result wrong";
      if (expected) { maxUsed = p.token; }
    }
  }
}

spec SafetyContract observes eGrant, eRevoke, eAccept {
  var last: int;
  var active: bool;
  var holder: int;
  var token: int;
  var expires: int;
  var maxUsed: int;
  start state Tracking {
    on eGrant do (p: (client: int, token: int, now: int)) {
      assert p.token > last, "tokens must strictly increase";
      assert !(active && p.now < expires), "granted while an unexpired lease exists";
      last = p.token; active = true; holder = p.client; token = p.token; expires = p.now + 2;
    }
    on eRevoke do (p: (client: int, token: int, now: int)) {
      assert active && holder == p.client && token == p.token && p.now < expires, "bad release commit";
      active = false;
    }
    on eAccept do (p: (token: int)) {
      assert p.token >= 1 && p.token <= last, "accepted a token that was never issued";
      assert p.token >= maxUsed, "accepted a token lower than one already used";
      maxUsed = p.token;
    }
  }
}
