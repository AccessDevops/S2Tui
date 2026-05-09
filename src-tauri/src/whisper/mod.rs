pub mod compat;
mod gpu;
mod worker;

// `ImportWarning` is referenced via `ValidationResult.warnings`; the
// re-export keeps it available for direct match expressions in later
// steps (Step 9 will pattern-match on it for memory pre-flight).
#[allow(unused_imports)]
pub use compat::{ImportWarning, ModelCapabilities, ModelCompatError, ValidationResult};
pub use gpu::{check_system_health, detect_active_backend, GpuBackend, GpuInfo, SystemHealthCheck};
// macOS doesn't ship a Vulkan-startup-check (Metal is always available),
// so only re-export the symbol on platforms where it actually exists.
// Mirrors the cfg gate in gpu.rs and the single call site in lib.rs.
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub use gpu::is_vulkan_available_at_startup;
pub use worker::{ModelLoadResult, WhisperWorker};
