// Negative control, kept apart so its code is not part of the passing tests: storage
// that ignores fencing tokens. A holder that paused past its lease overwrites its
// successor's write. tcBugStorageIgnoresTokens shows the contract catching it.
machine UnfencedStorage {
  start state Serving {
    on eWrite do (w: (client: machine, id: int, token: int)) {
      // BUG: no comparison with the highest token seen.
      announce eWriteAccepted, (id = w.id, token = w.token);
      send w.client, eWriteResp, true;
    }
  }
}
