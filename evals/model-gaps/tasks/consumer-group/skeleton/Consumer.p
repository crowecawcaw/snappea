// A group member. For each assigned partition it fetches the committed offset, then
// processes records one at a time and commits the next offset after each, tagged with
// its generation. See DESIGN.md.
//
// It announces Adopted and Processed (PSpec/Contract.p). Each partition's log has
// `records` records; offsets run 0 .. records - 1.

// From the application (the test driver): shut down cleanly.
event Shutdown;

machine Consumer {
  start state Joining {
    entry (input: (coordinator: machine, id: int, records: int)) {
      // TODO
    }
  }
}
