//! Logical MC GPU peak accounting, including retained capacity and pending uploads.

/// GPU bytes per triangle: the raw 9-float buffer (36) plus the certified shaders' 16-float constants (64).
pub(crate) const TRIANGLE_BYTES: u64 = 36 + 64;

pub(crate) const MC_BLOCK_SAMPLES: usize = 256;
pub(crate) const MC_RADIUS_BATCH: usize = 128;
pub(crate) const MC_PARAMS_BYTES: u64 = 608;
pub(crate) const MC_UNCERTAIN_WORDS: usize = 7;
pub(crate) const MC_UNCERTAIN_INITIAL: usize = 1024;

// AI-FUNC-SUMMARY: Byte size of an uncertain-sample list holding `entries` records (4-byte counter plus 7 words per entry), saturating on overflow; returns u64; side effects: None.
pub(crate) fn mc_uncertain_bytes(entries: usize) -> u64 {
    (entries as u64)
        .saturating_mul(MC_UNCERTAIN_WORDS as u64 * 4)
        .saturating_add(4)
}

// AI-FUNC-SUMMARY: Bound an update-plus-evaluation peak using current triangle/output/uncertain-list capacity, queued upload bytes and next workload; outputs are sized for one radius batch of at most MC_RADIUS_BATCH radii; the certification list and its staging count at their retained size or the initial capacity; count old plus new allocations conservatively on growth and reject arithmetic overflow without allocating. List regrowth after an uncertain overflow is checked separately against the same budget with mc_regrowth_peak when the pipeline has one (GpuS2Pipeline::set_memory_limit_mb).
#[cfg(any(feature = "gpu", test))]
pub(crate) fn mc_evaluation_peak(
    triangle_capacity: u64,
    output_capacity: u64,
    pending_upload: u64,
    uncertain_capacity: u64,
    faces: usize,
    r_max: usize,
    samples: usize,
) -> Result<u64, String> {
    mc_evaluation_peak_batched(triangle_capacity, output_capacity, pending_upload, uncertain_capacity, faces, r_max, samples, MC_RADIUS_BATCH)
}

// AI-FUNC-SUMMARY: mc_evaluation_peak with the radius batch (radii per dispatch, 1..=MC_RADIUS_BATCH) as a parameter; returns the peak or an overflow error; side effects: none.
#[allow(clippy::too_many_arguments)]
pub(crate) fn mc_evaluation_peak_batched(
    triangle_capacity: u64,
    output_capacity: u64,
    pending_upload: u64,
    uncertain_capacity: u64,
    faces: usize,
    r_max: usize,
    samples: usize,
    batch: usize,
) -> Result<u64, String> {
    let overflow = || "GPU MC working-set size overflow".to_string();
    let triangles = u64::try_from(faces)
        .ok()
        .and_then(|n| n.checked_mul(TRIANGLE_BYTES))
        .ok_or_else(overflow)?;
    let output = r_max
        .checked_add(1)
        .map(|n| n.min(batch.clamp(1, MC_RADIUS_BATCH)))
        .and_then(|n| n.checked_mul(samples.max(200).div_ceil(MC_BLOCK_SAMPLES)))
        .and_then(|n| u64::try_from(n).ok())
        .and_then(|n| n.checked_mul(4))
        .ok_or_else(overflow)?;
    let triangle_peak = if triangles > triangle_capacity {
        triangle_capacity
            .checked_add(triangles)
            .ok_or_else(overflow)?
    } else {
        triangle_capacity
    };
    let output_peak = if output > output_capacity {
        output_capacity.checked_add(output).ok_or_else(overflow)?
    } else {
        output_capacity
    };
    triangle_peak
        .checked_add(output_peak.checked_mul(4).ok_or_else(overflow)?)
        .and_then(|n| n.checked_add(pending_upload))
        .and_then(|n| n.checked_add(triangles))
        .and_then(|n| n.checked_add(MC_PARAMS_BYTES * 2))
        .and_then(|n| {
            uncertain_capacity
                .max(mc_uncertain_bytes(MC_UNCERTAIN_INITIAL))
                .checked_mul(2)
                .and_then(|list| n.checked_add(list))
        })
        .ok_or_else(overflow)
}

// AI-FUNC-SUMMARY: Peak while an overflowed uncertain list is regrown: the retained peak (which already counts the old list and its staging) plus the new list and its staging, both alive before the old pair is dropped; returns u64 or an overflow error; side effects: None.
#[cfg(feature = "gpu")]
pub(crate) fn mc_regrowth_peak(retained_peak: u64, entries: usize) -> Result<u64, String> {
    mc_uncertain_bytes(entries)
        .checked_mul(2)
        .and_then(|list| retained_peak.checked_add(list))
        .ok_or_else(|| "GPU MC working-set size overflow".to_string())
}

