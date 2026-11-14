// The payment gateway (the side effect). Every call charges the card; it does not
// deduplicate, which is why the API in front of it must.

event Charge: (client: machine, key: int, amount: int);
event ChargeResp: (charge: int);

machine Gateway {
  var nextCharge: int;
  start state Serving {
    on Charge do (c: (client: machine, key: int, amount: int)) {
      // snappea-lint: allow mutation:swap-operator -- charge ids are only compared for equality; counting down keeps them unique
      nextCharge = nextCharge + 1;
      announce Charged, (key = c.key, amount = c.amount, charge = nextCharge);
      send c.client, ChargeResp, (charge = nextCharge,);
    }
  }
}

type Request = (req: int, key: int, amount: int);
