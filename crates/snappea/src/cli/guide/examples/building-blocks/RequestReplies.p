// Building block v1: every planned logical request gets exactly one terminal reply.
// Announce one plan before work starts, one request per unique ID (not per retry),
// and each actual terminal reply with the ID it carries. See README.md.
event BBPlan: int;
event BBRequested: int;
event BBReplied: int;

spec BBRequestReplies observes BBPlan, BBRequested, BBReplied {
  var planned: int;
  var requested: set[int];
  var replied: set[int];

  start hot state AwaitingPlan {
    on BBPlan goto RepliesPending with (count: int) {
      assert count > 0, "plan must contain at least one request";
      planned = count;
    }
    on BBRequested do { assert false, "request before plan"; }
    on BBReplied do { assert false, "reply before plan"; }
  }

  hot state RepliesPending {
    on BBPlan do { assert false, "plan announced twice"; }
    on BBRequested do (id: int) {
      assert !(id in requested), "logical request ID reused";
      assert sizeof(requested) < planned, "more requests than planned";
      requested += (id);
    }
    on BBReplied do (id: int) {
      assert id in requested, "reply for an unknown request";
      assert !(id in replied), "duplicate terminal reply";
      replied += (id);
      if (sizeof(replied) == planned) { goto Settled; }
    }
  }

  cold state Settled {
    on BBPlan do { assert false, "plan announced twice"; }
    on BBRequested do { assert false, "request after plan settled"; }
    on BBReplied do { assert false, "duplicate terminal reply"; }
  }
}
