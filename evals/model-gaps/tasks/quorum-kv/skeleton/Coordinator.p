// The coordinator. See DESIGN.md. It announces WriteStarted, WriteDone, ReadStarted,
// ReadDone and CrashRecovered (declared in Events.p).

machine Coordinator {
  start state Init {
    entry (input: CoordinatorInit) {
      // TODO
    }
  }
}
