// A chat room server. It assigns each message the room's next sequence number, keeps
// the room log, and pushes each message to every subscribed connection. A client that
// reconnects subscribes with "resume after seq N". The server first adds the new
// connection to the live set (so nothing posted from now on is missed), then, as a
// separate step, replays the log from N + 1 up to its end at that moment. Messages
// posted in between go out both live and in the replay, and a live one can overtake
// the replay: the client sees duplicates and early arrivals, and orders by seq.
//
// Real atomic primitives, one handler each:
//   post:      seq + 1, append to the log, push to every subscribed connection
//   subscribe: add the connection to the live set
//   replay:    read the log after `after` and send it on the connection
// A connection can drop at any moment; whatever it was carrying is lost.

event ePost: (msg: int);
event eSubscribe: (conn: machine, after: int);
// Server -> connection -> client.
event eDeliver: (sn: int, msg: int);
// Server to itself: the replay step of a subscription.
event eReplay: (conn: machine, after: int);

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
      conns += (s.conn);
      send this, eReplay, s;
    }
    on eReplay do (r: (conn: machine, after: int)) {
      var i: int;
      // Sequence numbers start at 1: log[i] has seq i + 1.
      // snappea-lint: allow mutation:remove-statement -- replaying from the start only adds duplicates, which the client skips by seq; the contract does not cover bandwidth
      i = r.after;
      while (i < sizeof(log)) {
        send r.conn, eDeliver, (sn = i + 1, msg = log[i]);
        i = i + 1;
      }
    }
  }
}
