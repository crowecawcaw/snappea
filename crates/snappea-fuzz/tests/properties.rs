//! Property tests: generated inputs, checked against what the result must be.
//!
//! - Timestamps written by `snappea::emit` read back to the same instant.
//! - On generated event sequences, `snappea::test` and `snappea observe` agree with a
//!   reference model of [`snappea_fuzz::OBSERVE_SPEC`], whatever the payload strings,
//!   numbers and `any` values.
//!
//! Failing cases are saved to `proptest-regressions/` next to this file; commit them.

use proptest::prelude::*;
use serde_json::{json, Value};
use snappea::checker::observe_json::parse_instant;
use snappea::emit::Time;
use snappea::host::{Host, MemHost};
use snappea::test::Model;

/// Nanoseconds since the epoch that `snappea::emit` can write: all of `i64`.
fn instant() -> impl Strategy<Value = i64> {
    prop_oneof![any::<i64>(), -10_000_000_000_000_000i64..10_000_000_000_000_000, Just(0), Just(-1)]
}

proptest! {
    #[test]
    fn emitted_timestamps_read_back(nanos in instant()) {
        let iso = Time::from_nanos(nanos).to_iso();
        let read = parse_instant(&iso);
        prop_assert!(read.is_some(), "cannot read {iso}");
        let read = read.unwrap();
        prop_assert_eq!((read.secs, i64::from(read.nanos)), (nanos.div_euclid(1_000_000_000), nanos.rem_euclid(1_000_000_000)), "{}", iso);
    }

    /// Instants in the years 0000 to 9999, the ones log timestamps can hold.
    #[test]
    fn instants_print_and_read_back(secs in -62_167_219_200i64..=253_402_300_799, nanos in 0u32..1_000_000_000) {
        let t = snappea::checker::observe::Timestamp::new(secs, nanos);
        prop_assert_eq!(parse_instant(&t.to_iso()), Some(t), "{}", t.to_iso());
    }
}

/// One event of the order spec.
#[derive(Debug, Clone)]
enum Op {
    Open { id: i64, owner: String, items: Vec<(String, i64)>, tags: Vec<String> },
    Update { id: i64, prices: Vec<(String, f64)>, note: Value },
    Close { id: i64, failed: bool, ok: bool },
}

impl Op {
    fn id(&self) -> i64 {
        match self {
            Op::Open { id, .. } | Op::Update { id, .. } | Op::Close { id, .. } => *id,
        }
    }

    fn event(&self) -> (&'static str, Value) {
        match self {
            Op::Open { id, owner, items, tags } => (
                "eOpen",
                json!({
                    "id": id, "owner": owner, "tags": tags,
                    "items": items.iter().map(|(name, qty)| json!({"name": name, "qty": qty})).collect::<Vec<_>>(),
                }),
            ),
            Op::Update { id, prices, note } => (
                "eUpdate",
                json!({
                    "id": id, "note": note,
                    "prices": prices.iter().map(|(k, v)| (k.clone(), json!(v))).collect::<serde_json::Map<_, _>>(),
                }),
            ),
            Op::Close { id, failed, ok } => {
                ("eClose", json!({"id": id, "status": if *failed { "FAILED" } else { "OK" }, "ok": ok}))
            }
        }
    }
}

/// Whether OrdersComplete (`OBSERVE_SPEC`) fails on `ops`, by the rules it states. Each
/// key (the order id) has its own spec instance.
fn reference_fails(ops: &[Op]) -> bool {
    let mut open = std::collections::HashSet::new();
    for op in ops {
        let fails = match op {
            Op::Open { id, .. } => !open.insert(*id),
            Op::Update { id, .. } => !open.contains(id),
            Op::Close { id, failed, ok } => !open.remove(id) || (!*failed != *ok),
        };
        if fails {
            return true;
        }
    }
    false
}

fn text() -> impl Strategy<Value = String> {
    prop_oneof![any::<String>(), "[a-z]{0,3}", Just(String::new()), Just("\"\\\n\u{0}é😀".into())]
}

fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i64>().prop_map(Value::from),
        any::<f64>().prop_filter("JSON numbers are finite", |f| f.is_finite()).prop_map(Value::from),
        text().prop_map(Value::from),
    ];
    leaf.prop_recursive(3, 16, 4, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..4).prop_map(Value::from),
            prop::collection::btree_map(text(), inner, 0..4).prop_map(|m| Value::Object(m.into_iter().collect())),
        ]
    })
}

