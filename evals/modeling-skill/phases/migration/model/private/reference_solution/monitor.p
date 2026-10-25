spec MigrationContract observes ePrepare, eApply, eWrite, eDelete, eCutover, eRead, eNewRow, eReady {
  var logicalValue: map[string, int];
  var logicalRevision: map[string, int];
  var logicalDeleted: set[string];
  var shadowValue: map[string, int];
  var shadowRevision: map[string, int];
  var shadowDeleted: set[string];
  var snapshotValue: map[string, int];
  var snapshotRevision: map[string, int];
  var snapshotDeleted: set[string];
  var snapshotToken: int;
  var nextToken: int;
  var lastRevision: int;
  var readyExpected: bool;
  var presentExpected: bool;
  var valueExpected: int;
  var rowRevision: int;
  var rowDeleted: bool;
  var rowEncoded: int;

  start state Watching {
    entry {
      logicalValue = default(map[string, int]); logicalRevision = default(map[string, int]);
      logicalDeleted = default(set[string]); shadowValue = default(map[string, int]);
      shadowRevision = default(map[string, int]); shadowDeleted = default(set[string]);
      snapshotValue = default(map[string, int]); snapshotRevision = default(map[string, int]);
      snapshotDeleted = default(set[string]);
      logicalValue["a"] = 10; logicalValue["b"] = 20;
      logicalRevision["a"] = 0; logicalRevision["b"] = 0;
      nextToken = 0; lastRevision = 0; snapshotToken = -1;
    }

    on ePrepare do (p: (token: int, watermark: int)) {
      assert p.token > nextToken, "prepare token must increase";
      nextToken = p.token;
      assert p.watermark == lastRevision, "prepare watermark must match current revision";
      snapshotValue = logicalValue; snapshotRevision = logicalRevision;
      snapshotDeleted = logicalDeleted; snapshotToken = p.token;
    }

    on eApply do (p: (token: int, applied: bool)) {
      assert p.applied == (p.token == snapshotToken), "only the current snapshot token can be applied";
      if (p.applied) {
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
      }
    }

    on eWrite do (p: (item: string, value: int, revision: int)) {
      assert p.revision > lastRevision, "write revision must increase";
      lastRevision = p.revision;
      logicalValue[p.item] = p.value; logicalRevision[p.item] = p.revision;
      logicalDeleted -= (p.item);
      shadowValue[p.item] = 10 * p.value + 1; shadowRevision[p.item] = p.revision;
      shadowDeleted -= (p.item);
    }

    on eDelete do (p: (item: string, revision: int)) {
      assert p.revision > lastRevision, "delete revision must increase";
      lastRevision = p.revision;
      logicalValue[p.item] = 0; logicalRevision[p.item] = p.revision;
      logicalDeleted += (p.item); shadowValue[p.item] = 0;
      shadowRevision[p.item] = p.revision; shadowDeleted += (p.item);
    }

    on eCutover do (p: (accepted: bool)) {
      readyExpected = true;
      if (!("a" in shadowRevision) || shadowRevision["a"] != logicalRevision["a"] ||
          ("a" in shadowDeleted) != ("a" in logicalDeleted) ||
          (!("a" in logicalDeleted) && shadowValue["a"] != 10 * logicalValue["a"] + 1)) { readyExpected = false; }
      if (!("b" in shadowRevision) || shadowRevision["b"] != logicalRevision["b"] ||
          ("b" in shadowDeleted) != ("b" in logicalDeleted) ||
          (!("b" in logicalDeleted) && shadowValue["b"] != 10 * logicalValue["b"] + 1)) { readyExpected = false; }
      assert p.accepted == readyExpected, "cutover must succeed exactly for a current complete shadow";
    }

    on eRead do (p: (item: string, present: bool, value: int)) {
      presentExpected = p.item in logicalRevision && !(p.item in logicalDeleted);
      valueExpected = 0;
      if (presentExpected) { valueExpected = logicalValue[p.item]; }
      assert p.present == presentExpected && p.value == valueExpected,
        "logical read disagrees with the authoritative table";
    }

    on eNewRow do (p: (item: string, revision: int, deleted: bool, encoded: int)) {
      rowRevision = -1; rowDeleted = false; rowEncoded = -1;
      if (p.item in shadowRevision) {
        rowRevision = shadowRevision[p.item]; rowDeleted = p.item in shadowDeleted;
        if (!rowDeleted) { rowEncoded = shadowValue[p.item]; }
      }
      assert p.revision == rowRevision && p.deleted == rowDeleted && p.encoded == rowEncoded,
        "shadow inspection disagrees with the merged projection";
    }

    on eReady do (p: (complete: bool)) {
      readyExpected = true;
      if (!("a" in shadowRevision) || shadowRevision["a"] != logicalRevision["a"] ||
          ("a" in shadowDeleted) != ("a" in logicalDeleted) ||
          (!("a" in logicalDeleted) && shadowValue["a"] != 10 * logicalValue["a"] + 1)) { readyExpected = false; }
      if (!("b" in shadowRevision) || shadowRevision["b"] != logicalRevision["b"] ||
          ("b" in shadowDeleted) != ("b" in logicalDeleted) ||
          (!("b" in logicalDeleted) && shadowValue["b"] != 10 * logicalValue["b"] + 1)) { readyExpected = false; }
      assert p.complete == readyExpected, "ready result disagrees with full-row equality";
    }
  }
}
