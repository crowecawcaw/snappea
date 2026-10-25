event eRead: Publisher;
event eVersion: (version: int, record: int);
event eTryPublish: (requester: Publisher, expected: int, evidence: int, baseRecord: int, record: int);
event eConflict;
event ePublished: (version: int, record: int);
event eRecorded: (version: int, evidence: int, baseRecord: int, record: int);

spec EvidenceMatchesCurrentVersion observes eRecorded, ePublished {
  var currentVersion: int;
  var currentRecord: int;
  start state Watching {
    entry {
      currentVersion = 0;
      currentRecord = 0;
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

      send input.store, eRead, this;
      receive { case eVersion: (initial: (version: int, record: int)) {
        snapshot = initial;
        expected = snapshot.version;
        baseRecord = snapshot.record;
        candidate = snapshot.record + 1;
        evidence = snapshot.version;
      } }
      send input.store, eTryPublish,
        (requester = this, expected = expected, evidence = evidence,
          baseRecord = baseRecord, record = candidate);
      receive {
        case ePublished: (published: (version: int, record: int)) { goto Done; }
        case eConflict: {
          // The retry bug: the publisher gets a fresh snapshot and advances
          // only the CAS expectation. Candidate and evidence remain stale.
          send input.store, eRead, this;
          receive { case eVersion: (fresh: (version: int, record: int)) {
            expected = fresh.version;
          } }
          send input.store, eTryPublish,
            (requester = this, expected = expected, evidence = evidence,
              baseRecord = baseRecord, record = candidate);
          receive { case ePublished: (published: (version: int, record: int)) { goto Done; } }
        }
      }
    }
  }
  state Done {}
}

machine BrokenMain {
  start state Init {
    entry {
      var store: Store;
      store = new Store();
      new Publisher((store = store,));
      new Publisher((store = store,));
    }
  }
}

test tcStaleRetry [main=BrokenMain]:
  assert EvidenceMatchesCurrentVersion in { BrokenMain, Store, Publisher };
