//! Timeline abstractions of a schedule (`TimelineObserver` and its `ITimelineRepresentation`s):
//! the event deliveries of one schedule turned into a set of tokens, which the feedback
//! strategies use as their diversity signal and the report counts as explored timelines.

use rustc_hash::FxHashSet;
use std::rc::Rc;

/// Field separator inside tokens.
const S: char = '\u{1f}';

/// `--timeline-repr`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TimelineRepr {
    /// Per receiver, ordered pairs of labels (upstream's default).
    #[default]
    Pairwise,
    /// Per receiver, contiguous label sequences of length 1..k.
    KGram,
    /// Happens-before / concurrency of every pair of deliveries.
    Causal,
    /// Causal and k-gram tokens together.
    Hybrid,
}

impl TimelineRepr {
    /// `ITimelineRepresentation.Create` (unknown names fall back to pairwise).
    pub fn parse(s: &str) -> TimelineRepr {
        match s.to_lowercase().as_str() {
            "kgram" => TimelineRepr::KGram,
            "causal" => TimelineRepr::Causal,
            "hybrid" => TimelineRepr::Hybrid,
            _ => TimelineRepr::Pairwise,
        }
    }

    /// Whether deliveries carry vector clocks.
    pub fn causal(self) -> bool {
        matches!(self, TimelineRepr::Causal | TimelineRepr::Hybrid)
    }

    fn kgram(self) -> bool {
        matches!(self, TimelineRepr::KGram | TimelineRepr::Hybrid)
    }
}

/// How schedules are abstracted into timelines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimelineConfig {
    pub repr: TimelineRepr,
    /// `--timeline-kgram`.
    pub kgram: usize,
    /// `--timeline-payload`: labels carry a hash of the event payload.
    pub payload: bool,
}

impl Default for TimelineConfig {
    fn default() -> Self {
        TimelineConfig { repr: TimelineRepr::Pairwise, kgram: 3, payload: false }
    }
}

/// A machine's vector clock (`VectorTime`): `(machine operation id, count)` sorted by id.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VectorTime(Vec<(u64, u32)>);

impl VectorTime {
    pub fn new(own: u64) -> VectorTime {
        VectorTime(vec![(own, 0)])
    }

    fn slot(&mut self, id: u64) -> &mut u32 {
        let i = match self.0.binary_search_by_key(&id, |e| e.0) {
            Ok(i) => i,
            Err(i) => {
                self.0.insert(i, (id, 0));
                i
            }
        };
        &mut self.0[i].1
    }

    pub fn increment(&mut self, own: u64) {
        *self.slot(own) += 1;
    }

    pub fn merge(&mut self, other: &VectorTime) {
        for &(id, t) in &other.0 {
            let s = self.slot(id);
            *s = (*s).max(t);
        }
    }

    /// Negative: `self` happens before `other`; positive: after; 0: equal or concurrent.
    pub fn compare(&self, other: &VectorTime) -> i32 {
        let (mut less, mut greater) = (false, false);
        let (a, b) = (&self.0, &other.0);
        let (mut i, mut j) = (0, 0);
        while i < a.len() || j < b.len() {
            let (x, y) = match (a.get(i), b.get(j)) {
                (Some(p), Some(q)) if p.0 == q.0 => {
                    i += 1;
                    j += 1;
                    (p.1, q.1)
                }
                (Some(p), Some(q)) if p.0 < q.0 => {
                    i += 1;
                    (p.1, 0)
                }
                (Some(p), None) => {
                    i += 1;
                    (p.1, 0)
                }
                (_, Some(q)) => {
                    j += 1;
                    (0, q.1)
                }
                (None, None) => unreachable!(),
            };
            less |= x < y;
            greater |= x > y;
            if less && greater {
                return 0;
            }
        }
        match (less, greater) {
            (true, false) => -1,
            (false, true) => 1,
            _ => 0,
        }
    }
}

