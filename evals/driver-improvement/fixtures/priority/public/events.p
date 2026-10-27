type Command = (op: int, id: int, priority: int, reply: machine);
type Observation = (op: int, id: int, priority: int, accepted: bool, result: int);
event eCommand: Command;
event eAck;
event eFinish;
event eObserved: Observation;
event eFinished;
