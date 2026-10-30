// Model: lease lock with fencing tokens. Resource accepts only non-decreasing issued tokens.
event eAccepted: int;   // token of an accepted write
event eIssued: int;     // token granted by the lock

// snappea-lint: allow unselected-spec -- checked against implementation events via pysnappea
spec FencingMonotonic observes eAccepted, eIssued {
  var maxAccepted: int;
  var lastIssued: int;
  start state Init {
    on eIssued do (t: int) { assert t > lastIssued, "token not increasing"; lastIssued = t; }
    on eAccepted do (t: int) {
      assert t <= lastIssued, "accepted a never-issued token";
      assert t >= maxAccepted, "accepted a stale token";
      maxAccepted = t;
    }
  }
}
