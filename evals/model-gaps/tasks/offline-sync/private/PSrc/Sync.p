// Offline note sync with base versions: the types and events. See DESIGN.md.
// These declarations are fixed: the test environment and the specs use them.

// A note. Content is the set of edit ids it contains; a deleted note has none.
type Note = (version: int, content: set[int], deleted: bool);

// ---- Messages ----

// client: where the reply goes. clientId: the client's id. req: the id of the user's
// change (the same in every attempt). base: the version this attempt is based on.
type UpdateRequest = (client: machine, clientId: int, req: int, base: int, content: set[int], deleted: bool);

event Update: UpdateRequest;
// The update is applied; `version` is the version it became.
event UpdateOk: (req: int, version: int);
// The update is refused; `server` is the server's current copy.
event UpdateConflict: (req: int, server: Note);
// Asks for the current copy; the payload is where the FetchResp goes.
event Fetch: machine;
event FetchResp: Note;

// Environment to client: fetch the server's copy now.
event Pull;
// Environment to client: the request in flight timed out. Its reply is lost or late.
event SyncTimeout;
// Client to its owner: the user's change is resolved and the client stopped editing.
event ClientIdle;

// What a client starts from: the copy its user last saw, and the change the user made
// to it offline (`edit` names it; isDelete: the user deleted the note).
type ClientInit = (owner: machine, server: machine, id: int, note: Note, edit: int, isDelete: bool);

// ---- Announced by the design, for the specs ----

// Server, at its commit point: update `req` from client `client`, based on `base`,
// became version `version` with this content.
event Accepted: (client: int, req: int, base: int, version: int, content: set[int], deleted: bool);
// Server, when it answers a request it already applied: `base` is the base of this
// attempt, `version` the version the request became when it was applied.
event Replayed: (client: int, req: int, base: int, version: int);
// Server, when it refuses an update because its base is not the current version
// `current`.
event Conflicted: (client: int, req: int, base: int, current: int);
// Client: its user's change is resolved. applied=true: an update with it was accepted
// and became `version`. applied=false: a delete the client dropped; `version` is the
// version of the server copy it now holds.
event OpDone: (client: int, edit: int, applied: bool, version: int);
// Client: its local copy after a pull.
event Synced: (client: int, note: Note);

// ---- Announced by the test environment ----

// The note every client starts from, and how many clients will sync.
event SyncPlanned: (clients: int, initial: Note);
// What one client's user did offline.
event OpPlanned: (client: int, edit: int, isDelete: bool);
// The network took away a reply to this client's update and sent it a SyncTimeout.
event RequestTimedOut: (client: int);
