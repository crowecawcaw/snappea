// snappea: driver
machine Main {
  var service: machine;
  var completed: int;
  start state Run {
    entry {
      service = new ResourceManager();
      Call(2, 1, 2);
      Call(0, 1, 1);
      Call(0, 1, 2);
      Call(2, 1, 1);
      Call(2, 1, 2);
      Call(1, 1, 2);
      Call(2, 1, 1);
      Call(0, 2, 1);
      Call(2, 1, 1);
      Call(1, 2, 1);
      Call(2, 1, 1);
      Call(1, 1, 1);
      Call(0, 2, 1);
      Call(0, 2, 1);
      Call(1, 2, 1);
      Call(1, 1, 1);
      Call(2, 1, 1);
      Call(2, 1, 2);
      announce eFinished, (expected = completed,);
    }
  }
  fun Call(kind: int, client: int, resource: int) {
    send service, eCommand, (reply = this, kind = kind, client = client, resource = resource);
    receive {
      case eReply: (r: Result) {
        assert r.kind == kind && r.client == client && r.resource == resource;
        completed = completed + 1;
      }
    }
  }
}
