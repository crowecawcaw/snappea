//! Rust ports of the log parsers of upstream's PObserve regression tests
//! (`Src/PObserve/PObserveRegressionTesting/src/main/java/pobserve/testing/parsers`).
//!
//! Each follows its Java original step by step, including which malformed lines are
//! skipped (`Stream.empty()`) and which throw (a parser error in PObserve's report).

#![allow(dead_code)]

use std::rc::Rc;

use snappea::checker::observe::{java_trim, ObservedEvent, Parser, Spec, Timestamp};
use snappea::checker::value::{SlotMap, Value};

/// All six parsers, named like their Java classes.
pub(crate) fn all() -> Vec<Box<dyn Parser>> {
    vec![
        Box::new(TestCase2Parser),
        Box::new(TestCase3Parser),
        Box::new(TestCase4Parser),
        Box::new(TestCase5Parser),
        Box::new(TestCase6Parser),
        Box::new(TestCase7Parser),
    ]
}

/// `String.split(regex)` for a literal separator: trailing empty strings are removed.
fn split<'a>(s: &'a str, sep: &str) -> Vec<&'a str> {
    if !s.contains(sep) {
        return vec![s];
    }
    let mut v: Vec<&str> = s.split(sep).collect();
    while v.last() == Some(&"") {
        v.pop();
    }
    v
}

fn get<'a>(parts: &[&'a str], i: usize) -> Result<&'a str, String> {
    parts.get(i).copied().ok_or_else(|| {
        format!("java.lang.ArrayIndexOutOfBoundsException: Index {} out of bounds for length {}", i, parts.len())
    })
}

/// `Long.parseLong`.
fn parse_long(s: &str) -> Result<i64, String> {
    s.parse::<i64>().map_err(|_| format!("java.lang.NumberFormatException: For input string: \"{}\"", s))
}

/// `Integer.parseInt`.
fn parse_int(s: &str) -> Result<i64, String> {
    s.parse::<i32>()
        .map(|v| v as i64)
        .map_err(|_| format!("java.lang.NumberFormatException: For input string: \"{}\"", s))
}

/// `Math.round(Double.parseDouble(s))`.
fn parse_rounded(s: &str) -> Result<i64, String> {
    let t = java_trim(s);
    let t = t.strip_suffix(['d', 'D', 'f', 'F']).unwrap_or(t);
    let d: f64 = t.parse().map_err(|_| format!("java.lang.NumberFormatException: For input string: \"{}\"", s))?;
    Ok((d + 0.5).floor() as i64)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    // lenient: months outside 1..12 roll over into years
    let y = y + (m - 1).div_euclid(12);
    let m = (m - 1).rem_euclid(12) + 1;
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468
}

