// Events the specs observe besides the account's replies. Shared by the strong spec
// (PSpec/Withdrawals.p) and the weak one (weak/Withdrawals.p).

// Announced by the driver before any withdrawal: the opening balance and how many
// withdrawals follow.
event Opened: (balance: int, withdrawals: int);
// Announced by a client as it asks.
event WithdrawRequested: (id: int, amount: int);
