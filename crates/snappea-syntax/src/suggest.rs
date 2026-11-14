//! "Did you mean" suggestions for misspelled names.

/// The candidate closest to `name`, if it is close enough to be a likely misspelling of
/// it: for "did you mean" hints on names, commands, options and lint rule IDs.
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).max(1);
    candidates
        .into_iter()
        .filter(|c| *c != name)
        .map(|c| (if c.eq_ignore_ascii_case(name) { 0 } else { edit_distance(c, name) }, c))
        .filter(|(d, c)| *d <= limit && *d < c.chars().count().min(name.chars().count()))
        .min()
        .map(|(_, c)| c)
}

/// Levenshtein distance over chars.
pub fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != *cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}
