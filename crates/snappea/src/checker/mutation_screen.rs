//! Four bounded random witnesses shared within one mutation worker. Each sample
//! starts on the original model. A mutant can fork only before its changed root
//! function first acquired a frame (including suspended receive handlers).
use std::cell::RefCell;
use std::rc::Rc;

use super::compile::{compile_program_plain, compile_random_variant, Code};
use super::engine::{run_config, CheckerOptions};
use super::random::RandomValueGenerator;
use super::runtime::{Prefix, Runtime, SavePolicy, Setup};
use super::strategy::RandomStrategy;
use crate::sema::{FunId, Program, TestCase};

/// Where a screened sample saves prefixes: densely, so mutants fork close to their first effect.
const FORK_DEPTHS: SavePolicy = SavePolicy::AtDepths(&[4, 8, 16, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512]);

struct Sample {
    seed: u32,
    touched: Vec<bool>,
    prefixes: Vec<Prefix>,
}

pub(crate) struct MutationScreen {
    base: Code,
    // Bound memory to one test's corpus per worker, also on multi-test projects.
    corpus: Option<(usize, Vec<Sample>)>,
}

impl MutationScreen {
    pub(crate) fn new(prog: &Program) -> Self {
        Self { base: compile_program_plain(prog), corpus: None }
    }

    /// Returns executed mutant schedules and whether one witnessed a failure.
    /// Misses carry no verdict, including when no seed reaches the changed code.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check(
        &mut self,
        original: &Program,
        variant: &Program,
        mut root: FunId,
        test: usize,
        tc: &TestCase,
        opts: &CheckerOptions,
        cancelled: &dyn Fn() -> bool,
    ) -> (usize, bool) {
        while let Some(parent) = original.fun(root).parent {
            root = parent;
        }
        let (module, Some(main)) = original.test_case_module(tc) else { return (0, false) };
        let setup = || Setup {
            assignment: original.test_case_assignment(tc),
            cancelled: Some(cancelled),
            ..Default::default()
        };
        let config = run_config(opts);
        // A shorter screen bound only stops sampling. It cannot become a detection.
        let bound = match opts.max_fair_steps {
            0 => 1000,
            max => max.min(1000),
        };
        if self.corpus.as_ref().is_none_or(|(id, _)| *id != test) {
            let mut samples = Vec::new();
            for offset in 0..4 {
                if cancelled() {
                    return (0, false);
                }
                let seed = opts.seed.wrapping_add(offset);
                let rng = Rc::new(RefCell::new(RandomValueGenerator::new(seed)));
                let mut strategy = RandomStrategy::new(bound, rng.clone());
                let mut rt = Runtime::new(original, &self.base, module, &config, &mut strategy, setup());
                rt.touched.resize(original.funs.len(), false);
                rt.checkpoint_rng = Some(rng);
                let mut prefixes = Vec::new();
                rt.run_test_saving(main, &tc.name, Some((&mut prefixes, FORK_DEPTHS)));
                rt.check_hot_monitors_at_termination();
                if rt.bug.is_none() && !cancelled() {
                    samples.push(Sample { seed, touched: rt.touched, prefixes });
                }
            }
            self.corpus = Some((test, samples));
        }
        let mut code = None;
        let mut ran = 0;
        for sample in &self.corpus.as_ref().unwrap().1 {
            if cancelled() {
                break;
            }
            if !sample.touched[root.idx()] {
                continue;
            }
            let code = code.get_or_insert_with(|| compile_random_variant(variant, root, &self.base));
            let prefix = sample.prefixes.iter().rev().find(|p| !p.touched[root.idx()]);
            let random =
                prefix.map_or_else(|| RandomValueGenerator::new(sample.seed), |p| p.random.as_ref().unwrap().clone());
            let mut strategy = RandomStrategy::from_checkpoint(
                bound,
                Rc::new(RefCell::new(random)),
                prefix.map_or(0, |p| p.steps.len()),
            );
            let mut rt = Runtime::new(variant, code, module, &config, &mut strategy, setup());
            if let Some(p) = prefix {
                rt.continue_prefix(p);
            } else {
                rt.run_test(main, &tc.name);
            }
            rt.check_hot_monitors_at_termination();
            ran += 1;
            if rt.bug.is_some() && rt.bound_hit_at.is_none() && !cancelled() {
                return (ran, true);
            }
        }
        (ran, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checker::{compile::compile_program_with_probes, mutation};

    #[test]
    fn neither_an_existing_failure_nor_the_screens_own_cutoff_kills_a_mutant() {
        let model = "
            event tick;
            machine Server { var done: bool;
                start state S {
                    entry { send this, tick; }
                    on tick do { if (!done) { done = true; send this, tick; } }
                }
            }
            machine Main { start state S { entry { new Server(); } } }
            test tc [main=Main]: { Server, Main };";
        for original_fails in [false, true] {
            let mut sm = crate::syntax::SourceMap::new();
            let source =
                if original_fails { model.replace("if (!done)", "assert false; if (!done)") } else { model.to_owned() };
            sm.add("test.p", source);
            let prog = crate::sema::compile(&sm, &Default::default()).unwrap().program;
            let tests = prog.test_cases();
            let code = compile_program_with_probes(&prog);
            let funs = mutation::mutable_funs(&prog, &tests, &[]);
            let mutant = mutation::mutants(&prog, &funs, &code.probes)
                .into_iter()
                .find(|m| m.description == "removed `done = true;`")
                .unwrap();
            let variant = mutation::apply(&prog, &mutant);
            let opts = CheckerOptions { fail_on_max_steps: true, max_fair_steps: 5000, ..Default::default() };
            let mut screen = MutationScreen::new(&prog);
            assert_eq!(
                screen.check(&prog, &variant, mutant.fun.unwrap(), 0, &tests[0], &opts, &|| false),
                (if original_fails { 0 } else { 4 }, false)
            );
        }
    }
}