fn op() -> impl Strategy<Value = Op> {
    let id = prop_oneof![0i64..3, any::<i64>()];
    let float = any::<f64>().prop_filter("JSON numbers are finite", |f| f.is_finite());
    prop_oneof![
        (id.clone(), text(), prop::collection::vec((text(), any::<i64>()), 0..3), prop::collection::vec(text(), 0..3))
            .prop_map(|(id, owner, items, tags)| Op::Open { id, owner, items, tags }),
        (id.clone(), prop::collection::vec(("[a-z]{0,2}", float), 0..3), json_value())
            .prop_map(|(id, prices, note)| Op::Update { id, prices, note }),
        (id, any::<bool>(), any::<bool>()).prop_map(|(id, failed, ok)| Op::Close { id, failed, ok }),
    ]
}

/// One order's events: usually open, update, close; sometimes with one of them missing
/// or repeated. `failed` and `ok` disagree half the time.
fn order() -> impl Strategy<Value = Vec<Op>> {
    (0i64..4, text(), json_value(), any::<bool>(), any::<bool>(), 0u8..8).prop_map(
        |(id, owner, note, failed, ok, shape)| {
            let open = Op::Open { id, owner, items: vec![("x".into(), 1)], tags: vec![] };
            let update = Op::Update { id, prices: vec![("x".into(), 1.5)], note };
            let close = Op::Close { id, failed, ok };
            match shape {
                0 => vec![update, open, close],
                1 => vec![open.clone(), open, close],
                2 => vec![open, close.clone(), close],
                3 => vec![close, open],
                4 => vec![open],
                _ => vec![open, update, close],
            }
        },
    )
}

/// Event sequences: orders one after another, orders interleaved, or any events.
fn ops() -> impl Strategy<Value = Vec<Op>> {
    let orders = || prop::collection::vec(order(), 0..4).prop_map(|o| o.concat());
    prop_oneof![
        2 => orders(),
        2 => orders().prop_shuffle(),
        1 => prop::collection::vec(op(), 0..8),
    ]
}

fn model() -> &'static Model {
    static MODEL: std::sync::OnceLock<Model> = std::sync::OnceLock::new();
    MODEL.get_or_init(|| Model::compile(&[("spec.p", snappea_fuzz::OBSERVE_SPEC)]).expect("the spec compiles"))
}

/// The event's time: one millisecond apart, from 2026.
fn at(i: usize) -> Time {
    Time::from_millis(1_767_225_600_000 + i as i64)
}

/// Whether `snappea::test` reports a violation for `ops`.
fn test_fails(ops: &[Op]) -> Result<bool, String> {
    let recording = model().recording();
    for (i, op) in ops.iter().enumerate() {
        let (event, payload) = op.event();
        recording.emit_at(at(i), event, op.id(), payload);
    }
    match recording.verify() {
        Ok(_) => Ok(false),
        Err(e) if e.violation().is_some() => Ok(true),
        Err(e) => Err(e.to_string()),
    }
}

/// Whether `snappea observe` (in memory) reports a violation for `ops` as JSON lines.
fn observe_fails(ops: &[Op]) -> Result<bool, String> {
    let mut log = String::new();
    for (i, op) in ops.iter().enumerate() {
        let (event, payload) = op.event();
        let line = json!({"event": event, "key": op.id().to_string(), "timestamp": at(i).to_iso(), "payload": payload});
        log.push_str(&line.to_string());
        log.push('\n');
    }
    let mut host = MemHost::new("/work");
    host.write_file("/work/spec.p".as_ref(), snappea_fuzz::OBSERVE_SPEC).unwrap();
    host.write_file("/work/log.jsonl".as_ref(), &log).unwrap();
    let args: Vec<String> = ["observe", "spec.p", "log.jsonl", "--all-specs"].map(String::from).to_vec();
    match snappea::cli::run(&args, &mut host) {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(format!("{}{}", host.stdout, host.stderr)),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    #[test]
    fn test_and_observe_agree_with_the_spec(ops in ops()) {
        let expected = reference_fails(&ops);
        prop_assert_eq!(test_fails(&ops), Ok(expected), "snappea::test");
        prop_assert_eq!(observe_fails(&ops), Ok(expected), "snappea observe");
    }
}
