// 发布构建时隐藏 Windows 控制台窗口
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod answer_sheet_materialization;
mod answer_sheet_template_provider;
mod answer_sheet_template_run;
mod answer_source_provider;
mod answer_source_run;
mod archive;
mod asr;
mod audio;
mod backup;
mod commands;
mod dictation_materialization;
mod dictation_provider;
mod dictation_run;
mod exam_commands;
mod exam_intake;
mod objective_provider;
mod objective_run;
mod office_answers;
mod ordinary_paper_materialization;
mod ordinary_paper_provider;
mod ordinary_paper_run;
mod pdf_pages;
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
            commands::backups_list,
            commands::backup_create,
            commands::backup_restore,
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
            commands::recognition_failures_list,
            commands::recognition_relocate,
            commands::recognition_void,
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
            exam_commands::exam_answer_suggest,
            exam_commands::exam_answer_human_decide,
            exam_commands::exam_answers_list,
            exam_commands::exam_objective_workbench,
            exam_commands::exam_objective_accept,
            exam_commands::exam_objective_correct,
            exam_commands::exam_objective_strict_batch_accept,
            exam_commands::exam_objective_publish_attempt,
            exam_commands::exam_fixed_intake_options,
            exam_commands::exam_fixed_intake_infer_page_cycle,
            exam_commands::exam_fixed_intake_prepare,
            exam_commands::exam_fixed_intake_confirm_material_type,
            exam_commands::exam_fixed_intake_confirm_grouping,
            exam_commands::exam_fixed_intake_grouping_evidence,
            exam_commands::exam_fixed_intake_confirm_grouping_quality,
            exam_commands::exam_fixed_intake_replace_rejected_page,
            exam_commands::exam_answer_source_analyze,
            exam_commands::exam_answer_source_confirm_matches,
            exam_commands::exam_answer_source_keep_bound,
            exam_commands::exam_answer_source_adopt_new_version,
            exam_commands::exam_objective_recognize_region,
            exam_commands::exam_ordinary_paper_analyze_page,
            exam_commands::exam_ordinary_paper_confirm_page_structure,
            exam_commands::exam_answer_sheet_process_page,
            exam_commands::exam_answer_sheet_template_status,
            exam_commands::exam_answer_sheet_analyze_template,
            exam_commands::exam_answer_sheet_confirm_template,
            exam_commands::exam_dictation_template_status,
            exam_commands::exam_dictation_analyze_template,
            exam_commands::exam_dictation_confirm_template,
            exam_commands::exam_dictation_process_page,
            exam_commands::exam_dictation_recognize_region,
            exam_commands::exam_dictation_workbench,
            exam_commands::exam_dictation_correct_transcription,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
