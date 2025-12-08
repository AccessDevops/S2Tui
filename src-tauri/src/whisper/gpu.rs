//! GPU backend detection and management for Whisper

use serde::{Deserialize, Serialize};

#[cfg(all(
    feature = "gpu-vulkan",
    any(target_os = "windows", target_os = "linux")
))]
use std::path::Path;
#[cfg(all(
    feature = "gpu-vulkan",
    any(target_os = "windows", target_os = "linux")
))]
use std::sync::OnceLock;

/// Cache for Vulkan availability check (tested once at startup)
#[cfg(all(
    feature = "gpu-vulkan",
    any(target_os = "windows", target_os = "linux")
))]
static VULKAN_AVAILABLE: OnceLock<bool> = OnceLock::new();

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

    // Vulkan for Windows/Linux (only when gpu-vulkan feature is enabled)
    #[cfg(all(
        feature = "gpu-vulkan",
        any(target_os = "windows", target_os = "linux")
    ))]
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
        any(target_os = "windows", target_os = "linux")
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

    // Vulkan compiled for Windows/Linux only when gpu-vulkan feature is enabled
    #[cfg(all(
        feature = "gpu-vulkan",
        any(target_os = "windows", target_os = "linux")
    ))]
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

/// Check if Vulkan is available on the system (cached result)
#[cfg(all(
    feature = "gpu-vulkan",
    any(target_os = "windows", target_os = "linux")
))]
fn is_vulkan_available() -> bool {
    *VULKAN_AVAILABLE.get_or_init(|| {
        tracing::info!("Vulkan: Performing initial availability check...");

        // Quick check first: is the Vulkan library present?
        if !quick_vulkan_check() {
            tracing::info!("Vulkan: Quick check failed - library not found");
            return false;
        }

        // Thorough check: can we actually initialize Vulkan?
        let result = is_vulkan_truly_available();
        if result {
            tracing::info!("Vulkan: Verified - GPU acceleration available");
        } else {
            tracing::warn!("Vulkan: Library present but initialization failed - using CPU");
        }
        result
    })
}

/// Quick check for Vulkan library presence (fast, not conclusive)
#[cfg(all(
    feature = "gpu-vulkan",
    any(target_os = "windows", target_os = "linux")
))]
fn quick_vulkan_check() -> bool {
    use std::process::Command;

    // Try vulkaninfo first (most reliable quick check)
    if let Ok(output) = Command::new("vulkaninfo").arg("--summary").output() {
        if output.status.success() {
            tracing::debug!("Vulkan: vulkaninfo detected support");
            return true;
        }
    }

    // Fallback: check for library files
    #[cfg(target_os = "linux")]
    {
        if Path::new("/usr/lib/x86_64-linux-gnu/libvulkan.so").exists()
            || Path::new("/usr/lib64/libvulkan.so").exists()
            || Path::new("/usr/lib/libvulkan.so").exists()
        {
            tracing::debug!("Vulkan: Linux library found");
            return true;
        }
    }

    #[cfg(target_os = "windows")]
    {
        if Path::new("C:\\Windows\\System32\\vulkan-1.dll").exists() {
            tracing::debug!("Vulkan: Windows DLL found");
            return true;
        }
    }

    false
}

