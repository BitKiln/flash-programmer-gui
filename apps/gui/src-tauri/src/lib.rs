pub mod commands;
pub mod state;

use state::AppState;

/// Runs the Tauri application.
///
/// This is the main entry point called from `main.rs`. It configures
/// the Tauri builder with plugins, managed state, and IPC command handlers.
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_probes,
            commands::connect_probe,
            commands::load_firmware,
            commands::flash_firmware,
            commands::erase_chip,
            commands::verify_firmware,
            commands::reset_target,
            commands::get_flash_events,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
