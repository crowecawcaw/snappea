"""Small online schema migration service used by the observation evaluation."""

from copy import deepcopy


class Service:
    def __init__(self):
        self.__legacy = {
            "a": (0, False, 10),
            "b": (0, False, 20),
        }
        self.__shadow = {}
        self.__snapshots = {}
        self.__next_token = 0
        self.__revision = 0
        self.__cutover = False

    def prepare(self):
        self.__next_token += 1
        token = self.__next_token
        snapshot = deepcopy(self.__legacy)
        watermark = max((row[0] for row in snapshot.values()), default=0)
        self.__snapshots = {token: (watermark, snapshot)}
        return token, watermark

    def apply(self, token):
        if token not in self.__snapshots:
            return False
        _, snapshot = self.__snapshots[token]
        for key, (revision, deleted, value) in snapshot.items():
            current = self.__shadow.get(key)
            if current is None or revision > current[0]:
                encoded = -1 if deleted else 10 * value + 1
                self.__shadow[key] = (revision, deleted, encoded)
        return True

    def write(self, key, value):
        self.__revision += 1
        revision = self.__revision
        self.__legacy[key] = (revision, False, value)
        self.__shadow[key] = (revision, False, 10 * value + 1)
        return revision

    def delete(self, key):
        self.__revision += 1
        revision = self.__revision
        self.__legacy[key] = (revision, True, -1)
        self.__shadow[key] = (revision, True, -1)
        return revision

    def ready(self):
        if self.__legacy.keys() != self.__shadow.keys():
            return False
        for key, (revision, deleted, value) in self.__legacy.items():
            projected = self.__shadow[key]
            expected = -1 if deleted else 10 * value + 1
            if projected != (revision, deleted, expected):
                return False
        return True

    def cutover(self):
        if not self.ready():
            return False
        self.__cutover = True
        return True

    def read(self, key):
        table = self.__shadow if self.__cutover else self.__legacy
        row = table.get(key)
        if row is None or row[1]:
            return False, 0
        return True, (row[2] - 1) // 10 if self.__cutover else row[2]

    def inspect_new(self, key):
        return self.__shadow.get(key, (-1, False, -1))