/// `prefix` followed by `parts` separated by [`S`]. Builds the strings of
/// [`Timeline::tokens`] without `format!`, which feedback-guided search calls for every
/// token of every schedule.
fn join_token(prefix: &str, parts: &[&str]) -> String {
    let len = prefix.len() + parts.iter().map(|p| p.len() + S.len_utf8()).sum::<usize>();
    let mut t = String::with_capacity(len);
    t.push_str(prefix);
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            t.push(S);
        }
        t.push_str(p);
    }
    t
}

/// A 128-bit hash of a receiver or label name, for [`Timeline::fingerprint`].
fn name_hash(s: &str) -> u128 {
    let (mut hi, mut lo) = (0x4528_21e6_38d0_1377u64 ^ s.len() as u64, 0xbe54_66cf_34e9_0c6cu64 ^ s.len() as u64);
    for chunk in s.as_bytes().chunks(8) {
        let mut word = [0u8; 8];
        word[..chunk.len()].copy_from_slice(chunk);
        let w = u64::from_le_bytes(word);
        hi = mix64(hi ^ w).wrapping_add(0x9e37_79b9_7f4a_7c15);
        lo = mix64(lo.rotate_left(29) ^ w);
    }
    ((mix64(hi) as u128) << 64) | mix64(lo) as u128
}

/// The SplitMix64 finalizer: a fast, well-mixing bijection on 64 bits.
fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94d0_49bb_1331_11eb);
    x ^ (x >> 31)
}

/// A 128-bit hash of one token: its kind followed by the hashes of its parts. The parts
/// are already random (see [`name_hash`]), so a fast mix suffices: the two 64-bit halves
/// of the result chain independent halves of the name hashes. Both halves see every small
/// part (kind, length, flag) in full.
fn token_hash(parts: &[u128]) -> u128 {
    let (mut hi, mut lo) = (0x243f_6a88_85a3_08d3u64, 0x1319_8a2e_0370_7344u64);
    for p in parts {
        hi = mix64(hi ^ (*p >> 64) as u64 ^ *p as u64);
        lo = mix64(lo ^ *p as u64);
    }
    ((hi as u128) << 64) | lo as u128
}

/// The timeline of one schedule.
#[derive(Debug, Default, Clone)]
pub struct Timeline {
    pub config: TimelineConfig,
    receivers: Vec<Rc<str>>,
    labels: Vec<Rc<str>>,
    /// [`name_hash`] of each receiver and label.
    receiver_hashes: Vec<u128>,
    label_hashes: Vec<u128>,
    /// The caller's key of each receiver and label, if it gave one (see [`Timeline::record_keyed`]).
    receiver_keys: Vec<Option<u64>>,
    label_keys: Vec<Option<u64>>,
    /// The sum of the hashes of the distinct tokens so far (see [`Timeline::fingerprint`]).
    fingerprint: u128,
    /// Pairwise: the labels seen so far by each receiver, as a bitset.
    seen: Vec<Vec<u64>>,
    /// Pairwise: the tokens `(receiver, previous label, label)`, as a bitset of previous
    /// labels per receiver and label: each event checks all previous labels at once.
    pairs: Vec<Vec<Vec<u64>>>,
    /// Pairwise: the number of tokens in `pairs`.
    pair_count: usize,
    /// K-gram: delivery sequence per receiver.
    seqs: Vec<Vec<u32>>,
    /// K-gram: `(receiver, gram)`.
    grams: FxHashSet<(u32, Vec<u32>)>,
    /// Causal: deliveries so far.
    deliveries: Vec<(u32, u32, Option<Rc<VectorTime>>)>,
    /// Causal: `(concurrent, a, b)` (`hb a b` or `co a b`).
    orders: FxHashSet<(bool, u32, u32)>,
}

impl Timeline {
    pub fn new(config: TimelineConfig) -> Timeline {
        Timeline { config, ..Default::default() }
    }

