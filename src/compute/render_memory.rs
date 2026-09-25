//! Checked logical working-set planning for the standalone GPU scene preview.

pub(crate) const SCENE_VERTEX_BYTES: u64 = 40;
pub(crate) const LINE_VERTEX_BYTES: u64 = 32;
pub(crate) const SCENE_UNIFORM_BYTES: u64 = 128;

#[derive(Debug, Clone, Copy)]
pub(crate) struct SceneRenderMemory {
    pub triangle_vertices: u64,
    pub line_vertices: u64,
    pub triangle_bytes: u64,
    pub line_bytes: u64,
    pub padded_row_bytes: u64,
    pub staging_bytes: u64,
    pub gpu_peak_bytes: u64,
}

impl SceneRenderMemory {
    // AI-FUNC-SUMMARY: Plan expanded visible triangles, enabled overlays, one color/depth/readback set and pending queue uploads with checked arithmetic; allocate nothing and return overflow/zero-size errors.
    pub fn plan(
        triangles: usize,
        segments: usize,
        markers: usize,
        width: usize,
        height: usize,
    ) -> Result<Self, String> {
        let overflow = || "scene working-set size overflow".to_string();
        if width == 0 || height == 0 {
            return Err("render resolution must be positive".into());
        }
        let triangles = u64::try_from(triangles).map_err(|_| overflow())?;
        let segments = u64::try_from(segments).map_err(|_| overflow())?;
        let markers = u64::try_from(markers).map_err(|_| overflow())?;
        let width = u64::try_from(width).map_err(|_| overflow())?;
        let height = u64::try_from(height).map_err(|_| overflow())?;
        let triangle_vertices = triangles.checked_mul(3).ok_or_else(overflow)?;
        let line_vertices = segments
            .checked_mul(2)
            .and_then(|s| markers.checked_mul(6).and_then(|m| s.checked_add(m)))
            .ok_or_else(overflow)?;
        let triangle_bytes = triangle_vertices
            .checked_mul(SCENE_VERTEX_BYTES)
            .ok_or_else(overflow)?;
        let line_bytes = line_vertices
            .checked_mul(LINE_VERTEX_BYTES)
            .ok_or_else(overflow)?;
        let row = width.checked_mul(4).ok_or_else(overflow)?;
        let padded_row_bytes = row
            .checked_add(255)
            .map(|n| n / 256 * 256)
            .ok_or_else(overflow)?;
        let staging_bytes = padded_row_bytes.checked_mul(height).ok_or_else(overflow)?;
        let targets = width
            .checked_mul(height)
            .and_then(|n| n.checked_mul(8))
            .ok_or_else(overflow)?;
        // Queue uploads coexist with destination buffers until the first submission completes.
        let gpu_peak_bytes = triangle_bytes
            .checked_add(line_bytes)
            .and_then(|n| n.checked_mul(2))
            .and_then(|n| n.checked_add(SCENE_UNIFORM_BYTES * 2))
            .and_then(|n| n.checked_add(targets))
            .and_then(|n| n.checked_add(staging_bytes))
            .ok_or_else(overflow)?;
        Ok(Self {
            triangle_vertices,
            line_vertices,
            triangle_bytes,
            line_bytes,
            padded_row_bytes,
            staging_bytes,
            gpu_peak_bytes,
        })
    }

    // AI-FUNC-SUMMARY: Enforce an optional total logical GPU byte budget; equality is allowed and MiB conversion overflow is an error, without probing or allocating GPU resources.
    pub fn check_budget(&self, limit_mb: Option<u64>) -> Result<(), String> {
        if let Some(mb) = limit_mb {
            let limit = mb
                .checked_mul(1024 * 1024)
                .ok_or("scene GPU memory budget overflows bytes")?;
            if self.gpu_peak_bytes > limit {
                return Err(format!(
                    "scene GPU working set {} bytes exceeds budget {limit} bytes",
                    self.gpu_peak_bytes
                ));
            }
        }
        Ok(())
    }

