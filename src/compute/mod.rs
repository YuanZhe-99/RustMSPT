pub mod backend;
pub mod policy;

pub use backend::{AccelerationMode, BackendCaps, ComputeBackend};
pub use policy::select_backend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_backend_is_not_gpu() {
        let sel = select_backend(AccelerationMode::Cpu, None, None, 0);
        assert!(!sel.backend.is_gpu());
        assert!(sel.fallback.is_none());
    }

    #[test]
    fn auto_falls_back_for_small_workload() {
        let sel = select_backend(AccelerationMode::Auto, Some(250_000), None, 100);
        assert!(!sel.backend.is_gpu());
        assert!(sel.fallback.is_some());
        assert!(sel.fallback.unwrap().reason.contains("below gpu_min_voxels"));
    }

    #[test]
    fn default_mode_is_auto() {
        assert_eq!(AccelerationMode::default(), AccelerationMode::Auto);
    }

    #[test]
    fn display_formats_correctly() {
        assert_eq!(format!("{}", AccelerationMode::Auto), "auto");
        assert_eq!(format!("{}", AccelerationMode::Cpu), "cpu");
        assert_eq!(format!("{}", AccelerationMode::Gpu), "gpu");
        assert_eq!(format!("{}", ComputeBackend::Cpu), "cpu");
    }

    #[test]
    fn cpu_caps_show_no_gpu_support() {
        let caps = ComputeBackend::Cpu.caps();
        assert!(!caps.supports_gpu);
        assert_eq!(caps.name, "cpu");
    }

    #[test]
    #[cfg(feature = "gpu")]
    fn gpu_adapter_detection() {
        let sel = select_backend(AccelerationMode::Gpu, None, None, 1_000_000);
        println!("Backend: {}", sel.backend);
        if let Some(fb) = &sel.fallback {
            println!("Fallback: {}", fb.reason);
        } else {
            let caps = sel.backend.caps();
            println!(
                "GPU caps: name={}, max_buffer_size={}, max_storage_buffer_binding_size={}",
                caps.name, caps.max_buffer_size, caps.max_storage_buffer_binding_size
            );
            assert!(sel.backend.is_gpu());
        }
    }
}