    /// Empties the timeline for a new schedule, keeping its memory.
    pub fn reset(&mut self, config: TimelineConfig) {
        self.config = config;
        self.receivers.clear();
        self.labels.clear();
        self.receiver_hashes.clear();
        self.label_hashes.clear();
        self.receiver_keys.clear();
        self.label_keys.clear();
        self.fingerprint = 0;
        // the per-receiver lists keep their memory: clear each one, not the outer list
        for v in &mut self.seen {
            v.clear();
        }
        for v in self.pairs.iter_mut().flatten() {
            v.clear();
        }
        self.pair_count = 0;
        for v in &mut self.seqs {
            v.clear();
        }
        self.grams.clear();
        self.deliveries.clear();
        self.orders.clear();
    }

    /// The index of name `s` in `table`, added if new. A name with a `key` is looked up by
    /// the key first, which saves comparing strings on every event.
    fn intern(
        table: &mut Vec<Rc<str>>,
        hashes: &mut Vec<u128>,
        keys: &mut Vec<Option<u64>>,
        key: Option<u64>,
        s: &str,
    ) -> u32 {
        if let Some(i) = key.and_then(|k| keys.iter().position(|x| *x == Some(k))) {
            return i as u32;
        }
        match table.iter().position(|x| &**x == s) {
            Some(i) => {
                keys[i] = keys[i].or(key);
                i as u32
            }
            None => {
                table.push(Rc::from(s));
                hashes.push(name_hash(s));
                keys.push(key);
                (table.len() - 1) as u32
            }
        }
    }

    pub fn record(&mut self, receiver: &str, label: &str, time: Option<Rc<VectorTime>>) {
        self.record_keyed((None, receiver), (None, label), time)
    }

    /// [`Timeline::record`] with optional keys for the receiver and label: the same key
    /// must always come with the same name (a machine id with its name, say).
    pub fn record_keyed(
        &mut self,
        (receiver_key, receiver): (Option<u64>, &str),
        (label_key, label): (Option<u64>, &str),
        time: Option<Rc<VectorTime>>,
    ) {
        let r = Self::intern(
            &mut self.receivers,
            &mut self.receiver_hashes,
            &mut self.receiver_keys,
            receiver_key,
            receiver,
        );
        let l = Self::intern(&mut self.labels, &mut self.label_hashes, &mut self.label_keys, label_key, label);
        let repr = self.config.repr;
        if repr == TimelineRepr::Pairwise {
            let (ri, li) = (r as usize, l as usize);
            let words = self.labels.len().div_ceil(64);
            if self.seen.len() <= ri {
                self.seen.resize(ri + 1, Vec::new());
                self.pairs.resize(ri + 1, Vec::new());
            }
            if self.pairs[ri].len() <= li {
                self.pairs[ri].resize(li + 1, Vec::new());
            }
            let seen = &mut self.seen[ri];
            seen.resize(words, 0);
            let pairs = &mut self.pairs[ri][li];
            pairs.resize(words, 0);
            // the pairs (prev, l) for every label prev seen before, l included if seen
            for (w, (seen_word, pair_word)) in seen.iter().zip(pairs.iter_mut()).enumerate() {
                let mut new = seen_word & !*pair_word;
                *pair_word |= new;
                while new != 0 {
                    let prev = w * 64 + new.trailing_zeros() as usize;
                    new &= new - 1;
                    self.pair_count += 1;
                    let parts = [0, self.receiver_hashes[ri], self.label_hashes[prev], self.label_hashes[li]];
                    self.fingerprint = self.fingerprint.wrapping_add(token_hash(&parts));
                }
            }
            seen[li / 64] |= 1 << (li % 64);
        }
        if repr.kgram() {
            if self.seqs.len() <= r as usize {
                self.seqs.resize(r as usize + 1, Vec::new());
            }
            let seq = &mut self.seqs[r as usize];
            seq.push(l);
            let k = self.config.kgram.max(1);
            for len in 1..=k.min(seq.len()) {
                let gram = &seq[seq.len() - len..];
                if self.grams.insert((r, gram.to_vec())) {
                    let mut parts = vec![1, self.receiver_hashes[r as usize], len as u128];
                    parts.extend(gram.iter().map(|l| self.label_hashes[*l as usize]));
                    self.fingerprint = self.fingerprint.wrapping_add(token_hash(&parts));
                }
            }
        }
        if repr.causal() {
            for (pr, pl, pt) in &self.deliveries {
                let cmp = match (pt, &time) {
                    (Some(p), Some(t)) => p.compare(t),
                    _ => 0,
                };
                let token = if cmp < 0 || (cmp == 0 && *pr == r) {
                    (false, *pl, l)
                } else if cmp > 0 {
                    (false, l, *pl)
                } else if self.labels[*pl as usize] <= self.labels[l as usize] {
                    (true, *pl, l)
                } else {
                    (true, l, *pl)
                };
                if self.orders.insert(token) {
                    let (co, a, b) = token;
                    let parts = [2, co as u128, self.label_hashes[a as usize], self.label_hashes[b as usize]];
                    self.fingerprint = self.fingerprint.wrapping_add(token_hash(&parts));
                }
            }
            self.deliveries.push((r, l, time));
        }
    }

