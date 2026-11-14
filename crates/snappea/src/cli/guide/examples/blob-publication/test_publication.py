"""Force protocol boundaries on real durable state, including lost CAS and response."""
import pytest

from publication import BlobStore, Publisher


class Crash(Exception):
    pass


@pytest.mark.parametrize('point', [1, 2, 3, 4])
def test_restart_after_each_durable_operation(point):
    store = BlobStore()
    writes = 0

    def crash(operation):
        nonlocal writes
        if operation != 'read-root':
            writes += 1
            if writes == point:
                raise Crash()

    store.hook = crash
    with pytest.raises(Crash):
        Publisher(store).publish('first')
    assert writes == point  # prove the requested crash actually happened
    store.hook = None
    assert Publisher(store).heads() == ([1, 1] if point == 4 else [0, 0])
    assert Publisher(store).publish('first') == 1
    assert Publisher(store).publish('second') == 2
    # An old response is not the current state; replay must not restore the old root.
    assert Publisher(store).publish('first') == 1
    assert Publisher(store).heads() == [2, 2]


def test_staged_writes_are_not_visible_until_root_cas():
    store = BlobStore()
    snapshots = []

    def inspect(operation):
        if operation == 'stage':
            snapshots.append(Publisher(store).heads())

    store.hook = inspect
    assert Publisher(store).publish('first') == 1
    store.hook = None
    assert snapshots == [[0, 0], [0, 0], [0, 0]]
    assert Publisher(store).heads() == [1, 1]


def test_losing_root_cas_rebuilds_both_heads_and_the_outcome_memo():
    store = BlobStore()
    fired = 0

    def compete(operation):
        nonlocal fired
        if operation == 'read-root':
            store.hook = None
            fired += 1
            assert Publisher(store).publish('other') == 1

    store.hook = compete
    assert Publisher(store).publish('mine') == 2
    assert fired == 1
    assert Publisher(store).heads() == [2, 2]
    assert Publisher(store).publish('other') == 1
    assert Publisher(store).publish('mine') == 2


def test_record_limit_fails_before_publication():
    store = BlobStore(max_record_bytes=100)
    with pytest.raises(ValueError, match='exceeds'):
        Publisher(store).publish('first')
    assert Publisher(store).heads() == [0, 0]
    assert store.root is None
