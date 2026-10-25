event ePrepareReq: machine;
event ePrepareResp: (token: int, watermark: int);
event eApplyReq: (caller: machine, token: int);
event eApplyResp: bool;
event eWriteReq: (caller: machine, item: string, value: int);
event eDeleteReq: (caller: machine, item: string);
event eMutationResp: int;
event eInitBackfill: (store: machine, driver: machine);
event eInitWriter: (store: machine, driver: machine);
event eStageReady: int;
event eRunLive;
event eApplyNow;
event eWorkerDone;

machine MigrationStore {
  var logicalValue: map[string, int];
  var logicalRevision: map[string, int];
  var logicalDeleted: set[string];
  var shadowValue: map[string, int];
  var shadowRevision: map[string, int];
  var shadowDeleted: set[string];
  var snapshotValue: map[string, int];
  var snapshotRevision: map[string, int];
  var snapshotDeleted: set[string];
  var revision: int;
  var token: int;
  var activeToken: int;

  start state Serving {
    entry {
      logicalValue = default(map[string, int]); logicalRevision = default(map[string, int]);
      logicalDeleted = default(set[string]); shadowValue = default(map[string, int]);
      shadowRevision = default(map[string, int]); shadowDeleted = default(set[string]);
      snapshotValue = default(map[string, int]); snapshotRevision = default(map[string, int]);
      snapshotDeleted = default(set[string]);
      logicalValue["a"] = 10; logicalValue["b"] = 20;
      logicalRevision["a"] = 0; logicalRevision["b"] = 0;
      revision = 0; token = 0; activeToken = -1;
    }

    on ePrepareReq do (caller: machine) {
      token = token + 1; activeToken = token;
      snapshotValue = logicalValue; snapshotRevision = logicalRevision;
      snapshotDeleted = logicalDeleted;
      announce ePrepare, (token = token, watermark = revision);
      send caller, ePrepareResp, (token = token, watermark = revision);
    }

    on eApplyReq do (p: (caller: machine, token: int)) {
      if (p.token == activeToken) {
        if (!("a" in shadowRevision) || snapshotRevision["a"] > shadowRevision["a"]) {
          shadowRevision["a"] = snapshotRevision["a"];
          if ("a" in snapshotDeleted) { shadowDeleted += ("a"); shadowValue["a"] = 0; }
          else { shadowDeleted -= ("a"); shadowValue["a"] = 10 * snapshotValue["a"] + 1; }
        }
        if (!("b" in shadowRevision) || snapshotRevision["b"] > shadowRevision["b"]) {
          shadowRevision["b"] = snapshotRevision["b"];
          if ("b" in snapshotDeleted) { shadowDeleted += ("b"); shadowValue["b"] = 0; }
          else { shadowDeleted -= ("b"); shadowValue["b"] = 10 * snapshotValue["b"] + 1; }
        }
        announce eApply, (token = p.token, applied = true);
        send p.caller, eApplyResp, true;
      } else {
        announce eApply, (token = p.token, applied = false);
        send p.caller, eApplyResp, false;
      }
    }

    on eWriteReq do (p: (caller: machine, item: string, value: int)) {
      revision = revision + 1;
      logicalValue[p.item] = p.value; logicalRevision[p.item] = revision;
      logicalDeleted -= (p.item);
      shadowValue[p.item] = 10 * p.value + 1; shadowRevision[p.item] = revision;
      shadowDeleted -= (p.item);
      announce eWrite, (item = p.item, value = p.value, revision = revision);
      send p.caller, eMutationResp, revision;
    }

    on eDeleteReq do (p: (caller: machine, item: string)) {
      revision = revision + 1;
      logicalValue[p.item] = 0; logicalRevision[p.item] = revision;
      logicalDeleted += (p.item); shadowValue[p.item] = 0;
      shadowRevision[p.item] = revision; shadowDeleted += (p.item);
      announce eDelete, (item = p.item, revision = revision);
      send p.caller, eMutationResp, revision;
    }
  }
}

machine BackfillWorker {
  var store: machine;
  var driver: machine;
  var token: int;
  start state WaitingForInit {
    on eInitBackfill do (p: (store: machine, driver: machine)) {
      store = p.store; driver = p.driver; send store, ePrepareReq, this;
      goto WaitingForSnapshot;
    }
  }
  state WaitingForSnapshot {
    on ePrepareResp do (p: (token: int, watermark: int)) {
      token = p.token; send driver, eStageReady, token; goto WaitingToApply;
    }
  }
  state WaitingToApply {
    on eApplyNow do { send store, eApplyReq, (caller = this, token = token); goto WaitingForApply; }
  }
  state WaitingForApply {
    on eApplyResp do (p: bool) { send driver, eWorkerDone; goto Done; }
  }
  state Done {}
}

machine LiveWriter {
  var store: machine;
  var driver: machine;
  start state WaitingForInit {
    on eInitWriter do (p: (store: machine, driver: machine)) {
      store = p.store; driver = p.driver; goto WaitingForRun;
    }
  }
  state WaitingForRun {
    on eRunLive do { send store, eWriteReq, (caller = this, item = "a", value = 31); goto WaitingForWrite; }
  }
  state WaitingForWrite {
    on eMutationResp do (p: int) {
      send store, eDeleteReq, (caller = this, item = "b"); goto WaitingForDelete;
    }
  }
  state WaitingForDelete {
    on eMutationResp do (p: int) { send driver, eWorkerDone; goto Done; }
  }
  state Done {}
}

machine MigrationCase {
  var store: machine;
  var backfill: machine;
  var writer: machine;
  var completed: int;
  start state Init {
    entry {
      store = new MigrationStore(); backfill = new BackfillWorker(); writer = new LiveWriter();
      send backfill, eInitBackfill, (store = store, driver = this);
      send writer, eInitWriter, (store = store, driver = this);
    }
    on eStageReady do (p: int) {
      send writer, eRunLive; send backfill, eApplyNow; goto WaitingForWorkers;
    }
  }
  state WaitingForWorkers {
    on eWorkerDone do {
      completed = completed + 1;
      if (completed == 2) {
        announce eRead, (item = "a", present = true, value = 31);
        announce eRead, (item = "b", present = false, value = 0);
      }
    }
  }
}
