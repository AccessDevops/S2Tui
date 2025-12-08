mod audio;
mod commands;
mod platform;
mod state;
mod whisper;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager,
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub use commands::*;
pub use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Check GPU availability on Windows/Linux BEFORE starting full Tauri app
    // If Vulkan is not available, launch only the warning window
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    {
        if !whisper::is_vulkan_available_at_startup() {
            tracing::warn!("Vulkan not available - launching warning window only");
            run_vulkan_warning_mode();
            return;
        }
        tracing::info!("Vulkan detected - starting full application");
    }

    run_full_app();
}

/// Run the full application with all features
fn run_full_app() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_os::init())
        .setup(|app| {
            // Initialize app state
            let state = AppState::new();
            app.manage(state);

            // Setup global shortcut
            setup_global_shortcut(app.handle())?;

            // Configure overlay window with platform-specific behavior
            if let Some(window) = app.get_webview_window("main") {
                tracing::info!("Main window found, configuring platform-specific settings");

                if let Err(e) = platform::get_platform().configure_overlay_window(&window) {
                    tracing::warn!("Failed to configure overlay window: {}", e);
                } else {
                    tracing::info!("Platform overlay configuration applied");
                }
            } else {
                tracing::error!("Main window NOT FOUND! This is a critical error.");
            }

            // Setup system tray
            setup_system_tray(app)?;

            tracing::info!("S2Tui initialized successfully");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::start_listen,
            commands::stop_listen,
            commands::set_model,
            commands::set_language,
            commands::set_shortcut,
            commands::load_whisper_model,
            commands::is_model_loaded,
            commands::check_permissions,
            commands::request_microphone_permission,
            commands::get_available_models,
            commands::get_gpu_info,
            commands::check_system_health,
            commands::get_gpu_status,
            commands::load_whisper_model_with_options,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            tracing::error!("Failed to run Tauri application: {}", e);
            std::process::exit(1);
        });
}

// Window configuration is now handled by the platform module

fn setup_global_shortcut(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

    // Try different shortcuts in order of preference
    let shortcuts = [
        "CommandOrControl+Shift+Space", // Primary: Cmd+Shift+Space
        "CommandOrControl+Alt+Space",   // Fallback 1
        "CommandOrControl+Shift+S",     // Fallback 2
    ];

    for shortcut_str in shortcuts {
        let shortcut: Shortcut = match shortcut_str.parse() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to parse shortcut {}: {}", shortcut_str, e);
                continue;
            }
        };

        // on_shortcut both registers the shortcut AND sets the handler
        match app
            .global_shortcut()
            .on_shortcut(shortcut, move |_app, _shortcut, event| {
                if event.state == ShortcutState::Pressed {
                    tracing::info!("Global shortcut triggered");
                    if let Err(e) = _app.emit("shortcut:triggered", ()) {
                        tracing::error!("Failed to emit shortcut event: {}", e);
                    }
                }
            }) {
            Ok(_) => {
                tracing::info!("Global shortcut registered: {}", shortcut_str);
                return Ok(());
            }
            Err(e) => {
                tracing::warn!("Failed to register {}: {}", shortcut_str, e);
                continue;
            }
        }
    }

    // No shortcut worked, but don't crash - just warn
    tracing::warn!("Could not register any global shortcut. App will work without hotkey.");
    Ok(())
}

fn setup_system_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // Create tray menu
    let show_item = MenuItem::with_id(app, "show", "Show S2Tui", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &settings_item, &quit_item])?;

    // Load tray icon from embedded bytes
    let icon_bytes = include_bytes!("../icons/32x32.png");
    let icon = match Image::from_bytes(icon_bytes) {
        Ok(img) => img,
        Err(e) => {
            tracing::error!(
                "Failed to load tray icon: {}. Continuing without system tray.",
                e
            );
            return Ok(());
        }
    };

    // Build and store the tray icon
    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("S2Tui - Speech to Text")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
            "settings" => {
                // Emit event to open settings
                let _ = app.emit("open:settings", ());
            }
            "quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    tracing::info!("System tray initialized");
    Ok(())
}

/// Run the application in Vulkan warning mode only.
/// This launches a minimal Tauri app with only the vulkan-warning window,
/// blocking further use until Vulkan is installed.
#[cfg(any(target_os = "windows", target_os = "linux"))]
fn run_vulkan_warning_mode() {
    use tauri::WebviewWindowBuilder;

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_os::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            // Create only the vulkan-warning window
            let window = WebviewWindowBuilder::new(
                app,
                "vulkan-warning",
                tauri::WebviewUrl::App("vulkan-warning.html".into()),
            )
            .title("S2Tui - Vulkan Required")
            .inner_size(520.0, 620.0)
            .min_inner_size(450.0, 500.0)
            .resizable(true)
            .center()
            .decorations(true)
            .build()?;

            // Exit app when window is closed
            let app_handle = app.handle().clone();
            window.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { .. } = event {
                    app_handle.exit(0);
                }
            });

            tracing::info!("Vulkan warning window launched");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![commands::check_system_health,])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            tracing::error!("Failed to run Vulkan warning mode: {}", e);
            std::process::exit(1);
        });
}
