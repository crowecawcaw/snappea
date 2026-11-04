// Bank withdrawals: clients ask the server to debit accounts. The database owns the
// balances. The abstract-server test intentionally violates the minimum balance rule.
// Each machine owns its variables. Sending queues an event; the receiver handles it when
// the scheduler runs that actor.

// Client module
module Client = {
  Client
}
;

// Bank module
module Bank = {
  BankServer, Database
}
;

// Abstract Bank Server module
module AbstractBank = {
  AbstractBankServer -> BankServer
}
;
