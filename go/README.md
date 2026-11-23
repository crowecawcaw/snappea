# Go helper

Module `github.com/crowecawcaw/snappea` (this repository's root), Go 1.22+, with two
standard-library-only packages:

- [`github.com/crowecawcaw/snappea/go/snappeatest`](snappeatest): check events from Go tests
  against P specs. It runs the `snappea` binary.
- [`github.com/crowecawcaw/snappea/go/snappeaemit`](snappeaemit): emit events from application
  code, in the test process or in processes a test starts; in production, write them
  as a log for `snappea observe`.

The **[Go guide](https://snappea.dev/docs/go.html)** covers setup, recordings, emitting, other
processes, failures and every option; the API reference is on
[pkg.go.dev](https://pkg.go.dev/github.com/crowecawcaw/snappea/go/snappeatest). Both packages
follow the [language helper requirements](../docs/language-support.md), and the runnable
[inventory example](../examples/go/README.md) uses all of it.

## Setup

```sh
go get github.com/crowecawcaw/snappea
```

`snappeatest` runs the `snappea` checker: `SNAPPEA_BIN`, else `snappea` on `PATH`, else the release
binary of the module's version, downloaded once into the user cache
(`os.UserCacheDir()/snappea/<version>/`) and verified against checksums committed in the
module. Offline, or with `SNAPPEA_DOWNLOAD=0`, install it yourself (`pip install pysnappea`
also brings it):

```sh
cargo install snappea
```

To use a checkout instead of a release, `replace` the module and build `snappea` from the
same revision (a checkout downloads nothing):

```go
require github.com/crowecawcaw/snappea v0.0.0
replace github.com/crowecawcaw/snappea => ../snappea
```

## Example

```go
func TestReservation(t *testing.T) {
    snappeatest.Start(t, snappeatest.Config{SpecFiles: []string{"../specs/inventory.p"}})
    app := inventory.New()       // emits with snappeaemit.Emit
    app.Reserve("coffee", "order-1", 2)
}                                // the events are checked when the test ends
```

## Develop

```sh
cargo build -p snappea --bin snappea --locked
export SNAPPEA_BIN="$(pwd)/target/debug/snappea"
go test -race -count=1 ./... && go vet ./...   # the module is the repository root
(cd examples/go && go test -race -count=1 ./...)
```

Tests that need the checker skip when no `snappea` is found. `TestConformance` replays
[tests/conformance](../tests/conformance/README.md).
