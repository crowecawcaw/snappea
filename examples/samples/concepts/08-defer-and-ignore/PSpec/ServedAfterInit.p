// Contract: the server finishes initializing; it answers requests and health checks
// only after it has loaded its configuration; every answer uses the loaded
// configuration; every request is eventually answered, and so is a health check sent
// once the server is known to be serving.

event Ready;
event Requested: (id: int);
event HealthCheckDue;

spec ServedAfterInit observes Loaded, Ready, Requested, Response, HealthCheckDue, Healthy {
  var loaded: int;
  var pending: set[int];
  var healthDue: bool;

  // Hot: a server that never finishes initializing fails.
  start hot state Starting {
    on Loaded do (l: (config: int)) { loaded = l.config; }
    on Ready do {
      assert loaded != 0, "server ready before storage sent its configuration";
      Settle();
    }
    on Requested do (r: (id: int)) { pending += (r.id); }
    on Response do (r: (id: int, config: int)) {
      assert false, format("request {0} answered with config {1} before the server was ready", r.id, r.config);
    }
    on Healthy do { assert false, "server reported healthy before it was ready"; }
  }

  // Ready, with a request or a due health check still unanswered.
  hot state Owing {
    on Requested do (r: (id: int)) { pending += (r.id); }
    on HealthCheckDue do { healthDue = true; }
    on Healthy do { healthDue = false; Settle(); }
    on Response do (r: (id: int, config: int)) {
      assert r.id in pending, format("response to request {0}, which nobody is waiting for", r.id);
      assert r.config == loaded,
        format("request {0} answered with config {1}, but storage holds {2}", r.id, r.config, loaded);
      pending -= (r.id);
      Settle();
    }
  }

  // Ready, nothing owed.
  cold state Settled {
    on Requested do (r: (id: int)) { pending += (r.id); goto Owing; }
    on HealthCheckDue do { healthDue = true; goto Owing; }
    on Healthy do { healthDue = false; }
  }

  fun Settle() {
    if (sizeof(pending) == 0 && !healthDue) {
      goto Settled;
    } else {
      goto Owing;
    }
  }
}
