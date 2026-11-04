// Raft: servers elect a leader and replicate its log despite delayed or dropped messages.
// Safety rules compare leaders, log entries, and applied commands.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

module Server = {
  Server
}
;
module Timer = {
  Timer
}
;
module Client = {
  Client
}
;
module View = {
  View
}
;