// AI-FUNC-SUMMARY: The largest radius batch (1..=MC_RADIUS_BATCH) whose evaluation peak fits the MiB limit, or MC_RADIUS_BATCH without one; returns the batch or the one-radius error; side effects: none.
// Notes: Counts are identical for every batch size (logical sample ids are global), so shrinking the batch is
// always preferable to falling back; only a peak that one radius per dispatch still exceeds is an error.
#[allow(clippy::too_many_arguments)]
#[cfg(any(feature = "gpu", test))]
pub(crate) fn mc_largest_batch(
    triangle_capacity: u64,
    output_capacity: u64,
    pending_upload: u64,
    uncertain_capacity: u64,
    faces: usize,
    r_max: usize,
    samples: usize,
    limit_mb: Option<u64>,
) -> Result<usize, String> {
    let fits = |batch: usize| -> Result<bool, String> {
        let peak = mc_evaluation_peak_batched(triangle_capacity, output_capacity, pending_upload, uncertain_capacity, faces, r_max, samples, batch)?;
        Ok(check_mc_budget(peak, limit_mb).is_ok())
    };
    if fits(MC_RADIUS_BATCH)? {
        return Ok(MC_RADIUS_BATCH);
    }
    if !fits(1)? {
        let peak = mc_evaluation_peak_batched(triangle_capacity, output_capacity, pending_upload, uncertain_capacity, faces, r_max, samples, 1)?;
        return check_mc_budget(peak, limit_mb).map(|()| 1);
    }
    let (mut lo, mut hi) = (1usize, MC_RADIUS_BATCH);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if fits(mid)? {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok(lo)
}

// AI-FUNC-SUMMARY: Check a logical GPU MC peak against an optional MiB cap; equality is allowed, cap conversion overflow and excess return errors without GPU probing.
pub(crate) fn check_mc_budget(peak: u64, limit_mb: Option<u64>) -> Result<(), String> {
    if let Some(mb) = limit_mb {
        let limit = mb
            .checked_mul(1024 * 1024)
            .ok_or("GPU MC memory budget overflows bytes")?;
        if peak > limit {
            return Err(format!(
                "GPU MC working set {peak} bytes exceeds budget {limit} bytes"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // AI-FUNC-SUMMARY: mc_largest_batch returns the full batch when it fits, the exact boundary batch under a tight budget (peak(b) fits, peak(b+1) does not), and the one-radius error when nothing fits.
    #[test]
    fn largest_batch_is_the_exact_budget_boundary() {
        let peak = |b: usize| mc_evaluation_peak_batched(4, 4, 0, 0, 100, 300, 300_000, b).unwrap();
        assert_eq!(mc_largest_batch(4, 4, 0, 0, 100, 300, 300_000, None).unwrap(), MC_RADIUS_BATCH);
        let limit_mb = 1u64;
        let limit = limit_mb * 1024 * 1024;
        assert!(peak(MC_RADIUS_BATCH) > limit && peak(1) <= limit, "fixture must straddle the budget");
        let b = mc_largest_batch(4, 4, 0, 0, 100, 300, 300_000, Some(limit_mb)).unwrap();
        assert!((1..MC_RADIUS_BATCH).contains(&b));
        assert!(peak(b) <= limit && peak(b + 1) > limit, "batch {b}: {} / {}", peak(b), peak(b + 1));
        assert!(mc_largest_batch(4, 4, 0, 0, 100, 300, 300_000, Some(0)).is_err());
    }
    // AI-FUNC-SUMMARY: Verify cold growth, retained high-water capacities, queued uploads, exact budget edges and oversized arithmetic using independent byte counts.
    #[test]
    fn mc_budget_counts_growth_and_pending_uploads() {
        assert_eq!(
            mc_evaluation_peak(4, 4, 0, 0, 2, 1, 200).unwrap(),
            4 + 200 + 4 * (4 + 8) + 200 + 1216 + 2 * (4 + 28 * 1024)
        );
        assert_eq!(
            mc_evaluation_peak(720, 3200, 144, 0, 1, 0, 200).unwrap(),
            720 + 4 * 3200 + 144 + 100 + 1216 + 2 * (4 + 28 * 1024)
        );
        assert_eq!(
            mc_evaluation_peak(720, 3200, 144, 1_000_000, 1, 0, 200).unwrap(),
            720 + 4 * 3200 + 144 + 100 + 1216 + 2_000_000
        );
        assert!(mc_evaluation_peak(4, 4, u64::MAX, 0, 0, 0, 200).is_err());
        assert!(mc_evaluation_peak(4, 4, 0, 0, usize::MAX, 0, 200).is_err());
        assert!(mc_evaluation_peak(4, 4, 0, 0, 0, usize::MAX, 200).is_err());
        assert_eq!(
            mc_evaluation_peak(4, 4, 0, 0, 0, 10_000, 256).unwrap(),
            mc_evaluation_peak(4, 4, 0, 0, 0, MC_RADIUS_BATCH - 1, 256).unwrap()
        );
        assert!(check_mc_budget(1024 * 1024, Some(1)).is_ok());
        assert!(check_mc_budget(1024 * 1024 + 1, Some(1)).is_err());
        assert!(check_mc_budget(0, Some(u64::MAX)).is_err());
    }
}
