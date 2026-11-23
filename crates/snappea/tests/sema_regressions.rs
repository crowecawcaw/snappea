//! Every upstream regression test type-checks, or fails to, as upstream expects.

use std::path::Path;

mod common {
    pub(crate) mod corpus;
}

#[test]
fn upstream_regression_tests() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/upstream/RegressionTests");
    let outcomes = common::corpus::run(&root);
    assert_eq!(outcomes.len(), 424, "number of regression tests");
    let mismatched: Vec<String> = outcomes
        .iter()
        .filter(|t| !t.ok)
        .map(|t| {
            let outcome = if t.failed { "failed" } else { "passed" };
            format!("{} ({}): {outcome} {}", t.dir.display(), t.category, t.messages.join(" | "))
        })
        .collect();
    assert!(mismatched.is_empty(), "outcome differs from upstream:\n{}", mismatched.join("\n"));
    let params: Vec<_> = outcomes.iter().filter_map(|t| t.params.as_ref().map(|p| (&t.dir, p))).collect();
    assert_eq!(params.len(), 2, "number of parametric test expectations");
    for (dir, (want, got)) in params {
        assert_eq!(want, got, "parametric tests of {}", dir.display());
    }
}
