use crate::config::placement::{ResolvedClasses, ResolvedDistribution};
use crate::error::{Result, RustMsptError};
use crate::pipeline::pack_targets::{load_target_distribution_csv, TargetDistribution};
use crate::pipeline::rng::u01;
use rand_chacha::rand_core::RngCore;

// AI-FUNC-SUMMARY:
// Purpose: Invert the standard normal CDF.
// Inputs: a probability in (0, 1).
// Returns: the quantile; -inf at 0, +inf at 1, NaN outside [0, 1].
// Side effects: None.
// Notes: Wichura's AS241 PPND16, absolute error below 1e-15 across the whole range. Deliberately
// not built on split_filter.rs's erf_approx (Abramowitz & Stegun 7.1.26, |eps| <= 1.5e-7): the
// truncation bounds of a lognormal are evaluated in the tails, where that error visibly moves how
// much mass is cut off and therefore how many large particles a run plans for.
// The coefficients are transcribed from the published algorithm at its stated precision. Some carry
// a digit or two beyond what f64 can represent; they are kept verbatim so the source can be checked
// against the paper, and truncating them to match f64 exactly would change nothing but the reader's
// ability to do that.
#[allow(clippy::excessive_precision)]
pub fn inverse_normal_cdf(p: f64) -> f64 {
    if !(0.0..=1.0).contains(&p) {
        return f64::NAN;
    }
    if p == 0.0 {
        return f64::NEG_INFINITY;
    }
    if p == 1.0 {
        return f64::INFINITY;
    }
    let q = p - 0.5;
    if q.abs() <= 0.425 {
        let r = 0.180625 - q * q;
        return q * poly(
            r,
            &[
                2509.0809287301226727,
                33430.575583588128105,
                67265.770927008700853,
                45921.953931549871457,
                13731.693765509461125,
                1971.5909503065514427,
                133.14166789178437745,
                3.387132872796366608,
            ],
        ) / poly(
            r,
            &[
                5226.495278852854561,
                28729.085735721942674,
                39307.89580009271061,
                21213.794301586595867,
                5394.1960214247511077,
                687.1870074920579083,
                42.313330701600911252,
                1.0,
            ],
        );
    }
    let mut r = if q < 0.0 { p } else { 1.0 - p };
    r = (-r.ln()).sqrt();
    let value = if r <= 5.0 {
        let r = r - 1.6;
        poly(
            r,
            &[
                7.7454501427834140764e-4,
                0.0227238449892691845833,
                0.24178072517745061177,
                1.27045825245236838258,
                3.64784832476320460504,
                5.7694972214606914055,
                4.6303378461565452959,
                1.42343711074968357734,
            ],
        ) / poly(
            r,
            &[
                1.05075007164441684324e-9,
                5.475938084995344946e-4,
                0.0151986665636164571966,
                0.14810397642748007459,
                0.68976733498510000455,
                1.6763848301838038494,
                2.05319162663775882187,
                1.0,
            ],
        )
    } else {
        let r = r - 5.0;
        poly(
            r,
            &[
                2.01033439929228813265e-7,
                2.71155556874348757815e-5,
                0.0012426609473880784386,
                0.026532189526576123093,
                0.29656057182850489123,
                1.7848265399172913358,
                5.4637849111641143699,
                6.6579046435011037772,
            ],
        ) / poly(
            r,
            &[
                2.04426310338993978564e-15,
                1.4215117583164458887e-7,
                1.8463183175100546818e-5,
                7.868691311456132591e-4,
                0.0148753612908506148525,
                0.13692988092273580531,
                0.59983220655588793769,
                1.0,
            ],
        )
    };
    if q < 0.0 {
        -value
    } else {
        value
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Evaluate a polynomial by Horner's rule.
// Inputs: the argument and the coefficients, HIGHEST degree first.
// Returns: the value.
// Side effects: None.
// Notes: Highest-degree-first is the order the AS241 paper prints its constants in, so they can be
// transcribed and checked against it directly. Feeding ascending coefficients here silently
// evaluates a different polynomial - it does not error, it just returns wrong quantiles, which is
// why the quantile test compares against published values rather than against our own CDF.
fn poly(x: f64, coefficients: &[f64]) -> f64 {
    let mut acc = 0.0;
    for c in coefficients {
        acc = acc * x + c;
    }
    acc
}

// AI-FUNC-SUMMARY: Standard normal CDF via the complementary error function; returns f64 in [0, 1]; side effects: none.
fn normal_cdf(x: f64) -> f64 {
    0.5 * erfc(-x / std::f64::consts::SQRT_2)
}

// AI-FUNC-SUMMARY:
// Purpose: Complementary error function.
// Inputs: the argument.
// Returns: erfc(x), accurate to about 1e-16 relative.
// Side effects: None.
// Notes: Rational Chebyshev approximation with an exp factor; used only to convert the truncation
// bounds into probabilities, which happens once per run rather than once per draw.
fn erfc(x: f64) -> f64 {
    let z = x.abs();
    let t = 1.0 / (1.0 + 0.5 * z);
    let poly = -z * z - 1.26551223
        + t * (1.00002368
            + t * (0.37409196
                + t * (0.09678418
                    + t * (-0.18628806
                        + t * (0.27886807
                            + t * (-1.13520398
                                + t * (1.48851587
                                    + t * (-0.82215223 + t * 0.17087277))))))));
    let ans = t * poly.exp();
    if x >= 0.0 {
        ans
    } else {
        2.0 - ans
    }
}

/// A drawn size, and which reporting class it belongs to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeDraw {
    pub diameter: f64,
    pub class: usize,
    /// Position in the order the sizes were drawn, before any sorting. Used as the
    /// tie-break when sorting by diameter, so equal diameters keep a total order.
    pub draw_index: usize,
}

/// One reporting class: a diameter band and the share of particles it should hold.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SizeClass {
    pub lo: f64,
    pub hi: f64,
    pub target_frequency: f64,
}

