// 发布构建时隐藏 Windows 控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod asr;
mod audio;
mod commands;
mod exam_commands;
mod secrets;
mod state;
mod vlm;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let st = state::init(app)?;
            app.manage(st);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::dashboard_today,
            commands::verdict_human_decide,
            commands::students_list,
            commands::contents_list,
            commands::seed_demo,
            commands::config_get,
            commands::config_set,
            commands::secrets_get,
            commands::secrets_set,
            commands::students_upsert,
            commands::contents_upsert,
            commands::tasks_generate,
            commands::import_paths,
            commands::import_autoname,
            commands::asr_and_score,
            commands::anomalies_list,
            commands::anomaly_reassign,
            exam_commands::kp_list,
            exam_commands::kp_create,
            exam_commands::kp_rename,
            exam_commands::kp_delete,
            exam_commands::questions_list,
            exam_commands::question_get,
            exam_commands::question_create,
            exam_commands::question_set_options,
            exam_commands::question_delete,
            exam_commands::question_vlm_analyze,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
