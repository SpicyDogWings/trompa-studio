mod db;
mod preferences;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(db::DbState::default())
        .invoke_handler(tauri::generate_handler![
            db::connect,
            db::disconnect,
            db::run_query,
            db::list_schema,
            preferences::read_preferences,
            preferences::write_preferences,
            db::start_query,
            db::cancel_query,
        ])
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
