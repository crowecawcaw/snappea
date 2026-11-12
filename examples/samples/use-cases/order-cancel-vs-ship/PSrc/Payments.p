// The payment gateway: authorize places a hold on the card, capture takes the money,
// void releases the hold. Capture is idempotent per order (the order id is the
// idempotency key), so retrying a capture whose reply was lost never charges twice.
//
// Capture can fail transiently, up to `failures` times per gateway. A failure either
// happens before the gateway commits, or after it commits with the reply lost; the
// caller cannot tell which and must retry with the same key.
//
// The gateway is a third-party service, not part of this design: it is test
// environment, so it is marked as a driver and its failures are the test's choices.
// Its announcements are the commit points the contract observes.

event eAuthorize: (client: machine, order: int);
event eAuthorizeResp: (order: int);
event eCapture: (client: machine, order: int);
event eCaptureResp: (order: int, ok: bool);
event eVoid: (order: int);

// snappea: driver
machine Payments {
  var captured: set[int];
  var failures: int;

  start state Serving {
    entry (budget: int) {
      failures = budget;
    }
    on eAuthorize do (r: (client: machine, order: int)) {
      announce eAuthorized, (order = r.order,);
      send r.client, eAuthorizeResp, (order = r.order,);
    }
    on eCapture do (r: (client: machine, order: int)) {
      if (failures > 0 && $) {
        failures = failures - 1;
        if ($) {
          // Committed, but the reply is lost on the way back.
          Capture(r.order);
        }
        send r.client, eCaptureResp, (order = r.order, ok = false);
        return;
      }
      Capture(r.order);
      send r.client, eCaptureResp, (order = r.order, ok = true);
    }
    on eVoid do (r: (order: int)) {
      announce eVoided, (order = r.order,);
    }
  }

  fun Capture(order: int) {
    if (!(order in captured)) {
      captured += (order);
      announce eCaptured, (order = order,);
    }
  }
}