/// Thorough Vulkan check: actually try to initialize Vulkan and enumerate GPUs
/// This catches cases where the library exists but Vulkan doesn't work
#[cfg(all(
    feature = "gpu-vulkan",
    any(target_os = "windows", target_os = "linux")
))]
fn is_vulkan_truly_available() -> bool {
    use ash::{vk, Entry};

    // Step 1: Load the Vulkan library
    let entry = match unsafe { Entry::load() } {
        Ok(e) => {
            tracing::debug!("Vulkan: Entry loaded successfully");
            e
        }
        Err(e) => {
            tracing::debug!("Vulkan: Failed to load entry: {}", e);
            return false;
        }
    };

    // Step 2: Create a minimal Vulkan instance
    let app_info = vk::ApplicationInfo {
        api_version: vk::make_api_version(0, 1, 0, 0),
        ..Default::default()
    };

    let create_info = vk::InstanceCreateInfo {
        p_application_info: &app_info,
        ..Default::default()
    };

    let instance = match unsafe { entry.create_instance(&create_info, None) } {
        Ok(i) => {
            tracing::debug!("Vulkan: Instance created successfully");
            i
        }
        Err(e) => {
            tracing::debug!("Vulkan: Failed to create instance: {:?}", e);
            return false;
        }
    };

    // Step 3: Check if at least one physical device (GPU) exists
    let has_gpu = match unsafe { instance.enumerate_physical_devices() } {
        Ok(devices) => {
            let count = devices.len();
            tracing::debug!("Vulkan: Found {} physical device(s)", count);

            if count > 0 {
                // Log device info for debugging
                for (i, device) in devices.iter().enumerate() {
                    let props = unsafe { instance.get_physical_device_properties(*device) };
                    let device_name = unsafe {
                        std::ffi::CStr::from_ptr(props.device_name.as_ptr()).to_string_lossy()
                    };
                    tracing::debug!("Vulkan: Device {}: {}", i, device_name);
                }
                true
            } else {
                false
            }
        }
        Err(e) => {
            tracing::debug!("Vulkan: Failed to enumerate devices: {:?}", e);
            false
        }
    };

    // Step 4: Clean up
    unsafe { instance.destroy_instance(None) };
    tracing::debug!("Vulkan: Instance destroyed, check complete");

    has_gpu
}

#[cfg(not(all(
    feature = "gpu-vulkan",
    any(target_os = "windows", target_os = "linux")
)))]
#[allow(dead_code)]
fn is_vulkan_available() -> bool {
    false
}

// ============================================================================
// Public Vulkan Detection (for startup check in lib.rs)
// ============================================================================

/// Public function to check Vulkan availability at application startup.
/// This is called BEFORE Tauri initialization to decide whether to show
/// the Vulkan warning window or proceed with normal app launch.
#[cfg(all(
    feature = "gpu-vulkan",
    any(target_os = "windows", target_os = "linux")
))]
pub fn is_vulkan_available_at_startup() -> bool {
    // Perform the full Vulkan check
    is_vulkan_available()
}

/// On macOS, Metal is always available (built-in), so we return true.
#[cfg(target_os = "macos")]
pub fn is_vulkan_available_at_startup() -> bool {
    true
}

/// Fallback for other platforms or when gpu-vulkan feature is disabled.
#[cfg(not(any(
    target_os = "macos",
    all(
        feature = "gpu-vulkan",
        any(target_os = "windows", target_os = "linux")
    )
)))]
pub fn is_vulkan_available_at_startup() -> bool {
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

// ============================================================================
// System Health Check Structures
// ============================================================================

/// Vérification complète de la santé système pour GPU/Vulkan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemHealthCheck {
    /// Vulkan est-il disponible sur le système ?
    pub vulkan_available: bool,
    /// Version de Vulkan détectée (si disponible)
    pub vulkan_version: Option<String>,
    /// Backend GPU actif
    pub gpu_backend: GpuBackend,
    /// Informations sur le système d'exploitation
    pub os_info: OsInfo,
    /// Guide d'installation Vulkan (si Vulkan non disponible)
    pub install_guide: Option<VulkanInstallGuide>,
    /// L'application peut-elle fonctionner sans Vulkan ? (toujours true)
    pub can_run_without_vulkan: bool,
}

/// Informations sur le système d'exploitation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OsInfo {
    /// Plateforme: "windows", "linux", "macos"
    pub platform: String,
    /// Version de l'OS (si détectable)
    pub version: Option<String>,
    /// Distribution Linux (ubuntu, fedora, arch, etc.)
    pub distribution: Option<String>,
}

/// Guide d'installation Vulkan
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VulkanInstallGuide {
    /// Titre du guide
    pub title: String,
    /// Description courte
    pub description: String,
    /// Étapes d'installation
    pub steps: Vec<String>,
    /// Liens de téléchargement (Windows)
    pub download_urls: Vec<DownloadLink>,
    /// Commandes terminal (Linux)
    pub terminal_commands: Option<Vec<String>>,
}

/// Lien de téléchargement pour drivers
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadLink {
    /// Nom du driver/vendor
    pub name: String,
    /// URL de téléchargement
    pub url: String,
    /// Description
    pub description: String,
}

// ============================================================================
// OS Detection Functions
// ============================================================================

/// Détecte les informations sur le système d'exploitation
fn detect_os_info() -> OsInfo {
    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unknown"
    }
    .to_string();

    let version = detect_os_version();
    let distribution = if cfg!(target_os = "linux") {
        detect_linux_distribution()
    } else {
        None
    };

    OsInfo {
        platform,
        version,
        distribution,
    }
}

