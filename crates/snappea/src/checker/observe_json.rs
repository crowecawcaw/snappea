//! Reading observed events from JSON: ISO-8601 instants and P values from JSON payloads.
//! Shared by `snappea observe`'s parsers and [`crate::test`].

use std::rc::Rc;

use crate::checker::observe::{ObservedEvent, Spec, Timestamp};
use crate::checker::value::{MachineRef, NamedTuple, SlotMap, SlotSet, Value};
use crate::sema::{PType, Program};
use serde_json::Value as Json;

/// Civil date of a day count since 1970-01-01 (proleptic Gregorian).
pub(crate) fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + if m <= 2 { 1 } else { 0 }, m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Parses an ISO-8601 instant (`2024-02-02T17:27:34.309Z`, offsets allowed).
pub fn parse_instant(s: &str) -> Option<Timestamp> {
    let s = s.trim();
    let b = s.as_bytes();
    let num = |a: usize, n: usize| -> Option<i64> {
        let t = s.get(a..a + n)?;
        t.chars().all(|c| c.is_ascii_digit()).then(|| t.parse().ok())?
    };
    let y = num(0, 4)?;
    if b.get(4) != Some(&b'-') || b.get(7) != Some(&b'-') || !matches!(b.get(10), Some(b'T') | Some(b't') | Some(b' '))
    {
        return None;
    }
    let (mo, d, h, mi) = (num(5, 2)?, num(8, 2)?, num(11, 2)?, num(14, 2)?);
    if b.get(13) != Some(&b':') {
        return None;
    }
    let (sec, mut i) = if b.get(16) == Some(&b':') { (num(17, 2)?, 19) } else { (0, 16) };
    let mut nanos: u32 = 0;
    if b.get(i) == Some(&b'.') || b.get(i) == Some(&b',') {
        i += 1;
        let start = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        let frac = &s[start..i];
        if frac.is_empty() {
            return None;
        }
        let mut f: String = frac.chars().take(9).collect();
        while f.len() < 9 {
            f.push('0');
        }
        nanos = f.parse().ok()?;
    }
    let offset = match &s[i..] {
        "Z" | "z" | "" => 0,
        o if (o.starts_with('+') || o.starts_with('-')) && o.len() >= 3 => {
            let sign = if o.starts_with('-') { -1 } else { 1 };
            let rest = o[1..].replace(':', "");
            let hh: i64 = rest.get(0..2)?.parse().ok()?;
            let mm: i64 = if rest.len() >= 4 { rest.get(2..4)?.parse().ok()? } else { 0 };
            sign * (hh * 3600 + mm * 60)
        }
        _ => return None,
    };
    if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || sec > 60 {
        return None;
    }
    let days = days_from_civil(y, mo as u32, d as u32);
    Some(Timestamp::new(days * 86400 + h * 3600 + mi * 60 + sec - offset, nanos))
}

/// Converts JSON to a P value of type `ty`.
///
/// Integers, floats, booleans and strings map directly; enums accept the element name or
/// value; tuples accept an array or an object keyed by field name (or index); sequences and
/// sets take arrays; maps take an object (keys parsed by the key type) or an array of
/// `[key, value]` pairs; machines take their id number or `Name(id)`; events their name.
pub fn value_from_json(prog: &Program, j: &Json, ty: &PType) -> Result<Value, String> {
    let bad = || format!("cannot read {} as {}", j, type_name(prog, ty));
    Ok(match ty {
        PType::TypeDef(t) => return value_from_json(prog, j, &prog.typedefs[t.idx()].ty),
        _ if j.is_null() && !matches!(ty, PType::Bool | PType::Int | PType::Float | PType::Enum(_)) => Value::Null,
        PType::Bool => Value::Bool(j.as_bool().ok_or_else(bad)?),
        PType::Int => Value::Int(json_int(j).ok_or_else(bad)?),
        PType::Float => Value::Float(j.as_f64().ok_or_else(bad)? as f32 as f64),
        PType::String => Value::str(j.as_str().ok_or_else(bad)?),
        PType::Enum(e) => {
            let en = &prog.enums[e.idx()];
            match j {
                Json::String(s) => Value::Int(en.elems.iter().find(|x| &x.name == s).ok_or_else(bad)?.value),
                _ => {
                    let v = json_int(j).ok_or_else(bad)?;
                    en.elems.iter().find(|x| x.value == v).ok_or_else(bad)?;
                    Value::Int(v)
                }
            }
        }
        PType::Machine | PType::Permission(_) => {
            let id = match j {
                Json::String(s) => {
                    let inner = s.rsplit_once('(').map(|(_, r)| r.trim_end_matches(')')).unwrap_or(s);
                    inner.trim().parse::<u64>().map_err(|_| bad())?
                }
                _ => j.as_u64().ok_or_else(bad)?,
            };
            let name = match j {
                Json::String(s) => s.clone(),
                _ => id.to_string(),
            };
            Value::Machine(Rc::new(MachineRef { id, name: name.into(), permissions: Rc::from(Vec::new()) }))
        }
        PType::Event => {
            let s = j.as_str().ok_or_else(bad)?;
            let s = s.rsplit('.').next().unwrap_or(s);
            let id = prog.events.iter().position(|e| e.name == s).ok_or_else(bad)?;
            Value::Event(crate::sema::EventId(id as u32))
        }
        PType::Tuple(ts) => {
            let items: Vec<&Json> = match j {
                Json::Array(a) => a.iter().collect(),
                Json::Object(o) => (0..ts.len()).map(|i| o.get(&i.to_string()).unwrap_or(&Json::Null)).collect(),
                _ => return Err(bad()),
            };
            if items.len() != ts.len() {
                return Err(bad());
            }
            Value::Tuple(Rc::new(
                items.iter().zip(ts).map(|(v, t)| value_from_json(prog, v, t)).collect::<Result<_, _>>()?,
            ))
        }
        PType::NamedTuple(fs) => {
            let mut values = Vec::new();
            for (i, (n, t)) in fs.iter().enumerate() {
                let v = match j {
                    Json::Object(o) => o.get(n).ok_or_else(|| format!("missing field '{}' in {}", n, j))?,
                    Json::Array(a) if a.len() == fs.len() => &a[i],
                    _ => return Err(bad()),
                };
                values.push(value_from_json(prog, v, t)?);
            }
            let names: Vec<String> = fs.iter().map(|(n, _)| n.clone()).collect();
            Value::NamedTuple(Rc::new(NamedTuple { names: names.into(), values }))
        }
        PType::Seq(t) => {
            let a = j.as_array().ok_or_else(bad)?;
            Value::Seq(Rc::new(a.iter().map(|v| value_from_json(prog, v, t)).collect::<Result<_, _>>()?))
        }
        PType::Set(t) => {
            let a = j.as_array().ok_or_else(bad)?;
            let mut s = SlotSet::new();
            for v in a {
                s.insert(value_from_json(prog, v, t)?);
            }
            Value::Set(Rc::new(s))
        }
        PType::Map(kt, vt) => {
            let mut m = SlotMap::new();
            match j {
                Json::Object(o) => {
                    for (k, v) in o {
                        m.set(map_key(prog, k, kt)?, value_from_json(prog, v, vt)?);
                    }
                }
                Json::Array(a) => {
                    for pair in a {
                        match pair.as_array().map(|p| p.as_slice()) {
                            Some([k, v]) => m.set(value_from_json(prog, k, kt)?, value_from_json(prog, v, vt)?),
                            _ => return Err(bad()),
                        }
                    }
                }
                _ => return Err(bad()),
            }
            Value::Map(Rc::new(m))
        }
        PType::Any | PType::Data | PType::Null | PType::Error | PType::Foreign(_) => infer(j),
    })
}

