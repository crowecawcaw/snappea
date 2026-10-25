# Monitor calibration trace

This private fixture emits a known-valid history to validate the monitor and checking path. It is not a reference aggregation algorithm or a response satisfying the public model task.

`eAdd` and `eWatermark` each represent one serialized public call. All `eOutput` records returned by that call follow it, and `eEndCall` closes the returned list. The monitor computes expected outputs independently and checks the list is complete, including at the end of a history.

The contract monitor tracks per-window sums, whether a first output has appeared, the current revision, and finalization. Its boundary policy handles a watermark that jumps across both the initial and final thresholds: if the window had no visible output, it produces a single final revision 0. A late add can create an already-closed window before finalization and causes that window's initial revision immediately.

The model trace exercises windows `[0,10)` and `[10,20)`, negative values, a late correction, a rejected add at the final threshold, revision progression, and final output. Bounds are a fixed trace with two windows and a few calls; the monitor itself is independent of those bounds. It omits malformed inputs, persistence, concurrency between calls, and automatic watermarks. Calls are deliberately serialized so this task checks event-time semantics rather than implementation scheduling.
