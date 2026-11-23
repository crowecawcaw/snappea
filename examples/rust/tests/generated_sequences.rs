mod support;

use inventory_integration_example::Error;
use proptest::prelude::*;
use support::{inventory, model};

#[derive(Clone, Debug)]
enum Command {
    Reserve { sku: usize, order: u8, units: i64 },
    Release { sku: usize, order: u8 },
}

fn command() -> impl Strategy<Value = Command> {
    prop_oneof![
        (0usize..2, 0u8..6, -1i64..7).prop_map(|(sku, order, units)| Command::Reserve { sku, order, units }),
        (0usize..2, 0u8..6).prop_map(|(sku, order)| Command::Release { sku, order }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, max_shrink_iters: 2048, failure_persistence: Some(Box::new(proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"))), ..ProptestConfig::default() })]

    #[test]
    fn generated_stateful_operations(commands in prop::collection::vec(command(), 1..41)) {
        // Runs once per case and shrink attempt: never share inventory or recorder.
        let recorder = model().recording();
        let mut inventory = inventory(recorder.emitter());
        inventory.stock("coffee", 3).unwrap();
        inventory.stock("tea", 2).unwrap();
        for command in &commands {
            let result = match command {
                Command::Reserve { sku, order, units } =>
                    inventory.reserve(["coffee", "tea"][*sku], &format!("order-{order}"), *units),
                Command::Release { sku, order } =>
                    inventory.release(["coffee", "tea"][*sku], &format!("order-{order}")),
            };
            // Rejections are valid input; any other error is a failure.
            prop_assert!(matches!(result, Ok(()) | Err(Error::InvalidQuantity)
                | Err(Error::DuplicateOrder) | Err(Error::InsufficientStock) | Err(Error::UnknownOrder)),
                "unexpected result for {:?}: {:?}", command, result);
        }
        // prop_assert! (not unwrap) lets proptest shrink the command sequence.
        let checked = recorder.verify();
        prop_assert!(checked.is_ok(), "{}", checked.unwrap_err());
        for sku in ["coffee", "tea"] {
            prop_assert!(inventory.available(sku).unwrap() >= 0);
        }
    }
}
