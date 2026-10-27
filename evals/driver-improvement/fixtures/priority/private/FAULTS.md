# Contract-derived fault inventory (before examining driver facts)

1. Equal priorities pop the most recently admitted item (unstable tie order).
2. Reprioritization gives an active item a new admission position.
3. Cancel/pop then re-add reuses the old admission position.
4. Ranking uses absolute priority instead of signed priority.
5. A rejected duplicate admission silently overwrites the existing priority.

The initial distinct-priority script is intended to miss all five. Reversed
internal iteration order is the independently correct representation variant.
These variants change the algorithm, not the driver or monitor.

Regression control (separate from the five latent faults): `cancel_no_effect`
acknowledges cancellation without removing the item. The initial driver already
detects this when it re-admits the canceled ID; losing this detection is a
test-suite regression. Baseline detects 1/6; strong reference detects 6/6.