/// Détecte la version de l'OS
fn detect_os_version() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        // Sur Windows, on peut utiliser la commande ver ou les registres
        // Pour simplifier, on retourne None pour l'instant
        None
    }

    #[cfg(target_os = "linux")]
    {
        // Lire VERSION_ID depuis /etc/os-release
        if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
            for line in content.lines() {
                if line.starts_with("VERSION_ID=") {
                    return Some(line[11..].trim_matches('"').to_string());
                }
            }
        }
        None
    }

    #[cfg(target_os = "macos")]
    {
        // Utiliser sw_vers pour obtenir la version
        use std::process::Command;
        if let Ok(output) = Command::new("sw_vers").arg("-productVersion").output() {
            if output.status.success() {
                return Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
            }
        }
        None
    }

    #[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
    None
}

/// Détecte la distribution Linux
#[cfg(target_os = "linux")]
fn detect_linux_distribution() -> Option<String> {
    // 1. Lire /etc/os-release (méthode standard)
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if line.starts_with("ID=") {
                let id = line[3..].trim_matches('"').to_lowercase();
                return Some(id);
            }
        }
    }

    // 2. Fallback: vérifier fichiers spécifiques
    if Path::new("/etc/debian_version").exists() {
        return Some("debian".to_string());
    }
    if Path::new("/etc/fedora-release").exists() {
        return Some("fedora".to_string());
    }
    if Path::new("/etc/arch-release").exists() {
        return Some("arch".to_string());
    }
    if Path::new("/etc/SuSE-release").exists() {
        return Some("opensuse".to_string());
    }

    None
}

#[cfg(not(target_os = "linux"))]
fn detect_linux_distribution() -> Option<String> {
    None
}

// ============================================================================
// Vulkan Version Detection
// ============================================================================

/// Récupère la version de Vulkan installée
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn get_vulkan_version() -> Option<String> {
    use std::process::Command;

    // Exécuter vulkaninfo et extraire la version
    if let Ok(output) = Command::new("vulkaninfo").arg("--summary").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            // Chercher la ligne "Vulkan Instance Version:"
            for line in stdout.lines() {
                if line.contains("Vulkan Instance Version:") || line.contains("apiVersion") {
                    // Extraire la version
                    if let Some(version) = line.split_whitespace().last() {
                        return Some(version.to_string());
                    }
                }
            }
        }
    }

    None
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
fn get_vulkan_version() -> Option<String> {
    None
}

// ============================================================================
// Installation Guide Generation
// ============================================================================

/// Génère le guide d'installation Vulkan selon l'OS
fn generate_install_guide(os: &OsInfo) -> VulkanInstallGuide {
    match os.platform.as_str() {
        "windows" => generate_windows_guide(),
        "linux" => generate_linux_guide(os.distribution.as_deref()),
        _ => generate_generic_guide(),
    }
}

fn generate_windows_guide() -> VulkanInstallGuide {
    VulkanInstallGuide {
        title: "Install Vulkan on Windows".to_string(),
        description: "Modern GPU drivers include Vulkan automatically. Update your graphics drivers to enable GPU acceleration.".to_string(),
        steps: vec![
            "Download the latest drivers for your graphics card".to_string(),
            "Install the drivers and restart your computer".to_string(),
            "Relaunch S2Tui to enable GPU acceleration".to_string(),
        ],
        download_urls: vec![
            DownloadLink {
                name: "NVIDIA GeForce Drivers".to_string(),
                url: "https://www.nvidia.com/Download/index.aspx".to_string(),
                description: "For NVIDIA GeForce and Quadro cards".to_string(),
            },
            DownloadLink {
                name: "AMD Radeon Drivers".to_string(),
                url: "https://www.amd.com/en/support".to_string(),
                description: "For AMD Radeon graphics cards".to_string(),
            },
            DownloadLink {
                name: "Intel Graphics Drivers".to_string(),
                url: "https://www.intel.com/content/www/us/en/download-center/home.html".to_string(),
                description: "For Intel integrated GPUs (UHD, Iris)".to_string(),
            },
        ],
        terminal_commands: None,
    }
}