/// Where sizes are drawn from, prepared once per run.
pub enum SizeSource {
    Lognormal {
        /// ln of the median, which is the mean of ln d for a lognormal.
        mu: f64,
        sigma: f64,
        min: f64,
        max: f64,
        /// The CDF values at the truncation bounds, so a draw is one inverse CDF
        /// evaluation rather than a rejection loop of unbounded length.
        p_lo: f64,
        p_hi: f64,
    },
    Histogram {
        distribution: TargetDistribution,
        /// Cumulative frequencies, so a bin is chosen by one binary-free scan.
        cumulative: Vec<f64>,
    },
}

impl SizeSource {
    // AI-FUNC-SUMMARY:
    // Purpose: Prepare a size source from a resolved distribution, loading the histogram CSV if needed.
    // Inputs: the resolved distribution.
    // Returns: a SizeSource ready to sample, or an error naming what was wrong.
    // Side effects: Reads the histogram CSV from disk when the distribution is a histogram.
    // Notes: The truncation probabilities are computed once here, not per draw.
    pub fn prepare(distribution: &ResolvedDistribution) -> Result<SizeSource> {
        match distribution {
            ResolvedDistribution::Lognormal {
                median,
                sigma_log,
                min,
                max,
            } => {
                let mu = median.ln();
                let p_lo = normal_cdf((min.ln() - mu) / sigma_log);
                let p_hi = normal_cdf((max.ln() - mu) / sigma_log);
                // Written against NaN rather than as `p_hi <= p_lo`: a comparison
                // with NaN is false, so the shorter form would let a NaN bound pass.
                if !(p_hi - p_lo).is_finite() || p_hi - p_lo <= 0.0 {
                    return Err(RustMsptError::InvalidConfig(format!(
                        "the truncated lognormal has no probability mass between {min} and {max} \
                         for median {median} and sigma_log {sigma_log}"
                    )));
                }
                Ok(SizeSource::Lognormal {
                    mu,
                    sigma: *sigma_log,
                    min: *min,
                    max: *max,
                    p_lo,
                    p_hi,
                })
            }
            ResolvedDistribution::Histogram { csv, .. } => {
                let distribution = load_target_distribution_csv(csv)?;
                let mut cumulative = Vec::with_capacity(distribution.bins.len());
                let mut running = 0.0;
                for bin in &distribution.bins {
                    running += bin.frequency;
                    cumulative.push(running);
                }
                Ok(SizeSource::Histogram {
                    distribution,
                    cumulative,
                })
            }
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Draw one diameter from the target number distribution.
    // Inputs: the generator.
    // Returns: a diameter inside the distribution's support.
    // Side effects: Advances the generator by exactly one u64.
    // Notes: Exactly one draw either way, which is what the fixed RNG consumption schedule needs -
    // a rejection loop would consume an unpredictable number and make the stream depend on the
    // parameters. Lognormal: inverse CDF on the truncated probability interval. Histogram: the bin
    // is chosen by cumulative frequency and the diameter is uniform inside it, so the sub-bin law
    // is stated rather than defaulting to the midpoint.
    pub fn sample<R: RngCore + ?Sized>(&self, rng: &mut R) -> f64 {
        let u = u01(rng);
        match self {
            SizeSource::Lognormal {
                mu,
                sigma,
                min,
                max,
                p_lo,
                p_hi,
            } => {
                let p = p_lo + u * (p_hi - p_lo);
                let d = (mu + sigma * inverse_normal_cdf(p)).exp();
                // The inverse CDF can land a hair outside the bounds through
                // rounding; the bounds are what the config promised, so they win.
                d.clamp(*min, *max)
            }
            SizeSource::Histogram {
                distribution,
                cumulative,
            } => {
                let index = cumulative
                    .iter()
                    .position(|c| u < *c)
                    .unwrap_or(distribution.bins.len().saturating_sub(1));
                let bin = &distribution.bins[index];
                // Rescale u to the position within this bin, so one draw does both
                // jobs and the stream stays one-u64-per-size.
                let lo = if index == 0 { 0.0 } else { cumulative[index - 1] };
                let span = cumulative[index] - lo;
                let inner = if span > 0.0 { (u - lo) / span } else { 0.0 };
                bin.left + inner * (bin.right - bin.left)
            }
        }
    }

    // AI-FUNC-SUMMARY: The smallest and largest diameter this source can produce; returns (min, max); side effects: none.
    pub fn support(&self) -> (f64, f64) {
        match self {
            SizeSource::Lognormal { min, max, .. } => (*min, *max),
            SizeSource::Histogram { distribution, .. } => {
                let lo = distribution.bins.first().map(|b| b.left).unwrap_or(0.0);
                let hi = distribution.bins.last().map(|b| b.right).unwrap_or(0.0);
                (lo, hi)
            }
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Build the reporting classes a run compares its target against its actual over.
// Inputs: the resolved class choice and the prepared size source.
// Returns: the classes, in increasing diameter order, with their target frequencies.
// Side effects: None.
// Notes: A histogram reports over its own bins, so target frequency is the bin's own. Equal-width
// classes over a lognormal's truncated support get their target frequency from the truncated CDF,
// not from the untruncated one, or the shares would not sum to one.
pub fn build_classes(classes: &ResolvedClasses, source: &SizeSource) -> Vec<SizeClass> {
    match (classes, source) {
        (ResolvedClasses::FromHistogram, SizeSource::Histogram { distribution, .. }) => distribution
            .bins
            .iter()
            .map(|b| SizeClass {
                lo: b.left,
                hi: b.right,
                target_frequency: b.frequency,
            })
            .collect(),
        (ResolvedClasses::FromHistogram, SizeSource::Lognormal { .. }) => {
            build_equal_width(source, 10)
        }
        (ResolvedClasses::EqualWidth { count }, _) => build_equal_width(source, *count),
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Split a source's support into equal-width diameter classes with their target shares.
// Inputs: the source and the class count.
// Returns: the classes in increasing order.
// Side effects: None.
// Notes: For a lognormal the share of a class is the truncated CDF difference over it; for a
// histogram it is the overlap-weighted sum of the bins it covers, so a class boundary falling
// inside a bin splits that bin's frequency proportionally rather than assigning it whole.
fn build_equal_width(source: &SizeSource, count: usize) -> Vec<SizeClass> {
    let (lo, hi) = source.support();
    let count = count.max(1);
    let width = (hi - lo) / count as f64;
    (0..count)
        .map(|i| {
            let a = lo + width * i as f64;
            let b = if i + 1 == count { hi } else { lo + width * (i + 1) as f64 };
            SizeClass {
                lo: a,
                hi: b,
                target_frequency: source.mass_between(a, b),
            }
        })
        .collect()
}

impl SizeSource {
    // AI-FUNC-SUMMARY:
    // Purpose: The share of the target distribution lying between two diameters.
    // Inputs: the interval bounds.
    // Returns: a probability in [0, 1].
    // Side effects: None.
    // Notes: For a lognormal this is a difference of truncated CDFs; for a histogram it is the
    // overlap-weighted sum of bin frequencies, treating each bin's density as uniform inside it,
    // which is the same assumption `sample` makes.
    pub fn mass_between(&self, a: f64, b: f64) -> f64 {
        // NaN-safe: a bare `b <= a` would let a NaN bound through.
        if !(b - a).is_finite() || b - a <= 0.0 {
            return 0.0;
        }
        match self {
            SizeSource::Lognormal {
                mu,
                sigma,
                min,
                max,
                p_lo,
                p_hi,
            } => {
                let a = a.max(*min);
                let b = b.min(*max);
                if b - a <= 0.0 {
                    return 0.0;
                }
                let pa = normal_cdf((a.ln() - mu) / sigma);
                let pb = normal_cdf((b.ln() - mu) / sigma);
                ((pb - pa) / (p_hi - p_lo)).clamp(0.0, 1.0)
            }
            SizeSource::Histogram { distribution, .. } => distribution
                .bins
                .iter()
                .map(|bin| {
                    let lo = bin.left.max(a);
                    let hi = bin.right.min(b);
                    let span = bin.right - bin.left;
                    if hi > lo && span > 0.0 {
                        bin.frequency * (hi - lo) / span
                    } else {
                        0.0
                    }
                })
                .sum::<f64>()
                .clamp(0.0, 1.0),
        }
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Find which reporting class a diameter falls in.
// Inputs: the classes and the diameter.
// Returns: the class index; the nearest end class when the diameter falls outside every class.
// Side effects: None.
// Notes: Classes are half-open [lo, hi) except the last, whose upper edge is inclusive, matching
// how the histogram bins already behave so a diameter exactly at the maximum still lands somewhere.
pub fn class_for_diameter(classes: &[SizeClass], diameter: f64) -> usize {
    if classes.is_empty() {
        return 0;
    }
    for (i, c) in classes.iter().enumerate() {
        let last = i + 1 == classes.len();
        if diameter >= c.lo && (diameter < c.hi || (last && diameter <= c.hi)) {
            return i;
        }
    }
    if diameter < classes[0].lo {
        0
    } else {
        classes.len() - 1
    }
}

/// The sizes a run intends to place, drawn before any placement is attempted.
#[derive(Debug, Clone)]
pub struct SizePlan {
    pub draws: Vec<SizeDraw>,
    pub planned_volume: f64,
    pub target_volume: f64,
    /// `planned_volume - target_volume`. Signed, and reported: the plan lands on
    /// whichever side is closer, so a reader can see how far off the plan was
    /// before placement had a chance to make it worse.
    pub planned_volume_error: f64,
}

// AI-FUNC-SUMMARY:
// Purpose: Draw the whole multiset of sizes a run will attempt, before any placement.
// Inputs: the generator, the prepared source, the classes, the target volume, and a hard cap on count.
// Returns: the plan, or an error when the target cannot be reached within the cap.
// Side effects: Advances the generator by one u64 per draw.
// Notes: Drawing every size first, and never re-drawing after a failure, is what stops a run from
// quietly compensating for large particles that would not fit by adding small ones - the shortfall
// stays visible per class instead. Each draw's volume is exact, not expected: scaling a shell to an
// equivalent diameter d gives it volume pi*d^3/6 by definition of that diameter.
// The stopping rule is symmetric. Draws accumulate until the running volume first reaches the
// target, then the plan keeps whichever of the last two counts lands closer to it. Stopping at the
// first count to exceed the target would overshoot by about one particle's volume every time, and
// the particle it added would be the largest one - exactly the one most likely to fail and to
// dominate the shortfall.
pub fn plan_size_multiset<R: RngCore + ?Sized>(
    rng: &mut R,
    source: &SizeSource,
    classes: &[SizeClass],
    target_volume: f64,
    max_particles: usize,
) -> Result<SizePlan> {
    if !target_volume.is_finite() || target_volume <= 0.0 {
        return Err(RustMsptError::InvalidConfig(format!(
            "the target volume must be finite and positive, got {target_volume}"
        )));
    }
    let sphere_volume = |d: f64| std::f64::consts::PI * d * d * d / 6.0;

    let mut draws: Vec<SizeDraw> = Vec::new();
    let mut running = 0.0;
    while running < target_volume {
        if draws.len() >= max_particles {
            return Err(RustMsptError::InvalidConfig(format!(
                "reaching a target volume of {target_volume} needs more than {max_particles} \
                 particles at this size distribution; raise the cap or lower the target"
            )));
        }
        let d = source.sample(rng);
        let index = draws.len();
        draws.push(SizeDraw {
            diameter: d,
            class: class_for_diameter(classes, d),
            draw_index: index,
        });
        running += sphere_volume(d);
    }

    // Keep the count whose total sits closer to the target.
    if draws.len() > 1 {
        let last = sphere_volume(draws[draws.len() - 1].diameter);
        let without = running - last;
        if (target_volume - without).abs() < (running - target_volume).abs() {
            draws.pop();
            running = without;
        }
    }

    Ok(SizePlan {
        planned_volume: running,
        target_volume,
        planned_volume_error: running - target_volume,
        draws,
    })
}

// AI-FUNC-SUMMARY:
// Purpose: Order a drawn multiset for placement.
// Inputs: the draws (mutated in place) and whether to sort largest first.
// Returns: None.
// Side effects: Reorders the slice.
// Notes: Largest first by default, because large particles are the ones that stop fitting: placing
// them while there is still room is what keeps a failure visible as a shortfall in its own class
// rather than as a run that quietly became a pile of small ones. The comparison is total_cmp with
// the draw index as tie-break, so equal diameters have a defined order and the result does not
// depend on the sort's stability.
pub fn order_for_placement(draws: &mut [SizeDraw], descending: bool) {
    if descending {
        draws.sort_by(|a, b| {
            b.diameter
                .total_cmp(&a.diameter)
                .then(a.draw_index.cmp(&b.draw_index))
        });
    } else {
        draws.sort_by_key(|d| d.draw_index);
    }
}
