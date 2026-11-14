/*
 * Immutable staging and root-CAS publication. Read README.md before this model.
 * Store persists three separate staging operations and one atomic root update.
 * Publisher drops local progress at one chosen boundary, rereads after recovery,
 * and retains the request ID. Two clients also race on the root version.
 * Run: snappea check Publication.p --timeout 30s --deny mutation --json
 * Then: python -m pip install pysnappea pytest; python -m pytest -q
 */
type Record = (id: int, generation: int);
type Candidate = (total: int, outcomes: map[int, int]);
type Snapshot = (version: int, content: Candidate);
event eRead: machine;
event eSnapshot: Snapshot;
event eStage: (client: machine, part: int, record: Record, content: Candidate);
event eStaged;
event eTry: (client: machine, expected: int, record: Record, id: int);
event eAnswer: bool;
event eStarted: (id: int, crash: int);
event eCrashed: (id: int, point: int);
event eReloaded: int;
event eStored: (part: int, record: Record, value: int);
event eCommitted: (id: int, record: Record, headA: int, headB: int);
event eReturned: (id: int, result: int);
event eFinished;
event eDone;
event ePaused;
event eContinue;

spec PublicationContract observes eStarted, eCrashed, eReloaded, eStored, eCommitted, eReturned, eFinished {
  var started: set[int];
  var faults: map[int, int];
  var crashed: set[int];
  var reload: set[int];
  var a: map[Record, int];
  var b: map[Record, int];
  var manifests: set[Record];
  var outcomes: map[int, int];
  var returned: set[int];
  var total: int;
  start hot state Running {
    on eStarted do (p: (id: int, crash: int)) {
      assert !(p.id in started); started += (p.id); faults[p.id] = p.crash;
    }
    on eCrashed do (p: (id: int, point: int)) {
      assert p.id in faults;
      assert !(p.id in crashed), "driver exceeded one crash per client";
      assert p.point == faults[p.id], "wrong crash boundary";
      if (p.point == 4) { assert p.id in outcomes, "lost response without a committed root"; }
      crashed += (p.id);
      reload += (p.id);
    }
    on eReloaded do (id: int) { reload -= (id); }
    on eStored do (p: (part: int, record: Record, value: int)) {
      assert p.part >= 0 && p.part < 3, "invalid staging part";
      assert !(p.record.id in reload), "continued volatile work after a crash";
      if (p.part == 0) { a[p.record] = p.value; }
      else {
        if (p.part == 1) { b[p.record] = p.value; }
        else {
          assert p.record in a && p.record in b, "manifest before durable head blobs";
          assert a[p.record] == p.value && b[p.record] == p.value;
          manifests += (p.record);
        }
      }
    }
    on eCommitted do (p: (id: int, record: Record, headA: int, headB: int)) {
      assert p.id in started;
      assert !(p.id in outcomes), "request applied twice after restart";
      assert p.record in manifests, "root points to an unstaged manifest";
      total = total + 1;
      assert p.headA == total && p.headB == total, "stale or split publication";
      outcomes[p.id] = total;
    }
    on eReturned do (p: (id: int, result: int)) {
      assert p.id in outcomes;
      assert !(p.id in reload), "returned before recovering durable state";
      if (faults[p.id] > 0) { assert p.id in crashed, "requested crash never happened"; }
      assert p.result == outcomes[p.id], "retry changed its original result";
      returned += (p.id);
    }
    on eFinished do {
      assert sizeof(started) == 2 && sizeof(returned) == 2;
      assert total == 2;
      goto Complete;
    }
  }
  state Complete { }
}

machine Store {
  var version: int;
  var root: Record;
  var a: map[Record, int];
  var b: map[Record, int];
  var manifests: map[Record, Candidate];
  start state Ready {
    entry { manifests[default(Record)] = default(Candidate); }
    on eRead do (client: machine) {
      send client, eSnapshot, (version = version, content = manifests[root]);
    }
    on eStage do (p: (client: machine, part: int, record: Record, content: Candidate)) {
      if (p.part == 0) {
        a[p.record] = p.content.total;
        announce eStored, (part = 0, record = p.record, value = a[p.record]);
      } else {
        if (p.part == 1) {
          b[p.record] = p.content.total;
          announce eStored, (part = 1, record = p.record, value = b[p.record]);
        } else {
          manifests[p.record] = p.content;
          announce eStored, (part = 2, record = p.record, value = manifests[p.record].total);
        }
      }
      send p.client, eStaged;
    }
    on eTry do (p: (client: machine, expected: int, record: Record, id: int)) {
      if (p.expected == version) {
        root = p.record;
        version = version + 1;
        announce eCommitted, (id = p.id, record = root, headA = a[root], headB = b[root]);
        send p.client, eAnswer, true;
      } else { send p.client, eAnswer, false; }
    }
  }
}

