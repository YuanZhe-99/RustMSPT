//! Conservative logical working-set planning for one fresh resident GPU exact evaluation.

pub(crate) const EXACT_MAX_PARTIALS: usize = 200_000;

pub(crate) struct ExactMemoryPlan {
    pub batch_partials: usize,
    pub peak_bytes: u64,
}

impl ExactMemoryPlan {
    // AI-FUNC-SUMMARY: Bound resident voxel/shell resources including upload and old/new batch storage, and select a batch fitting an optional MiB cap when at least one partial fits; no allocations or GPU probes.
    pub fn new(faces: usize, cells: usize, limit_mb: Option<u64>) -> Result<Self, String> {
        if cells == 0 {
            return Err("GPU exact grid is empty".into());
        }
        let overflow = || "GPU exact working-set size overflow".to_string();
        let triangles = u64::try_from(faces)
            .ok()
            .and_then(|n| n.checked_mul(36))
            .ok_or_else(overflow)?
            .max(4);
        let occupancy = u64::try_from(cells)
            .ok()
            .and_then(|n| n.checked_mul(4))
            .ok_or_else(overflow)?;
        // 2T includes storage and pending triangle upload. 128 bounds voxel/shell
        // parameters and their uploads, count/readback and initial occupancy handles.
        let base = triangles
            .checked_mul(2)
            .and_then(|n| n.checked_add(occupancy))
            .and_then(|n| n.checked_add(128))
            .ok_or_else(overflow)?;
        // Per partial: offsets 16 + outputs/staging 16, old capacity <=32,
        // and pending offset upload <=16. Direct exact has no tile reducer.
        let batch_partials = match limit_mb {
            Some(mb) => {
                let limit = mb.checked_mul(1024 * 1024).ok_or_else(overflow)?;
                (limit.saturating_sub(base) / 80).clamp(1, EXACT_MAX_PARTIALS as u64) as usize
            }
            None => EXACT_MAX_PARTIALS,
        };
        let peak_bytes = base
            .checked_add(batch_partials as u64 * 80)
            .ok_or_else(overflow)?;
        Ok(Self {
            batch_partials,
            peak_bytes,
        })
    }

    // AI-FUNC-SUMMARY: Reject a budget too small for even one partial or overflowing MiB conversion before any GPU initialization.
    pub fn check_budget(&self, limit_mb: Option<u64>) -> Result<(), String> {
        if let Some(mb) = limit_mb {
            let limit = mb
                .checked_mul(1024 * 1024)
                .ok_or("GPU exact budget overflows bytes")?;
            if self.peak_bytes > limit {
                return Err(format!(
                    "GPU exact working set {} bytes exceeds budget {limit} bytes",
                    self.peak_bytes
                ));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // AI-FUNC-SUMMARY: Check known resource components, batch shrinking, minimum infeasibility and overflow without allocating GPU resources.
    #[test]
    fn exact_budget_selects_bounded_batches() {
        let base = 12 * 36 * 2 + 1000 * 4 + 128;
        let full = ExactMemoryPlan::new(12, 1000, None).unwrap();
        assert_eq!(full.peak_bytes, base + 200_000 * 80);
        let small = ExactMemoryPlan::new(12, 1000, Some(1)).unwrap();
        assert_eq!(small.batch_partials as u64, (1024 * 1024 - base) / 80);
        small.check_budget(Some(1)).unwrap();
        assert!(small.peak_bytes + 80 > 1024 * 1024);
        assert!(ExactMemoryPlan::new(12, 1000, Some(0))
            .unwrap()
            .check_budget(Some(0))
            .is_err());
        assert!(ExactMemoryPlan::new(12, 1_000_000, Some(1))
            .unwrap()
            .check_budget(Some(1))
            .is_err());
        assert!(ExactMemoryPlan::new(usize::MAX, 1, None).is_err());
        assert!(ExactMemoryPlan::new(1, 0, None).is_err());
        assert!(ExactMemoryPlan::new(1, 1, Some(u64::MAX)).is_err());
    }
}
