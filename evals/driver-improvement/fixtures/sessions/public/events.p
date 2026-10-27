type Command = (reply: machine, kind: int, client: int, resource: int);
type Result = (kind: int, client: int, resource: int, accepted: bool, owner: int);
event eCommand: Command;
event eReply: Result;
event eObserved: Result;
event eFinished: (expected: int);
