"""Private histories for checking QueueContract against independent traces."""


def _event(name, **fields):
    return {"event": name, "payload": fields}


def cases():
    return [
        ("fifo_success", True, [
            _event("ePut", request_id="p1", item=1, accepted=True),
            _event("ePut", request_id="p2", item=2, accepted=True),
            _event("eTake", request_id="t1", item=1, found=True),
            _event("eTake", request_id="t2", item=2, found=True),
            _event("eSnapshot", items=[]),
        ]),
        ("full_and_memoized_rejection", True, [
            _event("ePut", request_id="a", item=1, accepted=True),
            _event("ePut", request_id="b", item=2, accepted=True),
            _event("ePut", request_id="negative", item=3, accepted=False),
            _event("eSnapshot", items=[1, 2]),
            _event("eTake", request_id="t", item=1, found=True),
            _event("ePut", request_id="negative", item=3, accepted=False),
            _event("eSnapshot", items=[2]),
        ]),
        ("put_retry_after_take", True, [
            _event("ePut", request_id="same", item=7, accepted=True),
            _event("eTake", request_id="remove", item=7, found=True),
            _event("ePut", request_id="same", item=7, accepted=True),
            _event("eSnapshot", items=[]),
        ]),
        ("empty_take_retry_after_put", True, [
            _event("eTake", request_id="empty", item=-1, found=False),
            _event("ePut", request_id="later", item=8, accepted=True),
            _event("eTake", request_id="empty", item=-1, found=False),
            _event("eSnapshot", items=[8]),
        ]),
        ("accepts_over_capacity", False, [
            _event("ePut", request_id="a", item=1, accepted=True),
            _event("ePut", request_id="b", item=2, accepted=True),
            _event("ePut", request_id="c", item=3, accepted=True),
            _event("eSnapshot", items=[1, 2, 3]),
        ]),
        ("takes_newest_first", False, [
            _event("ePut", request_id="a", item=1, accepted=True),
            _event("ePut", request_id="b", item=2, accepted=True),
            _event("eTake", request_id="t", item=2, found=True),
        ]),
        ("put_retry_applies_twice", False, [
            _event("ePut", request_id="same", item=5, accepted=True),
            _event("eTake", request_id="remove", item=5, found=True),
            _event("ePut", request_id="same", item=5, accepted=True),
            _event("eSnapshot", items=[5]),
        ]),
        ("take_retry_removes_later_value", False, [
            _event("eTake", request_id="empty", item=-1, found=False),
            _event("ePut", request_id="later", item=8, accepted=True),
            _event("eTake", request_id="empty", item=8, found=True),
            _event("eSnapshot", items=[]),
        ]),
    ]
