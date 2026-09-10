use rand_chacha::rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha12Rng;
use sha2::{Digest, Sha256};

// AI-FUNC-SUMMARY:
// Purpose: Build the reproducible random stream a seeded run draws every variate from.
// Inputs: the run's u64 seed.
// Returns: a ChaCha12 generator.
// Side effects: None.
// Notes: The 32-byte state is sha256("rustmspt.placement/1|" ++ seed_le), not SeedableRng::seed_from_u64,
// whose expansion is not contractually stable across rand_core majors. Naming the algorithm and deriving
// the state here is what makes "same seed, same version, same placement" a claim we can keep.
pub fn seeded_rng(seed: u64) -> ChaCha12Rng {
    let mut hasher = Sha256::new();
    hasher.update(b"rustmspt.placement/1|");
    hasher.update(seed.to_le_bytes());
    let digest = hasher.finalize();
    let mut state = [0u8; 32];
    state.copy_from_slice(&digest);
    ChaCha12Rng::from_seed(state)
}

// AI-FUNC-SUMMARY:
// Purpose: Draw one uniform in [0, 1) from the raw generator.
// Inputs: the generator.
// Returns: f64 in [0, 1).
// Side effects: Advances the generator by one u64.
// Notes: The single uniform primitive. Every other sampler builds on it, so a future `rand` bump
// cannot silently move placements: rand's own Uniform/gen_range are explicitly not value-stable
// across minor versions. Takes the top 53 bits, which is exactly f64's mantissa width, so the
// result is uniform on the representable multiples of 2^-53 and can never reach 1.0.
pub fn u01<R: RngCore + ?Sized>(rng: &mut R) -> f64 {
    ((rng.next_u64() >> 11) as f64) * (1.0 / 9007199254740992.0)
}

// AI-FUNC-SUMMARY:
// Purpose: Draw one uniform in [lo, hi).
// Inputs: the generator, the half-open bounds.
// Returns: f64 in [lo, hi), or lo when hi <= lo.
// Side effects: Advances the generator by one u64.
// Notes: Computed as lo + (hi - lo) * u01. The guard is written out rather than as `hi <= lo`
// so that a NaN bound takes it too: a comparison against NaN is false, so `hi <= lo` alone would
// let NaN through and return NaN. An empty, inverted or non-finite range yields lo, and the draw
// is still consumed so the RNG consumption schedule does not depend on the bounds.
pub fn uniform_range<R: RngCore + ?Sized>(rng: &mut R, lo: f64, hi: f64) -> f64 {
    let width = hi - lo;
    if !lo.is_finite() || !width.is_finite() || width <= 0.0 {
        let _ = u01(rng);
        return lo;
    }
    lo + width * u01(rng)
}

// AI-FUNC-SUMMARY:
// Purpose: Draw one index uniformly from 0..n.
// Inputs: the generator, the exclusive upper bound.
// Returns: an index in 0..n, or 0 when n is 0.
// Side effects: Advances the generator by one u64.
// Notes: (u01 * n) as usize, clamped to n - 1 against the rounding case. The modulo bias of this
// construction is bounded by 2^-53 per index, far below any effect a packing run could observe,
// and it costs exactly one u64 per draw, which is what the fixed RNG consumption schedule needs.
pub fn uniform_index<R: RngCore + ?Sized>(rng: &mut R, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let raw = (u01(rng) * n as f64) as usize;
    raw.min(n - 1)
}