pub(crate) fn json_int(j: &Json) -> Option<i64> {
    j.as_i64().or_else(|| j.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64))
}

/// A JSON object key read as a map key of type `ty`.
fn map_key(prog: &Program, k: &str, ty: &PType) -> Result<Value, String> {
    let mut t = ty;
    while let PType::TypeDef(id) = t {
        t = &prog.typedefs[id.idx()].ty;
    }
    if matches!(t, PType::String) {
        // P traces quote string keys
        if k.len() >= 2 && k.starts_with('"') && k.ends_with('"') {
            return Ok(Value::str(&k[1..k.len() - 1].replace("\\\"", "\"")));
        }
        return Ok(Value::str(k));
    }
    let j: Json = serde_json::from_str(k).unwrap_or_else(|_| Json::String(k.to_string()));
    value_from_json(prog, &j, ty)
}

fn infer(j: &Json) -> Value {
    match j {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => match n.as_i64() {
            Some(i) => Value::Int(i),
            None => Value::Float(n.as_f64().unwrap_or(0.0) as f32 as f64),
        },
        Json::String(s) => Value::str(s),
        Json::Array(a) => Value::Seq(Rc::new(a.iter().map(infer).collect())),
        Json::Object(o) => {
            let names: Vec<String> = o.keys().cloned().collect();
            Value::NamedTuple(Rc::new(NamedTuple { names: names.into(), values: o.values().map(infer).collect() }))
        }
    }
}

fn type_name(prog: &Program, ty: &PType) -> String {
    match ty {
        PType::Enum(e) => prog.enums[e.idx()].name.clone(),
        PType::TypeDef(t) => prog.typedefs[t.idx()].name.clone(),
        other => format!("{:?}", other).to_lowercase(),
    }
}

/// Builds the event `name` with a JSON payload; `Ok(None)` if the program declares no such
/// event.
pub fn event_from_json(
    spec: &Spec,
    key: String,
    timestamp: Timestamp,
    name: &str,
    payload: Option<&Json>,
    log_line: Option<&str>,
) -> Result<Option<ObservedEvent>, String> {
    let Some(event) = spec.event_id(name) else { return Ok(None) };
    let ty = spec.payload_type(event).clone();
    let payload = match payload {
        None | Some(Json::Null) => Value::Null,
        Some(p) => value_from_json(spec.prog, p, &ty)?,
    };
    Ok(Some(ObservedEvent {
        key,
        timestamp,
        event,
        payload,
        log_line: log_line.map(String::from),
        custom_payload: None,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instants() {
        assert_eq!(parse_instant("1970-01-01T00:00:00Z"), Some(Timestamp::new(0, 0)));
        assert_eq!(parse_instant("2024-02-02T17:27:34.309Z").map(|t| t.to_millis()), Some(1706894854309));
        assert_eq!(parse_instant("2024-02-02T18:27:34.309+01:00").map(|t| t.to_millis()), Some(1706894854309));
        assert_eq!(parse_instant("nope"), None);
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2024, 2, 2), 19755);
        assert_eq!(civil_from_days(19755), (2024, 2, 2));
    }
}
