mod audio;
mod commands;
mod events;
mod permissions;
mod state;
mod whisper;

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, WebviewWindow,
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

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .setup(|app| {
            // Initialize app state
            let state = AppState::new();
            app.manage(state);

            // Setup global shortcut
            setup_global_shortcut(app.handle())?;

            // Configure window as non-focusable on macOS
            if let Some(window) = app.get_webview_window("main") {
                configure_non_focusable_window(&window);
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Configure window to not steal focus when clicked (macOS only)
#[cfg(target_os = "macos")]
fn configure_non_focusable_window(window: &WebviewWindow) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;

    // Get the NSWindow handle
    if let Ok(ns_window) = window.ns_window() {
        let ns_window = ns_window as *mut AnyObject;
        unsafe {
            // Set the window to be a non-activating panel
            // NSWindowCollectionBehaviorCanJoinAllSpaces | NSWindowCollectionBehaviorStationary | NSWindowCollectionBehaviorIgnoresCycle
            let behavior: u64 = (1 << 0) | (1 << 4) | (1 << 6);
            let _: () = msg_send![ns_window, setCollectionBehavior: behavior];
        }
    }
    tracing::info!("Window configured as non-focusable");
}

#[cfg(not(target_os = "macos"))]
fn configure_non_focusable_window(_window: &WebviewWindow) {
    // Non-macOS platforms: no special configuration needed
}

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
    let icon = Image::from_bytes(icon_bytes)
        .expect("Failed to load tray icon");

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
