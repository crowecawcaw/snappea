// The contract, checked from events alone: every planned ping gets one pong, and the
// k-th pong the counter sends reports a count of k (no ping is lost or counted twice).

// Announced by the test driver before it starts: how many pings the clients will send.
event ePingsPlanned: int;

spec PongsCountPings observes ePingsPlanned, ePong {
  var planned: int;
  var ponged: int;

  start state Idle {
    on ePingsPlanned do (n: int) {
      planned = n;
      goto Waiting;
    }
    on ePong do { assert false, "pong before any ping was planned"; }
  }

  // Hot: the run must not end here. A pong still owed is a failure.
  hot state Waiting {
    on ePong do (p: (count: int)) {
      ponged = ponged + 1;
      assert p.count == ponged, format("pong {0} reported count {1}", ponged, p.count);
      if (ponged == planned) {
        goto AllAnswered;
      }
    }
  }

  state AllAnswered {
    on ePong do (p: (count: int)) {
      assert false, format("pong with count {0} answers no planned ping", p.count);
    }
  }
}
