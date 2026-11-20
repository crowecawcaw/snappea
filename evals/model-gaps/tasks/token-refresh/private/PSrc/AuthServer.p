// The auth server, for one token family (one login). Tokens are numbered: refresh
// token n comes with access token n. A refresh rotates: it issues n + 1 and n is no
// longer accepted. One handler is one transaction on the family's row.

machine AuthServer {
  // the refresh token issued last
  var current: int;
  var revoked: bool;

  start state Serving {
    entry (initial: int) {
      current = initial;
    }
    on Refresh do (r: (client: machine, tab: machine, refresh: int)) {
      if (revoked) {
        Refuse(r.client, r.tab);
        return;
      }
      if (r.refresh == current) {
        current = current + 1;
        announce Rotated, (from = r.refresh, next = current);
        send r.client, RefreshResp, (tab = r.tab, ok = true, token = current);
        return;
      }
      revoked = true;
      announce FamilyRevoked, (refresh = r.refresh,);
      Refuse(r.client, r.tab);
    }
  }

  fun Refuse(client: machine, tab: machine) {
    send client, RefreshResp, (tab = tab, ok = false, token = 0);
  }
}
