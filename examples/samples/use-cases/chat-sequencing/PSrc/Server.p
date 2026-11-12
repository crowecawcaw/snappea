// A chat room server. It assigns each message the room's next sequence number, keeps
// the room log, and pushes each message to every subscribed connection. A client that
// reconnects subscribes with "resume after seq N", and the server replays the log
// from N + 1 on the new connection before pushing new messages on it.
//
// Real atomic primitives, one handler each:
//   post:      seq + 1, append to the log, push to every subscribed connection
//   subscribe: replay the log after `after`, then add the connection
// A connection can drop at any moment; whatever it was carrying is lost.

event ePost: (msg: int);
event eSubscribe: (conn: machine, after: int);
// Server -> connection -> client.
event eDeliver: (sn: int, msg: int);

machine Server {
  var log: seq[int];
  var conns: set[machine];

  start state Serving {
    on ePost do (p: (msg: int)) {
      var c: machine;
      log += (sizeof(log), p.msg);
      announce eSequenced, (sn = sizeof(log), msg = p.msg);
      foreach (c in conns) {
        send c, eDeliver, (sn = sizeof(log), msg = p.msg);
      }
    }
    on eSubscribe do (s: (conn: machine, after: int)) {
      var i: int;
      // Sequence numbers start at 1: log[i] has seq i + 1.
      i = s.after;
      while (i < sizeof(log)) {
        send s.conn, eDeliver, (sn = i + 1, msg = log[i]);
        i = i + 1;
      }
      conns += (s.conn);
    }
  }
}
