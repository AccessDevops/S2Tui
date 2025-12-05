fn main() {
    // Configure Windows icon for the executable
    #[cfg(target_os = "windows")]
    {
        embed_resource::compile("icons/app-icon.rc", embed_resource::NONE);
    }

    tauri_build::build()
}
