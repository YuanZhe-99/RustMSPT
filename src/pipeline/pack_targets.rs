use crate::error::{Result, RustMsptError};
use crate::geometry::MeshMetrics;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DiameterBin {
    pub left: f64,
    pub right: f64,
    pub frequency: f64,
}

impl DiameterBin {
    // AI-FUNC-SUMMARY: Return the arithmetic midpoint of the interval; returns f64; side effects: None.
    pub fn midpoint(&self) -> f64 {
        self.left + (self.right - self.left) * 0.5
    }
}

#[derive(Debug, Clone)]
pub struct TargetDistribution {
    pub bins: Vec<DiameterBin>,
}

#[derive(Debug, Clone, Default)]
pub struct DistributionState {
    pub counts: Vec<usize>,
    pub attempts: Vec<usize>,
    pub natural: usize,
    pub scaled: usize,
    pub fallback: usize,
    pub scale_factor_min: f64,
    pub scale_factor_sum: f64,
    pub scale_factor_max: f64,
    pub scale_factor_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinChoiceKind {
    Natural,
    Scaled,
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BinChoice {
    pub index: usize,
    pub kind: BinChoiceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DistributionSummary {
    pub count: usize,
    pub max_absolute_error: f64,
    pub total_variation_distance: f64,
    pub rounding_max_absolute_error: f64,
}

#[derive(Debug, Clone, Default)]
pub struct SphericityState {
    pub sum: f64,
    pub count: usize,
}

impl TargetDistribution {
    // AI-FUNC-SUMMARY: Initialize accepted/attempted counters for every parsed bin; returns DistributionState; side effects: None.
    pub fn state(&self) -> DistributionState {
        DistributionState {
            counts: vec![0; self.bins.len()],
            attempts: vec![0; self.bins.len()],
            ..DistributionState::default()
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Find the bin containing an equivalent diameter using [left,right) and a closed final interval.
    // Inputs: candidate diameter.
    // Returns: bin index or None for values outside configured support or explicit gaps.
    // Side effects: None.
    pub fn bin_for_diameter(&self, diameter: f64) -> Option<usize> {
        if !diameter.is_finite() {
            return None;
        }
        let final_right = self.bins.last().map(|bin| bin.right);
        self.bins.iter().position(|bin| {
            diameter >= bin.left
                && (diameter < bin.right
                    || (Some(bin.right) == final_right && diameter <= bin.right))
        })
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Choose the target bin for the next candidate, preferring capacity debt and falling back after per-round failures.
    // Inputs: mutable distribution counters, natural bin for the selected shape, and bins already attempted in this success round.
    // Returns: Natural, Scaled, or Fallback target choice.
    // Side effects: None; callers must mark attempts and commit success explicitly.
    pub fn choose_bin(
        &self,
        state: &DistributionState,
        natural_bin: Option<usize>,
        attempted: &mut BTreeSet<usize>,
    ) -> Option<BinChoice> {
        let next_count = state.counts.iter().sum::<usize>() + 1;
        let mut best_debt = f64::NEG_INFINITY;
        let mut best_index = None;
        for (index, bin) in self.bins.iter().enumerate() {
            if bin.frequency <= 0.0 || attempted.contains(&index) {
                continue;
            }
            let debt = bin.frequency * next_count as f64 - state.counts[index] as f64;
            if debt > 1e-12 && debt > best_debt {
                best_debt = debt;
                best_index = Some(index);
            }
        }

        if let Some(index) = best_index {
            let is_relaxed = !attempted.is_empty();
            attempted.insert(index);
            let kind = if is_relaxed {
                BinChoiceKind::Fallback
            } else if natural_bin == Some(index) {
                BinChoiceKind::Natural
            } else {
                BinChoiceKind::Scaled
            };
            return Some(BinChoice { index, kind });
        }

        let mut best = None;
        let mut best_error = f64::INFINITY;
        let mut best_midpoint = f64::INFINITY;
        let accepted_count = next_count;
        for (index, bin) in self.bins.iter().enumerate() {
            if bin.frequency <= 0.0 || attempted.contains(&index) {
                continue;
            }
            let mut variation = 0.0;
            for (other_index, other_bin) in self.bins.iter().enumerate() {
                let observed =
                    state.counts[other_index] as f64 + usize::from(other_index == index) as f64;
                variation += (observed / accepted_count as f64 - other_bin.frequency).abs();
            }
            let midpoint = bin.midpoint();
            if variation < best_error - 1e-15
                || ((variation - best_error).abs() <= 1e-15 && midpoint < best_midpoint)
            {
                best = Some(index);
                best_error = variation;
                best_midpoint = midpoint;
            }
        }

        best.map(|index| {
            attempted.insert(index);
            BinChoice {
                index,
                kind: BinChoiceKind::Fallback,
            }
        })
    }

    // AI-FUNC-SUMMARY: Record one geometry attempt for a chosen bin; mutates state; side effects: increments target attempts.
    pub fn record_attempt(&self, state: &mut DistributionState, index: usize) {
        if index < state.attempts.len() {
            state.attempts[index] += 1;
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Commit one successful placement to its actual target bin and record scale statistics.
    // Inputs: mutable counters, actual bin, placement kind, and optional positive scale factor.
    // Returns: None.
    // Side effects: Mutates accepted counts and scale/fallback statistics.
    pub fn record_success(
        &self,
        state: &mut DistributionState,
        index: usize,
        kind: BinChoiceKind,
        scale_factor: Option<f64>,
    ) {
        if index >= state.counts.len() {
            return;
        }
        state.counts[index] += 1;
        match kind {
            BinChoiceKind::Natural => state.natural += 1,
            BinChoiceKind::Scaled => state.scaled += 1,
            BinChoiceKind::Fallback => state.fallback += 1,
        }
        if let Some(factor) = scale_factor {
            if factor.is_finite() && factor > 0.0 {
                if state.scale_factor_count == 0 {
                    state.scale_factor_min = factor;
                    state.scale_factor_max = factor;
                } else {
                    state.scale_factor_min = state.scale_factor_min.min(factor);
                    state.scale_factor_max = state.scale_factor_max.max(factor);
                }
                state.scale_factor_sum += factor;
                state.scale_factor_count += 1;
            }
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Summarize observed count frequency against requested frequency and best possible integer rounding.
    // Inputs: accepted bin counts.
    // Returns: DistributionSummary with total variation and maximum errors.
    // Side effects: None.
    pub fn summarize(&self, state: &DistributionState) -> DistributionSummary {
        let count = state.counts.iter().sum::<usize>();
        if count == 0 {
            return DistributionSummary::default();
        }
        let mut max_absolute_error: f64 = 0.0;
        let mut total_variation_distance: f64 = 0.0;
        let mut rounding_max_absolute_error: f64 = 0.0;
        let mut integer_targets: Vec<usize> = self
            .bins
            .iter()
            .map(|bin| (bin.frequency * count as f64).floor() as usize)
            .collect();
        let assigned = integer_targets.iter().sum::<usize>();
        let mut remainders: Vec<usize> = (0..self.bins.len()).collect();
        remainders.sort_by(|left, right| {
            let left_remainder =
                self.bins[*left].frequency * count as f64 - integer_targets[*left] as f64;
            let right_remainder =
                self.bins[*right].frequency * count as f64 - integer_targets[*right] as f64;
            right_remainder
                .total_cmp(&left_remainder)
                .then_with(|| left.cmp(right))
        });
        for index in remainders.into_iter().take(count - assigned) {
            integer_targets[index] += 1;
        }
        for (index, bin) in self.bins.iter().enumerate() {
            let observed = state.counts[index] as f64 / count as f64;
            let error = (observed - bin.frequency).abs();
            let ideal_integer = integer_targets[index] as f64 / count as f64;
            let rounding_error = (ideal_integer - bin.frequency).abs();
            max_absolute_error = max_absolute_error.max(error);
            rounding_max_absolute_error = rounding_max_absolute_error.max(rounding_error);
            total_variation_distance += error;
        }
        DistributionSummary {
            count,
            max_absolute_error,
            total_variation_distance: 0.5 * total_variation_distance,
            rounding_max_absolute_error,
        }
    }
}

impl SphericityState {
    // AI-FUNC-SUMMARY: Return the accepted arithmetic mean sphericity; returns None before any success; side effects: None.
    pub fn mean(&self) -> Option<f64> {
        if self.count == 0 {
            None
        } else {
            Some(self.sum / self.count as f64)
        }
    }

    // AI-FUNC-SUMMARY:
    // Purpose: Score a candidate by the distance of the projected accepted mean from the target value or interval.
    // Inputs: current accepted state, candidate metrics, target mean, and optional absolute tolerance.
    // Returns: zero inside the target band, otherwise absolute projected distance; None for invalid target/candidate values.
    // Side effects: None.
    pub fn projected_error(
        &self,
        metrics: MeshMetrics,
        target: f64,
        tolerance: Option<f64>,
    ) -> Option<f64> {
        if !target.is_finite() || !metrics.sphericity.is_finite() {
            return None;
        }
        let projected = (self.sum + metrics.sphericity) / (self.count + 1) as f64;
        if !projected.is_finite() {
            return None;
        }
        let tolerance = tolerance.unwrap_or(0.0);
        let lower = target - tolerance;
        let upper = target + tolerance;
        if projected < lower {
            Some(lower - projected)
        } else if projected > upper {
            Some(projected - upper)
        } else {
            Some(0.0)
        }
    }

    // AI-FUNC-SUMMARY: Commit the sphericity of one successfully placed candidate; mutates state; side effects: increments count and sum.
    pub fn record_success(&mut self, metrics: MeshMetrics) {
        self.sum += metrics.sphericity;
        self.count += 1;
    }
}

// AI-FUNC-SUMMARY:
// Purpose: Parse and validate an optional bin/right/frequency CSV into normalized target diameter intervals.
// Inputs: CSV path.
// Returns: TargetDistribution with strictly ordered non-overlapping bins and frequencies summing exactly to one.
// Side effects: Reads CSV from disk.
pub fn load_target_distribution_csv(path: &Path) -> Result<TargetDistribution> {
    let mut reader = csv::ReaderBuilder::new()
        .trim(csv::Trim::All)
        .from_path(path)
        .map_err(|error| {
            RustMsptError::InvalidConfig(format!(
                "Failed to read target diameter CSV '{}': {error}",
                path.display()
            ))
        })?;

    let headers = reader
        .headers()
        .map_err(|error| {
            RustMsptError::InvalidConfig(format!(
                "Failed to read target diameter CSV headers '{}': {error}",
                path.display()
            ))
        })?
        .clone();
    let allowed: BTreeSet<&str> = ["bin", "right", "frequency"].into_iter().collect();
    let mut seen = BTreeSet::new();
    for header in headers.iter() {
        if !allowed.contains(header) {
            return Err(RustMsptError::InvalidConfig(format!(
                "Target diameter CSV '{}' has unsupported column '{header}' (allowed: bin,right,frequency)",
                path.display()
            )));
        }
        if !seen.insert(header.to_string()) {
            return Err(RustMsptError::InvalidConfig(format!(
                "Target diameter CSV '{}' has duplicate column '{header}'",
                path.display()
            )));
        }
    }
    for required in ["bin", "frequency"] {
        if !seen.contains(required) {
            return Err(RustMsptError::InvalidConfig(format!(
                "Target diameter CSV '{}' is missing required column '{required}'",
                path.display()
            )));
        }
    }
    let has_right = seen.contains("right");
    let bin_index = headers
        .iter()
        .position(|header| header == "bin")
        .unwrap_or(0);
    let right_index = headers.iter().position(|header| header == "right");
    let frequency_index = headers
        .iter()
        .position(|header| header == "frequency")
        .unwrap_or(0);

    let mut lefts = Vec::new();
    let mut rights = Vec::new();
    let mut frequencies = Vec::new();
    for (record_index, record) in reader.records().enumerate() {
        let row = record_index + 2;
        let record = record.map_err(|error| {
            RustMsptError::InvalidConfig(format!(
                "Invalid target diameter CSV '{}' at row {row}: {error}",
                path.display()
            ))
        })?;
        let bin = parse_csv_f64(record.get(bin_index), "bin", path, row)?;
        if !bin.is_finite() || bin < 0.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "Invalid target diameter CSV '{}' at row {row}: bin must be finite and >= 0",
                path.display()
            )));
        }
        if let Some(previous) = lefts.last().copied() {
            if bin <= previous {
                return Err(RustMsptError::InvalidConfig(format!(
                    "Invalid target diameter CSV '{}' at row {row}: bin values must strictly increase",
                    path.display()
                )));
            }
        }
        let right = if has_right {
            parse_optional_csv_f64(record.get(right_index.unwrap_or(0)), "right", path, row)?
        } else {
            None
        };
        if let Some(right) = right {
            if !right.is_finite() || right <= bin {
                return Err(RustMsptError::InvalidConfig(format!(
                    "Invalid target diameter CSV '{}' at row {row}: right must be finite and > bin",
                    path.display()
                )));
            }
        }
        let frequency = parse_csv_f64(record.get(frequency_index), "frequency", path, row)?;
        if !frequency.is_finite() || frequency < 0.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "Invalid target diameter CSV '{}' at row {row}: frequency must be finite and >= 0",
                path.display()
            )));
        }
        lefts.push(bin);
        rights.push(right);
        frequencies.push(frequency);
    }

    if lefts.is_empty() {
        return Err(RustMsptError::InvalidConfig(format!(
            "Target diameter CSV '{}' has no data rows",
            path.display()
        )));
    }

    let mut bins: Vec<DiameterBin> = Vec::with_capacity(lefts.len());
    for index in 0..lefts.len() {
        let right = rights[index].or_else(|| {
            if index + 1 < lefts.len() {
                Some(lefts[index + 1])
            } else if index > 0 {
                let previous_width = bins[index - 1].right - bins[index - 1].left;
                Some(lefts[index] + previous_width)
            } else {
                None
            }
        }).ok_or_else(|| {
            RustMsptError::InvalidConfig(format!(
                "Invalid target diameter CSV '{}' at row {}: single-row distribution requires an explicit right value",
                path.display(),
                index + 2
            ))
        })?;
        if !right.is_finite() || right <= lefts[index] {
            return Err(RustMsptError::InvalidConfig(format!(
                "Invalid target diameter CSV '{}' at row {}: inferred right must be finite and > bin",
                path.display(),
                index + 2
            )));
        }
        let midpoint = lefts[index] + (right - lefts[index]) * 0.5;
        if !midpoint.is_finite() || midpoint <= 0.0 {
            return Err(RustMsptError::InvalidConfig(format!(
                "Invalid target diameter CSV '{}' at row {}: interval midpoint must be finite and > 0",
                path.display(),
                index + 2
            )));
        }
        if let Some(previous) = bins.last().copied() {
            if lefts[index] < previous.right {
                return Err(RustMsptError::InvalidConfig(format!(
                    "Invalid target diameter CSV '{}' at row {}: intervals must not overlap",
                    path.display(),
                    index + 2
                )));
            }
        }
        bins.push(DiameterBin {
            left: lefts[index],
            right,
            frequency: frequencies[index],
        });
    }

    let frequency_sum: f64 = bins.iter().map(|bin| bin.frequency).sum();
    if !frequency_sum.is_finite() || frequency_sum <= 0.0 || (frequency_sum - 1.0).abs() > 1e-6 {
        return Err(RustMsptError::InvalidConfig(format!(
            "Invalid target diameter CSV '{}': frequency sum must be 1.0 within 1e-6 (got {frequency_sum})",
            path.display()
        )));
    }
    for bin in &mut bins {
        bin.frequency /= frequency_sum;
    }
    if !bins.iter().any(|bin| bin.frequency > 0.0) {
        return Err(RustMsptError::InvalidConfig(format!(
            "Invalid target diameter CSV '{}': at least one frequency must be positive",
            path.display()
        )));
    }
    Ok(TargetDistribution { bins })
}

// AI-FUNC-SUMMARY:
// Purpose: Write a per-bin target-versus-actual diameter frequency comparison beside the packed STL.
// Inputs: packed STL output path, parsed target distribution, and successful placement counters.
// Returns: Path to the generated comparison CSV.
// Side effects: Creates or overwrites the derived CSV file.
pub fn write_distribution_comparison_csv(
    output_stl: &Path,
    distribution: &TargetDistribution,
    state: &DistributionState,
) -> Result<PathBuf> {
    let stem = output_stl
        .file_stem()
        .unwrap_or_else(|| OsStr::new("packed_result"))
        .to_string_lossy();
    let comparison_path = output_stl.with_file_name(format!("{stem}_diameter_distribution.csv"));
    let mut writer = csv::Writer::from_path(&comparison_path).map_err(|error| {
        RustMsptError::Io(std::io::Error::other(format!(
            "Failed to create diameter comparison CSV '{}': {error}",
            comparison_path.display()
        )))
    })?;
    writer
        .write_record([
            "bin",
            "right",
            "target_frequency",
            "target_count",
            "actual_count",
            "actual_frequency",
            "frequency_error",
            "count_error",
            "attempts",
        ])
        .map_err(|error| {
            RustMsptError::Io(std::io::Error::other(format!(
                "Failed to write diameter comparison CSV '{}': {error}",
                comparison_path.display()
            )))
        })?;

    let total_count = state.counts.iter().sum::<usize>();
    for (index, bin) in distribution.bins.iter().enumerate() {
        let actual_count = state.counts.get(index).copied().unwrap_or(0);
        let attempts = state.attempts.get(index).copied().unwrap_or(0);
        let target_count = bin.frequency * total_count as f64;
        let actual_frequency = if total_count > 0 {
            actual_count as f64 / total_count as f64
        } else {
            0.0
        };
        writer
            .write_record([
                format!("{:.12}", bin.left),
                format!("{:.12}", bin.right),
                format!("{:.12}", bin.frequency),
                format!("{target_count:.12}"),
                actual_count.to_string(),
                format!("{actual_frequency:.12}"),
                format!("{:.12}", actual_frequency - bin.frequency),
                format!("{:.12}", actual_count as f64 - target_count),
                attempts.to_string(),
            ])
            .map_err(|error| {
                RustMsptError::Io(std::io::Error::other(format!(
                    "Failed to write diameter comparison CSV '{}': {error}",
                    comparison_path.display()
                )))
            })?;
    }
    writer.flush()?;
    Ok(comparison_path)
}

// AI-FUNC-SUMMARY: Parse a required finite CSV float with path/row context; returns f64 or InvalidConfig; side effects: None.
fn parse_csv_f64(value: Option<&str>, column: &str, path: &Path, row: usize) -> Result<f64> {
    let Some(raw) = value else {
        return Err(RustMsptError::InvalidConfig(format!(
            "Invalid target diameter CSV '{}' at row {row}: missing '{column}' value",
            path.display()
        )));
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(RustMsptError::InvalidConfig(format!(
            "Invalid target diameter CSV '{}' at row {row}: missing '{column}' value",
            path.display()
        )));
    }
    trimmed.parse::<f64>().map_err(|error| {
        RustMsptError::InvalidConfig(format!(
            "Invalid target diameter CSV '{}' at row {row}, column '{column}': {error}",
            path.display()
        ))
    })
}

// AI-FUNC-SUMMARY: Parse an optional CSV float where blank means absent; returns Option<f64>; side effects: None.
fn parse_optional_csv_f64(
    value: Option<&str>,
    column: &str,
    path: &Path,
    row: usize,
) -> Result<Option<f64>> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    parse_csv_f64(Some(trimmed), column, path, row).map(Some)
}
