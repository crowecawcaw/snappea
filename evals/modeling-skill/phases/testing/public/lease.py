"""Deterministic, single-resource lease service used by the testing evaluation."""


class Service:
    def __init__(self):
        self.__now = 0
        self.__value = 0
        self.__owner = None
        self.__token = None
        self.__expires_at = None
        self.__last_token = 0

    def __active(self):
        return self.__owner is not None and self.__expires_at > self.__now

    def acquire(self, client, ttl):
        if self.__active():
            return None
        self.__last_token += 1
        self.__owner = client
        self.__token = self.__last_token
        self.__expires_at = self.__now + ttl
        return self.__token

    def write(self, client, token, value):
        if not self.__active() or self.__owner != client or self.__token != token:
            return False
        self.__value = value
        return True

    def release(self, client, token):
        if not self.__active() or self.__owner != client or self.__token != token:
            return False
        self.__owner = None
        self.__token = None
        self.__expires_at = None
        return True

    def advance(self, ticks):
        self.__now += ticks

    def read(self):
        return self.__value

    def peek(self):
        if not self.__active():
            return None
        return self.__owner, self.__token, self.__expires_at
