event eIssued: (token: int);
event eWrote: (token: int, ok: bool);
// Oracle for sequential histories: write accepted iff issued and >= max accepted.
spec FencedWrites observes eIssued, eWrote {
  var issued: set[int];
  var last: int;
  var maxAcc: int;
  start state S {
    on eIssued do (p: (token: int)) {
      assert p.token > last, "token not strictly increasing";
      last = p.token; issued += (p.token);
    }
    on eWrote do (p: (token: int, ok: bool)) {
      var should: bool;
      should = (p.token in issued) && p.token >= maxAcc;
      assert p.ok == should, "write result disagrees with fencing contract";
      if (p.ok) maxAcc = p.token;
    }
  }
}
