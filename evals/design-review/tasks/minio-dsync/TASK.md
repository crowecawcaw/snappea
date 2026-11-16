# Design review: MinIO distributed locking

`/work/src` holds the distributed locking subsystem of MinIO, an S3-compatible
object store, at a historical commit. Several MinIO servers each run a lock
server (`cmd/local-locker.go`, `cmd/lock-rest-server.go`); a client takes a
read or write lock on a resource by asking every lock server and counting
grants (`pkg/dsync/drwmutex.go`). A background loop removes stale locks.

Find design bugs in this subsystem: ways two writers (or a writer and a
reader) can hold the same resource at once, and ways a resource can become
permanently unlockable. Assume servers can crash and restart, and network
calls can fail or time out.

Use P to model the design and check it. {tool_line}

Deliverables:

- `/work/model/`: your P model.
- `/work/REPORT.md`: one section per design bug you found, with a title, the
  sequence of events that triggers it, the code involved, and whether the P
  checker produced a counterexample for it. Report only bugs you can explain.
