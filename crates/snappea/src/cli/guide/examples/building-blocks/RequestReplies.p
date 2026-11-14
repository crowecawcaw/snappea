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
      assert count > 0, format("plan of {0} requests must contain at least one request", count);
      planned = count;
    }
    on BBRequested do (id: int) { assert false, format("request {0} before plan", id); }
    on BBReplied do (id: int) { assert false, format("reply for request {0} before plan", id); }
  }

  hot state RepliesPending {
    on BBPlan do { assert false, "plan announced twice"; }
    on BBRequested do (id: int) {
      assert !(id in requested), format("logical request ID {0} reused", id);
      assert sizeof(requested) < planned, format("request {0} is more requests than the {1} planned", id, planned);
      requested += (id);
    }
    on BBReplied do (id: int) {
      assert id in requested, format("reply for an unknown request {0}", id);
      assert !(id in replied), format("duplicate terminal reply for request {0}", id);
      replied += (id);
      if (sizeof(replied) == planned) { goto Settled; }
    }
  }

  cold state Settled {
    on BBPlan do { assert false, "plan announced twice"; }
    on BBRequested do (id: int) { assert false, format("request {0} after plan settled", id); }
    on BBReplied do (id: int) { assert false, format("duplicate terminal reply for request {0}", id); }
  }
}
