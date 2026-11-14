// Events the specs observe. Shared by the strong spec (PSpec/Contract.p) and the weak
// one (weak/Contract.p).

// Announced by the driver before any withdrawal: the opening balance and how many
// withdrawals will follow.
event Opened: (balance: int, withdrawals: int);
// Announced by a client as it asks, and when it gets its answer.
event Requested: (id: int, amount: int);
event Answered: (id: int, ok: bool);
// Announced by the account at the commit.
event Withdrawn: (id: int, ok: bool, balance: int);
