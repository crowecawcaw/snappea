# Contract-derived fault inventory

Chosen before inspecting driver-limit reports:

1. `foreign_release`: releasing a resource owned by another client succeeds and
   clears that owner's reservation. Violates ownership authorization.
2. `resource_alias`: every operation uses resource 1 as its storage slot. Violates
   independent resources; one client can hold both resources concurrently.
3. `rejected_acquire_steals`: a contender receives False but becomes the owner.
   Violates rejected-operation noninterference and exclusivity.

The weak driver uses client 1/resource 1 only. It covers ordinary repeated
acquisition, release, and reacquisition but cannot distinguish these faults.
The distinct correct variant removes empty map entries rather than storing zero.

The reference driver uses two resources and clients, probes rejected operations,
and verifies that wrong-owner release leaves the original owner in place.

## Existing-coverage retention control

Added after fixture review, separately from the three latent faults:
`baseline_dropped_release` rejects a legitimate owner's release and leaves the
reservation unchanged. The weak driver already catches it. A revised driver must
retain that detection as well as expanding coverage to the latent faults.
