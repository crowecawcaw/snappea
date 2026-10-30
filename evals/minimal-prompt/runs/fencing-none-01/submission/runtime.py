"""Shared storage: independent records by key. Each key has atomic load and
compare-and-swap; there are no transactions across keys."""

from copy import deepcopy
from threading import Lock


class Store:
    def __init__(self):
        self.__records = {}
        self.__lock = Lock()

    def load(self, key):
        """Return (version, value) for key; (0, None) if never written."""
        with self.__lock:
            return deepcopy(self.__records.get(key, (0, None)))

    def compare_swap(self, key, version, value):
        """Store value if key is still at version; return whether it did."""
        with self.__lock:
            if self.__records.get(key, (0, None))[0] != version:
                return False
            self.__records[key] = (version + 1, deepcopy(value))
            return True
