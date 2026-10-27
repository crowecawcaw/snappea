// snappea: driver
machine Main {
  var service: machine;
  var commands: seq[Command];
  var index: int;
  start state Init {
    entry {
      service = new Configuration();
      commands += (0, (id = 0, expected = 0, value = 10));
      commands += (1, (id = 0, expected = 0, value = 10));
      commands += (2, (id = 1, expected = 2, value = 30));
      commands += (3, (id = 1, expected = 2, value = 30));
      goto Next;
    }
  }
  state Next {
    entry {
      if (index < sizeof(commands)) {
        send service, eSet, (caller = this, command = commands[index]);
        goto Waiting;
      } else {
        announce eFinished;
        goto Done;
      }
    }
  }
  state Waiting {
    on eReply do {
      index = index + 1;
      goto Next;
    }
  }
  state Done { }
}
