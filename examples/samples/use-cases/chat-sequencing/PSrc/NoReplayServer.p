// A plausible broken design, kept as a negative control: the server pushes live
// messages but keeps no log to replay, so a resume only gets messages posted after it.
// A message lost with a dropped connection is never displayed by that client.
// tcBugNoReplay shows the progress spec catching it.
machine NoReplayServer {
  var next: int;
  var conns: set[machine];

  start state Serving {
    on ePost do (p: (msg: int)) {
      var c: machine;
      next = next + 1;
      announce eSequenced, (sn = next, msg = p.msg);
      foreach (c in conns) {
        send c, eDeliver, (sn = next, msg = p.msg);
      }
    }
    // BUG: `after` is ignored; nothing is replayed.
    on eSubscribe do (s: (conn: machine, after: int)) {
      conns += (s.conn);
    }
  }
}
