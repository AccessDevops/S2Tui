//! GPU backend detection and management for Whisper

use serde::{Deserialize, Serialize};

/// Available GPU backends for Whisper acceleration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GpuBackend {
    /// CPU-only processing (no GPU acceleration)
    Cpu,
    /// Apple Metal (macOS)
    Metal,
    /// NVIDIA CUDA (Windows/Linux)
    Cuda,
    /// AMD ROCm/HIPBlas (Linux)
    HipBlas,
    /// Vulkan cross-platform GPU
    Vulkan,
}

impl GpuBackend {
    /// Get the human-readable name of this backend
    pub fn name(&self) -> &'static str {
        match self {
            GpuBackend::Cpu => "CPU",
            GpuBackend::Metal => "Metal",
            GpuBackend::Cuda => "CUDA",
            GpuBackend::HipBlas => "HIPBlas (ROCm)",
            GpuBackend::Vulkan => "Vulkan",
        }
    }

    /// Get a description of this backend
    pub fn description(&self) -> &'static str {
        match self {
            GpuBackend::Cpu => "CPU-only processing (no GPU acceleration)",
            GpuBackend::Metal => "Apple Metal GPU acceleration (macOS)",
            GpuBackend::Cuda => "NVIDIA CUDA GPU acceleration",
            GpuBackend::HipBlas => "AMD ROCm/HIPBlas GPU acceleration (Linux)",
            GpuBackend::Vulkan => "Vulkan cross-platform GPU acceleration",
        }
    }
}

/// Detect the active GPU backend based on compilation features
#[allow(unreachable_code)]
pub fn detect_active_backend() -> GpuBackend {
    // Check for enabled GPU features in order of preference

    // macOS always has Metal enabled in Cargo.toml
    #[cfg(target_os = "macos")]
    {
        tracing::info!("GPU: Metal backend enabled (macOS default)");
        return GpuBackend::Metal;
    }

    #[cfg(feature = "gpu-cuda")]
    {
        if is_cuda_available() {
            tracing::info!("GPU: CUDA backend enabled and available");
            return GpuBackend::Cuda;
        } else {
            tracing::warn!("GPU: CUDA backend enabled but not available, falling back to CPU");
        }
    }

    #[cfg(feature = "gpu-hipblas")]
    {
        if is_hipblas_available() {
            tracing::info!("GPU: HIPBlas backend enabled and available");
            return GpuBackend::HipBlas;
        } else {
            tracing::warn!("GPU: HIPBlas backend enabled but not available, falling back to CPU");
        }
    }

    #[cfg(feature = "gpu-vulkan")]
    {
        if is_vulkan_available() {
            tracing::info!("GPU: Vulkan backend enabled and available");
            return GpuBackend::Vulkan;
        } else {
            tracing::warn!("GPU: Vulkan backend enabled but not available, falling back to CPU");
        }
    }

    #[cfg(not(any(
        target_os = "macos",
        feature = "gpu-cuda",
        feature = "gpu-hipblas",
        feature = "gpu-vulkan"
    )))]
    tracing::info!("GPU: Using CPU-only processing");

    GpuBackend::Cpu
}

/// Get list of all backends compiled into this build
pub fn get_compiled_backends() -> Vec<GpuBackend> {
    let backends = vec![GpuBackend::Cpu];

    #[cfg(target_os = "macos")]
    let backends = {
        let mut b = backends;
        b.push(GpuBackend::Metal);
        b
    };

    #[cfg(feature = "gpu-cuda")]
    let backends = {
        let mut b = backends;
        b.push(GpuBackend::Cuda);
        b
    };

    #[cfg(feature = "gpu-hipblas")]
    let backends = {
        let mut b = backends;
        b.push(GpuBackend::HipBlas);
        b
    };

    #[cfg(feature = "gpu-vulkan")]
    let backends = {
        let mut b = backends;
        b.push(GpuBackend::Vulkan);
        b
    };

    backends
}

/// Check if NVIDIA CUDA is available on the system
#[cfg(feature = "gpu-cuda")]
fn is_cuda_available() -> bool {
    use std::process::Command;

    // Try nvidia-smi to detect NVIDIA GPU
    match Command::new("nvidia-smi").output() {
        Ok(output) => {
            let available = output.status.success();
            if available {
                tracing::debug!("CUDA: nvidia-smi detected GPU");
            } else {
                tracing::debug!("CUDA: nvidia-smi failed (no GPU or driver issue)");
            }
            available
        }
        Err(e) => {
            tracing::debug!("CUDA: nvidia-smi not found: {}", e);
            false
        }
    }
}

