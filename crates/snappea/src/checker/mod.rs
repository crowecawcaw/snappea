//! Systematic testing of P programs.
//!
//! The checker interprets a [`crate::sema::Program`] directly (no code generation)
//! under a controlled, deterministic scheduler, reproducing the behaviour of
//! the upstream P checker (`PChecker`) closely enough that the same bugs are
//! found and the same trace lines are produced.
//!
//! No threads or process APIs, and no file access outside
//! [`observe_inputs::StdFiles`], so it compiles to `wasm32`.

mod auto;
pub mod bytecode;
pub mod code_coverage;
pub mod compile;
pub mod coverage;
pub mod driver_facts;
pub mod engine;
pub mod exhaustive;
pub mod feedback;
pub mod foreign;
pub mod interp;
pub mod javafmt;
pub mod jsonlog;
pub mod mutation;
pub mod observe;
pub mod observe_inputs;
pub mod observe_json;
pub mod observe_report;
pub mod random;
pub mod rl;
pub mod runtime;
pub mod statehash;
pub mod strategy;
pub mod time;
pub mod timeline;
pub mod value;
pub mod xmllog;
