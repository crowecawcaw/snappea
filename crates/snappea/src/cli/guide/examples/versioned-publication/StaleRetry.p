/*
 * File map (search these names):
 * EvidenceMatchesCurrentVersion = oracle; Store = atomic primitives;
 * Publisher = faulty stale retry; Main/test = two competing clients.
 *
 * NEGATIVE CONTROL: stale publication evidence reused after a CAS conflict.
 *
 * Read Revalidated.p for the correct design and full assumptions. This variant
 * changes only expected CAS version after losing; its old prepared record/evidence
 * remain tied to version 0. Another publisher already advanced the active pointer.
 * The independent monitor requires evidence/base/generation to match current state,
 * so version-0 evidence must not publish at version 1.
 *
 * Run: snappea check StaleRetry.p
 * Expected: exit 1 with "was validated for version"; mutation analysis is skipped
 * after the counterexample. A parse/lint/coverage error is not defect detection.
 * Both files model two clients, atomic storage reads/CAS, no crashes or message
 * loss; production needs matching atomic guarantees. See Revalidated.p for progress
 * obligations and why one retry is sufficient only for this bounded workload.
 */

event ePublisherStarted: machine;
event ePublisherDone: machine;
event eRead: Publisher;
event eVersion: (version: int, record: int);
event eTryPublish: (requester: Publisher, expected: int, evidence: int, baseRecord: int, record: int);
event eConflict;
event ePublished: (version: int, record: int);
event eRecorded: (version: int, evidence: int, baseRecord: int, record: int);

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
  state Done {
    entry { announce ePublisherDone, this; }
  }
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