#[cfg(not(feature = "gpu-cuda"))]
#[allow(dead_code)]
fn is_cuda_available() -> bool {
    false
}

/// Check if AMD ROCm/HIPBlas is available on the system
#[cfg(feature = "gpu-hipblas")]
fn is_hipblas_available() -> bool {
    use std::path::Path;
    use std::process::Command;

    // Check for ROCm installation directory
    if Path::new("/opt/rocm").exists() {
        tracing::debug!("HIPBlas: /opt/rocm directory found");

        // Try rocminfo to confirm
        match Command::new("rocminfo").output() {
            Ok(output) => {
                let available = output.status.success();
                if available {
                    tracing::debug!("HIPBlas: rocminfo confirmed GPU");
                } else {
                    tracing::debug!("HIPBlas: rocminfo failed");
                }
                return available;
            }
            Err(e) => {
                tracing::debug!("HIPBlas: rocminfo not found: {}", e);
            }
        }

        // ROCm directory exists, assume available
        return true;
    }

    tracing::debug!("HIPBlas: ROCm not detected");
    false
}

#[cfg(not(feature = "gpu-hipblas"))]
#[allow(dead_code)]
fn is_hipblas_available() -> bool {
    false
}

/// Check if Vulkan is available on the system
#[cfg(feature = "gpu-vulkan")]
fn is_vulkan_available() -> bool {
    use std::process::Command;

    // Try vulkaninfo to detect Vulkan support
    match Command::new("vulkaninfo").output() {
        Ok(output) => {
            let available = output.status.success();
            if available {
                tracing::debug!("Vulkan: vulkaninfo detected support");
            } else {
                tracing::debug!("Vulkan: vulkaninfo failed");
            }
            available
        }
        Err(e) => {
            tracing::debug!("Vulkan: vulkaninfo not found: {}", e);

            // On some systems, Vulkan might be available but vulkaninfo not installed
            // Check for common Vulkan library locations as fallback
            #[cfg(target_os = "linux")]
            {
                use std::path::Path;
                if Path::new("/usr/lib/x86_64-linux-gnu/libvulkan.so").exists()
                    || Path::new("/usr/lib64/libvulkan.so").exists()
                {
                    tracing::debug!("Vulkan: Library found (assuming available)");
                    return true;
                }
            }

            #[cfg(target_os = "windows")]
            {
                use std::path::Path;
                if Path::new("C:\\Windows\\System32\\vulkan-1.dll").exists() {
                    tracing::debug!("Vulkan: DLL found (assuming available)");
                    return true;
                }
            }

            false
        }
    }
}

#[cfg(not(feature = "gpu-vulkan"))]
#[allow(dead_code)]
fn is_vulkan_available() -> bool {
    false
}

/// Information about GPU support in this build
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    /// Currently active backend
    pub active_backend: GpuBackend,
    /// All backends compiled into this build
    pub compiled_backends: Vec<GpuBackend>,
    /// Whether hardware acceleration is actually being used
    pub hardware_accelerated: bool,
}

impl GpuInfo {
    /// Detect and return current GPU information
    pub fn detect() -> Self {
        let active_backend = detect_active_backend();
        let compiled_backends = get_compiled_backends();
        let hardware_accelerated = active_backend != GpuBackend::Cpu;

        GpuInfo {
            active_backend,
            compiled_backends,
            hardware_accelerated,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_names() {
        assert_eq!(GpuBackend::Cpu.name(), "CPU");
        assert_eq!(GpuBackend::Metal.name(), "Metal");
        assert_eq!(GpuBackend::Cuda.name(), "CUDA");
    }

    #[test]
    fn test_detect_backend() {
        let backend = detect_active_backend();
        println!("Detected backend: {:?}", backend);

        // Should always have at least CPU
        assert!(get_compiled_backends().contains(&GpuBackend::Cpu));
    }

    #[test]
    fn test_gpu_info() {
        let info = GpuInfo::detect();
        println!("GPU Info: {:?}", info);

        assert!(!info.compiled_backends.is_empty());
        assert!(info.compiled_backends.contains(&info.active_backend));
    }
}
