// The ticket contract, as clients see it: tickets are unique and increasing, across
// crashes too, and every client that asks eventually holds a ticket issued to it.

// Announced by the driver before it starts any client: how many will ask.
event ClientsPlanned: int;
// Announced by the server right after it sends a ticket.
event Issued: (client: machine, ticket: int);
// Announced by a client when it accepts its ticket.
event GotTicket: (client: machine, ticket: int);

spec TicketsUnique observes ClientsPlanned, Issued, GotTicket {
  // ticket -> client it was issued to
  var issued: map[int, machine];
  var highest: int;
  var planned: int;
  var served: set[machine];

  // Hot while some client has no ticket: a client stuck after a crash fails here.
  start hot state Running {
    on ClientsPlanned do (n: int) { planned = planned + n; }
    on Issued do (p: (client: machine, ticket: int)) {
      assert !(p.ticket in issued), format("ticket {0} issued twice", p.ticket);
      assert p.ticket > highest, format("ticket {0} issued after ticket {1}", p.ticket, highest);
      issued[p.ticket] = p.client;
      highest = p.ticket;
    }
    on GotTicket do (p: (client: machine, ticket: int)) {
      assert !(p.client in served), format("{0} accepted a second ticket", p.client);
      assert p.ticket in issued && issued[p.ticket] == p.client,
        format("{0} accepted ticket {1}, which was not issued to it", p.client, p.ticket);
      served += (p.client);
      if (sizeof(served) == planned) { goto AllServed; }
    }
  }

  state AllServed {
    on ClientsPlanned do { assert false, "clients planned after every client was served"; }
    on Issued do { assert false, "ticket issued after every client was served"; }
    on GotTicket do { assert false, "ticket accepted after every client was served"; }
  }
}
