//! Read-only composition. Domain tables remain the sole owners of task facts.
use crate::state::AppState;
use module_exam::service::{dictation_pipeline, objective, subjective};
use rusqlite::Connection;
use serde::Serialize;
use suite_core::error::CoreResult;
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceTask {
    pub kind: String,
    pub source_id: i64,
    pub title: String,
    pub class_id: Option<i64>,
    pub class_name: Option<String>,
    pub student_name: Option<String>,
    pub status: String,
    pub updated_at: String,
    pub assessment_version_id: Option<i64>,
    pub attempt_ids: Vec<i64>,
    pub has_evidence: bool,
}

pub(crate) fn batch_attempt_ids(conn: &Connection, batch_id: i64) -> CoreResult<Vec<i64>> {
    let mut stmt = conn.prepare("SELECT DISTINCT m.attempt_id FROM exam_ingest_pages_v2 p
        JOIN exam_page_match_revisions_v2 m ON m.page_id=p.id AND m.state='active' AND m.decision='teacher_confirmed'
        JOIN exam_attempts_v2 a ON a.id=m.attempt_id AND a.state<>'voided'
        WHERE p.batch_id=?1 AND p.state<>'voided' ORDER BY m.attempt_id")?;
    let rows = stmt
        .query_map([batch_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub(crate) fn read_tasks(conn: &Connection) -> CoreResult<Vec<WorkspaceTask>> {
    let mut tasks = Vec::new();
    let mut recitations = conn.prepare(
        "SELECT s.id,COALESCE(r.title,'待匹配录音'),st.class_id,c.name,st.name,
        CASE WHEN s.recognize_status='failed' THEN 'recognition_failed'
             WHEN s.recognize_status='processing' THEN 'processing'
             WHEN s.status='confirmed' AND v.human_result IN ('pass','fail') THEN 'confirmed'
             WHEN s.status='confirmed' THEN 'needs_review' ELSE s.status END,
        s.updated_at,(s.task_id IS NOT NULL AND s.student_id IS NOT NULL AND s.ref_id IS NOT NULL)
        FROM submissions s LEFT JOIN students st ON st.id=s.student_id
        LEFT JOIN classes c ON c.id=st.class_id LEFT JOIN rec_contents r ON r.id=s.ref_id
        LEFT JOIN verdicts v ON v.id=(SELECT MAX(id) FROM verdicts WHERE submission_id=s.id)
        WHERE s.module='recitation' AND s.status<>'voided'",
    )?;
    for row in recitations.query_map([], |row| {
        Ok(WorkspaceTask {
            kind: "recitation".into(),
            source_id: row.get(0)?,
            title: row.get(1)?,
            class_id: row.get(2)?,
            class_name: row.get(3)?,
            student_name: row.get(4)?,
            status: row.get(5)?,
            updated_at: row.get(6)?,
            has_evidence: row.get(7)?,
            assessment_version_id: None,
            attempt_ids: Vec::new(),
        })
    })? {
        tasks.push(row?);
    }

    let mut batches = conn.prepare("SELECT b.id,a.title,a.class_id,c.name,b.assessment_version_id,b.updated_at,
        COALESCE(g.student_group_count,0),COALESCE(x.mapped_group_count,0),COALESCE(x.rejected_group_count,0),
        EXISTS(SELECT 1 FROM exam_fixed_preflight_revisions_v2 f WHERE f.ingest_batch_id=b.id AND f.state='active')
        FROM exam_ingest_batches_v2 b JOIN exam_assessment_versions_v2 v ON v.id=b.assessment_version_id
        JOIN exam_assessments_v2 a ON a.id=v.assessment_id LEFT JOIN classes c ON c.id=a.class_id
        LEFT JOIN exam_ordered_grouping_revisions_v2 g ON g.ingest_batch_id=b.id AND g.state='active'
        LEFT JOIN exam_ordered_grouping_activations_v2 x ON x.ingest_batch_id=b.id AND x.state='active'
        WHERE b.state<>'voided'")?;
    let rows = batches.query_map([], |row| {
        Ok((
            WorkspaceTask {
                kind: "exam_batch".into(),
                source_id: row.get(0)?,
                title: row.get(1)?,
                class_id: row.get(2)?,
                class_name: row.get(3)?,
                assessment_version_id: Some(row.get(4)?),
                updated_at: row.get(5)?,
                student_name: None,
                status: String::new(),
                attempt_ids: Vec::new(),
                has_evidence: false,
            },
            row.get::<_, i64>(6)?,
            row.get::<_, i64>(7)?,
            row.get::<_, i64>(8)?,
            row.get::<_, bool>(9)?,
        ))
    })?;
    for row in rows {
        let (mut task, groups, mapped, rejected, prepared) = row?;
        task.attempt_ids = batch_attempt_ids(conn, task.source_id)?;
        let mut states = Vec::new();
        for id in &task.attempt_ids {
            states.push(conn.query_row(
                "SELECT state FROM exam_attempts_v2 WHERE id=?1",
                [id],
                |row| row.get::<_, String>(0),
            )?);
        }
        let scope = serde_json::to_string(&task.attempt_ids)
            .map_err(|error| suite_core::error::CoreError::Parse(error.to_string()))?;
        task.has_evidence = conn.query_row("SELECT
          EXISTS(SELECT 1 FROM exam_objective_grade_suggestions_v2 WHERE attempt_id IN (SELECT value FROM json_each(?1))) OR
          EXISTS(SELECT 1 FROM exam_subjective_grade_suggestions_v2 WHERE attempt_id IN (SELECT value FROM json_each(?1))) OR
          EXISTS(SELECT 1 FROM exam_dictation_transcription_revisions_v2 WHERE state='active' AND attempt_id IN (SELECT value FROM json_each(?1)))",
          [&scope],|row|row.get(0))?;
        let processing = crate::exam_intake::resume::read_history(
            conn,
            task.source_id,
            task.assessment_version_id.unwrap_or(0),
        )?
        .iter()
        .any(|run| run.status == "processing");
        // Older/manual batches have page matches but no ordered-intake ledger. Require every
        // live page to have a current teacher-confirmed match before trusting their final states.
        // Any ordered-intake history (even superseded) keeps the newer grouping checks in force.
        let complete_materials = (groups > 0 && mapped == groups && rejected == 0)
            || conn.query_row("SELECT
                NOT EXISTS(SELECT 1 FROM exam_import_order_revisions_v2 WHERE ingest_batch_id=?1) AND
                NOT EXISTS(SELECT 1 FROM exam_material_type_revisions_v2 WHERE ingest_batch_id=?1) AND
                NOT EXISTS(SELECT 1 FROM exam_ordered_grouping_revisions_v2 WHERE ingest_batch_id=?1) AND
                EXISTS(SELECT 1 FROM exam_ingest_pages_v2 WHERE batch_id=?1 AND state<>'voided') AND
                NOT EXISTS(SELECT 1 FROM exam_ingest_pages_v2 p WHERE p.batch_id=?1 AND p.state<>'voided'
                    AND NOT EXISTS(SELECT 1 FROM exam_page_match_revisions_v2 m
                        JOIN exam_attempts_v2 a ON a.id=m.attempt_id
                        WHERE m.page_id=p.id AND m.state='active' AND m.decision='teacher_confirmed'
                            AND a.state<>'voided' AND a.assessment_version_id=?2))",
                (task.source_id, task.assessment_version_id), |row| row.get::<_, bool>(0))?;
        task.status = if complete_materials
            && !states.is_empty()
            && states.iter().all(|s| s == "published")
        {
            "published"
        } else if complete_materials
            && !states.is_empty()
            && states
                .iter()
                .all(|s| s == "published" || s == "ready_to_publish")
        {
            "ready_to_publish"
        } else if !prepared && !task.has_evidence {
            "incomplete"
        } else if processing {
            "processing"
        } else if task.has_evidence {
            "needs_review"
        } else {
            "needs_material"
        }
        .into();
        tasks.push(task);
    }
    // Attempts created by older/manual flows also remain reachable; do not duplicate batch members.
    let mut attempts = conn.prepare(
        "SELECT t.id,a.title,a.class_id,c.name,st.name,t.state,t.updated_at,t.assessment_version_id
        FROM exam_attempts_v2 t JOIN exam_assessment_versions_v2 v ON v.id=t.assessment_version_id
        JOIN exam_assessments_v2 a ON a.id=v.assessment_id JOIN students st ON st.id=t.student_id
        LEFT JOIN classes c ON c.id=a.class_id WHERE t.state<>'voided' AND NOT EXISTS (
          SELECT 1 FROM exam_page_match_revisions_v2 m JOIN exam_ingest_pages_v2 p ON p.id=m.page_id
          JOIN exam_ingest_batches_v2 b ON b.id=p.batch_id
          WHERE m.attempt_id=t.id AND m.state='active' AND m.decision='teacher_confirmed'
            AND p.state<>'voided' AND b.state<>'voided')",
    )?;
    for row in attempts.query_map([], |row| {
        let id = row.get(0)?;
        Ok(WorkspaceTask {
            kind: "exam_attempt".into(),
            source_id: id,
            title: row.get(1)?,
            class_id: row.get(2)?,
            class_name: row.get(3)?,
            student_name: row.get(4)?,
            status: row.get(5)?,
            updated_at: row.get(6)?,
            assessment_version_id: Some(row.get(7)?),
            attempt_ids: vec![id],
            has_evidence: true,
        })
    })? {
        tasks.push(row?);
    }
    tasks.sort_by(|a, b| {
        b.updated_at
            .cmp(&a.updated_at)
            .then_with(|| b.source_id.cmp(&a.source_id))
    });
    Ok(tasks)
}

#[tauri::command]
pub fn workspace_tasks(state: State<'_, AppState>) -> Result<Vec<WorkspaceTask>, String> {
    let conn = state.db.lock().map_err(|error| error.to_string())?;
    read_tasks(&conn).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn exam_fixed_intake_resume(
    state: State<'_, AppState>,
    batch_id: i64,
) -> Result<crate::exam_intake::resume::IntakeResume, String> {
    let conn = state.db.lock().map_err(|error| error.to_string())?;
    crate::exam_intake::resume::read(&conn, batch_id).map_err(|error| error.to_string())
}

#[derive(Serialize)]
pub(crate) struct ExamReviewSnapshot {
    task: WorkspaceTask,
    objective: objective::ObjectiveWorkbench,
    subjective: subjective::SubjectiveWorkbench,
    dictation: dictation_pipeline::DictationWorkbench,
}

pub(crate) fn read_exam_review(
    conn: &Connection,
    kind: &str,
    source_id: i64,
) -> CoreResult<ExamReviewSnapshot> {
    if !matches!(kind, "exam_batch" | "exam_attempt") || source_id <= 0 {
        return Err(suite_core::error::CoreError::Invalid(
            "批改任务范围无效".into(),
        ));
    }
    let task = read_tasks(conn)?
        .into_iter()
        .find(|task| task.kind == kind && task.source_id == source_id)
        .ok_or_else(|| suite_core::error::CoreError::NotFound("本次批改任务".into()))?;
    let ids = Some(task.attempt_ids.as_slice());
    Ok(ExamReviewSnapshot {
        objective: objective::list_objective_workbench_scoped(
            conn,
            task.assessment_version_id,
            1000,
            ids,
        )?,
        subjective: subjective::list_subjective_workbench_scoped(
            conn,
            task.assessment_version_id,
            2000,
            ids,
        )?,
        dictation: dictation_pipeline::list_dictation_workbench_scoped(
            conn,
            task.assessment_version_id,
            2000,
            ids,
        )?,
        task,
    })
}

#[tauri::command]
pub(crate) fn workspace_exam_review(
    state: State<'_, AppState>,
    kind: String,
    source_id: i64,
) -> Result<ExamReviewSnapshot, String> {
    let conn = state.db.lock().map_err(|error| error.to_string())?;
    read_exam_review(&conn, &kind, source_id).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::run_all_migrations;

    #[test]
    fn workspace_reads_existing_submissions_without_writing_or_hiding_failures() {
        let conn = suite_core::db::open_in_memory().unwrap();
        run_all_migrations(&conn).unwrap();
        conn.execute_batch("INSERT INTO classes(name) VALUES ('一班');
            INSERT INTO students(student_no,name,class_id) VALUES ('S1','学生一',1);
            INSERT INTO submissions(module,student_id,media_type,file_path,file_hash,status,recognize_status)
            VALUES ('recitation',1,'audio','/synthetic.wav','hash1','pending','failed'),
                   ('recitation',NULL,'audio','/unmatched.wav','hash2','anomaly','ok'),
                   ('recitation',1,'audio','/voided.wav','hash3','voided','ok');").unwrap();
        let changes = || {
            conn.query_row("SELECT total_changes()", [], |row| row.get::<_, i64>(0))
                .unwrap()
        };
        let before = changes();
        conn.pragma_update(None, "query_only", true).unwrap();
        let tasks = read_tasks(&conn).unwrap();
        assert_eq!(tasks.len(), 2);
        let failed = tasks.iter().find(|task| task.source_id == 1).unwrap();
        assert_eq!(failed.class_id, Some(1));
        assert_eq!(failed.status, "recognition_failed");
        assert!(!failed.has_evidence);
        assert_eq!(
            tasks
                .iter()
                .find(|task| task.source_id == 2)
                .unwrap()
                .class_id,
            None
        );
        assert_eq!(changes(), before);
    }
}
