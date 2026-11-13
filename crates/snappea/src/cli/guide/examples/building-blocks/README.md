---
diataxis: how-to
personas: [application-developer, coding-agent]
questions:
  - How do I import reusable network and request/reply building blocks?
  - What behavior do these building blocks model?
---
# Import reusable P building blocks

These are ordinary P sources, versioned with Snappea. Import and use them instead of
copying a fresh courier or completion spec into every model. No compiler or runtime
feature is required. Export a local copy from the installed binary:

```sh
snappea guide example building-blocks --out vendor
```

The files land in `vendor/building-blocks`. Commit that directory with your model so
its semantics stay pinned. To update, export into a temporary directory and review
the diff before replacing your copy. Snappea does not download dependencies during
checking.

Include the **library-only** project from your model's `.pproj` when using both
blocks (use `Network.pproj` or `RequestReplies.pproj` for just one):

```xml
<Project>
  <ProjectName>MyModel</ProjectName>
  <IncludeProject>vendor/building-blocks/BuildingBlocks.pproj</IncludeProject>
  <InputFiles><PFile>Model.p</PFile></InputFiles>
</Project>
```

`IncludeProject` paths resolve relative to the including project. Keep your model's
input paths narrow: including `vendor/` as a source directory would also load the
bundle's deliberately broken example tests. `BuildingBlocks.pproj` includes only
`Network.p` and `RequestReplies.p` through their individual projects. Import only
the block you need; an unused spec can otherwise trigger the unselected-spec lint. Prefix `BB` reserves the library's names. A module
that uses the network must include both `BBNetwork` and `BBDelivery`.

Run the included consumer to see the import work:

```sh
cd vendor/building-blocks
snappea check tcReplies tcNetworkFaults Example.pproj --no-mutation
snappea check tcDuplicate Example.pproj --no-mutation  # expected: duplicate terminal reply
snappea check tcMissing Example.pproj --no-mutation    # expected: RepliesPending liveness failure
```

`tcReplies` demonstrates the completion spec over a reliable direct-send model.
`tcNetworkFaults` independently checks the network's one-drop/one-duplicate budget:
at least one of its two packets arrives, neither arrives more than twice, and total
deliveries stay at most three. Combining loss with request/reply progress requires a
protocol that retries or reports a terminal failure; the blocks do not supply one.

## Bounded network faults

Create `new BBNetwork((drops = 1, duplicates = 1))`, then forward typed payloads:

```p
send network, BBForward,
  (destination = service, message = eRequest, payload = (client = this, id = 1));
```

- Budgets are nonnegative, shared across that network instance's entire lifetime.
  At most `drops` forwarded packets are discarded; at most `duplicates` extra
  deliveries are created. A packet can be dropped or delivered once/twice.
- Each delivery uses a separate courier. Even packets submitted in order can arrive
  in a different order, and a delayed reply can arrive after a timeout. There is no
  real-time delay bound and no clock advancement; lateness means scheduler order.
- A surviving delivery remains runnable; the network does not model a permanent
  partition after its drop budget is exhausted. Use finite workloads and the
  checker's scheduling/liveness assumptions when making eventual-delivery claims.
- The event and its payload must have matching types. The `any` envelope cannot
  provide a static guarantee for a dynamically selected event.
- Both machines are marked drivers, so mutation testing targets the design rather
  than the fault injector. They still contribute coverage and scheduling cost.
  These building blocks do not optimize away courier hops.

Zero budgets still permit reordering and late delivery. Use direct `send` when you
want P's per-sender order. For retries, add your own bounded timer/workload; this
network does not retry, deduplicate, or acknowledge packets for the application.

## Planned request/reply obligations

Select `assert BBRequestReplies in ...` in a test. Announce `BBPlan, count` once,
before work starts, with a positive count of logical requests. Announce
`BBRequested, id` once per logical request before sending it. IDs are integers and
must be unique across all clients in that test execution. Retry attempts reuse the
logical ID without another `BBRequested`.

On each actual terminal reply, announce `BBReplied` with the ID that reply carries.
The spec rejects unknown IDs, duplicate replies, reused request IDs, extra requests,
and repeated plans. It remains hot until all planned requests have arrived and each
has exactly one reply, including when the pending set is temporarily empty between
requests. Zero-work plans fail rather than passing vacuously.

Use a thin adapter in the receiving driver, as `Example.p` does. Do not manufacture
a successful reply from the plan or from the absence of an error. Removing the
actual design reply must remove the adapter's `BBReplied` too. Add an independent
spec for reply contents and design correctness: this block checks identity,
cardinality, and eventual completion only.

A terminal rejection can count as a reply if your contract allows it. Cancellation,
multiple replies per request, ID reuse, and unbounded workloads require a different
contract; adapt or write a spec explicitly instead of suppressing this one's checks.
