"""Tiny reservation store with one atomic conditional operation."""

from dataclasses import dataclass
from threading import Lock

from snappea import emit


@dataclass(frozen=True)
class Snapshot:
    available: int
    version: int


class ReservationStore:
    def __init__(self, available: int):
        if available < 0:
            raise ValueError("available stock must be nonnegative")
        self.initial = available
        self.available = available
        self.version = 0
        self.reservations = {}
        self._lock = Lock()
        emit("eOpened", available=available)

    def read(self):
        """Read stock and its version. This operation does not reserve anything."""
        return Snapshot(self.available, self.version)

    def reserve_if_unchanged(self, order, units, snapshot):
        """A database-style conditional write: check and update are one operation."""
        if units <= 0:
            raise ValueError("units must be positive")
        with self._lock:
            if snapshot.version != self.version or snapshot.available != self.available:
                return False
            if self.available < units or order in self.reservations:
                return False
            self._commit(order, units, self.available - units)
        self._announce_reservation(order, units)
        return True

    def reserve_from_read_broken(self, order, units, snapshot):
        """Broken read/check/write: the final write does not validate its read."""
        if units <= 0:
            raise ValueError("units must be positive")
        if snapshot.available < units:
            return False
        # Both clients can pass the check using the same old snapshot. Each writes
        # the same new stock value, while both reservation records are committed.
        with self._lock:
            self._commit(order, units, snapshot.available - units)
        self._announce_reservation(order, units)
        return True

    def _commit(self, order, units, new_available):
        self.available = new_available
        self.version += 1
        self.reservations[order] = units

    @staticmethod
    def _announce_reservation(order, units):
        # Emitted only after the application state has changed successfully.
        emit("eReserved", order=order, units=units)


def assert_inventory_invariant(store):
    """Independent oracle: derive stock from committed reservation records."""
    reserved = sum(store.reservations.values())
    assert reserved <= store.initial, (
        f"reserved {reserved} units from initial stock {store.initial}"
    )
    assert store.available == store.initial - reserved, (
        f"stored available={store.available}, expected {store.initial - reserved}"
    )
