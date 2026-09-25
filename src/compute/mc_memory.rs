//! Logical MC GPU peak accounting, including retained capacity and pending uploads.

pub(crate) const MC_BLOCK_SAMPLES: usize = 256;
pub(crate) const MC_RADIUS_BATCH: usize = 128;

// AI-FUNC-SUMMARY: Bound an update-plus-evaluation peak using current triangle/output capacity, queued upload bytes and next workload; outputs are sized for one radius batch of at most MC_RADIUS_BATCH radii; count old plus new allocations conservatively on growth and reject arithmetic overflow without allocating.
pub(crate) fn mc_evaluation_peak(
    triangle_capacity: u64,
    output_capacity: u64,
    pending_upload: u64,
    faces: usize,
    r_max: usize,
    samples: usize,
) -> Result<u64, String> {
    let overflow = || "GPU MC working-set size overflow".to_string();
    let triangles = u64::try_from(faces)
        .ok()
        .and_then(|n| n.checked_mul(36))
        .ok_or_else(overflow)?;
    let output = r_max
        .checked_add(1)
        .map(|n| n.min(MC_RADIUS_BATCH))
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
        .and_then(|n| n.checked_add(576 * 2))
        .ok_or_else(overflow)
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
    // AI-FUNC-SUMMARY: Verify cold growth, retained high-water capacities, queued uploads, exact budget edges and oversized arithmetic using independent byte counts.
    #[test]
    fn mc_budget_counts_growth_and_pending_uploads() {
        assert_eq!(
            mc_evaluation_peak(4, 4, 0, 2, 1, 200).unwrap(),
            4 + 72 + 4 * (4 + 8) + 72 + 1152
        );
        assert_eq!(
            mc_evaluation_peak(720, 3200, 144, 1, 0, 200).unwrap(),
            720 + 4 * 3200 + 144 + 36 + 1152
        );
        assert!(mc_evaluation_peak(4, 4, u64::MAX, 0, 0, 200).is_err());
        assert!(mc_evaluation_peak(4, 4, 0, usize::MAX, 0, 200).is_err());
        assert!(mc_evaluation_peak(4, 4, 0, 0, usize::MAX, 200).is_err());
        assert_eq!(
            mc_evaluation_peak(4, 4, 0, 0, 10_000, 256).unwrap(),
            mc_evaluation_peak(4, 4, 0, 0, MC_RADIUS_BATCH - 1, 256).unwrap()
        );
        assert!(check_mc_budget(1024 * 1024, Some(1)).is_ok());
        assert!(check_mc_budget(1024 * 1024 + 1, Some(1)).is_err());
        assert!(check_mc_budget(0, Some(u64::MAX)).is_err());
    }
}
