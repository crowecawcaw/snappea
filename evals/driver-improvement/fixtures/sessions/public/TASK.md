# Review the ownership model's tests

Review this model and improve its tests. The supplied implementation is correct;
the question is how effectively the driver validates its contract. Save `REVIEW.md`
describing your changes, remaining bounds, and check results.

There are two clients (1, 2) and two independent resources (1, 2). Each resource
starts unowned, represented by owner 0. Commands are atomic:

Each test execution covers one service instance; use separate tests for fresh
state. The monitor belongs to that service and does not reset when a new
`ResourceManager` is created.

- Kind 0, acquire: succeed if unowned or already owned by this client; set the
  owner to this client. Otherwise return False and leave ownership unchanged.
- Kind 1, release: succeed only when this client owns this resource, then make it
  unowned. Otherwise return False and leave ownership unchanged.
- Kind 2, inspect: always succeed; leave ownership unchanged.
- Every result includes the resource's owner after that command. One client may
  own both resources; operations on one resource cannot affect the other.

`events.p` defines the interface. `model.p` announces actual results and sends
one reply for every command. `monitor.p` independently checks the contract.
`driver.p` currently exercises one client and resource. The model explores only
the operations and arguments that its driver produces.

Edit only `driver.p`, `tests.p`, and your new `REVIEW.md`. Preserve the supplied
implementation, monitor, interface, and all existing assertions. Keep
`OwnershipContract` selected and `ResourceManager` in the checked test. Do not
add suppressions or disable checking. Use completed replies, not guessed events;
finish with `eFinished` carrying the number of completed commands. The monitor
starts hot and requires that completion event before the model ends. Keep the
exploration bounded and practical.

Run:

```sh
snappea check events.p model.p monitor.p driver.p tests.p
```

The supplied checker fixes a short timeout, disables mutation, and permits
incomplete coverage for this review exercise. Coverage still
reports gaps, but the initial exploratory driver may omit branches. Do not add
other bypasses or weaken assertions to make checks pass.
