// The idempotency-key table. See DESIGN.md. It is created without input and answers
// Claim, Complete, Release and ListOwned (declared in Payments.p).

machine KeyStore {
  start state Serving {
    // TODO
  }
}
