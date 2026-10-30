// Contract for unique usernames: the only file P-level observation tests need.
event eWrote: (key: int, old: int, nv: int);
event eReturned: (user: int, reg: bool, name: int, ok: bool);
event eSnapshot: (users: map[int, int], names: map[int, int]);
event eDone;

// Independent contract. Knows no versions or retries: tracks name transitions
// (a name is only claimed when free, released only by its holder), checks each
// reply against the user's own history, and checks the quiescent state.
spec Contract observes eWrote, eReturned, eSnapshot, eDone {
  var claims: set[int];          // name*10+user ever claimed
  var holder: map[int, int];
  var ownName: map[int, int];    // user's name derived from own replies
  var snapshots: int;
  start hot state Working {
    entry {
      var i: int;
      while (i < 3) { ownName[i] = -1; holder[i] = -1; i = i + 1; }
    }
    on eWrote do (p: (key: int, old: int, nv: int)) {
      if (p.key >= 3) {
        assert !(p.old != -1 && p.nv != -1), "name taken over from its holder";
        if (p.nv != -1) {
          assert holder[p.key - 3] == -1, "claimed a name that was held";
          holder[p.key - 3] = p.nv;
          claims += ((p.key - 3) * 10 + p.nv);
        } else {
          assert holder[p.key - 3] == p.old, "released by a non-holder";
          holder[p.key - 3] = -1;
        }
      }
    }
    on eReturned do (p: (user: int, reg: bool, name: int, ok: bool)) {
      var cur: int;
      var u: int;
      var other: bool;
      var attempt: bool;
      cur = ownName[p.user];
      attempt = true;
      if (p.reg && cur != -1) {
        assert p.ok == (cur == p.name), "wrong register result for user holding a name";
        attempt = false;
      }
      if (!p.reg && cur == -1) { assert !p.ok, "rename succeeded without a name"; attempt = false; }
      if (!p.reg && cur == p.name) { assert p.ok, "rename to own name must succeed"; attempt = false; }
      if (attempt) {
        if (p.ok) {
          assert ((p.name * 10 + p.user) in claims), "success without claiming the name";
          ownName[p.user] = p.name;
        } else {
          u = 0;
          while (u < 3) {
            if (u != p.user && ((p.name * 10 + u) in claims)) { other = true; }
            u = u + 1;
          }
          assert other, "rejected although nobody else ever held the name";
        }
      }
    }
    on eSnapshot do (p: (users: map[int, int], names: map[int, int])) {
      var i: int;
      i = 0;
      while (i < 3) {
        assert p.users[i] == ownName[i], "user's name differs from what its replies imply";
        if (p.users[i] != -1) { assert p.names[p.users[i]] == i, "name not owned by its user"; }
        i = i + 1;
      }
      i = 0;
      while (i < 2) {
        if (p.names[i] != -1) { assert p.users[p.names[i]] == i, "owner does not hold the name"; }
        i = i + 1;
      }
      snapshots = snapshots + 1;
    }
    on eDone do {
      assert snapshots == 1, "no quiescent snapshot";
      goto Complete;
    }
  }
  state Complete { }
}

