use std::time::Instant;

pub struct StageTimer {
    pipeline: &'static str,
    total: Instant,
    stage: Instant,
}

impl StageTimer {
    // AI-FUNC-SUMMARY: Start a stage timer for one pipeline, with both the total and the current-stage clocks set to now; returns StageTimer; side effects: none.
    pub fn start(pipeline: &'static str) -> Self {
        let now = Instant::now();
        Self { pipeline, total: now, stage: now }
    }

    // AI-FUNC-SUMMARY: Reset the current-stage clock without reporting, so untimed work between stages is excluded; returns nothing; side effects: none.
    pub fn restart(&mut self) {
        self.stage = Instant::now();
    }

    // AI-FUNC-SUMMARY: Print `[Timing] <pipeline> stage=<name> seconds=<f>` for the time since the last mark and restart the stage clock; returns the elapsed seconds; side effects: writes one stdout line.
    pub fn stage(&mut self, name: &str) -> f64 {
        let seconds = self.stage.elapsed().as_secs_f64();
        println!("{}", format_stage_line(self.pipeline, name, seconds));
        self.stage = Instant::now();
        seconds
    }

    // AI-FUNC-SUMMARY: Print a stage line for an externally accumulated duration (e.g. a sum over views) without touching the stage clock; returns nothing; side effects: writes one stdout line.
    pub fn report(&self, name: &str, seconds: f64) {
        println!("{}", format_stage_line(self.pipeline, name, seconds));
    }

    // AI-FUNC-SUMMARY: Print the time since the timer started as a stage line named `name` (e.g. total_in_pool) without touching the stage clock; returns the elapsed seconds; side effects: writes one stdout line.
    pub fn total(&self, name: &str) -> f64 {
        let seconds = self.total.elapsed().as_secs_f64();
        println!("{}", format_stage_line(self.pipeline, name, seconds));
        seconds
    }

    // AI-FUNC-SUMMARY: Print the calling thread's Rayon worker count and the process peak RSS for this pipeline; returns nothing; side effects: writes two stdout lines.
    pub fn report_resources(&self) {
        report_workers(self.pipeline);
        report_peak_rss(self.pipeline);
    }
}

// AI-FUNC-SUMMARY: Format one stage timing line in the shared `[Timing] <pipeline> stage=<name> seconds=<f>` layout with nine decimals; returns String; side effects: none.
pub fn format_stage_line(pipeline: &str, stage: &str, seconds: f64) -> String {
    format!("[Timing] {pipeline} stage={stage} seconds={seconds:.9}")
}

// AI-FUNC-SUMMARY: Print `[Timing] <pipeline> workers=<n>` using the current Rayon pool's thread count, so it must be called inside the pipeline's installed pool; returns nothing; side effects: writes one stdout line.
pub fn report_workers(pipeline: &str) {
    println!("[Timing] {pipeline} workers={}", rayon::current_num_threads());
}

// AI-FUNC-SUMMARY: Print `[Timing] <pipeline> peak_rss_bytes=<n|unavailable>` from VmHWM; returns nothing; side effects: reads /proc/self/status and writes one stdout line.
pub fn report_peak_rss(pipeline: &str) {
    match peak_rss_bytes() {
        Some(bytes) => println!("[Timing] {pipeline} peak_rss_bytes={bytes}"),
        None => println!("[Timing] {pipeline} peak_rss_bytes=unavailable"),
    }
}

// AI-FUNC-SUMMARY: Read the process high-water resident set size (VmHWM) from /proc/self/status; returns Some(bytes) or None when the file or field is unavailable, never an estimate; side effects: reads /proc/self/status.
pub fn peak_rss_bytes() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_vm_hwm(&status)
}

// AI-FUNC-SUMMARY: Parse the `VmHWM:` line of a /proc status document (value in kB) into bytes; returns Some(bytes) or None when absent or malformed; side effects: none.
pub fn parse_vm_hwm(status: &str) -> Option<u64> {
    let line = status.lines().find(|line| line.starts_with("VmHWM:"))?;
    let mut fields = line["VmHWM:".len()..].split_whitespace();
    let value: u64 = fields.next()?.parse().ok()?;
    match fields.next() {
        Some("kB") => value.checked_mul(1024),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // AI-FUNC-SUMMARY: Check the stage line layout matches the crop format consumed by scripts/perf_matrix.py.
    #[test]
    fn stage_line_matches_the_shared_format() {
        assert_eq!(
            format_stage_line("crop", "load", 1.5),
            "[Timing] crop stage=load seconds=1.500000000"
        );
    }

    // AI-FUNC-SUMMARY: Check VmHWM parsing for present, missing and malformed fields.
    #[test]
    fn vm_hwm_parses_kb_and_refuses_anything_else() {
        assert_eq!(parse_vm_hwm("Name:\tx\nVmHWM:\t  2048 kB\nVmRSS:\t1 kB\n"), Some(2048 * 1024));
        assert_eq!(parse_vm_hwm("Name:\tx\n"), None);
        assert_eq!(parse_vm_hwm("VmHWM:\tabc kB\n"), None);
        assert_eq!(parse_vm_hwm("VmHWM:\t12 MB\n"), None);
    }

    // AI-FUNC-SUMMARY: Check peak RSS is reported as a positive value on Linux and absent elsewhere.
    #[test]
    fn peak_rss_is_positive_on_linux() {
        if cfg!(target_os = "linux") {
            assert!(peak_rss_bytes().is_some_and(|bytes| bytes > 0));
        }
    }
}
