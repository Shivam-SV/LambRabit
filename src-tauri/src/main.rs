// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // WebKitGTK's DMA-BUF renderer aborts on systems without libGLESv2 (the AppImage can't
    // bundle GL libraries) and shows a blank window with some GPU drivers (NVIDIA, VMs).
    // The fallback renderer works everywhere; a value set by the user wins.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // SAFETY: nothing else is running yet, so no other thread reads the environment.
        unsafe { std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1") };
    }
    lamprabit_lib::run()
}
