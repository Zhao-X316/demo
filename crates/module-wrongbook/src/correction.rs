//! M3-2 单学生单题订正范围。
//!
//! 老师点击建立后，系统冻结一个 correction assessment 和目标学生；不预造空
//! attempt。后续照片归属确认时由 M2 创建 `attempt_kind=correction`。

use module_exam::service::assessment::{
    create_confirmed_single_item_correction_in_transaction, NewSingleItemCorrectionAssessment,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::domain::{ids, time};
use suite_core::error::{CoreError, CoreResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrectionAssignment {
    pub public_id: String,
    pub class_id: i64,
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub question_version_public_id: String,
    pub source_grade_decision_public_id: String,
    pub source_publication_public_id: String,
    pub assessment_public_id: String,
    pub assessment_version_public_id: String,
    pub assessment_title: String,
    pub status: String,
    pub latest_attempt_public_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct CreateCorrectionInput<'a> {
    pub class_id: i64,
    pub student_id: i64,
    pub question_version_public_id: &'a str,
    pub source_grade_decision_public_id: &'a str,
    pub source_publication_public_id: &'a str,
    pub created_by: &'a str,
}

struct SourceScope {
    student_no: String,
    student_name: String,
    stem: String,
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    score: f64,
    option_order_json: Option<String>,
    presentation_snapshot_json: String,
    grade_decision_id: i64,
    publication_id: i64,
}

fn compact_title(student_no: &str, student_name: &str, stem: &str) -> String {
    let compact_stem = stem.chars().take(24).collect::<String>();
    let suffix = if stem.chars().count() > 24 { "…" } else { "" };
    format!("{student_no}号 {student_name} · {compact_stem}{suffix} · 订正")
}

fn resolve_source(conn: &Connection, input: &CreateCorrectionInput<'_>) -> CoreResult<SourceScope> {
    let scope = conn
        .query_row(
            "SELECT student.student_no,student.name,question.stem,question.id,
                    source_item.answer_key_version_id,source_item.rubric_version_id,
                    source_item.link_set_id,source_item.score,source_item.option_order_json,
                    source_item.presentation_snapshot_json,decision.id,publication.id
             FROM exam_grade_decisions_v2 decision
             JOIN exam_attempts_v2 attempt
               ON attempt.id=decision.attempt_id
             JOIN students student
               ON student.id=attempt.student_id
              AND student.id=?2
              AND student.class_id=?1
              AND student.enabled=1
             JOIN exam_grade_publications_v2 publication
               ON publication.id=attempt.active_publication_id
              AND publication.public_id=?5
              AND publication.state='published'
             JOIN exam_grade_publication_items_v2 publication_item
               ON publication_item.publication_id=publication.id
              AND publication_item.attempt_id=attempt.id
             JOIN exam_grade_publication_decisions_v2 publication_decision
               ON publication_decision.publication_item_id=publication_item.id
              AND publication_decision.grade_decision_id=decision.id
             JOIN exam_assessment_items_v2 source_item
               ON source_item.id=decision.assessment_item_id
             JOIN k1_question_versions question
               ON question.id=source_item.question_version_id
              AND question.public_id=?3
             JOIN exam_assessment_versions_v2 source_version
               ON source_version.id=attempt.assessment_version_id
             JOIN exam_assessments_v2 source_assessment
               ON source_assessment.id=source_version.assessment_id
              AND source_assessment.class_id=?1
             WHERE decision.public_id=?4
               AND decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
               AND decision.teacher_score < source_item.score - 0.000001",
            (
                input.class_id,
                input.student_id,
                input.question_version_public_id,
                input.source_grade_decision_public_id,
                input.source_publication_public_id,
            ),
            |row| {
                Ok(SourceScope {
                    student_no: row.get(0)?,
                    student_name: row.get(1)?,
                    stem: row.get(2)?,
                    question_version_id: row.get(3)?,
                    answer_key_version_id: row.get(4)?,
                    rubric_version_id: row.get(5)?,
                    link_set_id: row.get(6)?,
                    score: row.get(7)?,
                    option_order_json: row.get(8)?,
                    presentation_snapshot_json: row.get(9)?,
                    grade_decision_id: row.get(10)?,
                    publication_id: row.get(11)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("该评分不是当前可订正的已发布非满分事实".into()))?;

    let latest_response_decision_id: i64 = conn.query_row(
        "SELECT decision.id
         FROM exam_grade_decisions_v2 decision
         JOIN exam_attempts_v2 attempt
           ON attempt.id=decision.attempt_id
          AND attempt.student_id=?1
         JOIN exam_grade_publications_v2 publication
           ON publication.id=attempt.active_publication_id
          AND publication.state='published'
         JOIN exam_grade_publication_items_v2 publication_item
           ON publication_item.publication_id=publication.id
          AND publication_item.attempt_id=attempt.id
         JOIN exam_grade_publication_decisions_v2 publication_decision
           ON publication_decision.publication_item_id=publication_item.id
          AND publication_decision.grade_decision_id=decision.id
         JOIN exam_assessment_items_v2 item
           ON item.id=decision.assessment_item_id
         JOIN k1_question_versions question
           ON question.id=item.question_version_id
          AND question.public_id=?2
         JOIN exam_assessment_versions_v2 version
           ON version.id=attempt.assessment_version_id
         JOIN exam_assessments_v2 assessment
           ON assessment.id=version.assessment_id
          AND assessment.class_id=?3
         WHERE decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
         ORDER BY publication.published_at DESC,decision.id DESC
         LIMIT 1",
        (
            input.student_id,
            input.question_version_public_id,
            input.class_id,
        ),
        |row| row.get(0),
    )?;
    if latest_response_decision_id != scope.grade_decision_id {
        return Err(CoreError::Invalid(
            "该错误已不是当前最新发布作答，请刷新后重试".into(),
        ));
    }
    Ok(scope)
}

fn load_by_source_decision(
    conn: &Connection,
    source_grade_decision_public_id: &str,
) -> CoreResult<Option<CorrectionAssignment>> {
    let row = conn
        .query_row(
            "SELECT assignment.public_id,assignment.class_id,assignment.student_id,
                    student.student_no,student.name,question.public_id,
                    source_decision.public_id,source_publication.public_id,
                    assessment.public_id,version.public_id,assessment.title,
                    assignment.created_by,assignment.created_at,version.id
             FROM wb_correction_assignments assignment
             JOIN students student ON student.id=assignment.student_id
             JOIN k1_question_versions question ON question.id=assignment.question_version_id
             JOIN exam_grade_decisions_v2 source_decision
               ON source_decision.id=assignment.source_grade_decision_id
             JOIN exam_grade_publications_v2 source_publication
               ON source_publication.id=assignment.source_publication_id
             JOIN exam_assessments_v2 assessment ON assessment.id=assignment.assessment_id
             JOIN exam_assessment_versions_v2 version
               ON version.id=assignment.assessment_version_id
             WHERE source_decision.public_id=?1",
            [source_grade_decision_public_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, i64>(13)?,
                ))
            },
        )
        .optional()?;
    let Some((
        public_id,
        class_id,
        student_id,
        student_no,
        student_name,
        question_version_public_id,
        source_grade_decision_public_id,
        source_publication_public_id,
        assessment_public_id,
        assessment_version_public_id,
        assessment_title,
        created_by,
        created_at,
        assessment_version_id,
    )) = row
    else {
        return Ok(None);
    };
    let attempt = conn
        .query_row(
            "SELECT public_id,state FROM exam_attempts_v2
             WHERE assessment_version_id=?1 AND student_id=?2 AND attempt_kind='correction'
               AND state<>'voided'
             ORDER BY attempt_no DESC,id DESC LIMIT 1",
            (assessment_version_id, student_id),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let (status, latest_attempt_public_id) = match attempt {
        None => ("waiting_upload".to_string(), None),
        Some((public_id, state)) => {
            let status = match state.as_str() {
                "ready_to_publish" => "ready_to_publish",
                "published" => "published",
                _ => "in_progress",
            };
            (status.to_string(), Some(public_id))
        }
    };
    Ok(Some(CorrectionAssignment {
        public_id,
        class_id,
        student_id,
        student_no,
        student_name,
        question_version_public_id,
        source_grade_decision_public_id,
        source_publication_public_id,
        assessment_public_id,
        assessment_version_public_id,
        assessment_title,
        status,
        latest_attempt_public_id,
        created_by,
        created_at,
    }))
}

pub fn load_for_source_decision(
    conn: &Connection,
    source_grade_decision_public_id: &str,
) -> CoreResult<Option<CorrectionAssignment>> {
    load_by_source_decision(conn, source_grade_decision_public_id)
}

fn ensure_existing_matches(
    existing: &CorrectionAssignment,
    input: &CreateCorrectionInput<'_>,
) -> CoreResult<()> {
    if existing.class_id != input.class_id
        || existing.student_id != input.student_id
        || existing.question_version_public_id != input.question_version_public_id
        || existing.source_publication_public_id != input.source_publication_public_id
    {
        return Err(CoreError::Invalid(
            "该订正已存在，但请求范围与原冻结范围不一致".into(),
        ));
    }
    Ok(())
}

pub fn create_single_correction(
    conn: &mut Connection,
    input: &CreateCorrectionInput<'_>,
) -> CoreResult<CorrectionAssignment> {
    if input.created_by.trim().is_empty() {
        return Err(CoreError::Invalid("订正创建人不能为空".into()));
    }
    if let Some(existing) = load_by_source_decision(conn, input.source_grade_decision_public_id)? {
        ensure_existing_matches(&existing, input)?;
        return Ok(existing);
    }

    let transaction = conn.transaction()?;
    if let Some(existing) =
        load_by_source_decision(&transaction, input.source_grade_decision_public_id)?
    {
        ensure_existing_matches(&existing, input)?;
        transaction.commit()?;
        return Ok(existing);
    }
    let scope = resolve_source(&transaction, input)?;
    let title = compact_title(&scope.student_no, &scope.student_name, &scope.stem);
    let assessment = create_confirmed_single_item_correction_in_transaction(
        &transaction,
        &NewSingleItemCorrectionAssessment {
            title: &title,
            class_id: input.class_id,
            target_student_id: input.student_id,
            created_by: input.created_by,
            question_version_id: scope.question_version_id,
            answer_key_version_id: scope.answer_key_version_id,
            rubric_version_id: scope.rubric_version_id,
            link_set_id: scope.link_set_id,
            score: scope.score,
            option_order_json: scope.option_order_json.as_deref(),
            presentation_snapshot_json: &scope.presentation_snapshot_json,
        },
    )?;
    let assignment_public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    transaction.execute(
        "INSERT INTO wb_correction_assignments
         (public_id,class_id,student_id,question_version_id,source_grade_decision_id,
          source_publication_id,assessment_id,assessment_version_id,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        (
            &assignment_public_id,
            input.class_id,
            input.student_id,
            scope.question_version_id,
            scope.grade_decision_id,
            scope.publication_id,
            assessment.assessment_id,
            assessment.assessment_version_id,
            input.created_by.trim(),
            &now,
        ),
    )?;
    let result = load_by_source_decision(&transaction, input.source_grade_decision_public_id)?
        .ok_or_else(|| CoreError::NotFound("刚创建的订正范围".into()))?;
    transaction.commit()?;
    Ok(result)
}
