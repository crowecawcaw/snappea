// The auth server, one token family (one login). Tokens are numbered: refresh token n
// comes with access token n. Each refresh rotates: it issues n + 1 and invalidates n.
// Presenting a refresh token that was already rotated is treated as theft: the whole
// family is revoked and the user is logged out everywhere.
//
// One handler is one atomic step on the family's row.

event Refresh: (client: machine, tab: machine, refresh: int);
event RefreshResp: (tab: machine, ok: bool, token: int);

machine AuthServer {
  // the only refresh token currently accepted
  var current: int;
  var revoked: bool;

  start state Serving {
    entry (initial: int) {
      current = initial;
    }
    on Refresh do (r: (client: machine, tab: machine, refresh: int)) {
      if (!revoked && r.refresh == current) {
        current = current + 1;
        announce Rotated, (from = r.refresh, next = current);
        send r.client, RefreshResp, (tab = r.tab, ok = true, token = current);
        return;
      }
      if (!revoked) {
        // Reuse of a rotated-out token: assume it was stolen.
        revoked = true;
        announce FamilyRevoked, (refresh = r.refresh,);
      }
      send r.client, RefreshResp, (tab = r.tab, ok = false, token = 0);
    }
  }
}
