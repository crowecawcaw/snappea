// The notes server. See DESIGN.md. It is created with the note every client starts
// from, and announces Accepted, Replayed and Conflicted (declared in Sync.p).

machine NoteServer {
  start state Serving {
    entry (initial: Note) {
      // TODO
    }
  }
}
