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
            // 启动即跑一次日切：没交→补背、到期→复习 自动上看板。失败只记录、不阻断启动。
            if let Ok(conn) = st.db.lock() {
                match commands::run_day_rollover(&conn) {
                    Ok((rolled, reviews)) => {
                        eprintln!("[启动日切] 结转补背 {rolled} 条，到期复习 {reviews} 条");
                    }
                    Err(err) => eprintln!("[启动日切] 跳过：{err}"),
                }
            }
            app.manage(st);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::dashboard_today,
            commands::day_rollover,
            commands::verdict_human_decide,
            commands::students_list,
            commands::contents_list,
            commands::seed_demo,
            commands::config_get,
            commands::config_set,
            commands::secrets_get,
            commands::secrets_set,
            commands::students_upsert,
            commands::students_import,
            commands::students_set_enabled,
            commands::students_delete,
            commands::classes_list,
            commands::class_create,
            commands::class_update,
            commands::class_delete,
            commands::students_set_class,
            commands::contents_upsert,
            commands::contents_import,
            commands::contents_set_enabled,
            commands::contents_delete,
            commands::parse_syllabus,
            commands::tasks_generate,
            commands::task_cancel,
            commands::tasks_reassign,
            commands::tasks_remove,
            commands::import_history,
            commands::import_stage,
            commands::import_autoname,
            commands::asr_and_score,
            commands::anomalies_list,
            commands::anomaly_reassign,
            commands::suggest_match,
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
