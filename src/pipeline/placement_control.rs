//! Cooperative stop at proposal-batch boundaries. Signal handlers only store an atomic flag.
use crate::config::placement::ResolvedPlacement;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static STOP: AtomicBool = AtomicBool::new(false);

#[cfg(unix)]
// AI-FUNC-SUMMARY: Record SIGINT/SIGTERM without allocation or I/O; returns nothing; sets a lock-free atomic flag.
extern "C" fn request_stop(_: libc::c_int) {
    STOP.store(true, Ordering::Relaxed);
}

/// CLI-only signal installation. Library callers use the per-output STOP file.
pub struct SignalGuard {
    #[cfg(unix)]
    previous: [libc::sigaction; 2],
}
impl SignalGuard {
    // AI-FUNC-SUMMARY: Install Unix SIGINT/SIGTERM handlers for one CLI placement; returns restoring guard or OS error; clears the process stop flag. Other platforms use STOP files.
    pub fn install() -> std::io::Result<Self> {
        STOP.store(false, Ordering::Relaxed);
        #[cfg(unix)]
        unsafe {
            // SAFETY: zeroed sigaction/masks are initialized before use. The C handler
            // has the required ABI and performs only a lock-free atomic store.
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = request_stop as *const () as libc::sighandler_t;
            libc::sigemptyset(&mut action.sa_mask);
            action.sa_flags = libc::SA_RESTART;
            let mut previous: [libc::sigaction; 2] = std::mem::zeroed();
            if libc::sigaction(libc::SIGINT, &action, &mut previous[0]) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::sigaction(libc::SIGTERM, &action, &mut previous[1]) != 0 {
                let error = std::io::Error::last_os_error();
                libc::sigaction(libc::SIGINT, &previous[0], std::ptr::null_mut());
                return Err(error);
            }
            Ok(Self { previous })
        }
        #[cfg(not(unix))]
        Ok(Self {})
    }
}
impl Drop for SignalGuard {
    // AI-FUNC-SUMMARY: Restore pre-run Unix signal handlers; returns nothing; changes process signal disposition.
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            // SAFETY: these actions were filled by successful sigaction calls at install.
            libc::sigaction(libc::SIGINT, &self.previous[0], std::ptr::null_mut());
            libc::sigaction(libc::SIGTERM, &self.previous[1], std::ptr::null_mut());
        }
    }
}

pub(crate) struct PlacementControl {
    pub interrupted: bool,
    pub phase: &'static str,
    started: Instant,
    last_poll: Instant,
    last_progress: Instant,
}
impl PlacementControl {
    // AI-FUNC-SUMMARY: Initialize per-run cancellation/progress clocks; returns control state; no I/O.
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            interrupted: false,
            phase: "packing",
            started: now,
            last_poll: now,
            last_progress: now,
        }
    }
    // AI-FUNC-SUMMARY: Check signal every batch, STOP file at most four times/sec, progress every 10 sec; returns latched cancellation; never modifies placement/RNG state.
    pub fn poll(
        &mut self,
        config: &ResolvedPlacement,
        force: bool,
        placed: usize,
        attempts: usize,
        vf: f64,
    ) -> bool {
        let previously_interrupted = self.interrupted;
        self.interrupted |= STOP.load(Ordering::Relaxed);
        if force || self.last_poll.elapsed() >= Duration::from_millis(250) {
            self.interrupted |= config.outputs.dir.join("STOP").exists();
            self.last_poll = Instant::now();
        }
        if self.interrupted && !previously_interrupted {
            eprintln!("[Info] Stop requested; finishing the current batch and saving accepted particles. Do not force-kill while saving.");
        }
        if force || self.last_progress.elapsed() >= Duration::from_secs(10) {
            self.publish(config, self.phase, placed, attempts, vf);
        }
        self.interrupted
    }
    // AI-FUNC-SUMMARY: Atomically replace progress.json and print heartbeat; returns nothing; reports telemetry write failures without discarding pack results.
    pub fn publish(
        &mut self,
        config: &ResolvedPlacement,
        state: &str,
        placed: usize,
        attempts: usize,
        vf: f64,
    ) {
        let value = serde_json::json!({
            "state": state, "particles": placed, "attempts": attempts,
            "volume_fraction_basis": vf, "target_volume_fraction": config.target_volume_fraction,
            "elapsed_s": self.started.elapsed().as_secs_f64(),
            "stop_requested": self.interrupted, "resumable": false
        });
        let temporary = config.outputs.dir.join("progress.json.tmp");
        let result =
            crate::pipeline::placement_outputs::write_json(&temporary, &value).and_then(|()| {
                std::fs::rename(&temporary, config.outputs.dir.join("progress.json"))?;
                Ok(())
            });
        if let Err(error) = result {
            eprintln!("[Warning] Cannot update placement progress: {error}");
        }
        eprintln!("[Progress] placement state={state} particles={placed} attempts={attempts} vf={vf:.8} elapsed_s={:.1}", self.started.elapsed().as_secs_f64());
        self.last_progress = Instant::now();
    }
}
