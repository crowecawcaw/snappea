// Events the specs observe. Shared by the strong spec (PSpec/Contract.p) and the weak
// one (weak/Contract.p).

// Announced by the driver before any withdrawal: the opening balance and how many
// withdrawals will follow.
event eOpened: (balance: int, withdrawals: int);
// Announced by a client as it asks, and when it gets its answer.
event eRequested: (id: int, amount: int);
event eAnswered: (id: int, ok: bool);
// Announced by the account at the commit.
event eWithdrawn: (id: int, ok: bool, balance: int);
