//! M2-B0 作业版本、attempt、追加式评分和显式发布事务。
//!
//! 这里只建立固定 K1 版本引用和审计骨架；页面、题区、OCR/OMR 留给 B1/B2。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::audit::NewAuditEvent;
use suite_core::db::repo::learning_evidence::NewLearningEvidence;
use suite_core::db::repo::outbox::NewOutboxEvent;
use suite_core::db::repo::{audit, learning_evidence, outbox};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{
    AssessmentContext, AuditActorType, ConfirmationLevel, EvidenceKind, EvidenceSourceModule,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssessmentDraft {
    pub assessment_id: i64,
    pub assessment_public_id: String,
    pub assessment_version_id: i64,
    pub assessment_version_public_id: String,
}

pub struct NewAssessmentDraft<'a> {
    pub title: &'a str,
    pub class_id: i64,
    pub assessment_context: &'a str,
    pub evidence_policy: &'a str,
    pub created_by: &'a str,
    pub template_version: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssessmentItem {
    pub id: i64,
    pub public_id: String,
    pub assessment_version_id: i64,
    pub question_version_id: i64,
    pub answer_key_version_id: i64,
    pub rubric_version_id: i64,
    pub link_set_id: i64,
    pub order_index: i64,
    pub score: f64,
    pub state: String,
}

pub struct NewAssessmentItem<'a> {
    pub question_version_id: i64,
    pub answer_key_version_id: i64,
    pub rubric_version_id: i64,
    pub link_set_id: i64,
    pub order_index: i64,
    pub score: f64,
    pub option_order_json: Option<&'a str>,
    pub presentation_snapshot_json: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub id: i64,
    pub public_id: String,
    pub assessment_version_id: i64,
    pub student_id: i64,
    pub attempt_no: i64,
    pub source_kind: String,
    pub attempt_kind: String,
    pub state: String,
    pub active_publication_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GradeDecision {
    pub id: i64,
    pub public_id: String,
    pub attempt_id: i64,
    pub assessment_item_id: i64,
    pub revision: i64,
    pub machine_grade_ai_run_id: Option<i64>,
    pub teacher_score: f64,
    pub point_results_json: String,
    pub teacher_note: Option<String>,
    pub confirmation_level: String,
    pub state: String,
    pub decided_by: String,
    pub decided_at: String,
}

pub struct NewGradeDecision<'a> {
    pub attempt_id: i64,
    pub assessment_item_id: i64,
    pub machine_grade_ai_run_id: Option<i64>,
    pub teacher_score: f64,
    pub point_results_json: &'a str,
    pub teacher_note: Option<&'a str>,
    pub confirmation_level: &'a str,
    pub decided_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Publication {
    pub id: i64,
    pub public_id: String,
    pub assessment_version_id: i64,
    pub revision: i64,
    pub state: String,
    pub attempt_id: i64,
    pub total_score: f64,
    pub grade_decision_set_hash: String,
    pub published_by: String,
    pub published_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LegacyAnswerCompat {
    pub legacy_answer_id: i64,
    pub source_kind: String,
    pub mapping_state: String,
    pub attempt_id: Option<i64>,
    pub assessment_item_id: Option<i64>,
    pub student_id: i64,
    pub question_id: i64,
    pub picked: Option<String>,
    pub score: Option<f64>,
    pub status: String,
    pub created_at: String,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Invalid(format!("{label}不能为空")));
    }
    Ok(())
}

fn validate_schema_object(json: &str, label: &str) -> CoreResult<()> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| CoreError::Invalid(format!("{label} JSON 无效：{error}")))?;
    if !value.is_object()
        || value
            .get("schema_version")
            .and_then(Value::as_i64)
            .is_none()
    {
        return Err(CoreError::Invalid(format!(
            "{label} 必须是带整数 schema_version 的对象"
        )));
    }
    Ok(())
}

fn quality_rank(value: &str) -> Option<i64> {
    match value {
        "C0" => Some(0),
        "L0" => Some(1),
        "L1" => Some(2),
        "L2" => Some(3),
        "L3" => Some(4),
        "L4" => Some(5),
        _ => None,
    }
}

pub fn create_assessment_draft(
    conn: &Connection,
    input: &NewAssessmentDraft<'_>,
) -> CoreResult<AssessmentDraft> {
    required(input.title, "作业名称")?;
    required(input.created_by, "创建人")?;
    if !matches!(
        input.assessment_context,
        "classwork" | "homework" | "quiz" | "exam" | "open_book" | "correction" | "demo"
    ) {
        return Err(CoreError::Invalid("作业场景非法".into()));
    }
    if !matches!(
        input.evidence_policy,
        "include" | "exclude" | "include_low_weight" | "progress_only"
    ) {
        return Err(CoreError::Invalid("证据策略非法".into()));
    }
    let class_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM classes WHERE id=?1)",
        [input.class_id],
        |row| row.get(0),
    )?;
    if !class_exists {
        return Err(CoreError::NotFound(format!("class#{}", input.class_id)));
    }

    let assessment_public_id = ids::new_public_id();
    let assessment_version_public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO exam_assessments_v2
         (public_id, title, class_id, assessment_context, evidence_policy, state,
          created_by, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,'draft',?6,?7,?7)",
        (
            &assessment_public_id,
            input.title.trim(),
            input.class_id,
            input.assessment_context,
            input.evidence_policy,
            input.created_by.trim(),
            &now,
        ),
    )?;
    let assessment_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO exam_assessment_versions_v2
         (public_id, assessment_id, revision, template_version, state, created_at)
         VALUES (?1,?2,1,?3,'draft',?4)",
        (
            &assessment_version_public_id,
            assessment_id,
            input.template_version,
            &now,
        ),
    )?;
    let assessment_version_id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(AssessmentDraft {
        assessment_id,
        assessment_public_id,
        assessment_version_id,
        assessment_version_public_id,
    })
}

fn get_item(conn: &Connection, id: i64) -> CoreResult<Option<AssessmentItem>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, assessment_version_id, question_version_id,
                    answer_key_version_id, rubric_version_id, link_set_id,
                    order_index, score, state
             FROM exam_assessment_items_v2 WHERE id=?1",
            [id],
            |row| {
                Ok(AssessmentItem {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    assessment_version_id: row.get(2)?,
                    question_version_id: row.get(3)?,
                    answer_key_version_id: row.get(4)?,
                    rubric_version_id: row.get(5)?,
                    link_set_id: row.get(6)?,
                    order_index: row.get(7)?,
                    score: row.get(8)?,
                    state: row.get(9)?,
                })
            },
        )
        .optional()?)
}