    fn label(&self, l: u32) -> &str {
        &self.labels[l as usize]
    }

    fn causal_tokens(&self, prefix: &str, out: &mut Vec<String>) {
        for (co, a, b) in &self.orders {
            out.push(join_token(prefix, &[if *co { "co" } else { "hb" }, self.label(*a), self.label(*b)]));
        }
    }

    fn kgram_tokens(&self, prefix: &str, out: &mut Vec<String>) {
        for (r, gram) in &self.grams {
            let mut t = format!("{}{}{}{}{}", prefix, self.receivers[*r as usize], S, gram.len(), S);
            for (i, l) in gram.iter().enumerate() {
                if i > 0 {
                    t.push('>');
                }
                t.push_str(self.label(*l));
            }
            out.push(t);
        }
    }

    /// The timeline tokens, sorted ordinally.
    pub fn tokens(&self) -> Vec<String> {
        let mut v = Vec::new();
        match self.config.repr {
            TimelineRepr::Pairwise => {
                v.reserve(self.pair_count);
                for (r, by_label) in self.pairs.iter().enumerate() {
                    for (l, prevs) in by_label.iter().enumerate() {
                        for (w, word) in prevs.iter().enumerate() {
                            let mut bits = *word;
                            while bits != 0 {
                                let p = w * 64 + bits.trailing_zeros() as usize;
                                bits &= bits - 1;
                                v.push(join_token(
                                    "",
                                    &[&self.receivers[r], self.label(p as u32), self.label(l as u32), ""],
                                ));
                            }
                        }
                    }
                }
            }
            TimelineRepr::KGram => self.kgram_tokens("", &mut v),
            TimelineRepr::Causal => self.causal_tokens("", &mut v),
            TimelineRepr::Hybrid => {
                self.causal_tokens(&format!("C{}", S), &mut v);
                self.kgram_tokens(&format!("K{}", S), &mut v);
            }
        }
        v.sort();
        v
    }

    /// `GetAbstractTimeline`: the sorted tokens joined with `;`.
    pub fn abstract_timeline(&self) -> String {
        self.tokens().join(";")
    }

