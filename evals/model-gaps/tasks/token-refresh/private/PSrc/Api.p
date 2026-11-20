// Events and types shared by the design (Tab, AuthServer) and the browser platform
// around it (storage, the cross-tab lock, the network). Fixed: do not change.

// What the tabs share in localStorage, written and read as one value. `lost` marks a
// session that was given up: the user has to log in again.
type Session = (token: int, lost: bool);

// Storage (localStorage). A tab sends itself with Load and gets Loaded; Save replaces
// the whole session and is answered with Saved.
event Load: machine;
event Loaded: Session;
event Save: (client: machine, session: Session);
event Saved;

// The cross-tab lock (Web Locks). A tab sends itself with Acquire and gets Granted
// when it holds the lock; it gives the lock back with Release.
event Acquire: machine;
event Granted;
event Release;

// A tab asks the network to present a refresh token to the auth server. It gets back
// RefreshResp (the server's answer), RefreshTimeout (the timer for that request,
// naming the refresh token the request presented), or both.
event SendRefresh: (tab: machine, refresh: int);
event RefreshTimeout: (refresh: int);

// The network to the auth server, and its answer. `client` gets the answer and must
// pass it on to `tab`. ok = false means the server refused the token.
event Refresh: (client: machine, tab: machine, refresh: int);
event RefreshResp: (tab: machine, ok: bool, token: int);

// Announced by the design for the specs.
// The server rotated refresh token `from` out and issued `next`.
event Rotated: (from: int, next: int);
// The server revoked the family because `refresh` was presented after it was rotated out.
event FamilyRevoked: (refresh: int);
// A tab (by its id) has a fresh token, or gave the session up.
event TokenObtained: (tab: int, token: int);
event SessionLost: (tab: int);
