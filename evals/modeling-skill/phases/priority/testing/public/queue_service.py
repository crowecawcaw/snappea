"""Small stable priority work queue used by the testing evaluation."""


class Service:
    def __init__(self):
        self.__entries = {}
        self.__next_order = 0

    def submit(self, item_id, priority):
        if item_id in self.__entries:
            return False
        self.__entries[item_id] = (priority, self.__next_order)
        self.__next_order += 1
        return True

    def pop(self):
        if not self.__entries:
            return None
        item_id = min(self.__entries,
                      key=lambda key: (self.__entries[key][0], self.__entries[key][1]))
        del self.__entries[item_id]
        return item_id

    def reprioritize(self, item_id, priority):
        if item_id not in self.__entries:
            return False
        _, order = self.__entries[item_id]
        self.__entries[item_id] = (priority, order)
        return True

    def cancel(self, item_id):
        if item_id not in self.__entries:
            return False
        del self.__entries[item_id]
        return True
