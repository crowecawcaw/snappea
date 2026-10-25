event eAcquire: (client: string, ttl: int, token: int);
event eWrite: (client: string, token: int, value: int, accepted: bool);
event eRelease: (client: string, token: int, accepted: bool);
event eAdvance: (ticks: int);
event eRead: (value: int);
event ePeek: (owner: string, token: int, expires_at: int);
