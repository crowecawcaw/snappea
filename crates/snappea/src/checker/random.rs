//! A port of .NET's seeded `System.Random` (Knuth's subtractive generator) so that a seed
//! produces the same schedule as the upstream checker.

const MBIG: i32 = i32::MAX;
const MSEED: i32 = 161_803_398;

/// Deterministic pseudo random generator with the exact behaviour of `new System.Random(seed)`.
#[derive(Debug, Clone)]
pub struct DotNetRandom {
    seed_array: [i32; 56],
    inext: usize,
    inextp: usize,
}

impl DotNetRandom {
    pub fn new(seed: i32) -> DotNetRandom {
        let mut seed_array = [0i32; 56];
        let subtraction = if seed == i32::MIN { i32::MAX } else { seed.abs() };
        let mut mj = MSEED.wrapping_sub(subtraction);
        seed_array[55] = mj;
        let mut mk: i32 = 1;
        for i in 1..55 {
            let ii = (21 * i) % 55;
            seed_array[ii] = mk;
            mk = mj.wrapping_sub(mk);
            if mk < 0 {
                mk = mk.wrapping_add(MBIG);
            }
            mj = seed_array[ii];
        }
        for _ in 1..5 {
            for i in 1..56 {
                let j = 1 + (i + 30) % 55;
                seed_array[i] = seed_array[i].wrapping_sub(seed_array[j]);
                if seed_array[i] < 0 {
                    seed_array[i] = seed_array[i].wrapping_add(MBIG);
                }
            }
        }
        DotNetRandom { seed_array, inext: 0, inextp: 21 }
    }

    fn internal_sample(&mut self) -> i32 {
        let mut loc_inext = self.inext + 1;
        let mut loc_inextp = self.inextp + 1;
        if loc_inext >= 56 {
            loc_inext = 1;
        }
        if loc_inextp >= 56 {
            loc_inextp = 1;
        }
        let mut ret = self.seed_array[loc_inext].wrapping_sub(self.seed_array[loc_inextp]);
        if ret == MBIG {
            ret -= 1;
        }
        if ret < 0 {
            ret = ret.wrapping_add(MBIG);
        }
        self.seed_array[loc_inext] = ret;
        self.inext = loc_inext;
        self.inextp = loc_inextp;
        ret
    }

    fn sample(&mut self) -> f64 {
        self.internal_sample() as f64 * (1.0 / MBIG as f64)
    }

    /// `Random.Next()`: a non-negative integer below `i32::MAX`.
    // Named after the .NET API it mirrors; this is not an iterator.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> i32 {
        self.internal_sample()
    }

    /// `Random.Next(maxValue)`: a non-negative integer below `max`.
    pub fn next_below(&mut self, max: i32) -> i32 {
        assert!(max >= 0, "maxValue must be non-negative");
        (self.sample() * max as f64) as i32
    }

    /// `Random.NextDouble()`.
    pub fn next_double(&mut self) -> f64 {
        self.sample()
    }
}

/// The upstream `RandomValueGenerator`: a seeded `System.Random` plus its seed.
#[derive(Debug, Clone)]
pub struct RandomValueGenerator {
    pub seed: u32,
    rng: DotNetRandom,
}

impl RandomValueGenerator {
    pub fn new(seed: u32) -> RandomValueGenerator {
        RandomValueGenerator { seed, rng: DotNetRandom::new(seed as i32) }
    }

    pub fn next_below(&mut self, max: i32) -> i32 {
        self.rng.next_below(max)
    }

    pub fn next_double(&mut self) -> f64 {
        self.rng.next_double()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_dotnet_sequence() {
        // Reference values produced by `new System.Random(0).Next()` on .NET.
        let mut r = DotNetRandom::new(0);
        let got: Vec<i32> = (0..5).map(|_| r.next()).collect();
        assert_eq!(got, vec![1559595546, 1755192844, 1649316166, 1198642031, 442452829]);
        // `Next(max)` is `Sample() * max`; `Sample()` is the same generator, so
        // the first bounded value for seed 0 follows from the first raw value.
        let mut r = DotNetRandom::new(0);
        assert_eq!(r.next_below(100), 72);
    }
}
