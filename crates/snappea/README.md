# Snap Pea

The [P](https://github.com/p-org/P) modeling language in Rust: a parser, type checker
and model checker with no .NET, Java or code generation, plus helpers to check the
events your application emits against P specs.

## The `snappea` tool

```sh
cargo install snappea        # installs snappea and snappea-compat
snappea check                 # check every test case of the .pproj in this folder
snappea observe --spec BankBalanceCorrect logs/      # check production logs
```

`snappea-compat` is a drop-in replacement for upstream's `p` command.

## Library

Tests need the `test` feature; production code needs only the emitter:

```toml
[dependencies]
snappea = { version = "0.2", default-features = false, features = ["emit"] }

[dev-dependencies]
snappea = { version = "0.2", default-features = false, features = ["test"] }
```

```rust,ignore
// Application code. With only the `emit` feature, no checker is compiled in.
snappea::emit::emit("eReserved", "coffee", snappea::emit::json!({"order": "o1", "units": 2}));

// A test: everything emitted while the closure runs is checked against the P specs
// configured in [package.metadata.snappea] of Cargo.toml.
let model = snappea::test::Model::load()?;
model.check(|_recording| app.reserve("coffee", "o1", 2));
```

| Feature | Adds |
| --- | --- |
| `emit` | `snappea::emit`: emit events to tests, `SNAPPEA_EVENTS_DIR` or production logs. Depends only on `serde` and `serde_json`. |
| `test` (default) | `snappea::test`: in-process checks in `#[test]`s. Implies `emit`. |
| `cli` (default) | The `snappea` and `snappea-compat` binaries. |

`test` and `cli` also expose the checker as a library: `snappea::syntax`, `snappea::sema`,
`snappea::checker`, `snappea::project` and `snappea::driver`.

## Documentation

- [The repository](https://github.com/crowecawcaw/snappea): `snappea` usage and compatibility with upstream P
- [Rust guide](https://crowecawcaw.github.io/snappea/docs/rust.html): tests, emitting, failures and settings
- [Rust inventory examples](https://github.com/crowecawcaw/snappea/tree/main/examples/rust)
- [Testing and production](https://github.com/crowecawcaw/snappea/blob/main/docs/production.md)
- [Checking logs with `snappea observe`](https://github.com/crowecawcaw/snappea/blob/main/docs/observe.md)
- [The checker as a library](https://github.com/crowecawcaw/snappea/blob/main/docs/library.md)

## License

MIT or Apache-2.0, at your option.