machine Publisher {
  start state Run {
    entry (input: (store: Store, coordinator: machine, id: int, crash: int, hold: bool)) {
      var snapshot: Snapshot;
      var candidate: Candidate;
      var record: Record;
      var part: int;
      var crashed: bool;
      var held: bool;
      var abandoned: bool;
      var success: bool;
      var finished: bool;
      var result: int;
      announce eStarted, (id = input.id, crash = input.crash);
      while (!finished) {
        // Recovery discards candidate/progress and begins with a fresh durable root.
        send input.store, eRead, this;
        receive { case eSnapshot: (p: Snapshot) { snapshot = p; } }
        announce eReloaded, input.id;
        if (input.hold && !held) {
          held = true;
          send input.coordinator, ePaused;
          receive { case eContinue: { } }
        }
        if (input.id in snapshot.content.outcomes) {
          result = snapshot.content.outcomes[input.id];
          finished = true;
        } else {
          candidate = snapshot.content;
          candidate.total = candidate.total + 1;
          candidate.outcomes[input.id] = candidate.total;
          record = (id = input.id, generation = candidate.total);
          part = 0;
          abandoned = false;
          while (part < 3 && !abandoned) {
            send input.store, eStage, (client = this, part = part, record = record, content = candidate);
            receive { case eStaged: { } }
            part = part + 1;
            if (!crashed && input.crash == part) {
              announce eCrashed, (id = input.id, point = part);
              crashed = true; abandoned = true;
            }
          }
          if (!abandoned) {
            send input.store, eTry, (client = this, expected = snapshot.version, record = record, id = input.id);
            receive { case eAnswer: (ok: bool) { success = ok; } }
            if (success && input.crash == 4) {
              announce eCrashed, (id = input.id, point = 4);
            }
            // Always recover the durable outcome after a CAS attempt. This also
            // handles the case where its successful response was lost.
          }
        }
      }
      announce eReturned, (id = input.id, result = result);
      send input.coordinator, eDone;
    }
  }
}

// snappea: driver
machine Scenario {
  start state Run {
    entry (fault: int) {
      var store: Store;
      var first: Publisher;
      store = new Store();
      if (fault == 5) {
        // Force a stale root before the first publisher stages its candidate.
        first = new Publisher((store = store, coordinator = this, id = 0, crash = 4, hold = true));
        receive { case ePaused: { } }
        new Publisher((store = store, coordinator = this, id = 1, crash = 0, hold = false));
        receive { case eDone: { } }
        send first, eContinue;
        receive { case eDone: { } }
      } else {
        first = new Publisher((store = store, coordinator = this, id = 0, crash = fault, hold = false));
        if (fault == 0) {
          new Publisher((store = store, coordinator = this, id = 1, crash = 0, hold = false));
          receive { case eDone: { } }
          receive { case eDone: { } }
        } else {
          receive { case eDone: { } }
          new Publisher((store = store, coordinator = this, id = 1, crash = 0, hold = false));
          receive { case eDone: { } }
        }
      }
      announce eFinished;
    }
  }
}

machine NoCrash {
  start state Run { entry { new Scenario(0); } }
}
test tcNoCrash [main=NoCrash]: assert PublicationContract in { NoCrash, Scenario, Store, Publisher };

machine AfterHeadA {
  start state Run { entry { new Scenario(1); } }
}
test tcAfterHeadA [main=AfterHeadA]: assert PublicationContract in { AfterHeadA, Scenario, Store, Publisher };

machine AfterHeadB {
  start state Run { entry { new Scenario(2); } }
}
test tcAfterHeadB [main=AfterHeadB]: assert PublicationContract in { AfterHeadB, Scenario, Store, Publisher };

machine AfterManifest {
  start state Run { entry { new Scenario(3); } }
}
test tcAfterManifest [main=AfterManifest]: assert PublicationContract in { AfterManifest, Scenario, Store, Publisher };

machine AfterRoot {
  start state Run { entry { new Scenario(4); } }
}
test tcAfterRoot [main=AfterRoot]: assert PublicationContract in { AfterRoot, Scenario, Store, Publisher };

machine ConflictThenCrash {
  start state Run { entry { new Scenario(5); } }
}
test tcConflictThenCrash [main=ConflictThenCrash]: assert PublicationContract in { ConflictThenCrash, Scenario, Store, Publisher };
