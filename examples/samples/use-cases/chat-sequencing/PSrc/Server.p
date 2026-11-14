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

event Post: (msg: int);
event Subscribe: (conn: machine, client: int, after: int);
// Server -> connection -> client.
event Deliver: (sn: int, msg: int);
// Server to itself: the replay step of a subscription.
event Replay: (conn: machine, client: int, after: int);

machine Server {
  var log: seq[int];
  var conns: set[machine];

  start state Serving {
    on Post do (p: (msg: int)) {
      var c: machine;
      // commit point: the append to the room log
      log += (sizeof(log), p.msg);
      announce Sequenced, (sn = sizeof(log), msg = p.msg);
      foreach (c in conns) {
        send c, Deliver, (sn = sizeof(log), msg = p.msg);
      }
    }
    on Subscribe do (s: (conn: machine, client: int, after: int)) {
      // commit point: the connection joins the live set
      conns += (s.conn);
      announce Subscribed, (client = s.client, after = s.after);
      send this, Replay, s;
    }
    on Replay do (r: (conn: machine, client: int, after: int)) {
      var i: int;
      // Sequence numbers start at 1: log[i] has seq i + 1.
      i = r.after;
      // commit point: the log read that starts the replay
      announce Replayed, (client = r.client, after = r.after, first = i + 1, last = sizeof(log));
      while (i < sizeof(log)) {
        send r.conn, Deliver, (sn = i + 1, msg = log[i]);
        i = i + 1;
      }
    }
  }
}
