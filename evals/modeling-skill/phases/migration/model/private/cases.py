"""Independent public-operation histories for MigrationContract calibration."""


def cases():
    def e(name, **payload):
        return {"event": name, "payload": payload, "key": "migration", "timestamp": 0}

    def prep(token=1, watermark=0):
        return e("ePrepare", token=token, watermark=watermark)

    def apply(token=1, applied=True):
        return e("eApply", token=token, applied=applied)

    def write(item, value, revision):
        return e("eWrite", item=item, value=value, revision=revision)

    def delete(item, revision):
        return e("eDelete", item=item, revision=revision)

    def cutover(accepted):
        return e("eCutover", accepted=accepted)

    def read(item, present, value):
        return e("eRead", item=item, present=present, value=value)

    def row(item, revision, deleted, encoded):
        return e("eNewRow", item=item, revision=revision, deleted=deleted, encoded=encoded)

    def ready(value):
        return e("eReady", complete=value)

    histories = [
        ("valid_initial_read_and_gate", True, [read("a", True, 10), read("z", False, 0),
                                                ready(False), cutover(False)]),
        ("valid_stale_backfill_after_live_write_delete", True,
         [prep(), write("a", 31, 4), delete("b", 7), apply(),
          row("a", 4, False, 311), row("b", 7, True, -1),
          read("a", True, 31), read("b", False, 0), ready(True), cutover(True)]),
        ("valid_delete_then_recreate_during_backfill", True,
         [prep(), delete("a", 2), write("a", 8, 5), apply(),
          row("a", 5, False, 81), read("a", True, 8), ready(True)]),
        ("valid_new_key_created_during_backfill", True,
         [prep(), write("c", 6, 3), apply(), row("c", 3, False, 61),
          ready(True), cutover(True), write("b", 9, 6), read("b", True, 9), ready(True)]),
        ("valid_cutover_reads_and_later_changes", True,
         [prep(2, 0), apply(2), ready(True), cutover(True), read("b", True, 20),
          write("b", 25, 1), read("b", True, 25), delete("a", 3),
          read("a", False, 0), row("a", 3, True, -1), ready(True)]),
        ("valid_reprepare_supersedes_pending_token", True,
         [prep(1, 0), write("a", 2, 5), prep(4, 5), apply(1, False), apply(4, True),
          row("a", 5, False, 21), ready(True)]),
        ("valid_arbitrary_key_write_delete_recreate_across_backfill", True,
         [write("d", 4, 2), prep(3, 2), delete("d", 6), write("d", 9, 9), apply(3),
          row("d", 9, False, 91), read("d", True, 9), ready(True)]),
        ("invalid_stale_backfill_overwrites_live_write", False,
         [prep(), write("a", 31, 1), apply(), row("a", 0, False, 101)]),
        ("invalid_stale_backfill_resurrects_deleted_row", False,
         [prep(), delete("b", 1), apply(), row("b", 0, False, 201)]),
        ("invalid_premature_cutover", False, [ready(False), cutover(True)]),
        ("invalid_write_does_not_update_shadow", False,
         [write("a", 12, 1), row("a", 0, False, 101)]),
        ("invalid_cutover_with_initial_rows_missing", False,
         [ready(True), cutover(True)]),
        ("invalid_new_read_returns_encoded_value", False,
         [prep(), apply(), cutover(True), read("a", True, 101)]),
        ("invalid_revision_reuse", False, [write("a", 5, 1), delete("a", 1)]),
        ("invalid_watermark_does_not_match_prepared_snapshot", False,
         [prep(1, 1)]),
    ]
    for name, valid, events in histories:
        for timestamp, record in enumerate(events):
            record["timestamp"] = timestamp
        yield name, valid, events