pub fn add_assessment_item(
    conn: &Connection,
    assessment_version_id: i64,
    input: &NewAssessmentItem<'_>,
) -> CoreResult<AssessmentItem> {
    validate_schema_object(input.presentation_snapshot_json, "题目呈现快照")?;
    if let Some(json) = input.option_order_json {
        validate_schema_object(json, "选项顺序")?;
    }
    if input.order_index < 0 || !input.score.is_finite() || input.score <= 0.0 {
        return Err(CoreError::Invalid("题目顺序/分值非法".into()));
    }
    let draft_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_assessment_versions_v2
         WHERE id=?1 AND state='draft')",
        [assessment_version_id],
        |row| row.get(0),
    )?;
    if !draft_exists {
        return Err(CoreError::Invalid("只能向 draft 作业版本添加题目".into()));
    }
    let refs: Option<(String, String, f64, String, f64, String)> = conn
        .query_row(
            "SELECT q.quality_level, q.state, q.max_score,
                    a.state, r.max_score, r.state
             FROM k1_question_versions q
             JOIN k1_answer_key_versions a ON a.id=?2 AND a.question_version_id=q.id
             JOIN k1_rubric_versions r ON r.id=?3 AND r.question_version_id=q.id
             JOIN k1_link_sets l ON l.id=?4 AND l.question_version_id=q.id
             WHERE q.id=?1",
            (
                input.question_version_id,
                input.answer_key_version_id,
                input.rubric_version_id,
                input.link_set_id,
            ),
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()?;
    let Some((quality, question_state, question_score, answer_state, rubric_score, rubric_state)) =
        refs
    else {
        return Err(CoreError::Invalid("K1 固定版本引用不完整或互不匹配".into()));
    };
    if quality_rank(&quality).unwrap_or(-1) < quality_rank("L2").unwrap()
        || question_state != "published"
        || answer_state != "confirmed"
        || rubric_state != "confirmed"
    {
        return Err(CoreError::Invalid(
            "自动批改作业只能引用已发布 L2+ 题目及已确认答案/rubric".into(),
        ));
    }
    if (input.score - question_score).abs() > 0.000_001
        || (input.score - rubric_score).abs() > 0.000_001
    {
        return Err(CoreError::Invalid(
            "B0 暂不做分值缩放，作业分值必须等于题目/rubric 总分".into(),
        ));
    }

    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO exam_assessment_items_v2
         (public_id, assessment_version_id, question_version_id, answer_key_version_id,
          rubric_version_id, link_set_id, order_index, score, option_order_json,
          presentation_snapshot_json, state, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11)",
        (
            &public_id,
            assessment_version_id,
            input.question_version_id,
            input.answer_key_version_id,
            input.rubric_version_id,
            input.link_set_id,
            input.order_index,
            input.score,
            input.option_order_json,
            input.presentation_snapshot_json,
            &now,
        ),
    )?;
    get_item(conn, conn.last_insert_rowid())?
        .ok_or_else(|| CoreError::NotFound("刚创建的 assessment item".into()))
}

#[derive(Serialize)]
struct ItemHashInput {
    item_id: i64,
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    order_index: i64,
    score_millis: i64,
}

pub fn confirm_assessment_version(
    conn: &Connection,
    assessment_version_id: i64,
    confirmed_by: &str,
) -> CoreResult<String> {
    required(confirmed_by, "作业版本确认人")?;
    let state: Option<String> = conn
        .query_row(
            "SELECT state FROM exam_assessment_versions_v2 WHERE id=?1",
            [assessment_version_id],
            |row| row.get(0),
        )
        .optional()?;
    match state.as_deref() {
        Some("draft") => {}
        Some("confirmed") => {
            return conn
                .query_row(
                    "SELECT item_set_hash FROM exam_assessment_versions_v2 WHERE id=?1",
                    [assessment_version_id],
                    |row| row.get(0),
                )
                .map_err(Into::into);
        }
        Some(_) => return Err(CoreError::Invalid("已停用作业版本不能确认".into())),
        None => {
            return Err(CoreError::NotFound(format!(
                "assessment_version#{assessment_version_id}"
            )))
        }
    }
    let mut stmt = conn.prepare(
        "SELECT id, question_version_id, answer_key_version_id, rubric_version_id,
                link_set_id, order_index, score
         FROM exam_assessment_items_v2
         WHERE assessment_version_id=?1 AND state='active'
         ORDER BY order_index, id",
    )?;
    let rows = stmt.query_map([assessment_version_id], |row| {
        Ok(ItemHashInput {
            item_id: row.get(0)?,
            question_version_id: row.get(1)?,
            answer_key_version_id: row.get(2)?,
            rubric_version_id: row.get(3)?,
            link_set_id: row.get(4)?,
            order_index: row.get(5)?,
            score_millis: (row.get::<_, f64>(6)? * 1000.0).round() as i64,
        })
    })?;
    let items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if items.is_empty() {
        return Err(CoreError::Invalid("作业版本至少需要一道有效题目".into()));
    }
    let bytes = serde_json::to_vec(&items)
        .map_err(|error| CoreError::Parse(format!("item set hash 序列化失败：{error}")))?;
    let hash = hashing::sha256_hex(&bytes);
    let now = time::utc_now_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE exam_assessment_versions_v2
         SET item_set_hash=?1, state='confirmed', confirmed_by=?2, confirmed_at=?3
         WHERE id=?4 AND state='draft'",
        (&hash, confirmed_by.trim(), &now, assessment_version_id),
    )?;
    tx.execute(
        "UPDATE exam_assessments_v2 SET state='active', updated_at=?1
         WHERE id=(SELECT assessment_id FROM exam_assessment_versions_v2 WHERE id=?2)",
        (&now, assessment_version_id),
    )?;
    tx.commit()?;
    Ok(hash)
}

fn get_attempt(conn: &Connection, id: i64) -> CoreResult<Option<Attempt>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, assessment_version_id, student_id, attempt_no,
                    source_kind, attempt_kind, state, active_publication_id
             FROM exam_attempts_v2 WHERE id=?1",
            [id],
            |row| {
                Ok(Attempt {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    assessment_version_id: row.get(2)?,
                    student_id: row.get(3)?,
                    attempt_no: row.get(4)?,
                    source_kind: row.get(5)?,
                    attempt_kind: row.get(6)?,
                    state: row.get(7)?,
                    active_publication_id: row.get(8)?,
                })
            },
        )
        .optional()?)
}

pub fn create_attempt(
    conn: &Connection,
    assessment_version_id: i64,
    student_id: i64,
    source_kind: &str,
    attempt_kind: &str,
) -> CoreResult<Attempt> {
    if !matches!(source_kind, "manual" | "image") {
        return Err(CoreError::Invalid("attempt source_kind 非法".into()));
    }
    if !matches!(attempt_kind, "first" | "correction" | "retry") {
        return Err(CoreError::Invalid("attempt kind 非法".into()));
    }
    let eligible: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM exam_assessment_versions_v2 v
           JOIN exam_assessments_v2 a ON a.id=v.assessment_id
           JOIN students s ON s.id=?2 AND s.enabled=1 AND s.class_id=a.class_id
           WHERE v.id=?1 AND v.state='confirmed'
         )",
        (assessment_version_id, student_id),
        |row| row.get(0),
    )?;
    if !eligible {
        return Err(CoreError::Invalid(
            "attempt 需要已确认作业版本和该班启用学生".into(),
        ));
    }
    let attempt_no: i64 = conn.query_row(
        "SELECT COALESCE(MAX(attempt_no), 0) + 1 FROM exam_attempts_v2
         WHERE assessment_version_id=?1 AND student_id=?2",
        (assessment_version_id, student_id),
        |row| row.get(0),
    )?;
    let now = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO exam_attempts_v2
         (public_id, assessment_version_id, student_id, attempt_no, source_kind,
          attempt_kind, state, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?8)",
        (
            ids::new_public_id(),
            assessment_version_id,
            student_id,
            attempt_no,
            source_kind,
            attempt_kind,
            if source_kind == "image" {
                "ingesting"
            } else {
                "grading"
            },
            &now,
        ),
    )?;
    get_attempt(conn, conn.last_insert_rowid())?
        .ok_or_else(|| CoreError::NotFound("刚创建的 attempt".into()))
}

fn decision_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GradeDecision> {
    Ok(GradeDecision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        attempt_id: row.get(2)?,
        assessment_item_id: row.get(3)?,
        revision: row.get(4)?,
        machine_grade_ai_run_id: row.get(5)?,
        teacher_score: row.get(6)?,
        point_results_json: row.get(7)?,
        teacher_note: row.get(8)?,
        confirmation_level: row.get(9)?,
        state: row.get(10)?,
        decided_by: row.get(11)?,
        decided_at: row.get(12)?,
    })
}

fn get_decision(conn: &Connection, id: i64) -> CoreResult<Option<GradeDecision>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, attempt_id, assessment_item_id, revision,
                    machine_grade_ai_run_id, teacher_score, point_results_json, teacher_note,
                    confirmation_level, state, decided_by, decided_at
             FROM exam_grade_decisions_v2 WHERE id=?1",
            [id],
            decision_row,
        )
        .optional()?)
}

