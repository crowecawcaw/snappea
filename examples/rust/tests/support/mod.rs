// Inventory-specific event mapping and fixtures; snappea::test does the checking.
#![allow(dead_code)]

use inventory_integration_example::{Event, Inventory};
use snappea::test::{json, Emitter, Model, Recording};
use std::sync::LazyLock;

pub type TestInventory = Inventory<Box<dyn FnMut(Event)>>;

/// Compiled once from `[package.metadata.snappea]` in Cargo.toml.
pub fn model() -> &'static Model {
    static MODEL: LazyLock<Model> = LazyLock::new(|| Model::load().expect("inventory specification must compile"));
    &MODEL
}

pub fn inventory(emitter: Emitter) -> TestInventory {
    Inventory::new(Box::new(move |event| record(&emitter, event)))
}

pub fn record(emitter: &Emitter, event: Event) {
    match event {
        Event::Stocked { sku, units } => emitter.emit("eStocked", sku, json!({"units": units})),
        Event::Reserved { sku, order, units } => {
            emitter.emit("eReserved", sku, json!({"order": order, "units": units}))
        }
        Event::Released { sku, order, units } => {
            emitter.emit("eReleased", sku, json!({"order": order, "units": units}))
        }
    }
}

/// Records from before seeding until `test` returns, then checks, panicking on a
/// violation. Nothing is checked while a panic unwinds, so finish cleanup inside
/// `test`. Each check merges monitor coverage into target/coverage/monitor.json.
pub fn with_inventory(test: impl FnOnce(&mut TestInventory)) {
    static FIXTURE: LazyLock<Model> = LazyLock::new(|| {
        model().clone().coverage(concat!(env!("CARGO_MANIFEST_DIR"), "/target/coverage/monitor.json"))
    });
    FIXTURE.check(|recording: &Recording| {
        let mut inventory = inventory(recording.emitter());
        inventory.stock("coffee", 3).unwrap();
        inventory.stock("tea", 2).unwrap();
        test(&mut inventory);
    });
}
