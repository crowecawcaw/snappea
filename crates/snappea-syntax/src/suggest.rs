//! "Did you mean" suggestions for misspelled names.

/// The candidate closest to `name`, if it is close enough to be a likely misspelling of
/// it: for "did you mean" hints on names, events, commands, options and lint rule IDs.
pub fn closest<'a>(name: &str, candidates: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    let limit = (name.chars().count() / 3).max(1);
    candidates
        .into_iter()
        .filter(|c| *c != name)
        .map(|c| (edit_distance(c, name), c))
        .filter(|(d, c)| *d <= limit && *d < c.chars().count().min(name.chars().count()))
        .min()
        .map(|(_, c)| c)
}

/// Edits (insert, delete, substitute, or swap two adjacent chars) between `a` and `b`,
/// ignoring ASCII case.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().map(|c| c.to_ascii_lowercase()).collect();
    let b: Vec<char> = b.chars().map(|c| c.to_ascii_lowercase()).collect();
    // rows i-2, i-1 and i of the distance table
    let mut before: Vec<usize> = Vec::new();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut cur = vec![i; b.len() + 1];
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            cur[j] = (prev[j] + 1).min(cur[j - 1] + 1).min(prev[j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                cur[j] = cur[j].min(before[j - 2] + 1);
            }
        }
        before = std::mem::replace(&mut prev, cur);
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances() {
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        assert_eq!(edit_distance("eTikc", "eTick"), 1);
        assert_eq!(edit_distance("STOCK", "stock"), 0);
        assert_eq!(edit_distance("", "ab"), 2);
    }

    #[test]
    fn suggestions() {
        assert_eq!(closest("eTikc", ["eTick", "eTock"]), Some("eTick"));
        assert_eq!(closest("Stok", ["Stock"]), Some("Stock"));
        assert_eq!(closest("eA", ["eB"]), Some("eB"));
        assert_eq!(closest("x", ["y"]), None);
    }
}