fn generate_linux_guide(distribution: Option<&str>) -> VulkanInstallGuide {
    let (title, commands) = match distribution {
        Some("ubuntu") | Some("debian") | Some("linuxmint") | Some("pop") => (
            "Install Vulkan on Ubuntu/Debian",
            vec![
                "sudo apt update".to_string(),
                "sudo apt install -y libvulkan1 vulkan-tools mesa-vulkan-drivers".to_string(),
            ],
        ),
        Some("fedora") | Some("rhel") | Some("centos") | Some("rocky") | Some("almalinux") => (
            "Install Vulkan on Fedora/RHEL",
            vec!["sudo dnf install -y vulkan-loader vulkan-tools mesa-vulkan-drivers".to_string()],
        ),
        Some("arch") | Some("manjaro") | Some("endeavouros") => (
            "Install Vulkan on Arch Linux",
            vec!["sudo pacman -S vulkan-icd-loader vulkan-tools mesa".to_string()],
        ),
        Some("opensuse") | Some("suse") => (
            "Install Vulkan on openSUSE",
            vec!["sudo zypper install libvulkan1 vulkan-tools Mesa-vulkan-drivers".to_string()],
        ),
        _ => (
            "Install Vulkan on Linux",
            vec![
                "# For Debian/Ubuntu:".to_string(),
                "sudo apt install -y libvulkan1 vulkan-tools mesa-vulkan-drivers".to_string(),
                "# For Fedora:".to_string(),
                "sudo dnf install -y vulkan-loader vulkan-tools mesa-vulkan-drivers".to_string(),
            ],
        ),
    };

    VulkanInstallGuide {
        title: title.to_string(),
        description: "Install Vulkan packages using your package manager.".to_string(),
        steps: vec![
            "Open a terminal".to_string(),
            "Run the commands below".to_string(),
            "Relaunch S2Tui".to_string(),
        ],
        download_urls: vec![],
        terminal_commands: Some(commands),
    }
}

fn generate_generic_guide() -> VulkanInstallGuide {
    VulkanInstallGuide {
        title: "GPU Acceleration".to_string(),
        description:
            "GPU acceleration is not available on this platform. The application will use CPU mode."
                .to_string(),
        steps: vec!["No action required - CPU mode will be used automatically".to_string()],
        download_urls: vec![],
        terminal_commands: None,
    }
}

// ============================================================================
// Main Health Check Function
// ============================================================================

/// Effectue une vérification complète de la santé système pour GPU/Vulkan
pub fn check_system_health() -> SystemHealthCheck {
    let os_info = detect_os_info();
    let gpu_backend = detect_active_backend();

    // Sur Windows/Linux avec gpu-vulkan, vérifier Vulkan
    #[cfg(all(
        feature = "gpu-vulkan",
        any(target_os = "windows", target_os = "linux")
    ))]
    let vulkan_available = is_vulkan_available();

    #[cfg(not(all(
        feature = "gpu-vulkan",
        any(target_os = "windows", target_os = "linux")
    )))]
    let vulkan_available = false;

    let vulkan_version = if vulkan_available {
        get_vulkan_version()
    } else {
        None
    };

    // Générer le guide d'installation si Vulkan non disponible
    let install_guide = if !vulkan_available && os_info.platform != "macos" {
        Some(generate_install_guide(&os_info))
    } else {
        None
    };

    tracing::info!(
        "System health check: platform={}, vulkan={}, backend={:?}",
        os_info.platform,
        vulkan_available,
        gpu_backend
    );

    SystemHealthCheck {
        vulkan_available,
        vulkan_version,
        gpu_backend,
        os_info,
        install_guide,
        can_run_without_vulkan: true, // Toujours true car on a le fallback CPU
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

    #[test]
    fn test_os_detection() {
        let os = detect_os_info();
        println!("OS Info: {:?}", os);
        assert!(!os.platform.is_empty());
    }

    #[test]
    fn test_system_health_check() {
        let health = check_system_health();
        println!("System Health: {:?}", health);
        assert!(health.can_run_without_vulkan);
    }

    #[test]
    fn test_install_guide_windows() {
        let guide = generate_windows_guide();
        assert!(!guide.download_urls.is_empty());
        assert!(guide.terminal_commands.is_none());
    }

    #[test]
    fn test_install_guide_linux_ubuntu() {
        let guide = generate_linux_guide(Some("ubuntu"));
        assert!(guide.terminal_commands.is_some());
        assert!(guide.download_urls.is_empty());
    }
}
