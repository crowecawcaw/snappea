type Output = (window_start: int, window_end: int, total: int, revision: int, final: bool);

// snappea-lint: allow coverage -- Python observations cover focused boundary histories
// snappea-lint: allow unselected-spec -- this contract is selected by external observation logs
spec WindowContract observes eAdd, eWatermark, eOutput, eEndCall {
  var now: int;
  var totals: map[int, int];
  var emitted: map[int, bool];
  var finalised: map[int, bool];
  var revisions: map[int, int];
  var pending: seq[Output];
  var ws: int;
  var we: int;
  var accepted: bool;
  var nextRevision: int;
  var key: int;

  start state Watching {
    entry {
      now = 0;
      totals = default(map[int, int]);
      emitted = default(map[int, bool]);
      finalised = default(map[int, bool]);
      revisions = default(map[int, int]);
      pending = default(seq[Output]);
    }
    on eAdd do (p: (timestamp: int, value: int, accepted: bool)) {
      assert sizeof(pending) == 0, "previous output list was not closed";
      ws = (p.timestamp / 10) * 10;
      we = ws + 10;
      accepted = now < we + 5;
      assert p.accepted == accepted, "add acceptance disagrees with allowed lateness";
      if (accepted) {
        if (ws in totals) { totals[ws] = totals[ws] + p.value; }
        else {
          totals[ws] = p.value;
          emitted[ws] = false;
          finalised[ws] = false;
          revisions[ws] = -1;
        }
        if (emitted[ws]) {
          nextRevision = revisions[ws] + 1;
          revisions[ws] = nextRevision;
          pending += (sizeof(pending), (window_start = ws, window_end = we, total = totals[ws], revision = nextRevision, final = false));
        } else if (now >= we) {
          emitted[ws] = true;
          revisions[ws] = 0;
          pending += (sizeof(pending), (window_start = ws, window_end = we, total = totals[ws], revision = 0, final = false));
        }
      }
    }
    on eWatermark do (p: (watermark: int)) {
      assert sizeof(pending) == 0, "previous output list was not closed";
      assert p.watermark >= now, "watermark moved backwards";
      now = p.watermark;
      ws = 0;
      while (ws <= now) {
        key = ws;
        if (key in totals) {
          we = key + 10;
          if (!emitted[key] && now >= we) {
            emitted[key] = true;
            if (now >= we + 5) {
              finalised[key] = true;
              revisions[key] = 0;
              pending += (sizeof(pending), (window_start = key, window_end = we, total = totals[key], revision = 0, final = true));
            } else {
              revisions[key] = 0;
              pending += (sizeof(pending), (window_start = key, window_end = we, total = totals[key], revision = 0, final = false));
            }
          } else if (emitted[key] && !finalised[key] && now >= we + 5) {
            finalised[key] = true;
            nextRevision = revisions[key] + 1;
            revisions[key] = nextRevision;
            pending += (sizeof(pending), (window_start = key, window_end = we, total = totals[key], revision = nextRevision, final = true));
          }
        }
        ws = ws + 10;
      }
    }
    on eOutput do (p: Output) {
      assert sizeof(pending) > 0, "unexpected output record";
      assert p == pending[0], "output disagrees with required aggregate or revision";
      pending -= (0);
    }
    on eEndCall do {
      assert sizeof(pending) == 0, "call omitted one or more required output records";
    }
  }
}
