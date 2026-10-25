// Independent state model. Calls and their results are observed by the Python
// wrapper; the monitor derives validity from its own clock, owner, and token.
spec LeaseContract observes eAcquire, eWrite, eRelease, eAdvance, eRead, ePeek {
  var now: int;
  var value: int;
  var owner: string;
  var token: int;
  var expires_at: int;
  var last_token: int;

  start state Open {
    entry {
      now = 0;
      value = 0;
      owner = "-";
      token = -1;
      expires_at = -1;
      last_token = 0;
    }

    on eAcquire do (p: (client: string, ttl: int, token: int)) {
      assert p.ttl > 0, "lease TTL must be positive";
      if (owner != "-" && expires_at > now) {
        assert p.token == -1, "acquire succeeded while a lease was active";
      } else {
        assert p.token > last_token, "acquire did not return a fresh increasing token";
        owner = p.client;
        token = p.token;
        expires_at = now + p.ttl;
        last_token = p.token;
      }
    }

    on eWrite do (p: (client: string, token: int, value: int, accepted: bool)) {
      if (owner != "-" && expires_at > now && owner == p.client && token == p.token) {
        assert p.accepted, "valid active lease write was rejected";
        value = p.value;
      } else {
        assert !p.accepted, "write accepted without the active owner and token";
      }
    }

    on eRelease do (p: (client: string, token: int, accepted: bool)) {
      if (owner != "-" && expires_at > now && owner == p.client && token == p.token) {
        assert p.accepted, "valid active lease release was rejected";
        owner = "-";
        token = -1;
        expires_at = -1;
      } else {
        assert !p.accepted, "release accepted without the active owner and token";
      }
    }

    on eAdvance do (p: (ticks: int)) {
      assert p.ticks >= 0, "clock cannot move backwards";
      now = now + p.ticks;
    }

    on eRead do (p: (value: int)) {
      assert p.value == value, "read returned a value different from modeled state";
    }

    on ePeek do (p: (owner: string, token: int, expires_at: int)) {
      if (owner != "-" && expires_at > now) {
        assert p.owner == owner && p.token == token && p.expires_at == expires_at,
          "peek did not report the active lease";
      } else {
        assert p.owner == "-" && p.token == -1 && p.expires_at == -1,
          "peek reported an expired or absent lease";
      }
    }
  }
}
