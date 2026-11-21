// One API server process. See DESIGN.md. It is created with ApiInit (declared in
// Payments.p) and answers every Charge with ChargeResponse, ChargeConflict or
// ChargeRejected.

machine PaymentApi {
  start state Booting {
    entry (input: ApiInit) {
      // TODO
    }
  }
}
