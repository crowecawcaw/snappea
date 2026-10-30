# forced interleaving

How to turn what a P model explores into deterministic Python tests. Read
[test_wallet.py](test_wallet.py): `SameVersionStore` makes two calls read the same
version; `InterferingStore` runs a competing call between a load and its
compare-and-swap; `Recorder` checks real calls against [Wallet.p](Wallet.p).

Run: `python -m pytest` (needs `pysnappea`).
