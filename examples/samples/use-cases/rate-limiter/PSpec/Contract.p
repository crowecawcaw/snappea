// The rate-limit contract, stated over events announced where decisions commit: the
// store announces each admission at its CAS, a gateway announces each rejection, and
// the clock announces each new window. The specs keep their own per-window counts;
// they never read the store's fields or trust a gateway's arithmetic.

// The clock moved to a new window.
event eTicked: int;
// The driver sent request `req` (to some gateway).
event eRequested: int;
// The driver plans this many requests in total.
event eRequestsPlanned: int;
// Store commit: `req` is counted in `window`, which now holds `count` admissions.
event eAdmitted: (req: int, window: int, count: int);
// A gateway rejected `req` because it found `window` full.
event eRejected: (req: int, window: int);
// A gateway's CAS for `req` lost to another writer; it will reload and decide again.
event eCasLost: int;
// The driver received the gateway's answer.
event eAnswered: (req: int, admitted: bool);

// Safety: no window admits more than Limit() requests across all gateways; a request
// is counted in, or rejected for, a window that was current while it was in flight;
// once a later window has admitted requests, no earlier window admits more; and a
// rejection is justified only by a full window.
spec WindowLimit observes eTicked, eRequested, eAdmitted, eRejected, eCasLost {
  var current: int;
  // req -> window current when the driver sent it
  var startedIn: map[int, int];
  // window -> admissions counted so far
  var admitted: map[int, int];
  var latest: int;
  var decided: set[int];

  start state Watching {
    on eTicked do (w: int) {
      assert w == current + 1, format("clock jumped from window {0} to {1}", current, w);
      current = w;
    }
    on eRequested do (req: int) {
      assert !(req in startedIn), format("request {0} sent twice", req);
      startedIn[req] = current;
    }
    on eAdmitted do (p: (req: int, window: int, count: int)) {
      Decided(p.req, p.window);
      assert p.window >= latest,
        format("request {0} admitted into window {1} after window {2} began", p.req, p.window, latest);
      latest = p.window;
      if (!(p.window in admitted)) { admitted[p.window] = 0; }
      admitted[p.window] = admitted[p.window] + 1;
      assert admitted[p.window] <= Limit(),
        format("window {0} admitted {1} requests, limit {2}", p.window, admitted[p.window], Limit());
      assert p.count == admitted[p.window],
        format("store counts {0} in window {1}, but {2} requests were admitted", p.count, p.window, admitted[p.window]);
    }
    on eCasLost do (req: int) {
      assert req in startedIn && !(req in decided),
        format("request {0} lost a CAS, but it was not in flight", req);
    }
    on eRejected do (p: (req: int, window: int)) {
      Decided(p.req, p.window);
      assert p.window in admitted && admitted[p.window] >= Limit(),
        format("request {0} rejected although window {1} was not full", p.req, p.window);
    }
  }

  fun Decided(req: int, window: int) {
    assert req in startedIn, format("request {0} decided but never sent", req);
    assert !(req in decided), format("request {0} decided twice", req);
    assert window >= startedIn[req] && window <= current,
      format("request {0} decided in window {1}, which was not current while it was in flight", req, window);
    decided += (req);
  }
}

// Progress: every request the driver plans is decided exactly once and answered with
// that decision. Hot while any answer is outstanding, so a dropped reply, a stuck CAS
// loop or a decision the gateway never reports fails.
spec RequestsAnswered observes eRequestsPlanned, eAdmitted, eRejected, eAnswered {
  // req -> admitted?
  var decisions: map[int, bool];
  var unanswered: int;

  start state Idle {
    on eRequestsPlanned do (n: int) {
      assert n > 0, "planned no requests";
      unanswered = n;
      goto Waiting;
    }
    on eAdmitted do { assert false, "admission before any request was planned"; }
    on eRejected do { assert false, "rejection before any request was planned"; }
    on eAnswered do { assert false, "answer before any request was planned"; }
  }

  hot state Waiting {
    on eRequestsPlanned do { assert false, "requests planned twice"; }
    // snappea-lint: allow unused-payload-field -- windows and counts are WindowLimit's job
    on eAdmitted do (p: (req: int, window: int, count: int)) { decisions[p.req] = true; }
    // snappea-lint: allow unused-payload-field -- windows are WindowLimit's job
    on eRejected do (p: (req: int, window: int)) { decisions[p.req] = false; }
    on eAnswered do (p: (req: int, admitted: bool)) {
      assert p.req in decisions, format("request {0} answered before it was decided", p.req);
      assert p.admitted == decisions[p.req],
        format("request {0} answered admitted={1}, but the decision was {2}", p.req, p.admitted, decisions[p.req]);
      decisions -= (p.req);
      unanswered = unanswered - 1;
      if (unanswered == 0) { goto Done; }
    }
  }

  state Done {
    on eRequestsPlanned do { assert false, "requests planned twice"; }
    on eAdmitted do { assert false, "admission after every request was answered"; }
    on eRejected do { assert false, "rejection after every request was answered"; }
    on eAnswered do { assert false, "answer after every request was answered"; }
  }
}
