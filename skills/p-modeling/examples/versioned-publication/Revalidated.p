event ePublisherStarted: machine;
event ePublisherDone: machine;
event eRead: Publisher;
event eVersion: (version: int, record: int);
event eTryPublish: (requester: Publisher, expected: int, evidence: int, baseRecord: int, record: int);
event eConflict;
event ePublished: (version: int, record: int);
event eRecorded: (version: int, evidence: int, baseRecord: int, record: int);

// The monitor tracks the committed version and record independently of Store.
// ePublished is observed at send, before arbitrary client delivery delay.
// Each eRecorded is a real storage transition; a client-supplied valid flag
// would merely repeat the implementation decision instead of checking it.
spec EvidenceMatchesCurrentVersion observes eRecorded, ePublished, ePublisherStarted, ePublisherDone {
  var currentVersion: int;
  var currentRecord: int;
  var started: set[machine];
  var finished: set[machine];
  // Safety alone permits all publishers to stop. This bounded experiment also
  // requires two real clients to return: no failures, two one-shot writers.
  start hot state Watching {
    entry {
      currentVersion = 0;
      currentRecord = 0;
    }
    on ePublisherStarted do (client: machine) {
      assert !(client in started), "publisher started twice";
      started += (client);
      assert sizeof(started) <= 2, "driver exceeds the two-publisher bound";
    }
    on ePublisherDone do (client: machine) {
      assert client in started, "completion without a started publisher";
      assert !(client in finished), "publisher completed twice";
      finished += (client);
      if (sizeof(finished) == 2) {
        assert currentVersion == 2, "both successful publications must be visible";
        goto Complete;
      }
    }
    on eRecorded do (p: (version: int, evidence: int, baseRecord: int, record: int)) {
      assert p.version == currentVersion, "publication version does not match monitor";
      assert p.evidence == currentVersion,
        format("record {0} was validated for version {1}, current version is {2}",
          p.record, p.evidence, currentVersion);
      assert p.baseRecord == currentRecord,
        format("record {0} was prepared from record {1}, current record is {2}",
          p.record, p.baseRecord, currentRecord);
      assert p.record == currentRecord + 1,
        format("record {0} is not the next configuration after {1}", p.record, currentRecord);
      currentVersion = currentVersion + 1;
      currentRecord = p.record;
    }
    on ePublished do (p: (version: int, record: int)) {
      assert p.version == currentVersion, "published version did not follow its pointer update";
      assert p.record == currentRecord, "published pointer does not name the announced record";
    }
  }
  state Complete { }
}

machine Store {
  var activeVersion: int;
  var activeRecord: int;

  start state Serving {
    entry {
      activeVersion = 0;
      activeRecord = 0;
    }
    on eRead do (publisher: Publisher) {
      send publisher, eVersion, (version = activeVersion, record = activeRecord);
    }
    on eTryPublish do (p: (requester: Publisher, expected: int, evidence: int, baseRecord: int, record: int)) {
      if (p.expected == activeVersion) {
        // The candidate is immutable and already prepared. This one transition
        // advances the version and moves the active pointer to that candidate.
        announce eRecorded, (version = activeVersion, evidence = p.evidence,
          baseRecord = p.baseRecord, record = p.record);
        activeVersion = activeVersion + 1;
        activeRecord = p.record;
        send p.requester, ePublished, (version = activeVersion, record = activeRecord);
      } else {
        send p.requester, eConflict;
      }
    }
  }
}

machine Publisher {
  start state Run {
    entry (input: (store: Store)) {
      var expected: int;
      var evidence: int;
      var baseRecord: int;
      var candidate: int;
      var snapshot: (version: int, record: int);

      announce ePublisherStarted, this;
      send input.store, eRead, this;
      receive { case eVersion: (initial: (version: int, record: int)) {
        snapshot = initial;
        expected = snapshot.version;
        baseRecord = snapshot.record;
        candidate = snapshot.record + 1;
        // Model validation: this rollout accepts exactly the next generation.
        // The evidence identifies the version used to derive and validate it.
        evidence = snapshot.version;
      } }
      send input.store, eTryPublish,
        (requester = this, expected = expected, evidence = evidence,
          baseRecord = baseRecord, record = candidate);
      receive {
        case ePublished: (published: (version: int, record: int)) { goto Done; }
        case eConflict: {
          // The failed CAS invalidates this snapshot. Re-read, recompute the
          // candidate from the new active record, and validate that candidate.
          // One retry suffices here only because there are exactly two one-shot
          // publishers: the winner has no further write. More writers require
          // a retry loop and a stated contention/fairness assumption.
          send input.store, eRead, this;
          receive { case eVersion: (fresh: (version: int, record: int)) {
            snapshot = fresh;
            expected = snapshot.version;
            baseRecord = snapshot.record;
            candidate = snapshot.record + 1;
            evidence = snapshot.version;
          } }
          send input.store, eTryPublish,
            (requester = this, expected = expected, evidence = evidence,
              baseRecord = baseRecord, record = candidate);
          receive { case ePublished: (published: (version: int, record: int)) { goto Done; } }
        }
      }
    }
  }
  state Done {
    entry { announce ePublisherDone, this; }
  }
}

machine Main {
  start state Init {
    entry {
      var store: Store;
      store = new Store();
      new Publisher((store = store,));
      new Publisher((store = store,));
    }
  }
}

test tcRevalidatedRetry [main=Main]:
  assert EvidenceMatchesCurrentVersion in { Main, Store, Publisher };
