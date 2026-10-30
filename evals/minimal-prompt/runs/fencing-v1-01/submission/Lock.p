// Model: lock record {last,holder,token,exp} updated by load/CAS; resource keeps high-water token.
event eAcq: (res: int);
event eWrite: (token: int, ok: bool);
event eDone;

spec Fencing observes eAcq, eWrite, eDone {
  var last: int;
  var high: int;
  start state S {
    on eAcq do (p: (res: int)) {
      assert p.res == last + 1, "tokens must strictly increase by issue";
      last = p.res;
    }
    on eWrite do (p: (token: int, ok: bool)) {
      if (p.token < 1 || p.token > last || p.token < high) { assert !p.ok, "stale/unissued token accepted"; }
      else { assert p.ok, "valid token rejected"; high = p.token; }
    }
    on eDone do { }
  }
}

event eReq: (token: int, issue: bool);

machine Service {
  var last: int;
  var high: int;
  start state Serving {
    on eReq do (p: (token: int, issue: bool)) {
      var ok: bool;
      if (p.issue) { last = last + 1; announce eAcq, (res = last,); return; }
      ok = !(p.token < 1 || p.token > last || p.token < high);
      if (ok) { high = p.token; }
      announce eWrite, (token = p.token, ok = ok);
    }
  }
}

machine Main {
  start state Init {
    entry {
      var i: int; var s: Service;
      s = new Service();
      while (i < 5) {
        if ($) { send s, eReq, (token = 0, issue = true); }
        else { send s, eReq, (token = choose(4), issue = false); }
        i = i + 1;
      }
      announce eDone;
    }
  }
}
test tcFence [main=Main]: assert Fencing in { Main, Service };
