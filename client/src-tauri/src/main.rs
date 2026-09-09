// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg_attr(target_os = "linux", allow(unsafe_code))]
fn main() {
    #[cfg(target_os = "linux")]
    // SAFETY: this runs before Tauri starts threads or any concurrent environment access.
    unsafe {
        std::env::set_var("__GL_THREADED_OPTIMIZATIONS", "0");
        std::env::set_var("__NV_DISABLE_EXPLICIT_SYNC", "1");
    }

    client_lib::run()
}
