fn main() {
    // Configure Windows icon for the executable
    #[cfg(windows)]
    {
        let mut res = tauri_winres::WindowsResource::new();
        res.set_icon("icons/icon.ico");
        if let Err(e) = res.compile() {
            eprintln!("Warning: Failed to compile Windows resources: {}", e);
        }
    }

    tauri_build::build()
}
