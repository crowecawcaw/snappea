"""Atomic, versioned, single-record storage used by the queue task."""

from copy import deepcopy
from threading import Lock


class Store:
    def __init__(self):
        self.__version = 0
        self.__record = None
        self.__lock = Lock()

    def load(self):
        with self.__lock:
            return self.__version, deepcopy(self.__record)

    def compare_swap(self, version, record):
        with self.__lock:
            if version != self.__version:
                return False
            self.__record = deepcopy(record)
            self.__version += 1
            return True
