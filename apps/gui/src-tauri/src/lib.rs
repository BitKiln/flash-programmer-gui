pub mod commands;
pub mod state;

use state::AppState;

/// Runs the Tauri application.
///
/// This is the main entry point called from `main.rs`. It configures
/// the Tauri builder with plugins, managed state, and IPC command handlers.
pub fn run() {
    tauri::Builder::default()
        // A second launch must not start a second programmer: two instances
        // would fight over the same USB debug probe. Focus the existing window.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            use tauri::Manager;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::list_probes,
            commands::connect_probe,
            commands::disconnect_probe,
            commands::auto_detect_target,
            commands::load_firmware,
            commands::flash_firmware,
            commands::start_batch,
            commands::erase_chip,
            commands::verify_firmware,
            commands::reset_target,
            commands::cancel_operation,
            commands::read_memory,
            commands::read_firmware_window,
            commands::save_memory_region,
            commands::list_profiles,
            commands::load_profile,
            commands::save_profile,
            commands::delete_profile,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
