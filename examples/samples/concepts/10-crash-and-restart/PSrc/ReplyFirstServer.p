// Negative control: a write-behind server. It replies at once and persists the new
// counter on its next turn, like a cache that flushes in the background. A crash between
// the reply and the flush loses a ticket the client already holds, and the restarted
// server hands it out again. tcBugReplyFirst shows the spec catching it.

// Internal: persist the counter now.
event Flush;

machine ReplyFirstServer {
  var storage: Storage;
  var owner: machine;
  var last: int;

  start state Recovering {
    entry (input: (storage: Storage, owner: machine)) {
      storage = input.storage;
      owner = input.owner;
      send storage, Read, this;
    }
    on ReadResponse do (highWater: int) {
      last = highWater;
      goto Serving;
    }
    defer Next;
    on Crash goto Crashed;
  }

  state Serving {
    on Next do (c: machine) {
      last = last + 1;
      // BUG: the reply goes out before the counter is durable.
      send c, Ticket, last;
      announce Issued, (client = c, ticket = last);
      send this, Flush;
      goto Flushing;
    }
    on Crash goto Crashed;
  }

  // The crash window: the client holds the ticket, storage does not know yet.
  state Flushing {
    on Flush do {
      send storage, Write, (server = this, value = last);
    }
    on WriteAck goto Serving;
    defer Next;
    on Crash goto Crashed;
  }

  state Crashed {
    entry {
      send owner, Exited;
      raise halt;
    }
  }
}