    // AI-FUNC-SUMMARY: Validate independent vertex draw counts and per-buffer capacities before expanding host vertices; no adapter calls or allocations.
    pub fn check_buffers(&self, max_buffer_bytes: u64) -> Result<(), String> {
        if self.triangle_vertices > u64::from(u32::MAX) || self.line_vertices > u64::from(u32::MAX)
        {
            return Err("scene vertex count exceeds GPU draw limits".into());
        }
        if [self.triangle_bytes, self.line_bytes, self.staging_bytes]
            .iter()
            .any(|&n| n > max_buffer_bytes)
        {
            return Err("scene working set exceeds GPU buffer limit".into());
        }
        Ok(())
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Choose the tallest horizontal strip of a width x height image whose GPU working set fits an optional MiB budget.
// Inputs: counted triangles/segments/markers, full image size, optional budget in MiB.
// Returns: Ok((rows per strip in 1..=height, the plan for that strip)); Err when even one row exceeds the budget or on overflow.
// Side effects: None; allocates nothing and probes no adapter.
// Notes: The peak is monotone in rows (only target and staging bytes grow), so a binary search gives the exact boundary: peak(rows) <= limit < peak(rows + 1).
pub(crate) fn scene_strip_rows(
    triangles: usize,
    segments: usize,
    markers: usize,
    width: usize,
    height: usize,
    limit_mb: Option<u64>,
) -> Result<(usize, SceneRenderMemory), String> {
    let full = SceneRenderMemory::plan(triangles, segments, markers, width, height)?;
    if full.check_budget(limit_mb).is_ok() {
        return Ok((height, full));
    }
    let one = SceneRenderMemory::plan(triangles, segments, markers, width, 1)?;
    one.check_budget(limit_mb)?;
    let (mut lo, mut hi) = (1usize, height);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        let fits = SceneRenderMemory::plan(triangles, segments, markers, width, mid)?
            .check_budget(limit_mb)
            .is_ok();
        if fits {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Ok((lo, SceneRenderMemory::plan(triangles, segments, markers, width, lo)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    // AI-FUNC-SUMMARY: Verify independently counted triangle/curve/marker bytes, padded rows, total-budget equality, per-buffer boundaries and huge logical inputs without allocating them.
    #[test]
    fn scene_memory_planner_boundaries() {
        let p = SceneRenderMemory::plan(2, 3, 4, 65, 2).unwrap();
        assert_eq!((p.triangle_vertices, p.line_vertices), (6, 30));
        assert_eq!((p.triangle_bytes, p.line_bytes), (240, 960));
        assert_eq!((p.padded_row_bytes, p.staging_bytes), (512, 1024));
        assert_eq!(p.gpu_peak_bytes, 2400 + 256 + 1040 + 1024);
        assert!(p.check_buffers(1024).is_ok());
        assert!(p.check_buffers(1023).is_err());
        assert!(p.check_budget(Some(0)).is_err());
        assert!(p.check_budget(Some(1)).is_ok());
        assert!(p.check_budget(Some(u64::MAX)).is_err());
        let mut exact = p;
        exact.gpu_peak_bytes = 1024 * 1024;
        assert!(exact.check_budget(Some(1)).is_ok());
        exact.gpu_peak_bytes += 1;
        assert!(exact.check_budget(Some(1)).is_err());
        assert!(SceneRenderMemory::plan(0, 0, 0, 0, 1).is_err());
        assert!(SceneRenderMemory::plan(usize::MAX, 0, 0, 1, 1).is_err());
        assert!(SceneRenderMemory::plan(0, 0, usize::MAX, 1, 1).is_err());
        assert!(SceneRenderMemory::plan(0, 0, 0, usize::MAX, usize::MAX).is_err());
        let huge = SceneRenderMemory::plan(u32::MAX as usize, 0, 0, 1, 1).unwrap();
        assert!(huge.check_buffers(u64::MAX).is_err());
    }

    // AI-FUNC-SUMMARY: Verify strip selection returns the whole image when it fits, the exact budget boundary when it does not, and an error when one row cannot fit.
    #[test]
    fn scene_strips_are_the_exact_budget_boundary() {
        let (rows, plan) = scene_strip_rows(12, 0, 0, 1024, 1024, None).unwrap();
        assert_eq!(rows, 1024);
        assert_eq!(plan.staging_bytes, 4096 * 1024);
        let (rows, plan) = scene_strip_rows(12, 0, 0, 1024, 1024, Some(1)).unwrap();
        assert!(rows > 1 && rows < 1024, "{rows}");
        assert!(plan.gpu_peak_bytes <= 1024 * 1024);
        let next = SceneRenderMemory::plan(12, 0, 0, 1024, rows + 1).unwrap();
        assert!(next.gpu_peak_bytes > 1024 * 1024);
        assert!(scene_strip_rows(12, 0, 0, 1024, 1024, Some(0)).is_err());
        assert!(scene_strip_rows(200_000, 0, 0, 16, 16, Some(1)).is_err());
    }
}
