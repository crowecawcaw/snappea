event ePut: (request_id: string, item: int, accepted: bool);
event eTake: (request_id: string, item: int, found: bool);
event eSnapshot: (items: seq[int]);