/// `new SimpleDateFormat("yyyy-MM-dd HH:mm:ss.SSSSSSSSSSSSSSS")` in UTC, lenient, parsing a
/// prefix: `None` where `parse` throws `ParseException`.
fn parse_date(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    let mut i = 0;
    let mut num = |sep: Option<u8>| -> Option<i64> {
        let start = i;
        let neg = b.get(i) == Some(&b'-');
        if neg {
            i += 1;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == start + neg as usize {
            return None;
        }
        let v: i64 = s[start..i].parse().ok()?;
        if let Some(c) = sep {
            if b.get(i) != Some(&c) {
                return None;
            }
            i += 1;
        }
        Some(v)
    };
    let y = num(Some(b'-'))?;
    let mo = num(Some(b'-'))?;
    let d = num(Some(b' '))?;
    let h = num(Some(b':'))?;
    let mi = num(Some(b':'))?;
    let sec = num(Some(b'.'))?;
    let ms = num(None)?;
    Some((days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + sec) * 1000 + ms)
}

fn npe() -> String {
    "java.lang.NullPointerException".into()
}

fn event(
    spec: &Spec,
    key: String,
    ms: i64,
    name: &str,
    values: Vec<Value>,
    line: &str,
) -> Result<ObservedEvent, String> {
    spec.tuple_event(key, Timestamp::from_millis(ms), name, values, Some(line))
}

fn enum_or_null(spec: &Spec, ty: &str, elem: Option<&str>) -> Value {
    elem.and_then(|e| spec.enum_value(ty, e)).unwrap_or(Value::Null)
}

pub(crate) struct TestCase2Parser;

impl Parser for TestCase2Parser {
    fn name(&self) -> &str {
        "TestCase2Parser"
    }

    fn parse(&self, line: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
        let parts = split(line, " - : ");
        if parts.len() != 2 {
            return Ok(vec![]);
        }
        let tt = split(parts[0], " ");
        let Some(time) = parse_date(&format!("{} {}", get(&tt, 0)?, get(&tt, 1)?)) else { return Ok(vec![]) };
        let (mut transaction_type, mut message_type, mut result, mut msg) = (None, None, None, None);
        let (mut account, mut amount, mut rid) = (0, 0, 0);
        for part in split(parts[1], "; ") {
            let kv = split(part, "=");
            if kv.len() != 2 {
                return Ok(vec![]);
            }
            let v = kv[1];
            match kv[0] {
                "MessageType" => message_type = Some(v),
                "AccountID" if v != "null" => account = parse_long(v)?,
                "RecipientAccountID" if v != "null" => {
                    parse_long(v)?;
                }
                "TransactionType" => transaction_type = Some(v),
                "Amount" if v != "null" => amount = parse_rounded(v)?,
                "TransactionID" if v != "null" => rid = parse_long(v)?,
                "Result" => result = Some(v),
                "Message" => msg = Some(v),
                _ => {}
            }
        }
        let transaction_type = transaction_type.ok_or_else(npe)?;
        if transaction_type == "WITHDRAW" {
            let result = result.ok_or_else(npe)?;
            let status = match result {
                "SUCCESS" => Some("WITHDRAW_SUCCESS"),
                "FAIL" => Some("WITHDRAW_ERROR"),
                _ => None,
            };
            let ev = match message_type.ok_or_else(npe)? {
                "REQUEST" => event(
                    spec,
                    account.to_string(),
                    time,
                    "eWithDrawReq",
                    vec![Value::Int(account), Value::Int(amount), Value::Int(rid)],
                    line,
                )?,
                "RESPONSE" => event(
                    spec,
                    account.to_string(),
                    time,
                    "eWithDrawResp",
                    vec![
                        enum_or_null(spec, "tWithDrawRespStatus", status),
                        Value::Int(account),
                        Value::Int(amount),
                        Value::Int(rid),
                    ],
                    line,
                )?,
                _ => return Ok(vec![]),
            };
            Ok(vec![ev])
        } else if transaction_type == "CREATE_ACCOUNT"
            && message_type.ok_or_else(npe)? == "RESPONSE"
            && result.ok_or_else(npe)? == "SUCCESS"
        {
            let msg = msg.ok_or_else(npe)?;
            let mut bank = SlotMap::new();
            let mut keys = Vec::new();
            for account in split(msg, "], ") {
                let trimmed = account.replace(['[', ']'], "");
                let pair = split(&trimmed, ", ");
                let k = parse_long(get(&pair, 0)?)?;
                let v = parse_rounded(get(&pair, 1)?)?;
                if !keys.contains(&k) {
                    keys.push(k);
                }
                bank.set(Value::Int(k), Value::Int(v));
            }
            let payload = Value::Map(Rc::new(bank));
            keys.into_iter()
                .map(|k| {
                    spec.event(
                        k.to_string(),
                        Timestamp::from_millis(time),
                        "eSpec_BankBalanceIsAlwaysCorrect_Init",
                        payload.clone(),
                        Some(line),
                    )
                })
                .collect()
        } else {
            Ok(vec![])
        }
    }
}

pub(crate) struct TestCase3Parser;

impl Parser for TestCase3Parser {
    fn name(&self) -> &str {
        "TestCase3Parser"
    }

    fn parse(&self, line: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
        let parts = split(line, " - : ");
        if parts.len() != 2 {
            return Ok(vec![]);
        }
        let tt = split(parts[0], " ");
        let Some(time) = parse_date(&format!("{} {}", get(&tt, 0)?, get(&tt, 1)?)) else { return Ok(vec![]) };
        let mut map: Vec<(&str, &str)> = Vec::new();
        for part in split(parts[1], ", ") {
            let kv = split(part, "=");
            if kv.len() != 2 {
                return Ok(vec![]);
            }
            map.retain(|(k, _)| *k != kv[0]);
            map.push((kv[0], kv[1]));
        }
        let get_or = |k: &str, d: &'static str| map.iter().find(|(x, _)| *x == k).map(|(_, v)| *v).unwrap_or(d);
        let long = |v: &str| if v == "null" { Ok(0) } else { parse_long(v) };
        let transaction_type = get_or("TransactionType", "");
        let message_type = get_or("MessageType", "");
        let client = long(get_or("ClientID", "null"))?;
        let lock = long(get_or("LockID", "null"))?;
        let rid = long(get_or("TransactionID", "null"))?;
        let ints = |v: &[i64]| v.iter().map(|&x| Value::Int(x)).collect::<Vec<_>>();
        let ev = match message_type {
            "REQUEST" => match transaction_type {
                "LOCK" => Some(event(spec, lock.to_string(), time, "eLockReq", ints(&[client, lock, rid]), line)?),
                "RELEASE" => {
                    Some(event(spec, lock.to_string(), time, "eReleaseReq", ints(&[client, lock, rid]), line)?)
                }
                _ => None,
            },
            "RESPONSE" => {
                let result = get_or("Result", "");
                let ls = get_or("LockStatus", "null");
                let lock_status = if ls == "null" {
                    Value::Null
                } else {
                    spec.enum_value("tLockStatus", ls).ok_or_else(|| {
                        format!("java.lang.IllegalArgumentException: No enum constant tLockStatus.{}", ls)
                    })?
                };
                let (name, ty, ok, err) = match transaction_type {
                    "LOCK" => ("eLockResp", "tLockRespStatus", "LOCK_SUCCESS", "LOCK_ERROR"),
                    "RELEASE" => ("eReleaseResp", "tReleaseRespStatus", "RELEASE_SUCCESS", "RELEASE_ERROR"),
                    _ => return Ok(vec![]),
                };
                let status = match result {
                    "SUCCESS" => Some(ok),
                    "FAIL" => Some(err),
                    _ => None,
                };
                let values = vec![
                    enum_or_null(spec, ty, status),
                    Value::Int(client),
                    Value::Int(lock),
                    lock_status,
                    Value::Int(rid),
                ];
                Some(event(spec, lock.to_string(), time, name, values, line)?)
            }
            _ => None,
        };
        Ok(ev.into_iter().collect())
    }
}

/// `timestamp, key, value` lines (test cases 4 and 5).
fn int_line(spec: &Spec, line: &str, name: &str) -> Result<Vec<ObservedEvent>, String> {
    let parts = split(line, ",");
    let ts = parse_long(java_trim(get(&parts, 0)?))?;
    let key = parse_long(java_trim(get(&parts, 1)?))?;
    let value = parse_int(java_trim(get(&parts, 2)?))?;
    Ok(vec![event(spec, key.to_string(), ts, name, vec![Value::Int(ts), Value::Int(key), Value::Int(value)], line)?])
}

pub(crate) struct TestCase4Parser;

impl Parser for TestCase4Parser {
    fn name(&self) -> &str {
        "TestCase4Parser"
    }

    fn parse(&self, line: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
        int_line(spec, line, "eNewIncreasingInt")
    }
}

pub(crate) struct TestCase5Parser;

impl Parser for TestCase5Parser {
    fn name(&self) -> &str {
        "TestCase5Parser"
    }

    fn parse(&self, line: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
        int_line(spec, line, "eNewAlternatingInt")
    }
}

pub(crate) struct TestCase6Parser;

impl Parser for TestCase6Parser {
    fn name(&self) -> &str {
        "TestCase6Parser"
    }

    fn parse(&self, line: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
        let p = split(line, ",");
        let f = |i: usize| -> Result<&str, String> { Ok(java_trim(get(&p, i)?)) };
        let ev = if line.starts_with("params") {
            let (ts, key) = (parse_long(f(1)?)?, parse_long(f(2)?)?);
            let (n1, n2, n) = (parse_long(f(3)?)?, parse_long(f(4)?)?, parse_long(f(5)?)?);
            let v = [ts, key, n1, n2, n].map(Value::Int).to_vec();
            event(spec, key.to_string(), ts, "eConfigReceivedDualNodesBinary", v, line)?
        } else if line.starts_with("finished") {
            let (ts, key) = (parse_long(f(1)?)?, parse_long(f(2)?)?);
            event(
                spec,
                key.to_string(),
                ts,
                "eEntriesFinishedDualNodesBinary",
                vec![Value::Int(ts), Value::Int(key)],
                line,
            )?
        } else {
            let (ts, key) = (parse_long(f(0)?)?, parse_long(f(1)?)?);
            let (node, value) = (parse_int(f(2)?)?, parse_int(f(3)?)?);
            let v = [ts, key, node, value].map(Value::Int).to_vec();
            event(spec, key.to_string(), ts, "eNewLogEntryDualNodesBinary", v, line)?
        };
        Ok(vec![ev])
    }
}

pub(crate) struct TestCase7Parser;

impl Parser for TestCase7Parser {
    fn name(&self) -> &str {
        "TestCase7Parser"
    }

    fn parse(&self, line: &str, spec: &Spec) -> Result<Vec<ObservedEvent>, String> {
        let p = split(line, ",");
        let f = |i: usize| -> Result<&str, String> { Ok(java_trim(get(&p, i)?)) };
        let ev = if line.to_lowercase().contains("finished") {
            let (ts, key) = (parse_long(f(0)?)?, parse_long(f(1)?)?);
            event(spec, key.to_string(), ts, "eEntriesFinishedEventPairs", vec![Value::Int(ts), Value::Int(key)], line)?
        } else {
            let (ts, key) = (parse_long(f(0)?)?, parse_long(f(1)?)?);
            let num = parse_int(f(2)?)?;
            let status = f(3)?;
            let name = if status == "start" { "eEventStartEventPairs" } else { "eEventEndEventPairs" };
            event(
                spec,
                key.to_string(),
                ts,
                name,
                vec![Value::Int(ts), Value::Int(key), Value::Int(num), Value::str(status)],
                line,
            )?
        };
        Ok(vec![ev])
    }
}
