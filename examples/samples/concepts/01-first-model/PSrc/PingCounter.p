// The design: clients ping a shared counter service, one ping at a time each, and the
// counter replies with how many pings it has counted so far.

event ePing: (client: Client);
event ePong: (count: int);

machine Counter {
  var count: int;

  start state Serving {
    on ePing do (p: (client: Client)) {
      count = count + 1;
      send p.client, ePong, (count = count,);
    }
  }
}

machine Client {
  var counter: Counter;
  var pings: int;
  var pinged: int;

  start state Init {
    entry (input: (counter: Counter, pings: int)) {
      counter = input.counter;
      pings = input.pings;
      goto Pinging;
    }
  }

  // Each entry sends the next ping; the pong decides whether to ping again.
  state Pinging {
    entry {
      pinged = pinged + 1;
      send counter, ePing, (client = this,);
    }
    on ePong do {
      if (pinged < pings) {
        goto Pinging;
      }
    }
  }
}
