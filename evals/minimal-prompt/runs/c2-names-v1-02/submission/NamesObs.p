/* Observation monitor for solution.py: same contract as Names.p, string-typed.
 * Events are emitted at the CAS commit point (under a lock), so order = commit order.
 * eQuiescent is emitted by tests when no call is in progress. */
event eClaimed: (name: string, user: string);
event eReleased: (name: string, user: string);
event eUserSet: (user: string, name: string);
event eQuiescent;

spec NamesObs observes eClaimed, eReleased, eUserSet, eQuiescent {
  var holder: map[string, string];
  var userName: map[string, string];
  start hot state Run {
    on eClaimed do (p: (name: string, user: string)) {
      assert !(p.name in holder), "name claimed while held by someone";
      holder[p.name] = p.user;
      goto Active;
    }
    on eReleased do { assert false, "release before any claim"; }
    on eUserSet do { assert false, "user set before any claim"; }
    on eQuiescent do { assert false, "no useful work: nothing was claimed"; }
  }
  state Active {
    on eClaimed do (p: (name: string, user: string)) {
      assert !(p.name in holder), "name claimed while held by someone";
      holder[p.name] = p.user;
    }
    on eReleased do (p: (name: string, user: string)) {
      assert p.name in holder, "released a name nobody holds";
      assert holder[p.name] == p.user, "released a name held by another user";
      holder -= (p.name);
    }
    on eUserSet do (p: (user: string, name: string)) {
      assert p.name in holder, "user set to an unclaimed name";
      assert holder[p.name] == p.user, "user set to a name held by another";
      userName[p.user] = p.name;
    }
    on eQuiescent do {
      var k: string;
      foreach (k in keys(holder)) {
        assert holder[k] in userName, "held name has no user record (leak)";
        assert userName[holder[k]] == k, "held name is not its owner's name";
      }
      foreach (k in keys(userName)) {
        assert userName[k] in holder, "user name not held";
        assert holder[userName[k]] == k, "user name held by another";
      }
    }
  }
}
