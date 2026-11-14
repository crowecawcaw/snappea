// Reorder replica writes with the imported delivery building block.
// Each delivery has its own sender, so a later request may reach a replica first.
// This wrapper belongs to the environment; the quorum algorithm stays in PSrc.
fun Deliver(replica: machine, write: (client: machine, version: int, value: int)) {
  new BBDelivery((destination = replica, message = ReplicaWrite, payload = write));
}
