"""Private monitor histories; every input call ends with its complete output list."""


def event(name, **payload):
    return {"event": name, "payload": payload}


def add(ts, value, accepted, outputs=()):
    records = [event("eAdd", timestamp=ts, value=value, accepted=accepted)]
    records.extend(output(*record) for record in outputs)
    records.append(event("eEndCall"))
    return records


def watermark(value, outputs=()):
    records = [event("eWatermark", watermark=value)]
    records.extend(output(*record) for record in outputs)
    records.append(event("eEndCall"))
    return records


def output(start, end, total, revision, final):
    return event("eOutput", window_start=start, window_end=end, total=total,
                 revision=revision, final=final)


def cases():
    histories = [
        ("valid_half_open_late_correction", True,
         add(9, 4, True) + add(10, -2, True) +
         watermark(10, [(0, 10, 4, 0, False)]) +
         add(0, 3, True, [(0, 10, 7, 1, False)]) +
         watermark(15, [(0, 10, 7, 2, True)])),
        ("valid_exact_final_threshold_reject", True,
         add(10, 8, True) + watermark(25, [(10, 20, 8, 0, True)]) +
         add(10, 2, False)),
        ("valid_late_first_arrival", True,
         watermark(12) + add(1, -5, True, [(0, 10, -5, 0, False)]) +
         add(8, 5, True, [(0, 10, 0, 1, False)]) +
         watermark(15, [(0, 10, 0, 2, True)])),
        ("valid_two_windows_sorted_final", True,
         add(11, 2, True) + add(1, 3, True) +
         watermark(25, [(0, 10, 3, 0, True), (10, 20, 2, 0, True)])),
        ("valid_direct_jump_final_only", True,
         add(22, -3, True) + watermark(35, [(20, 30, -3, 0, True)])),
        ("valid_empty_windows_and_equal_watermark", True,
         watermark(10) + watermark(10) + add(20, 1, True) + watermark(30, [(20, 30, 1, 0, False)])),
        ("reject_boundary_wrong_bucket", False,
         add(10, 2, True, [(0, 10, 2, 0, False)])),
        ("reject_accept_at_final_threshold", False,
         add(1, 2, True) + watermark(15, [(0, 10, 2, 0, True)]) + add(5, 1, True)),
        ("reject_incorrect_negative_sum", False,
         add(1, -4, True) + watermark(10, [(0, 10, 4, 0, False)])),
        ("reject_missing_output_at_close", False,
         add(2, 5, True) + watermark(10)),
        ("reject_missing_late_correction", False,
         add(1, 4, True) + watermark(10, [(0, 10, 4, 0, False)]) + add(2, 3, True)),
        ("reject_revision_gap", False,
         add(1, 4, True) + watermark(10, [(0, 10, 4, 0, False)]) +
         add(3, 3, True, [(0, 10, 7, 2, False)])),
        ("reject_final_record_missing", False,
         add(2, 5, True) + watermark(10, [(0, 10, 5, 0, False)]) + watermark(15)),
        ("reject_backward_watermark", False,
         watermark(12) + watermark(11)),
        ("reject_unsorted_advance_outputs", False,
         add(11, 2, True) + add(1, 3, True) +
         watermark(25, [(10, 20, 2, 0, True), (0, 10, 3, 0, True)])),
    ]
    for name, valid, records in histories:
        for timestamp, record in enumerate(records):
            record["timestamp"] = timestamp
            record["key"] = "window-service"
        yield name, valid, records
