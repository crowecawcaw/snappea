// Lock service with fencing tokens. Events mirror what solution.py emits.
type tGrant   = (client: string, token: int, now: int, expiry: int);

event eGrant:    tGrant;
event eReleased: int;   // token
event eIssued:   int;   // token
event eWriteOk:  int;   // token
event eAcquire:  (client: string, now: int);
event eRel:      (client: string, token: int, now: int);
event eWrite:    (token: int);

// Grants never overlap and tokens strictly increase.
spec LockSafety observes eGrant, eReleased {
  var lastToken: int;
  var heldUntil: int;
  var holder: string;
  start state Init {
    on eGrant do (g: tGrant) {
      assert g.token > lastToken, "token did not increase";
      assert g.now >= heldUntil, "granted while another lease is unexpired";
      lastToken = g.token; heldUntil = g.expiry; holder = g.client;
    }
    on eReleased do (token: int) {
      assert token == lastToken, "released a token that is not the latest";
      heldUntil = 0;
    }
  }
}

// An accepted write uses an issued token, and never a lower one than before.
spec Fencing observes eIssued, eWriteOk {
  var issued: int;
  var maxUsed: int;
  start state Init {
    on eIssued do (token: int) { issued = token; }
    on eWriteOk do (token: int) {
      assert token >= 1 && token <= issued, "write with unissued token accepted";
      assert token >= maxUsed, "stale token accepted";
      maxUsed = token;
    }
  }
}

machine Service {
  var ttl: int; var next: int; var holder: string; var expiry: int; var held: bool;
  var maxUsed: int;
  start state Init {
    entry (t: int) { ttl = t; next = 1; }
    on eAcquire do (a: (client: string, now: int)) {
      if (held && a.now < expiry) {
        return;
      }
      holder = a.client; held = true; expiry = a.now + ttl;
      grantEv(a.client, next, a.now);
      next = next + 1;
    }
    on eRel do (r: (client: string, token: int, now: int)) {
      if (held && holder == r.client && r.token == next - 1 && r.now < expiry) {
        held = false;
        releaseEv(r.token);
      }
    }
    on eWrite do (w: (token: int)) {
      if (w.token >= 1 && w.token < next && w.token >= maxUsed) {
        maxUsed = w.token;
        writeEv(w.token);
      }
    }
  }
  fun grantEv(c: string, t: int, n: int) { announce eIssued, t; announce eGrant, (client = c, token = t, now = n, expiry = expiry); }
  fun releaseEv(t: int) { announce eReleased, t; }
  fun writeEv(t: int) { announce eWriteOk, t; }
}

machine Driver {
  start state Run {
    entry {
      var s: machine; var i: int;
      s = new Service(2);
      while (i < 10) {
        if ($) { send s, eAcquire, (client = pick(), now = choose(5)); }
        else if ($) { send s, eRel, (client = pick(), token = choose(4), now = choose(5)); }
        else { send s, eWrite, (token = choose(4),); }
        i = i + 1;
      }
    }
  }
}

fun pick(): string {
  if ($) { return "a"; }
  return "b";
}