fn normalized_note(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|note| !note.is_empty())
}

pub(crate) fn decide_grade_in_transaction(
    conn: &Connection,
    input: &NewGradeDecision<'_>,
) -> CoreResult<GradeDecision> {
    validate_schema_object(input.point_results_json, "评分点结果")?;
    required(input.decided_by, "评分确认人")?;
    if !matches!(
        input.confirmation_level,
        "teacher_accepted" | "teacher_corrected"
    ) {
        return Err(CoreError::Invalid("评分确认级别非法".into()));
    }
    if !input.teacher_score.is_finite() || input.teacher_score < 0.0 {
        return Err(CoreError::Invalid("老师得分非法".into()));
    }
    let scope: Option<(String, i64, f64)> = conn
        .query_row(
            "SELECT at.state, at.assessment_version_id, i.score
             FROM exam_attempts_v2 at
             JOIN exam_assessment_items_v2 i
               ON i.id=?2 AND i.assessment_version_id=at.assessment_version_id
             WHERE at.id=?1 AND at.state<>'voided' AND i.state='active'",
            (input.attempt_id, input.assessment_item_id),
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((_attempt_state, assessment_version_id, max_score)) = scope else {
        return Err(CoreError::Invalid(
            "attempt 与 assessment item 不属于同一作业版本".into(),
        ));
    };
    if input.teacher_score > max_score + 0.000_001 {
        return Err(CoreError::Invalid("老师得分不能超过题目分值".into()));
    }
    if let Some(ai_run_id) = input.machine_grade_ai_run_id {
        let valid_run: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM ai_runs
             WHERE id=?1 AND run_type='answer_grade' AND status='succeeded')",
            [ai_run_id],
            |row| row.get(0),
        )?;
        if !valid_run {
            return Err(CoreError::Invalid("机器评分 run 尚未成功或类型不符".into()));
        }
    }
    let note = normalized_note(input.teacher_note);
    let current: Option<GradeDecision> = conn
        .query_row(
            "SELECT id, public_id, attempt_id, assessment_item_id, revision,
                    machine_grade_ai_run_id, teacher_score, point_results_json, teacher_note,
                    confirmation_level, state, decided_by, decided_at
             FROM exam_grade_decisions_v2
             WHERE attempt_id=?1 AND assessment_item_id=?2 AND state='active'",
            (input.attempt_id, input.assessment_item_id),
            decision_row,
        )
        .optional()?;
    if let Some(active) = current.as_ref() {
        if (active.teacher_score - input.teacher_score).abs() <= 0.000_001
            && active.machine_grade_ai_run_id == input.machine_grade_ai_run_id
            && active.point_results_json == input.point_results_json
            && active.teacher_note.as_deref() == note
            && active.confirmation_level == input.confirmation_level
        {
            return Ok(active.clone());
        }
    }
    let revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision), 0) + 1 FROM exam_grade_decisions_v2
         WHERE attempt_id=?1 AND assessment_item_id=?2",
        (input.attempt_id, input.assessment_item_id),
        |row| row.get(0),
    )?;
    if let Some(active) = current.as_ref() {
        suite_core::db::repo::learning_evidence::revert_for_decision(
            conn,
            "grade_decision",
            &active.public_id,
            active.revision,
        )?;
    }
    conn.execute(
        "UPDATE exam_grade_decisions_v2 SET state='superseded'
         WHERE attempt_id=?1 AND assessment_item_id=?2 AND state='active'",
        (input.attempt_id, input.assessment_item_id),
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO exam_grade_decisions_v2
         (public_id, attempt_id, assessment_item_id, revision, machine_grade_ai_run_id,
          teacher_score, point_results_json, teacher_note, confirmation_level, state,
          decided_by, decided_at, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',?10,?11,?11)",
        (
            &public_id,
            input.attempt_id,
            input.assessment_item_id,
            revision,
            input.machine_grade_ai_run_id,
            input.teacher_score,
            input.point_results_json,
            note,
            input.confirmation_level,
            input.decided_by.trim(),
            &now,
        ),
    )?;
    let decision_id = conn.last_insert_rowid();
    let (item_count, decision_count): (i64, i64) = conn.query_row(
        "SELECT
           (SELECT COUNT(*) FROM exam_assessment_items_v2
            WHERE assessment_version_id=?1 AND state='active'),
           (SELECT COUNT(*) FROM exam_grade_decisions_v2 d
            JOIN exam_assessment_items_v2 i ON i.id=d.assessment_item_id
            WHERE d.attempt_id=?2 AND d.state='active'
              AND i.assessment_version_id=?1 AND i.state='active')",
        (assessment_version_id, input.attempt_id),
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    conn.execute(
        "UPDATE exam_attempts_v2 SET state=?1, updated_at=?2 WHERE id=?3",
        (
            if item_count > 0 && item_count == decision_count {
                "ready_to_publish"
            } else {
                "grading"
            },
            &now,
            input.attempt_id,
        ),
    )?;
    get_decision(conn, decision_id)?
        .ok_or_else(|| CoreError::NotFound("刚创建的 grade decision".into()))
}

pub fn decide_grade(conn: &Connection, input: &NewGradeDecision<'_>) -> CoreResult<GradeDecision> {
    let tx = conn.unchecked_transaction()?;
    let decision = decide_grade_in_transaction(&tx, input)?;
    tx.commit()?;
    Ok(decision)
}

#[derive(Serialize)]
struct DecisionHashInput {
    decision_id: i64,
    assessment_item_id: i64,
    revision: i64,
    score_millis: i64,
}

#[derive(Debug)]
struct EvidenceDecision {
    id: i64,
    public_id: String,
    revision: i64,
    teacher_score: f64,
    confirmation_level: String,
    decided_at: String,
    item_public_id: String,
    item_score: f64,
    link_set_id: i64,
    dictation_rubric_point_public_id: Option<String>,
    question_type: String,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    subjective_source: bool,
    short_answer_result_json: Option<String>,
}

#[derive(Debug)]
struct EvidenceSource {
    source_type: &'static str,
    source_ref_type: &'static str,
    source_ref_id: String,
    link_source_type: &'static str,
    link_source_public_id: String,
    rule_version: &'static str,
    value: f64,
}

fn evidence_sources(
    conn: &Connection,
    decision: &EvidenceDecision,
) -> CoreResult<Vec<EvidenceSource>> {
    if let Some(point_public_id) = decision.dictation_rubric_point_public_id.as_ref() {
        return Ok(vec![EvidenceSource {
            source_type: "dictation_rubric_point",
            source_ref_type: "rubric_point",
            source_ref_id: point_public_id.clone(),
            link_source_type: "rubric_point",
            link_source_public_id: point_public_id.clone(),
            rule_version: "dictation-grading-v1",
            value: (decision.teacher_score / decision.item_score).clamp(0.0, 1.0),
        }]);
    }
    if decision.subjective_source {
        let component_rows: Vec<(String, String, f64, f64)> = {
            let mut stmt = conn.prepare(
                "SELECT source_type,source_public_id,teacher_score,max_score
                 FROM exam_grade_decision_subjective_components_v2
                 WHERE grade_decision_id=?1
                 ORDER BY order_index,id",
            )?;
            let rows = stmt.query_map([decision.id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        if !component_rows.is_empty() {
            let (expected_type, expected_count): (&str, i64) =
                if decision.question_type == "fill_blank" {
                    (
                        "answer_slot",
                        conn.query_row(
                            "SELECT COUNT(*) FROM k1_answer_slots
                             WHERE answer_key_version_id=?1",
                            [decision.answer_key_version_id],
                            |row| row.get(0),
                        )?,
                    )
                } else if decision.question_type == "short_answer" {
                    (
                        "rubric_point",
                        conn.query_row(
                            "SELECT COUNT(*) FROM k1_rubric_points
                             WHERE rubric_version_id=?1",
                            [decision.rubric_version_id],
                            |row| row.get(0),
                        )?,
                    )
                } else {
                    return Err(CoreError::Invalid(
                        "逐项主观题结论绑定了不支持的题型".into(),
                    ));
                };
            if component_rows.len() as i64 != expected_count
                || component_rows
                    .iter()
                    .any(|component| component.0 != expected_type)
            {
                return Err(CoreError::Invalid(
                    "逐项主观题结论没有完整覆盖当前槽位或评分点".into(),
                ));
            }
            let score_total: f64 = component_rows.iter().map(|component| component.2).sum();
            let max_total: f64 = component_rows.iter().map(|component| component.3).sum();
            if (score_total - decision.teacher_score).abs() > 0.000_001
                || (max_total - decision.item_score).abs() > 0.000_001
            {
                return Err(CoreError::Invalid(
                    "逐项主观题结论与老师总分或题目满分不一致".into(),
                ));
            }
            return component_rows
                .into_iter()
                .map(
                    |(source_type, source_public_id, teacher_score, max_score)| {
                        let (source_type, source_ref_type, link_source_type, rule_version) =
                            match source_type.as_str() {
                                "answer_slot" => (
                                    "fill_blank_slot",
                                    "answer_slot",
                                    "answer_slot",
                                    "fill-blank-teacher-components-v1",
                                ),
                                "rubric_point" => (
                                    "question_rubric_point",
                                    "rubric_point",
                                    "rubric_point",
                                    "short-answer-teacher-components-v1",
                                ),
                                _ => {
                                    return Err(CoreError::Invalid(
                                        "逐项主观题来源类型非法".into(),
                                    ));
                                }
                            };
                        Ok(EvidenceSource {
                            source_type,
                            source_ref_type,
                            source_ref_id: source_public_id.clone(),
                            link_source_type,
                            link_source_public_id: source_public_id,
                            rule_version,
                            value: (teacher_score / max_score).clamp(0.0, 1.0),
                        })
                    },
                )
                .collect();
        }
        if decision.question_type == "fill_blank" {
            let mut stmt = conn.prepare(
                "SELECT public_id FROM k1_answer_slots
                 WHERE answer_key_version_id=?1 ORDER BY order_index,id",
            )?;
            let slots = stmt
                .query_map([decision.answer_key_version_id], |row| {
                    row.get::<_, String>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if slots.len() != 1 {
                return Ok(Vec::new());
            }
            return Ok(vec![EvidenceSource {
                source_type: "fill_blank_slot",
                source_ref_type: "answer_slot",
                source_ref_id: slots[0].clone(),
                link_source_type: "answer_slot",
                link_source_public_id: slots[0].clone(),
                rule_version: "fill-blank-grading-v1",
                value: (decision.teacher_score / decision.item_score).clamp(0.0, 1.0),
            }]);
        }
        if decision.question_type == "short_answer"
            && decision.confirmation_level == "teacher_accepted"
        {
            let Some(result_json) = decision.short_answer_result_json.as_deref() else {
                return Ok(Vec::new());
            };
            let value: Value = serde_json::from_str(result_json)
                .map_err(|error| CoreError::Parse(format!("简答题逐点评分结果损坏：{error}")))?;
            let points = value
                .get("point_results")
                .and_then(Value::as_array)
                .ok_or_else(|| CoreError::Invalid("简答题逐点评分结果缺少 point_results".into()))?;
            let mut sources = Vec::with_capacity(points.len());
            let mut seen = std::collections::BTreeSet::new();
            for point in points {
                let point_id = point
                    .get("rubric_point_id")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| {
                        CoreError::Invalid("简答题评分点结果缺少 rubric_point_id".into())
                    })?;
                let suggested_score = point
                    .get("suggested_score")
                    .and_then(Value::as_f64)
                    .ok_or_else(|| {
                        CoreError::Invalid("简答题评分点结果缺少 suggested_score".into())
                    })?;
                if !seen.insert(point_id) {
                    return Err(CoreError::Invalid("简答题评分点结果重复".into()));
                }
                let (public_id, max_score): (String, f64) = conn.query_row(
                    "SELECT public_id,max_score FROM k1_rubric_points
                     WHERE id=?1 AND rubric_version_id=?2",
                    (point_id, decision.rubric_version_id),
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?;
                if !suggested_score.is_finite()
                    || suggested_score < 0.0
                    || suggested_score > max_score + 0.000_001
                {
                    return Err(CoreError::Invalid("简答题评分点得分越界".into()));
                }
                sources.push(EvidenceSource {
                    source_type: "question_rubric_point",
                    source_ref_type: "rubric_point",
                    source_ref_id: public_id.clone(),
                    link_source_type: "rubric_point",
                    link_source_public_id: public_id,
                    rule_version: "short-answer-rubric-grading-v1",
                    value: (suggested_score / max_score).clamp(0.0, 1.0),
                });
            }
            return Ok(sources);
        }
        // 老师只改了简答题总分时没有逐点人工结论；宁可不产出逐点证据，也不猜。
        return Ok(Vec::new());
    }
    Ok(vec![EvidenceSource {
        source_type: "objective_question",
        source_ref_type: "assessment_item",
        source_ref_id: decision.item_public_id.clone(),
        link_source_type: "question",
        link_source_public_id: String::new(),
        rule_version: "objective-grading-v1",
        value: (decision.teacher_score / decision.item_score).clamp(0.0, 1.0),
    }])
}

fn evidence_context(value: &str) -> AssessmentContext {
    match value {
        "homework" => AssessmentContext::Homework,
        "open_book" => AssessmentContext::OpenBook,
        "correction" => AssessmentContext::Correction,
        "classwork" | "demo" => AssessmentContext::InClass,
        "quiz" | "exam" => AssessmentContext::ClosedBook,
        _ => AssessmentContext::InClass,
    }
}

fn evidence_confirmation(value: &str) -> CoreResult<ConfirmationLevel> {
    match value {
        "teacher_accepted" => Ok(ConfirmationLevel::TeacherAccepted),
        "teacher_corrected" => Ok(ConfirmationLevel::TeacherCorrected),
        _ => Err(CoreError::Invalid("评分 revision 不是老师确认结果".into())),
    }
}

struct EvidenceTarget<'a> {
    target_kind: &'a str,
    target_public_id: &'a str,
    knowledge_node_id: Option<&'a str>,
    ability_dimension_id: Option<&'a str>,
    quality: f64,
    knowledge_map_version: &'a str,
    source: &'a EvidenceSource,
}

struct EvidenceContext<'a> {
    student_id: i64,
    source_module: EvidenceSourceModule,
    assessment_context: AssessmentContext,
    decision: &'a EvidenceDecision,
    published_by: &'a str,
}

fn activate_evidence_target(
    conn: &Connection,
    context: &EvidenceContext<'_>,
    target: &EvidenceTarget<'_>,
) -> CoreResult<()> {
    let decision = context.decision;
    let idempotency_key = if target.source.source_type == "objective_question"
        || target.source.source_type == "dictation_rubric_point"
    {
        format!(
            "exam:evidence:decision:{}:{}:{}",
            decision.public_id, target.target_kind, target.target_public_id
        )
    } else {
        format!(
            "exam:evidence:decision:{}:{}:{}:{}:{}",
            decision.public_id,
            target.source.source_ref_type,
            target.source.source_ref_id,
            target.target_kind,
            target.target_public_id
        )
    };
    let existed = learning_evidence::get_by_idempotency_key(conn, &idempotency_key)?.is_some();
    let evidence = learning_evidence::create_or_get(
        conn,
        &NewLearningEvidence {
            idempotency_key: &idempotency_key,
            student_id: context.student_id,
            source_module: context.source_module,
            source_type: target.source.source_type,
            source_ref_type: target.source.source_ref_type,
            source_ref_id: &target.source.source_ref_id,
            source_revision: decision.revision,
            decision_ref_type: Some("grade_decision"),
            decision_ref_id: Some(&decision.public_id),
            decision_revision: Some(decision.revision),
            knowledge_node_id: target.knowledge_node_id,
            ability_dimension_id: target.ability_dimension_id,
            evidence_kind: EvidenceKind::Accuracy,
            value: target.source.value,
            confirmation_level: evidence_confirmation(&decision.confirmation_level)?,
            evidence_quality: target.quality.clamp(0.0, 1.0),
            assessment_context: context.assessment_context,
            occurred_at: &decision.decided_at,
            rule_version: target.source.rule_version,
            knowledge_map_version: target.knowledge_map_version,
        },
    )?;
    if existed {
        return Ok(());
    }
    let payload = serde_json::json!({
        "schema_version": 1,
        "evidence_public_id": evidence.public_id,
        "student_id": context.student_id,
        "decision_public_id": decision.public_id,
        "decision_revision": decision.revision
    })
    .to_string();
    outbox::create_event(
        conn,
        &NewOutboxEvent {
            idempotency_key: &format!("exam:outbox:evidence:{}", evidence.public_id),
            event_type: "learning_evidence_changed",
            event_version: 1,
            aggregate_type: "learning_evidence",
            aggregate_id: &evidence.public_id,
            aggregate_revision: decision.revision,
            payload_json: &payload,
            occurred_at: &decision.decided_at,
        },
    )?;
    audit::append(
        conn,
        &NewAuditEvent {
            idempotency_key: &format!("exam:audit:evidence:{}", evidence.public_id),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(context.published_by),
            action: "exam.learning_evidence.activated",
            object_type: "learning_evidence",
            object_id: &evidence.public_id,
            object_revision: Some(decision.revision),
            note: Some(match target.source.source_type {
                "dictation_rubric_point" => "老师显式发布后激活默写评分点正式学习证据",
                "fill_blank_slot" => "老师逐槽确认或接受单槽建议并发布后激活填空正式学习证据",
                "question_rubric_point" => {
                    "老师接受或逐点修正评分并发布后激活简答评分点正式学习证据"
                }
                _ => "老师显式发布后激活客观题正式学习证据",
            }),
            meta_json: None,
            occurred_at: &decision.decided_at,
        },
    )?;
    Ok(())
}

fn activate_publication_evidence(
    conn: &Connection,
    attempt_id: i64,
    published_by: &str,
) -> CoreResult<()> {
    let scope: (i64, String, String, String, String) = conn.query_row(
        "SELECT at.student_id,at.attempt_kind,a.assessment_context,a.evidence_policy,
                at.public_id
         FROM exam_attempts_v2 at
         JOIN exam_assessment_versions_v2 v ON v.id=at.assessment_version_id
         JOIN exam_assessments_v2 a ON a.id=v.assessment_id
         WHERE at.id=?1",
        [attempt_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )?;
    let (student_id, attempt_kind, context_raw, evidence_policy, _attempt_public_id) = scope;
    if evidence_policy == "exclude"
        || (evidence_policy == "progress_only" && attempt_kind != "correction")
    {
        return Ok(());
    }
    let policy_weight = match evidence_policy.as_str() {
        "include" => 1.0,
        "include_low_weight" => 0.5,
        "progress_only" => 0.6,
        _ => return Err(CoreError::Invalid("作业证据策略非法".into())),
    };
    let source_module = if attempt_kind == "correction" {
        EvidenceSourceModule::Correction
    } else {
        EvidenceSourceModule::Grading
    };
    let assessment_context = evidence_context(&context_raw);
    let mut stmt = conn.prepare(
        "SELECT d.id,d.public_id,d.revision,d.teacher_score,d.confirmation_level,d.decided_at,
                i.public_id,i.score,i.link_set_id,rp.public_id,q.question_type,
                i.answer_key_version_id,i.rubric_version_id,
                CASE WHEN subjective.grade_decision_id IS NULL THEN 0 ELSE 1 END,
                analysis.result_json
         FROM exam_grade_decisions_v2 d
         JOIN exam_assessment_items_v2 i ON i.id=d.assessment_item_id
         JOIN k1_question_versions q ON q.id=i.question_version_id
         LEFT JOIN exam_grade_decision_dictation_sources_v2 ds
           ON ds.grade_decision_id=d.id
         LEFT JOIN exam_dictation_point_observations_v2 obs
           ON obs.id=ds.point_observation_id
         LEFT JOIN k1_rubric_points rp ON rp.id=obs.rubric_point_id
         LEFT JOIN exam_grade_decision_subjective_sources_v2 subjective
           ON subjective.grade_decision_id=d.id
         LEFT JOIN exam_grade_decision_short_answer_sources_v2 short_source
           ON short_source.grade_decision_id=d.id
         LEFT JOIN exam_short_answer_grade_analyses_v2 analysis
           ON analysis.id=short_source.analysis_id
         WHERE d.attempt_id=?1 AND d.state='active' AND i.state='active'
         ORDER BY i.order_index,d.id",
    )?;
    let rows = stmt.query_map([attempt_id], |row| {
        Ok(EvidenceDecision {
            id: row.get(0)?,
            public_id: row.get(1)?,
            revision: row.get(2)?,
            teacher_score: row.get(3)?,
            confirmation_level: row.get(4)?,
            decided_at: row.get(5)?,
            item_public_id: row.get(6)?,
            item_score: row.get(7)?,
            link_set_id: row.get(8)?,
            dictation_rubric_point_public_id: row.get(9)?,
            question_type: row.get(10)?,
            answer_key_version_id: row.get(11)?,
            rubric_version_id: row.get(12)?,
            subjective_source: row.get::<_, i64>(13)? != 0,
            short_answer_result_json: row.get(14)?,
        })
    })?;
    let decisions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    for decision in &decisions {
        let map: (String, i64) = conn.query_row(
            "SELECT m.public_id,m.revision
             FROM k1_link_sets l JOIN k1_knowledge_maps m ON m.id=l.knowledge_map_id
             WHERE l.id=?1 AND l.state='confirmed' AND m.state='confirmed'",
            [decision.link_set_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let knowledge_map_version = format!("{}:r{}", map.0, map.1);
        for source in evidence_sources(conn, decision)? {
            let context = EvidenceContext {
                student_id,
                source_module,
                assessment_context,
                decision,
                published_by: published_by.trim(),
            };
            let source_public_id = if source.link_source_type == "question" {
                None
            } else {
                Some(source.link_source_public_id.as_str())
            };
            let mut knowledge_stmt = conn.prepare(
                "SELECT DISTINCT n.public_id
                 FROM k1_knowledge_links l
                 JOIN k1_knowledge_nodes n ON n.id=l.knowledge_node_id AND n.state='active'
                 WHERE l.link_set_id=?1 AND l.source_type=?2
                   AND ((?3 IS NULL AND l.relation_type='direct_assessment')
                     OR (?3 IS NOT NULL AND l.source_public_id=?3
                         AND l.relation_type IN ('direct_assessment','answer_basis','rubric_basis')))
                   AND l.confirmation_level='teacher_confirmed'
                 ORDER BY n.public_id",
            )?;
            let knowledge_nodes = knowledge_stmt
                .query_map(
                    (
                        decision.link_set_id,
                        source.link_source_type,
                        source_public_id,
                    ),
                    |row| row.get::<_, String>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(knowledge_stmt);
            for node in &knowledge_nodes {
                activate_evidence_target(
                    conn,
                    &context,
                    &EvidenceTarget {
                        target_kind: "knowledge",
                        target_public_id: node,
                        knowledge_node_id: Some(node),
                        ability_dimension_id: None,
                        quality: policy_weight,
                        knowledge_map_version: &knowledge_map_version,
                        source: &source,
                    },
                )?;
            }
            let mut ability_stmt = conn.prepare(
                "SELECT DISTINCT d.public_id,l.evidence_strength
                 FROM k1_ability_links l
                 JOIN k1_ability_dimensions d
                   ON d.id=l.ability_dimension_id AND d.state='active'
                 WHERE l.link_set_id=?1 AND l.source_type=?2
                   AND (?3 IS NULL OR l.source_public_id=?3)
                   AND l.confirmation_level='teacher_confirmed'
                 ORDER BY d.public_id",
            )?;
            let ability_nodes = ability_stmt
                .query_map(
                    (
                        decision.link_set_id,
                        source.link_source_type,
                        source_public_id,
                    ),
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?)),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(ability_stmt);
            for (node, strength) in &ability_nodes {
                activate_evidence_target(
                    conn,
                    &context,
                    &EvidenceTarget {
                        target_kind: "ability",
                        target_public_id: node,
                        knowledge_node_id: None,
                        ability_dimension_id: Some(node),
                        quality: policy_weight * strength,
                        knowledge_map_version: &knowledge_map_version,
                        source: &source,
                    },
                )?;
            }
        }
    }
    Ok(())
}

pub fn publish_attempt(
    conn: &Connection,
    attempt_id: i64,
    published_by: &str,
) -> CoreResult<Publication> {
    required(published_by, "发布人")?;
    let tx = conn.unchecked_transaction()?;
    let attempt: Option<(i64, String, Option<i64>)> = tx
        .query_row(
            "SELECT assessment_version_id, state, active_publication_id
             FROM exam_attempts_v2 WHERE id=?1",
            [attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((assessment_version_id, state, old_publication_id)) = attempt else {
        return Err(CoreError::NotFound(format!("attempt#{attempt_id}")));
    };
    if state != "ready_to_publish" {
        return Err(CoreError::Invalid(
            "只有所有题目已终审且有新 revision 的 attempt 才能发布".into(),
        ));
    }
    let mut stmt = tx.prepare(
        "SELECT d.id, d.assessment_item_id, d.revision, d.teacher_score
         FROM exam_grade_decisions_v2 d
         JOIN exam_assessment_items_v2 i ON i.id=d.assessment_item_id
         WHERE d.attempt_id=?1 AND d.state='active'
           AND i.assessment_version_id=?2 AND i.state='active'
         ORDER BY i.order_index, d.id",
    )?;
    let rows = stmt.query_map((attempt_id, assessment_version_id), |row| {
        Ok(DecisionHashInput {
            decision_id: row.get(0)?,
            assessment_item_id: row.get(1)?,
            revision: row.get(2)?,
            score_millis: (row.get::<_, f64>(3)? * 1000.0).round() as i64,
        })
    })?;
    let decisions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let item_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM exam_assessment_items_v2
         WHERE assessment_version_id=?1 AND state='active'",
        [assessment_version_id],
        |row| row.get(0),
    )?;
    if decisions.is_empty() || decisions.len() as i64 != item_count {
        return Err(CoreError::Invalid("发布前仍有题目未终审".into()));
    }
    let bytes = serde_json::to_vec(&decisions)
        .map_err(|error| CoreError::Parse(format!("decision set hash 失败：{error}")))?;
    let set_hash = hashing::sha256_hex(&bytes);
    let total_score = decisions
        .iter()
        .map(|decision| decision.score_millis as f64 / 1000.0)
        .sum();
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision), 0) + 1 FROM exam_grade_publications_v2
         WHERE assessment_version_id=?1",
        [assessment_version_id],
        |row| row.get(0),
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    tx.execute(
        "INSERT INTO exam_grade_publications_v2
         (public_id, assessment_version_id, revision, state, published_by, published_at, created_at)
         VALUES (?1,?2,?3,'published',?4,?5,?5)",
        (
            &public_id,
            assessment_version_id,
            revision,
            published_by.trim(),
            &now,
        ),
    )?;
    let publication_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO exam_grade_publication_items_v2
         (publication_id, attempt_id, grade_decision_set_hash, total_score, created_at)
         VALUES (?1,?2,?3,?4,?5)",
        (publication_id, attempt_id, &set_hash, total_score, &now),
    )?;
    let publication_item_id = tx.last_insert_rowid();
    for decision in &decisions {
        tx.execute(
            "INSERT INTO exam_grade_publication_decisions_v2
             (publication_item_id, attempt_id, grade_decision_id, created_at)
             VALUES (?1,?2,?3,?4)",
            (publication_item_id, attempt_id, decision.decision_id, &now),
        )?;
    }
    if let Some(old_id) = old_publication_id {
        tx.execute(
            "UPDATE exam_grade_publications_v2 SET state='superseded'
             WHERE id=?1 AND state='published'",
            [old_id],
        )?;
    }
    activate_publication_evidence(&tx, attempt_id, published_by)?;
    tx.execute(
        "UPDATE exam_attempts_v2
         SET state='published', active_publication_id=?1, updated_at=?2 WHERE id=?3",
        (publication_id, &now, attempt_id),
    )?;
    tx.commit()?;
    Ok(Publication {
        id: publication_id,
        public_id,
        assessment_version_id,
        revision,
        state: "published".into(),
        attempt_id,
        total_score,
        grade_decision_set_hash: set_hash,
        published_by: published_by.trim().into(),
        published_at: now,
    })
}

pub fn list_legacy_answer_compat(
    conn: &Connection,
    limit: i64,
) -> CoreResult<Vec<LegacyAnswerCompat>> {
    let mut stmt = conn.prepare(
        "SELECT legacy_answer_id, source_kind, mapping_state, attempt_id,
                assessment_item_id, student_id, question_id, picked, score, status, created_at
         FROM exam_legacy_answer_compat_v2 ORDER BY legacy_answer_id DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit.clamp(1, 500)], |row| {
        Ok(LegacyAnswerCompat {
            legacy_answer_id: row.get(0)?,
            source_kind: row.get(1)?,
            mapping_state: row.get(2)?,
            attempt_id: row.get(3)?,
            assessment_item_id: row.get(4)?,
            student_id: row.get(5)?,
            question_id: row.get(6)?,
            picked: row.get(7)?,
            score: row.get(8)?,
            status: row.get(9)?,
            created_at: row.get(10)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use module_knowledge::db::content::{
        add_ability_link, add_knowledge_link, create_answer_key_version, create_link_set,
        create_question, create_question_version, create_rubric_version, promote_question_version,
        NewAbilityLink, NewAnswerKeyVersion, NewAnswerSlot, NewKnowledgeLink, NewQuestion,
        NewQuestionVersion, NewRubricPoint, NewRubricVersion,
    };
    use module_knowledge::db::taxonomy::{
        create_ability_dimension, create_curriculum_node, create_knowledge_map,
        create_knowledge_node, create_textbook_edition, NewAbilityDimension, NewCurriculumNode,
        NewKnowledgeMap, NewKnowledgeNode, NewTextbookEdition,
    };
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    struct Fixture {
        conn: Connection,
        version_id: i64,
        item_id: i64,
        attempt_id: i64,
    }

    fn setup() -> Fixture {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        conn.execute_batch(
            "INSERT INTO subjects(name) VALUES ('历史');
             INSERT INTO classes(name, term) VALUES ('八年级一班', '2026秋');
             INSERT INTO students(student_no, name, class_id) VALUES ('S001','小林',1);",
        )
        .unwrap();

        let edition = create_textbook_edition(
            &conn,
            &NewTextbookEdition {
                subject_id: 1,
                publisher_code: "PEP",
                edition_code: "2024",
                title: "中国历史八年级上册",
                grade: "8",
                volume: "upper",
                curriculum_region: None,
            },
        )
        .unwrap();
        let map = create_knowledge_map(
            &conn,
            &NewKnowledgeMap {
                textbook_edition_id: edition.id,
                revision: 1,
                state: "confirmed",
                supersedes_map_id: None,
            },
        )
        .unwrap();
        let lesson = create_curriculum_node(
            &conn,
            &NewCurriculumNode {
                stable_id: None,
                knowledge_map_id: map.id,
                parent_id: None,
                node_type: "lesson",
                code: Some("L1"),
                title: "鸦片战争",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        let knowledge = create_knowledge_node(
            &conn,
            &NewKnowledgeNode {
                stable_id: None,
                knowledge_map_id: map.id,
                curriculum_node_id: Some(lesson.id),
                parent_id: None,
                code: Some("K1"),
                title: "鸦片战争爆发时间",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        let ability = create_ability_dimension(
            &conn,
            &NewAbilityDimension {
                stable_id: None,
                subject_id: 1,
                revision: 1,
                code: "fact_recall",
                title: "事实识记与提取",
                description: None,
                supersedes_dimension_id: None,
            },
        )
        .unwrap();
        let question = create_question(
            &conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: "teacher",
                question_family_id: None,
                rights_status: "unknown",
                sharing_allowed: false,
            },
        )
        .unwrap();
        let question_version = create_question_version(
            &conn,
            &NewQuestionVersion {
                question_id: question.id,
                revision: 1,
                question_type: "true_false",
                stem: "鸦片战争爆发于 1840 年。",
                material_text: None,
                max_score: 1.0,
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "L0",
                state: "draft",
                options: &[],
            },
        )
        .unwrap();
        let answer = create_answer_key_version(
            &conn,
            &NewAnswerKeyVersion {
                question_version_id: question_version.id,
                revision: 1,
                answer_json: r#"{"schema_version":1,"correct":true}"#,
                state: "confirmed",
                confirmed_by: Some("teacher"),
                supersedes_answer_key_id: None,
                slots: &[],
            },
        )
        .unwrap();
        let rubric = create_rubric_version(
            &conn,
            &NewRubricVersion {
                question_version_id: question_version.id,
                revision: 1,
                max_score: 1.0,
                state: "confirmed",
                confirmed_by: Some("teacher"),
                supersedes_rubric_id: None,
                points: &[NewRubricPoint {
                    stable_id: None,
                    order_index: 0,
                    canonical_text: "判断为正确",
                    allowed_paraphrases_json: None,
                    required_concepts_json: None,
                    max_score: 1.0,
                }],
            },
        )
        .unwrap();
        let link_set = create_link_set(
            &conn,
            question_version.id,
            map.id,
            1,
            "confirmed",
            Some("teacher"),
            None,
        )
        .unwrap();
        add_knowledge_link(
            &conn,
            &NewKnowledgeLink {
                link_set_id: link_set.id,
                source_type: "question",
                source_public_id: &question_version.public_id,
                knowledge_node_id: knowledge.id,
                relation_type: "direct_assessment",
                confirmation_level: "teacher_confirmed",
                verified_by: Some("teacher"),
            },
        )
        .unwrap();
        add_ability_link(
            &conn,
            &NewAbilityLink {
                link_set_id: link_set.id,
                source_type: "question",
                source_public_id: &question_version.public_id,
                ability_dimension_id: ability.id,
                evidence_strength: 0.4,
                response_mode: "recognition",
                confirmation_level: "teacher_confirmed",
                verified_by: Some("teacher"),
            },
        )
        .unwrap();
        promote_question_version(&conn, question_version.id, "L3", "teacher", None).unwrap();

        let assessment = create_assessment_draft(
            &conn,
            &NewAssessmentDraft {
                title: "第一课随堂测",
                class_id: 1,
                assessment_context: "quiz",
                evidence_policy: "include",
                created_by: "teacher",
                template_version: None,
            },
        )
        .unwrap();
        let item = add_assessment_item(
            &conn,
            assessment.assessment_version_id,
            &NewAssessmentItem {
                question_version_id: question_version.id,
                answer_key_version_id: answer.id,
                rubric_version_id: rubric.id,
                link_set_id: link_set.id,
                order_index: 0,
                score: 1.0,
                option_order_json: None,
                presentation_snapshot_json: r#"{"schema_version":1,"question_no":"1"}"#,
            },
        )
        .unwrap();
        confirm_assessment_version(&conn, assessment.assessment_version_id, "teacher").unwrap();
        let attempt = create_attempt(
            &conn,
            assessment.assessment_version_id,
            1,
            "manual",
            "first",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exam_questions(qtype, stem, correct_answer, max_score)
             VALUES ('judge','旧题','true',1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exam_student_answers
             (student_id, question_id, picked, max_score, machine_correct, human_correct,
              is_correct, score, status, updated_at)
             VALUES (1,1,'true',1,1,1,1,1,'confirmed',datetime('now'))",
            [],
        )
        .unwrap();
        Fixture {
            conn,
            version_id: assessment.assessment_version_id,
            item_id: item.id,
            attempt_id: attempt.id,
        }
    }

    fn create_subjective_k1(conn: &Connection, question_type: &str) -> (i64, i64, i64, String) {
        let question = create_question(
            conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: "teacher",
                question_family_id: None,
                rights_status: "unknown",
                sharing_allowed: false,
            },
        )
        .unwrap();
        let question_version = create_question_version(
            conn,
            &NewQuestionVersion {
                question_id: question.id,
                revision: 1,
                question_type,
                stem: "主观题证据测试",
                material_text: None,
                max_score: 2.0,
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "L0",
                state: "draft",
                options: &[],
            },
        )
        .unwrap();
        let slots = if question_type == "fill_blank" {
            vec![NewAnswerSlot {
                stable_id: Some("slot-1"),
                order_index: 0,
                canonical_answers_json: r#"{"schema_version":1,"answers":["1842"]}"#,
                normalization_rules_json: None,
                max_score: 2.0,
            }]
        } else {
            vec![]
        };
        let answer = create_answer_key_version(
            conn,
            &NewAnswerKeyVersion {
                question_version_id: question_version.id,
                revision: 1,
                answer_json: r#"{"schema_version":1,"answer":"测试"}"#,
                state: "confirmed",
                confirmed_by: Some("teacher"),
                supersedes_answer_key_id: None,
                slots: &slots,
            },
        )
        .unwrap();
        let rubric = create_rubric_version(
            conn,
            &NewRubricVersion {
                question_version_id: question_version.id,
                revision: 1,
                max_score: 2.0,
                state: "confirmed",
                confirmed_by: Some("teacher"),
                supersedes_rubric_id: None,
                points: &[NewRubricPoint {
                    stable_id: Some("point-1"),
                    order_index: 0,
                    canonical_text: "评分点一",
                    allowed_paraphrases_json: None,
                    required_concepts_json: None,
                    max_score: 2.0,
                }],
            },
        )
        .unwrap();
        let point: (i64, String) = conn
            .query_row(
                "SELECT id,public_id FROM k1_rubric_points WHERE rubric_version_id=?1",
                [rubric.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        (answer.id, rubric.id, point.0, point.1)
    }

    fn subjective_evidence_decision(
        question_type: &str,
        answer_key_version_id: i64,
        rubric_version_id: i64,
        confirmation_level: &str,
        result_json: Option<String>,
    ) -> EvidenceDecision {
        EvidenceDecision {
            id: 1,
            public_id: "decision-test".into(),
            revision: 1,
            teacher_score: 1.0,
            confirmation_level: confirmation_level.into(),
            decided_at: "2026-07-16T00:00:00Z".into(),
            item_public_id: "item-test".into(),
            item_score: 2.0,
            link_set_id: 1,
            dictation_rubric_point_public_id: None,
            question_type: question_type.into(),
            answer_key_version_id,
            rubric_version_id,
            subjective_source: true,
            short_answer_result_json: result_json,
        }
    }

    #[test]
    fn subjective_evidence_uses_only_safe_slot_or_point_granularity() {
        let fixture = setup();
        let (fill_answer, fill_rubric, _, _) = create_subjective_k1(&fixture.conn, "fill_blank");
        let fill = subjective_evidence_decision(
            "fill_blank",
            fill_answer,
            fill_rubric,
            "teacher_corrected",
            None,
        );
        let fill_sources = evidence_sources(&fixture.conn, &fill).unwrap();
        assert_eq!(fill_sources.len(), 1);
        assert_eq!(fill_sources[0].source_ref_type, "answer_slot");
        assert!((fill_sources[0].value - 0.5).abs() < 0.000_001);

        let (short_answer, short_rubric, point_id, point_public_id) =
            create_subjective_k1(&fixture.conn, "short_answer");
        let corrected = subjective_evidence_decision(
            "short_answer",
            short_answer,
            short_rubric,
            "teacher_corrected",
            Some(
                serde_json::json!({
                    "schema_version": 1,
                    "point_results": [{"rubric_point_id": point_id, "suggested_score": 1.5}]
                })
                .to_string(),
            ),
        );
        assert!(evidence_sources(&fixture.conn, &corrected)
            .unwrap()
            .is_empty());

        let accepted = subjective_evidence_decision(
            "short_answer",
            short_answer,
            short_rubric,
            "teacher_accepted",
            Some(
                serde_json::json!({
                    "schema_version": 1,
                    "point_results": [{"rubric_point_id": point_id, "suggested_score": 1.5}]
                })
                .to_string(),
            ),
        );
        let short_sources = evidence_sources(&fixture.conn, &accepted).unwrap();
        assert_eq!(short_sources.len(), 1);
        assert_eq!(short_sources[0].source_ref_id, point_public_id);
        assert!((short_sources[0].value - 0.75).abs() < 0.000_001);
    }

    fn decision<'a>(
        fixture: &'a Fixture,
        score: f64,
        note: Option<&'a str>,
    ) -> NewGradeDecision<'a> {
        NewGradeDecision {
            attempt_id: fixture.attempt_id,
            assessment_item_id: fixture.item_id,
            machine_grade_ai_run_id: None,
            teacher_score: score,
            point_results_json: r#"{"schema_version":1,"result":"confirmed"}"#,
            teacher_note: note,
            confirmation_level: "teacher_corrected",
            decided_by: "teacher",
        }
    }

    #[test]
    fn decision_is_idempotent_and_publication_freezes_the_decision_set() {
        let fixture = setup();
        let first = decide_grade(&fixture.conn, &decision(&fixture, 1.0, None)).unwrap();
        let repeated = decide_grade(&fixture.conn, &decision(&fixture, 1.0, None)).unwrap();
        assert_eq!(first.id, repeated.id);
        assert_eq!(
            get_attempt(&fixture.conn, fixture.attempt_id)
                .unwrap()
                .unwrap()
                .state,
            "ready_to_publish"
        );
        let publications: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_grade_publications_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(publications, 0, "单题终审不能自动发布");

        let published = publish_attempt(&fixture.conn, fixture.attempt_id, "teacher").unwrap();
        assert_eq!(published.revision, 1);
        assert_eq!(published.total_score, 1.0);
        let first_evidence: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
                   (SELECT COUNT(*) FROM outbox_events
                    WHERE event_type='learning_evidence_changed'),
                   (SELECT COUNT(*) FROM audit_events
                    WHERE action='exam.learning_evidence.activated')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(first_evidence, (2, 2, 2));
        let changed = decide_grade(&fixture.conn, &decision(&fixture, 0.0, Some("改判"))).unwrap();
        assert_eq!(changed.revision, 2);
        assert_eq!(
            get_attempt(&fixture.conn, fixture.attempt_id)
                .unwrap()
                .unwrap()
                .state,
            "ready_to_publish"
        );
        let old_state: String = fixture
            .conn
            .query_row(
                "SELECT state FROM exam_grade_publications_v2 WHERE id=?1",
                [published.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_state, "published", "改分但未重发时旧发布仍然有效");
        let evidence_after_change: (i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='reverted')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(evidence_after_change, (0, 2));

        let republished = publish_attempt(&fixture.conn, fixture.attempt_id, "teacher").unwrap();
        assert_eq!(republished.revision, 2);
        assert_eq!(republished.total_score, 0.0);
        let old_state: String = fixture
            .conn
            .query_row(
                "SELECT state FROM exam_grade_publications_v2 WHERE id=?1",
                [published.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_state, "superseded");
        let frozen_old_decision: i64 = fixture
            .conn
            .query_row(
                "SELECT grade_decision_id FROM exam_grade_publication_decisions_v2 pd
                 JOIN exam_grade_publication_items_v2 pi ON pi.id=pd.publication_item_id
                 WHERE pi.publication_id=?1",
                [published.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(frozen_old_decision, first.id);
        let evidence_after_republish: (i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='reverted')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(evidence_after_republish, (2, 2));
        assert!(fixture
            .conn
            .execute(
                "UPDATE exam_assessment_items_v2 SET score=2 WHERE id=?1",
                [fixture.item_id]
            )
            .is_err());
    }

    #[test]
    fn decision_revision_rolls_back_if_insert_fails() {
        let fixture = setup();
        let first = decide_grade(&fixture.conn, &decision(&fixture, 1.0, None)).unwrap();
        publish_attempt(&fixture.conn, fixture.attempt_id, "teacher").unwrap();
        fixture
            .conn
            .execute_batch(
                "CREATE TRIGGER fail_second_decision
                 BEFORE INSERT ON exam_grade_decisions_v2 WHEN NEW.revision=2
                 BEGIN SELECT RAISE(ABORT, 'injected decision failure'); END;",
            )
            .unwrap();
        assert!(decide_grade(&fixture.conn, &decision(&fixture, 0.0, None)).is_err());
        let active: (i64, String) = fixture
            .conn
            .query_row(
                "SELECT id, state FROM exam_grade_decisions_v2
                 WHERE attempt_id=?1 AND assessment_item_id=?2 AND state='active'",
                (fixture.attempt_id, fixture.item_id),
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(active, (first.id, "active".into()));
        let evidence_states: (i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='reverted')",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(evidence_states, (2, 0));
    }

    #[test]
    fn publication_and_formal_evidence_roll_back_together() {
        let fixture = setup();
        decide_grade(&fixture.conn, &decision(&fixture, 1.0, None)).unwrap();
        fixture
            .conn
            .execute_batch(
                "CREATE TRIGGER fail_publication_evidence
                 BEFORE INSERT ON learning_evidence
                 BEGIN SELECT RAISE(ABORT, 'injected evidence failure'); END;",
            )
            .unwrap();
        assert!(publish_attempt(&fixture.conn, fixture.attempt_id, "teacher").is_err());
        let counts: (i64, i64, i64, String, Option<i64>) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_grade_publications_v2),
                   (SELECT COUNT(*) FROM learning_evidence),
                   (SELECT COUNT(*) FROM outbox_events
                    WHERE event_type='learning_evidence_changed'),
                   state,active_publication_id
                 FROM exam_attempts_v2 WHERE id=?1",
                [fixture.attempt_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(counts, (0, 0, 0, "ready_to_publish".into(), None));
    }

    #[test]
    fn migration_keeps_old_answers_explicitly_unmapped() {
        let fixture = setup();
        let legacy = list_legacy_answer_compat(&fixture.conn, 10).unwrap();
        assert_eq!(legacy.len(), 1);
        assert_eq!(legacy[0].source_kind, "legacy_manual");
        assert_eq!(legacy[0].mapping_state, "legacy_unmapped");
        let mapping_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_legacy_answer_mappings_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mapping_count, 0);
        assert!(fixture.version_id > 0);
    }
}
