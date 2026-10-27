type Command = (id: int, expected: int, value: int);
type Outcome = (accepted: bool, version: int);
event eSet: (caller: machine, command: Command);
event eReply: Outcome;
event eObserved: (command: Command, outcome: Outcome, version: int, value: int);
event eFinished;