    /// A 128-bit fingerprint of the abstract timeline. Schedules with the same
    /// [`Timeline::tokens`] have the same fingerprint, and different ones, with
    /// overwhelming probability, different fingerprints. It is kept up to date as events
    /// are recorded, so reading it costs nothing; use it to count distinct timelines.
    pub fn fingerprint(&self) -> u128 {
        self.fingerprint
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tl(repr: TimelineRepr) -> Timeline {
        Timeline::new(TimelineConfig { repr, kgram: 2, payload: false })
    }

    #[test]
    fn pairwise() {
        let mut t = tl(TimelineRepr::Pairwise);
        t.record("A(1)", "x", None);
        t.record("A(1)", "y", None);
        t.record("A(1)", "x", None);
        assert_eq!(
            t.tokens(),
            vec!["A(1)\u{1f}x\u{1f}x\u{1f}", "A(1)\u{1f}x\u{1f}y\u{1f}", "A(1)\u{1f}y\u{1f}x\u{1f}"]
        );
    }

    #[test]
    fn kgram() {
        let mut t = tl(TimelineRepr::KGram);
        t.record("A(1)", "x", None);
        t.record("A(1)", "y", None);
        t.record("A(1)", "x", None);
        assert_eq!(
            t.tokens(),
            vec!["A(1)\u{1f}1\u{1f}x", "A(1)\u{1f}1\u{1f}y", "A(1)\u{1f}2\u{1f}x>y", "A(1)\u{1f}2\u{1f}y>x"]
        );
    }

    #[test]
    fn causal_orders_by_clock_then_receiver() {
        let mut t = tl(TimelineRepr::Causal);
        let mut a = VectorTime::new(1);
        t.record("A(1)", "y", Some(Rc::new(a.clone())));
        a.increment(1);
        let mut b = VectorTime::new(2);
        b.merge(&a);
        t.record("B(2)", "x", Some(Rc::new(b)));
        t.record("C(3)", "a", Some(Rc::new(VectorTime::new(3))));
        assert_eq!(t.tokens(), vec!["co\u{1f}a\u{1f}y", "hb\u{1f}a\u{1f}x", "hb\u{1f}y\u{1f}x"]);
    }

    /// Fingerprints agree exactly when the token sets agree, for every representation and
    /// whatever order the receivers and labels were first seen in.
    #[test]
    fn fingerprints_match_tokens() {
        let runs: [&[(&str, &str)]; 6] = [
            &[("A(1)", "x"), ("A(1)", "y"), ("A(1)", "x")],
            &[("B(2)", "z"), ("A(1)", "x"), ("A(1)", "y"), ("A(1)", "x")],
            &[("A(1)", "x"), ("A(1)", "x"), ("A(1)", "y")],
            &[("A(1)", "y"), ("A(1)", "x"), ("A(1)", "y"), ("A(1)", "x")],
            &[("A(1)", "x"), ("B(2)", "y"), ("A(1)", "y"), ("A(1)", "x")],
            &[],
        ];
        for repr in [TimelineRepr::Pairwise, TimelineRepr::KGram, TimelineRepr::Causal, TimelineRepr::Hybrid] {
            let timelines: Vec<Timeline> = runs
                .iter()
                .map(|run| {
                    let mut t = tl(repr);
                    for (i, (r, l)) in run.iter().enumerate() {
                        let mut vt = VectorTime::new(i as u64 % 2);
                        vt.increment(i as u64 % 2);
                        t.record(r, l, Some(Rc::new(vt)));
                    }
                    t
                })
                .collect();
            for a in &timelines {
                for b in &timelines {
                    assert_eq!(a.tokens() == b.tokens(), a.fingerprint() == b.fingerprint(), "{:?}", repr);
                }
            }
            // keys only speed up the lookup of names
            for (run, unkeyed) in runs.iter().zip(&timelines) {
                let mut t = tl(repr);
                for (i, (r, l)) in run.iter().enumerate() {
                    let mut vt = VectorTime::new(i as u64 % 2);
                    vt.increment(i as u64 % 2);
                    let key = |s: &str| Some(s.bytes().map(u64::from).sum());
                    let label_key = if i % 3 == 0 { None } else { key(l) };
                    t.record_keyed((key(r), r), (label_key, l), Some(Rc::new(vt)));
                }
                assert_eq!(t.tokens(), unkeyed.tokens());
                assert_eq!(t.fingerprint(), unkeyed.fingerprint());
            }
        }
    }

    #[test]
    fn vector_time_compare_uses_key_union() {
        let mut a = VectorTime::new(1);
        let b = VectorTime::new(2);
        assert_eq!(a.compare(&b), 0);
        a.increment(1);
        assert_eq!(a.compare(&b), 1);
        assert_eq!(b.compare(&a), -1);
    }
}
