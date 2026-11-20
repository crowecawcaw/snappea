// The mobile client. See DESIGN.md. It is created with its user's offline change and
// announces OpDone and Synced (declared in Sync.p).

machine SyncClient {
  start state Offline {
    entry (input: ClientInit) {
      // TODO
    }
  }
}
