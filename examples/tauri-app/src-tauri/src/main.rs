// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Log from trace level unless RUST_LOG overrides it
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    tauri_app_lib::run();
}
