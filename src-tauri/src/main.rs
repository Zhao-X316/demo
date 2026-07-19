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
mod diagnostics;
mod dictation_materialization;
mod dictation_provider;
mod dictation_run;
mod exam_commands;
mod exam_intake;
mod knowledge_answer_run;
mod knowledge_commands;
mod knowledge_link_provider;
mod knowledge_link_run;
mod knowledge_source_provider;
mod knowledge_source_run;
mod learning_commands;
mod objective_provider;
mod objective_run;
mod office_answers;
mod ordinary_paper_materialization;
mod ordinary_paper_provider;
mod ordinary_paper_run;
mod pdf_pages;
mod secrets;
mod short_answer_provider;
mod short_answer_run;
mod state;
mod subjective_run;
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
            commands::rubric_setup_preview,
            commands::rubric_setup_confirm,
            commands::seed_demo,
            commands::config_get,
            commands::config_set,
            commands::backups_list,
            commands::backup_create,
            commands::backup_restore,
            diagnostics::diagnostic_preview,
            diagnostics::diagnostic_export,
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
            commands::recitation_submission_detail,
            commands::import_stage,
            commands::import_autoname,
            commands::asr_and_score,
            commands::recognition_failures_list,
            commands::recognition_relocate,
            commands::recognition_void,
            commands::anomalies_list,
            commands::anomaly_reassign,
            commands::suggest_match,
            exam_commands::class_operations_dashboard,
            learning_commands::class_wrongbook_dashboard,
            learning_commands::confirm_wrongbook_error_causes,
            learning_commands::create_wrongbook_single_correction,
            learning_commands::wrongbook_schedule_policy,
            learning_commands::update_wrongbook_schedule_policy,
            learning_commands::preview_wrongbook_reinforcement,
            learning_commands::confirm_wrongbook_reinforcement,
            learning_commands::wrongbook_statistics,
            learning_commands::create_wrongbook_report_snapshot,
            learning_commands::write_wrongbook_report_snapshot,
            learning_commands::list_profile_scope_options,
            learning_commands::preview_student_profile,
            learning_commands::generate_student_profile,
            learning_commands::latest_student_profile,
            learning_commands::save_profile_teacher_assessment,
            learning_commands::create_student_profile_report_snapshot,
            learning_commands::write_student_profile_report_snapshot,
            learning_commands::preview_class_profile,
            learning_commands::generate_class_profile,
            learning_commands::latest_class_profile,
            learning_commands::create_class_profile_export_snapshot,
            learning_commands::write_class_profile_export_snapshot,
            learning_commands::preview_class_teaching_input,
            learning_commands::confirm_class_teaching_input,
            learning_commands::list_class_teaching_inputs,
            learning_commands::list_class_teaching_events,
            learning_commands::create_class_teaching_event,
            learning_commands::revise_class_teaching_event,
            learning_commands::void_class_teaching_event,
            learning_commands::preview_class_action,
            learning_commands::confirm_class_action,
            learning_commands::list_class_action_drafts,
            learning_commands::materialize_class_action,
            knowledge_commands::k1_blueprint_options,
            knowledge_commands::k1_blueprint_preview,
            knowledge_commands::k1_blueprint_confirm,
            knowledge_commands::k1_blueprint_list,
            knowledge_commands::k1_question_search,
            knowledge_commands::k1_duplicate_review,
            knowledge_commands::k1_question_performance,
            knowledge_commands::k1_question_impact_preview,
            knowledge_commands::k1_question_impact_confirm,
            knowledge_commands::k1_question_impact_cases,
            knowledge_commands::k1_question_impact_prepare,
            knowledge_commands::k1_candidate_review_list,
            knowledge_commands::k1_candidate_promote_l1,
            knowledge_commands::k1_candidate_discard,
            knowledge_commands::k1_source_import_analyze,
            knowledge_commands::k1_source_inbox,
            knowledge_commands::k1_source_accept,
            knowledge_commands::k1_source_discard,
            knowledge_commands::k1_answer_targets,
            knowledge_commands::k1_answer_import_analyze,
            knowledge_commands::k1_answer_inbox,
            knowledge_commands::k1_answer_confirm,
            knowledge_commands::k1_link_review_catalog,
            knowledge_commands::k1_link_review_inbox,
            knowledge_commands::k1_link_review_editor,
            knowledge_commands::k1_link_suggest,
            knowledge_commands::k1_link_confirm,
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
            exam_commands::exam_ordinary_paper_sync_questions,
            exam_commands::exam_answer_sheet_process_page,
            exam_commands::exam_answer_sheet_recognize_subjective_region,
            exam_commands::exam_answer_sheet_correct_subjective_transcription,
            exam_commands::exam_answer_sheet_grade_short_answer,
            exam_commands::exam_answer_sheet_subjective_workbench,
            exam_commands::exam_answer_sheet_subjective_accept,
            exam_commands::exam_answer_sheet_subjective_correct,
            exam_commands::exam_answer_sheet_subjective_correct_components,
            exam_commands::exam_answer_sheet_promote_accepted_answer,
            exam_commands::exam_answer_sheet_promote_rubric_evidence,
            exam_commands::exam_subjective_link_editor,
            exam_commands::exam_subjective_link_save,
            exam_commands::exam_answer_sheet_subjective_publish_attempt,
            exam_commands::exam_answer_sheet_template_status,
            exam_commands::exam_answer_sheet_analyze_template,
            exam_commands::exam_answer_sheet_confirm_template,
            exam_commands::exam_dictation_template_status,
            exam_commands::exam_dictation_analyze_template,
            exam_commands::exam_dictation_confirm_template,
            exam_commands::exam_dictation_process_page,
            exam_commands::exam_dictation_recognize_region,
            exam_commands::exam_dictation_workbench,
            exam_commands::exam_dictation_accept,
            exam_commands::exam_dictation_correct_grade,
            exam_commands::exam_dictation_strict_batch_accept,
            exam_commands::exam_dictation_publish_attempt,
            exam_commands::exam_dictation_correct_transcription,
        ])
        .run(tauri::generate_context!())
        .expect("启动 Tauri 应用失败");
}
