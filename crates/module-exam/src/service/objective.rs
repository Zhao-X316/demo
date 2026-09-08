//! T6 客观题确定性观察与老师终审。
//!
//! 固定 fixture/OMR 只写 observation 和 machine suggestion；只有老师显式接受单题，
//! 或执行满足严格门槛的批量终审，才会调用 B0 的追加式 grade decision。

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AiRunStatus;

use super::assessment::{decide_grade_in_transaction, GradeDecision, NewGradeDecision};
use crate::objective_recognition::{ObjectiveRecognitionFailure, ObjectiveRecognitionOutput};

pub const DEFAULT_STRICT_BATCH_CONFIDENCE: f64 = 0.95;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveObservation {
    pub id: i64,
    pub public_id: String,
    pub idempotency_key: String,
    pub attempt_id: i64,
    pub assessment_item_id: i64,
    pub answer_region_revision_id: i64,
    pub revision: i64,
    pub source_kind: String,
    pub question_type: String,
    pub result_state: String,
    pub observed_answer_json: Option<String>,
    pub confidence: Option<f64>,
    pub alteration_detected: bool,
    pub ai_run_id: Option<i64>,
    pub failure_meta_json: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveGradeSuggestion {
    pub id: i64,
    pub public_id: String,
    pub observation_revision_id: i64,
    pub attempt_id: i64,
    pub assessment_item_id: i64,
    pub answer_key_version_id: i64,
    pub outcome: String,
    pub suggested_score: Option<f64>,
    pub result_json: String,
    pub batch_eligible: bool,
    pub exclusion_reason: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveObservationResult {
    pub observation: ObjectiveObservation,
    pub suggestion: ObjectiveGradeSuggestion,
}

pub struct NewObjectiveObservation<'a> {
    pub answer_region_revision_id: i64,
    pub source_kind: &'a str,
    pub result_state: &'a str,
    pub observed_answer_json: Option<&'a str>,
    pub confidence: Option<f64>,
    pub ai_run_id: Option<i64>,
    pub failure_meta_json: Option<&'a str>,
    pub idempotency_key: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchItemResult {
    pub suggestion_id: i64,
    pub outcome: String,
    pub reason_code: Option<String>,
    pub grade_decision_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveReviewBatch {
    pub id: i64,
    pub public_id: String,
    pub assessment_version_id: i64,
    pub idempotency_key: String,
    pub confidence_threshold: f64,
    pub requested_count: i64,
    pub confirmed_count: i64,
    pub excluded_count: i64,
    pub created_by: String,
    pub items: Vec<BatchItemResult>,
}

pub struct StrictBatchReview<'a> {
    pub suggestion_ids: &'a [i64],
    pub confidence_threshold: f64,
    pub reviewed_by: &'a str,
    pub idempotency_key: &'a str,
}

/// T6 按题工作台只读行。它把老师终审需要的证据聚到一个 DTO，
/// 但不暴露供应商凭据、任意模型参数或可写数据库状态。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveWorkbenchRow {
    pub assessment_id: i64,
    pub assessment_version_id: i64,
    pub assessment_title: String,
    pub attempt_id: i64,
    pub attempt_state: String,
    pub active_publication_id: Option<i64>,
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub assessment_item_id: i64,
    pub order_index: i64,
    pub max_score: f64,
    pub question_version_id: i64,
    pub question_no: String,
    pub question_type: String,
    pub question_stem: String,
    pub answer_region_revision_id: i64,
    pub crop_path: Option<String>,
    pub suggestion_id: i64,
    pub observation_state: String,
    pub observed_answer_json: Option<String>,
    pub confidence: Option<f64>,
    pub suggestion_outcome: String,
    pub suggested_score: Option<f64>,
    pub batch_eligible: bool,
    pub exclusion_reason: Option<String>,
    pub grade_decision_id: Option<i64>,
    pub grade_decision_revision: Option<i64>,
    pub teacher_score: Option<f64>,
    pub confirmation_level: Option<String>,
    pub review_mode: Option<String>,
    pub current_suggestion_confirmed: bool,
    pub decided_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveAttemptSummary {
    pub assessment_id: i64,
    pub assessment_version_id: i64,
    pub assessment_title: String,
    pub attempt_id: i64,
    pub attempt_state: String,
    pub active_publication_id: Option<i64>,
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub item_count: i64,
    pub observed_count: i64,
    pub confirmed_count: i64,
    pub teacher_total_score: f64,
    pub max_total_score: f64,
    pub published_total_score: Option<f64>,
    pub can_publish: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveWorkbench {
    pub rows: Vec<ObjectiveWorkbenchRow>,
    pub attempts: Vec<ObjectiveAttemptSummary>,
}

#[derive(Debug)]
struct ObservationScope {
    attempt_id: i64,
    assessment_item_id: i64,
    crop_artifact_id: i64,
    question_type: String,
    answer_key_version_id: i64,
    answer_json: String,
    max_score: f64,
}

#[derive(Debug)]
struct ReviewScope {
    assessment_version_id: i64,
    max_score: f64,
    suggestion: ObjectiveGradeSuggestion,
    observation_state: String,
    observation_result: String,
    confidence: Option<f64>,
    region_state: String,
    region_decision: String,
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{field}不能为空")))
    } else {
        Ok(())
    }
}

fn schema_object(value: &str, field: &str) -> CoreResult<Value> {
    let value: Value = serde_json::from_str(value)
        .map_err(|error| CoreError::Invalid(format!("{field} JSON 无效：{error}")))?;
    if !value.is_object()
        || value
            .get("schema_version")
            .and_then(Value::as_i64)
            .is_none()
    {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是带整数 schema_version 的对象"
        )));
    }
    Ok(value)
}

fn normalize_labels(value: &Value, field: &str) -> CoreResult<Vec<String>> {
    let labels = value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| CoreError::Invalid(format!("客观题答案缺少 {field}")))?;
    let mut normalized = BTreeSet::new();
    for label in labels {
        let label = label
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CoreError::Invalid(format!("{field} 只能包含非空字符串")))?;
        normalized.insert(label.to_uppercase());
    }
    if normalized.is_empty() {
        return Err(CoreError::Invalid(format!("{field} 不能为空")));
    }
    Ok(normalized.into_iter().collect())
}

fn observation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ObjectiveObservation> {
    Ok(ObjectiveObservation {
        id: row.get(0)?,
        public_id: row.get(1)?,
        idempotency_key: row.get(2)?,
        attempt_id: row.get(3)?,
        assessment_item_id: row.get(4)?,
        answer_region_revision_id: row.get(5)?,
        revision: row.get(6)?,
        source_kind: row.get(7)?,
        question_type: row.get(8)?,
        result_state: row.get(9)?,
        observed_answer_json: row.get(10)?,
        confidence: row.get(11)?,
        alteration_detected: row.get(12)?,
        ai_run_id: row.get(13)?,
        failure_meta_json: row.get(14)?,
        state: row.get(15)?,
    })
}

fn suggestion_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ObjectiveGradeSuggestion> {
    Ok(ObjectiveGradeSuggestion {
        id: row.get(0)?,
        public_id: row.get(1)?,
        observation_revision_id: row.get(2)?,
        attempt_id: row.get(3)?,
        assessment_item_id: row.get(4)?,
        answer_key_version_id: row.get(5)?,
        outcome: row.get(6)?,
        suggested_score: row.get(7)?,
        result_json: row.get(8)?,
        batch_eligible: row.get(9)?,
        exclusion_reason: row.get(10)?,
        state: row.get(11)?,
    })
}

fn get_observation(conn: &Connection, id: i64) -> CoreResult<Option<ObjectiveObservation>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, idempotency_key, attempt_id, assessment_item_id,
                    answer_region_revision_id, revision, source_kind, question_type,
                    result_state, observed_answer_json, confidence, alteration_detected,
                    ai_run_id, failure_meta_json, state
             FROM exam_objective_observation_revisions_v2 WHERE id=?1",
            [id],
            observation_row,
        )
        .optional()?)
}

pub fn get_objective_suggestion(
    conn: &Connection,
    id: i64,
) -> CoreResult<Option<ObjectiveGradeSuggestion>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, observation_revision_id, attempt_id,
                    assessment_item_id, answer_key_version_id, outcome, suggested_score,
                    result_json, batch_eligible, exclusion_reason, state
             FROM exam_objective_grade_suggestions_v2 WHERE id=?1",
            [id],
            suggestion_row,
        )
        .optional()?)
}

/// 读取当前 active observation/suggestion 对应的按题终审工作台。
/// `assessment_version_id=None` 时返回最近范围，供桌面端首次进入使用。
pub fn list_objective_workbench(
    conn: &Connection,
    assessment_version_id: Option<i64>,
    limit: i64,
) -> CoreResult<ObjectiveWorkbench> {
    list_objective_workbench_scoped(conn, assessment_version_id, limit, None)
}

pub fn list_objective_workbench_scoped(
    conn: &Connection,
    assessment_version_id: Option<i64>,
    limit: i64,
    attempt_ids: Option<&[i64]>,
) -> CoreResult<ObjectiveWorkbench> {
    let scope = attempt_ids
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| CoreError::Parse(format!("批改任务范围无法读取：{error}")))?;
    let limit = if attempt_ids.is_some() {
        i64::MAX
    } else {
        limit.clamp(1, 1000)
    };
    let mut row_stmt = conn.prepare(
        "SELECT a.id,v.id,a.title,
                at.id,at.state,at.active_publication_id,
                st.id,st.student_no,st.name,
                i.id,i.order_index,i.score,
                q.id,
                COALESCE(CAST(json_extract(i.presentation_snapshot_json,'$.question_no') AS TEXT),
                         CAST(i.order_index + 1 AS TEXT)),
                q.question_type,q.stem,
                o.answer_region_revision_id,art.archived_path,
                s.id,o.result_state,o.observed_answer_json,o.confidence,
                s.outcome,s.suggested_score,s.batch_eligible,s.exclusion_reason,
                d.id,d.revision,d.teacher_score,d.confirmation_level,src.review_mode,
                CASE WHEN src.suggestion_id=s.id THEN 1 ELSE 0 END,d.decided_at
         FROM exam_objective_grade_suggestions_v2 s
         JOIN exam_objective_observation_revisions_v2 o
           ON o.id=s.observation_revision_id AND o.state='active'
         JOIN exam_answer_region_revisions_v2 r
           ON r.id=o.answer_region_revision_id
         LEFT JOIN artifacts art ON art.id=r.crop_artifact_id
         JOIN exam_attempts_v2 at ON at.id=s.attempt_id AND at.state<>'voided'
         JOIN students st ON st.id=at.student_id
         JOIN exam_assessment_items_v2 i
           ON i.id=s.assessment_item_id AND i.state='active'
         JOIN exam_assessment_versions_v2 v ON v.id=i.assessment_version_id
         JOIN exam_assessments_v2 a ON a.id=v.assessment_id
         JOIN k1_question_versions q ON q.id=i.question_version_id
         LEFT JOIN exam_grade_decisions_v2 d
           ON d.attempt_id=at.id AND d.assessment_item_id=i.id AND d.state='active'
         LEFT JOIN exam_grade_decision_objective_sources_v2 src
           ON src.grade_decision_id=d.id
         WHERE s.state='active' AND (?1 IS NULL OR v.id=?1)
           AND (?3 IS NULL OR at.id IN (SELECT value FROM json_each(?3)))
         ORDER BY v.id DESC,i.order_index,st.student_no,at.attempt_no
         LIMIT ?2",
    )?;
    let rows = row_stmt
        .query_map((assessment_version_id, limit, scope.as_deref()), |row| {
            Ok(ObjectiveWorkbenchRow {
                assessment_id: row.get(0)?,
                assessment_version_id: row.get(1)?,
                assessment_title: row.get(2)?,
                attempt_id: row.get(3)?,
                attempt_state: row.get(4)?,
                active_publication_id: row.get(5)?,
                student_id: row.get(6)?,
                student_no: row.get(7)?,
                student_name: row.get(8)?,
                assessment_item_id: row.get(9)?,
                order_index: row.get(10)?,
                max_score: row.get(11)?,
                question_version_id: row.get(12)?,
                question_no: row.get(13)?,
                question_type: row.get(14)?,
                question_stem: row.get(15)?,
                answer_region_revision_id: row.get(16)?,
                crop_path: row.get(17)?,
                suggestion_id: row.get(18)?,
                observation_state: row.get(19)?,
                observed_answer_json: row.get(20)?,
                confidence: row.get(21)?,
                suggestion_outcome: row.get(22)?,
                suggested_score: row.get(23)?,
                batch_eligible: row.get(24)?,
                exclusion_reason: row.get(25)?,
                grade_decision_id: row.get(26)?,
                grade_decision_revision: row.get(27)?,
                teacher_score: row.get(28)?,
                confirmation_level: row.get(29)?,
                review_mode: row.get(30)?,
                current_suggestion_confirmed: row.get(31)?,
                decided_at: row.get(32)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(row_stmt);

    let mut attempt_stmt = conn.prepare(
        "SELECT a.id,v.id,a.title,
                at.id,at.state,at.active_publication_id,
                st.id,st.student_no,st.name,
                (SELECT COUNT(*) FROM exam_assessment_items_v2 i
                 WHERE i.assessment_version_id=v.id AND i.state='active'),
                (SELECT COUNT(*) FROM exam_objective_grade_suggestions_v2 s
                 WHERE s.attempt_id=at.id AND s.state='active'),
                (SELECT COUNT(*) FROM exam_grade_decisions_v2 d
                 JOIN exam_assessment_items_v2 i ON i.id=d.assessment_item_id
                 WHERE d.attempt_id=at.id AND d.state='active'
                   AND i.assessment_version_id=v.id AND i.state='active'),
                (SELECT COALESCE(SUM(d.teacher_score),0.0)
                 FROM exam_grade_decisions_v2 d
                 JOIN exam_assessment_items_v2 i ON i.id=d.assessment_item_id
                 WHERE d.attempt_id=at.id AND d.state='active'
                   AND i.assessment_version_id=v.id AND i.state='active'),
                (SELECT COALESCE(SUM(i.score),0.0)
                 FROM exam_assessment_items_v2 i
                 WHERE i.assessment_version_id=v.id AND i.state='active'),
                pi.total_score,
                CASE WHEN at.state='ready_to_publish' THEN 1 ELSE 0 END
         FROM exam_attempts_v2 at
         JOIN students st ON st.id=at.student_id
         JOIN exam_assessment_versions_v2 v ON v.id=at.assessment_version_id
         JOIN exam_assessments_v2 a ON a.id=v.assessment_id
         LEFT JOIN exam_grade_publication_items_v2 pi
           ON pi.publication_id=at.active_publication_id AND pi.attempt_id=at.id
         WHERE at.state<>'voided'
           AND (?1 IS NULL OR v.id=?1)
           AND EXISTS(SELECT 1 FROM exam_objective_grade_suggestions_v2 s
                      WHERE s.attempt_id=at.id AND s.state='active')
           AND (?3 IS NULL OR at.id IN (SELECT value FROM json_each(?3)))
         ORDER BY v.id DESC,st.student_no,at.attempt_no
         LIMIT ?2",
    )?;
    let attempts = attempt_stmt
        .query_map((assessment_version_id, limit, scope.as_deref()), |row| {
            Ok(ObjectiveAttemptSummary {
                assessment_id: row.get(0)?,
                assessment_version_id: row.get(1)?,
                assessment_title: row.get(2)?,
                attempt_id: row.get(3)?,
                attempt_state: row.get(4)?,
                active_publication_id: row.get(5)?,
                student_id: row.get(6)?,
                student_no: row.get(7)?,
                student_name: row.get(8)?,
                item_count: row.get(9)?,
                observed_count: row.get(10)?,
                confirmed_count: row.get(11)?,
                teacher_total_score: row.get(12)?,
                max_total_score: row.get(13)?,
                published_total_score: row.get(14)?,
                can_publish: row.get(15)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(ObjectiveWorkbench { rows, attempts })
}

fn get_observation_result(
    conn: &Connection,
    observation_id: i64,
) -> CoreResult<ObjectiveObservationResult> {
    let observation = get_observation(conn, observation_id)?
        .ok_or_else(|| CoreError::NotFound(format!("objective_observation#{observation_id}")))?;
    let suggestion = conn
        .query_row(
            "SELECT id, public_id, observation_revision_id, attempt_id,
                    assessment_item_id, answer_key_version_id, outcome, suggested_score,
                    result_json, batch_eligible, exclusion_reason, state
             FROM exam_objective_grade_suggestions_v2 WHERE observation_revision_id=?1",
            [observation_id],
            suggestion_row,
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("客观题观察缺少评分建议".into()))?;
    Ok(ObjectiveObservationResult {
        observation,
        suggestion,
    })
}

fn observation_scope(conn: &Connection, region_id: i64) -> CoreResult<ObservationScope> {
    conn.query_row(
        "SELECT m.attempt_id, r.assessment_item_id, r.crop_artifact_id, q.question_type,
                i.answer_key_version_id, ak.answer_json, i.score
         FROM exam_answer_region_revisions_v2 r
         JOIN exam_page_alignment_revisions_v2 al
           ON al.id=r.alignment_revision_id AND al.state='active'
              AND al.decision='teacher_confirmed'
         JOIN exam_page_match_revisions_v2 m
           ON m.id=al.match_revision_id AND m.state='active'
              AND m.decision='teacher_confirmed'
         JOIN exam_attempts_v2 at ON at.id=m.attempt_id AND at.state<>'voided'
         JOIN exam_assessment_items_v2 i
           ON i.id=r.assessment_item_id
              AND i.assessment_version_id=at.assessment_version_id AND i.state='active'
         JOIN k1_question_versions q ON q.id=i.question_version_id
         JOIN k1_answer_key_versions ak
           ON ak.id=i.answer_key_version_id AND ak.state='confirmed'
         WHERE r.id=?1 AND r.state='active' AND r.decision='teacher_confirmed'",
        [region_id],
        |row| {
            Ok(ObservationScope {
                attempt_id: row.get(0)?,
                assessment_item_id: row.get(1)?,
                crop_artifact_id: row.get(2)?,
                question_type: row.get(3)?,
                answer_key_version_id: row.get(4)?,
                answer_json: row.get(5)?,
                max_score: row.get(6)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| {
        CoreError::Invalid("客观题观察必须引用当前老师确认的页面、学生、配准和答案区域".into())
    })
}

fn compare_answer(question_type: &str, expected: &Value, observed: &Value) -> CoreResult<bool> {
    match question_type {
        "single" => {
            let expected = normalize_labels(expected, "correct_labels")?;
            let observed = normalize_labels(observed, "selected_labels")?;
            if expected.len() != 1 || observed.len() != 1 {
                return Err(CoreError::Invalid(
                    "单选题必须恰好有一个标准答案和作答".into(),
                ));
            }
            Ok(expected == observed)
        }
        "multiple" => Ok(normalize_labels(expected, "correct_labels")?
            == normalize_labels(observed, "selected_labels")?),
        "true_false" => {
            let expected = expected
                .get("correct")
                .and_then(Value::as_bool)
                .ok_or_else(|| CoreError::Invalid("判断题标准答案缺少 correct".into()))?;
            let observed = observed
                .get("selected")
                .and_then(Value::as_bool)
                .ok_or_else(|| CoreError::Invalid("判断题观察缺少 selected".into()))?;
            Ok(expected == observed)
        }
        _ => Err(CoreError::Invalid("T6 当前只处理选择和判断题".into())),
    }
}

fn validate_observation_input(input: &NewObjectiveObservation<'_>) -> CoreResult<Option<Value>> {
    required(input.idempotency_key, "客观题观察幂等键")?;
    if !matches!(input.source_kind, "fixed_fixture" | "omr") {
        return Err(CoreError::Invalid("客观题观察来源非法".into()));
    }
    if !matches!(
        input.result_state,
        "recognized" | "blank" | "altered" | "low_confidence" | "failed"
    ) {
        return Err(CoreError::Invalid("客观题观察状态非法".into()));
    }
    if input
        .confidence
        .is_some_and(|value| !(0.0..=1.0).contains(&value))
    {
        return Err(CoreError::Invalid("客观题识别置信度必须位于 0~1".into()));
    }
    let answer = match input.result_state {
        "recognized" | "altered" | "low_confidence" => Some(schema_object(
            input
                .observed_answer_json
                .ok_or_else(|| CoreError::Invalid("有效观察必须包含结构化答案".into()))?,
            "客观题观察答案",
        )?),
        _ => {
            if input.observed_answer_json.is_some() {
                return Err(CoreError::Invalid("空白/失败观察不能携带作答答案".into()));
            }
            None
        }
    };
    match input.result_state {
        "recognized"
            if input
                .confidence
                .is_none_or(|value| value < DEFAULT_STRICT_BATCH_CONFIDENCE) =>
        {
            return Err(CoreError::Invalid(
                "低于 0.95 的结果必须显式标记 low_confidence".into(),
            ))
        }
        "low_confidence"
            if input
                .confidence
                .is_none_or(|value| value >= DEFAULT_STRICT_BATCH_CONFIDENCE) =>
        {
            return Err(CoreError::Invalid(
                "low_confidence 必须携带低于 0.95 的置信度".into(),
            ))
        }
        "altered" if input.confidence.is_none() => {
            return Err(CoreError::Invalid("涂改观察必须携带置信度".into()))
        }
        _ => {}
    }
    if input.result_state == "failed" {
        schema_object(
            input
                .failure_meta_json
                .ok_or_else(|| CoreError::Invalid("识别失败必须包含脱敏失败元数据".into()))?,
            "客观题失败元数据",
        )?;
    } else if input.failure_meta_json.is_some() {
        return Err(CoreError::Invalid("非失败观察不能携带失败元数据".into()));
    }
    match input.source_kind {
        "fixed_fixture" if input.ai_run_id.is_some() => {
            return Err(CoreError::Invalid("固定 fixture 不得伪造 AI run".into()))
        }
        "omr" if input.ai_run_id.is_none() => {
            return Err(CoreError::Invalid(
                "生产 OMR 观察必须引用成功的 AI run".into(),
            ))
        }
        _ => {}
    }
    Ok(answer)
}

fn validate_omr_run_binding(
    conn: &Connection,
    input: &NewObjectiveObservation<'_>,
    scope: &ObservationScope,
    observed: Option<&Value>,
) -> CoreResult<()> {
    let run_id = input
        .ai_run_id
        .ok_or_else(|| CoreError::Invalid("生产 OMR 观察必须引用 AI run".into()))?;
    let run = ai_runs::get_by_id(conn, run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{run_id}")))?;
    if run.run_type != "omr"
        || run.source_module != "exam"
        || run.business_ref_type != "answer_region_revision"
        || run.business_ref_id != input.answer_region_revision_id.to_string()
        || run.input_artifact_id != Some(scope.crop_artifact_id)
    {
        return Err(CoreError::Invalid(
            "OMR run 与当前题区或裁剪 artifact 不一致".into(),
        ));
    }
    let artifact = artifacts::get_by_id(conn, scope.crop_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{}", scope.crop_artifact_id)))?;
    match run.status {
        AiRunStatus::Succeeded if input.result_state != "failed" => {
            let output_json = run
                .output_json
                .as_deref()
                .ok_or_else(|| CoreError::Invalid("成功 OMR run 缺少结构化输出".into()))?;
            if run.output_hash.as_deref() != Some(&hashing::sha256_hex(output_json.as_bytes())) {
                return Err(CoreError::Invalid("OMR run 输出 hash 不一致".into()));
            }
            let output: ObjectiveRecognitionOutput = serde_json::from_str(output_json)
                .map_err(|error| CoreError::Invalid(format!("OMR run 输出无效：{error}")))?;
            output.validate()?;
            if output.answer_region_revision_id != input.answer_region_revision_id
                || output.input_artifact_id != scope.crop_artifact_id
                || output.input_artifact_sha256 != artifact.sha256
                || output.input_hash != run.input_hash
                || output.question_type.as_str() != scope.question_type
                || output.descriptor.provider != run.provider
                || output.descriptor.model_name != run.model_name
                || output.descriptor.model_version != run.model_version
                || output.descriptor.config_version != run.config_version
                || output.descriptor.rule_version != run.prompt_or_rule_version
                || output.result_state.as_str() != input.result_state
                || output.confidence != input.confidence
            {
                return Err(CoreError::Invalid(
                    "OMR observation 与 run 输出合同不一致".into(),
                ));
            }
            let output_observed = output
                .observed_answer_json()?
                .map(|value| schema_object(&value, "OMR run 观察答案"))
                .transpose()?;
            if output_observed.as_ref() != observed {
                return Err(CoreError::Invalid(
                    "OMR observation 答案与 run 输出不一致".into(),
                ));
            }
        }
        AiRunStatus::Failed if input.result_state == "failed" => {
            let error_meta_json = run
                .error_meta_json
                .as_deref()
                .ok_or_else(|| CoreError::Invalid("失败 OMR run 缺少脱敏错误元数据".into()))?;
            let failure: ObjectiveRecognitionFailure = serde_json::from_str(error_meta_json)
                .map_err(|error| CoreError::Invalid(format!("OMR 失败元数据无效：{error}")))?;
            failure.validate()?;
            let persisted = schema_object(
                input
                    .failure_meta_json
                    .ok_or_else(|| CoreError::Invalid("失败 observation 缺少错误元数据".into()))?,
                "客观题失败元数据",
            )?;
            let run_error = schema_object(error_meta_json, "OMR run 失败元数据")?;
            if persisted != run_error {
                return Err(CoreError::Invalid(
                    "OMR observation 失败元数据与 run 不一致".into(),
                ));
            }
        }
        _ => {
            return Err(CoreError::Invalid(
                "OMR run 状态与 observation 结果状态不一致".into(),
            ))
        }
    }
    Ok(())
}

/// 把已完成的 OMR run 映射进 T6 既有 observation/suggestion 服务。
///
/// 该入口只做持久化映射；provider 调用和图片读取必须在数据库事务之外完成。
pub fn record_omr_ai_run_observation(
    conn: &Connection,
    ai_run_id: i64,
    idempotency_key: &str,
) -> CoreResult<ObjectiveObservationResult> {
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    if run.run_type != "omr"
        || run.source_module != "exam"
        || run.business_ref_type != "answer_region_revision"
    {
        return Err(CoreError::Invalid("AI run 不是 M2 客观题 OMR run".into()));
    }
    let region_id = run
        .business_ref_id
        .parse::<i64>()
        .map_err(|_| CoreError::Invalid("OMR run 题区引用不是有效整数".into()))?;
    match run.status {
        AiRunStatus::Succeeded => {
            let output_json = run
                .output_json
                .as_deref()
                .ok_or_else(|| CoreError::Invalid("成功 OMR run 缺少结构化输出".into()))?;
            let output: ObjectiveRecognitionOutput = serde_json::from_str(output_json)
                .map_err(|error| CoreError::Invalid(format!("OMR run 输出无效：{error}")))?;
            output.validate()?;
            let observed_answer_json = output.observed_answer_json()?;
            record_objective_observation(
                conn,
                &NewObjectiveObservation {
                    answer_region_revision_id: region_id,
                    source_kind: "omr",
                    result_state: output.result_state.as_str(),
                    observed_answer_json: observed_answer_json.as_deref(),
                    confidence: output.confidence,
                    ai_run_id: Some(ai_run_id),
                    failure_meta_json: None,
                    idempotency_key,
                },
            )
        }
        AiRunStatus::Failed => {
            let failure_meta_json = run
                .error_meta_json
                .as_deref()
                .ok_or_else(|| CoreError::Invalid("失败 OMR run 缺少错误元数据".into()))?;
            record_objective_observation(
                conn,
                &NewObjectiveObservation {
                    answer_region_revision_id: region_id,
                    source_kind: "omr",
                    result_state: "failed",
                    observed_answer_json: None,
                    confidence: None,
                    ai_run_id: Some(ai_run_id),
                    failure_meta_json: Some(failure_meta_json),
                    idempotency_key,
                },
            )
        }
        _ => Err(CoreError::Invalid(
            "只有 succeeded/failed 的 OMR run 可以写 observation".into(),
        )),
    }
}

pub fn record_objective_observation(
    conn: &Connection,
    input: &NewObjectiveObservation<'_>,
) -> CoreResult<ObjectiveObservationResult> {
    let observed = validate_observation_input(input)?;
    let scope = observation_scope(conn, input.answer_region_revision_id)?;
    if !matches!(
        scope.question_type.as_str(),
        "single" | "multiple" | "true_false"
    ) {
        return Err(CoreError::Invalid("题区不是 T6 支持的客观题".into()));
    }
    if input.source_kind == "omr" {
        validate_omr_run_binding(conn, input, &scope, observed.as_ref())?;
    }
    if let Some(value) = observed.as_ref() {
        match scope.question_type.as_str() {
            "single" => {
                let labels = normalize_labels(value, "selected_labels")?;
                if input.result_state != "altered" && labels.len() != 1 {
                    return Err(CoreError::Invalid("单选题观察必须恰好包含一个选项".into()));
                }
            }
            "multiple" => {
                normalize_labels(value, "selected_labels")?;
            }
            "true_false" => {
                if input.result_state == "altered" {
                    if value.get("selected").and_then(Value::as_bool).is_none() {
                        let selected = value
                            .get("selected_values")
                            .and_then(Value::as_array)
                            .ok_or_else(|| {
                                CoreError::Invalid(
                                    "判断题冲突观察缺少 selected/selected_values".into(),
                                )
                            })?;
                        let values = selected
                            .iter()
                            .map(Value::as_bool)
                            .collect::<Option<Vec<_>>>()
                            .ok_or_else(|| {
                                CoreError::Invalid("判断题冲突选项必须是布尔值".into())
                            })?;
                        if values.len() != 2 || !values.contains(&true) || !values.contains(&false)
                        {
                            return Err(CoreError::Invalid(
                                "判断题冲突观察必须同时包含 true/false".into(),
                            ));
                        }
                    }
                } else {
                    value
                        .get("selected")
                        .and_then(Value::as_bool)
                        .ok_or_else(|| CoreError::Invalid("判断题观察缺少 selected".into()))?;
                }
            }
            _ => unreachable!(),
        }
    }

    let canonical = serde_json::json!({
        "schema_version": 1,
        "answer_region_revision_id": input.answer_region_revision_id,
        "source_kind": input.source_kind,
        "result_state": input.result_state,
        "observed_answer": observed,
        "confidence_micros": input.confidence.map(|value| (value * 1_000_000.0).round() as i64),
        "ai_run_id": input.ai_run_id,
        "failure_meta": input
            .failure_meta_json
            .map(|value| schema_object(value, "客观题失败元数据"))
            .transpose()?
    });
    let input_hash = hashing::sha256_hex(
        &serde_json::to_vec(&canonical)
            .map_err(|error| CoreError::Parse(format!("客观题观察 hash 失败：{error}")))?,
    );
    if let Some((id, existing_hash)) = conn
        .query_row(
            "SELECT id,input_hash FROM exam_objective_observation_revisions_v2
             WHERE idempotency_key=?1",
            [input.idempotency_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if existing_hash != input_hash {
            return Err(CoreError::Invalid("客观题观察幂等键已属于不同输入".into()));
        }
        return get_observation_result(conn, id);
    }

    let expected = schema_object(&scope.answer_json, "客观题标准答案")?;
    let correctness = if matches!(input.result_state, "recognized" | "low_confidence") {
        Some(compare_answer(
            &scope.question_type,
            &expected,
            observed.as_ref().expect("validated observed answer"),
        )?)
    } else {
        None
    };
    let (outcome, score, batch_eligible, exclusion_reason) = match correctness {
        Some(true) if input.result_state == "recognized" => {
            ("correct", Some(scope.max_score), true, None)
        }
        Some(false) if input.result_state == "recognized" => ("incorrect", Some(0.0), true, None),
        Some(true) => (
            "correct",
            Some(scope.max_score),
            false,
            Some("LOW_CONFIDENCE"),
        ),
        Some(false) => ("incorrect", Some(0.0), false, Some("LOW_CONFIDENCE")),
        None => (
            "unscored",
            None,
            false,
            Some(match input.result_state {
                "blank" => "BLANK",
                "altered" => "ALTERED",
                "failed" => "RECOGNITION_FAILED",
                _ => "UNSCORABLE",
            }),
        ),
    };
    let result_json = serde_json::json!({
        "schema_version": 1,
        "question_type": scope.question_type,
        "observation_state": input.result_state,
        "observed_answer": observed,
        "expected_answer": expected,
        "outcome": outcome,
        "suggested_score": score,
        "max_score": scope.max_score,
        "reason_code": exclusion_reason
    })
    .to_string();

    let tx = conn.unchecked_transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1
         FROM exam_objective_observation_revisions_v2
         WHERE attempt_id=?1 AND assessment_item_id=?2",
        (scope.attempt_id, scope.assessment_item_id),
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_objective_grade_suggestions_v2 SET state='superseded'
         WHERE attempt_id=?1 AND assessment_item_id=?2 AND state='active'",
        (scope.attempt_id, scope.assessment_item_id),
    )?;
    tx.execute(
        "UPDATE exam_objective_observation_revisions_v2 SET state='superseded'
         WHERE attempt_id=?1 AND assessment_item_id=?2 AND state='active'",
        (scope.attempt_id, scope.assessment_item_id),
    )?;
    let observation_public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    tx.execute(
        "INSERT INTO exam_objective_observation_revisions_v2
         (public_id,idempotency_key,input_hash,attempt_id,assessment_item_id,
          answer_region_revision_id,revision,source_kind,question_type,result_state,
          observed_answer_json,confidence,alteration_detected,ai_run_id,failure_meta_json,
          state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,'active',?16)",
        (
            &observation_public_id,
            input.idempotency_key.trim(),
            &input_hash,
            scope.attempt_id,
            scope.assessment_item_id,
            input.answer_region_revision_id,
            revision,
            input.source_kind,
            &scope.question_type,
            input.result_state,
            input.observed_answer_json,
            input.confidence,
            input.result_state == "altered",
            input.ai_run_id,
            input.failure_meta_json,
            &now,
        ),
    )?;
    let observation_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO exam_objective_grade_suggestions_v2
         (public_id,observation_revision_id,attempt_id,assessment_item_id,
          answer_key_version_id,outcome,suggested_score,result_json,batch_eligible,
          exclusion_reason,state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11)",
        (
            ids::new_public_id(),
            observation_id,
            scope.attempt_id,
            scope.assessment_item_id,
            scope.answer_key_version_id,
            outcome,
            score,
            &result_json,
            batch_eligible,
            exclusion_reason,
            &now,
        ),
    )?;
    let result = get_observation_result(&tx, observation_id)?;
    tx.commit()?;
    Ok(result)
}

fn review_scope(conn: &Connection, suggestion_id: i64) -> CoreResult<ReviewScope> {
    conn.query_row(
        "SELECT i.assessment_version_id,
                s.id,s.public_id,s.observation_revision_id,s.attempt_id,
                s.assessment_item_id,s.answer_key_version_id,s.outcome,s.suggested_score,
                s.result_json,s.batch_eligible,s.exclusion_reason,s.state,
                o.state,o.result_state,o.confidence,r.state,r.decision,i.score
         FROM exam_objective_grade_suggestions_v2 s
         JOIN exam_objective_observation_revisions_v2 o ON o.id=s.observation_revision_id
         JOIN exam_answer_region_revisions_v2 r ON r.id=o.answer_region_revision_id
         JOIN exam_assessment_items_v2 i ON i.id=s.assessment_item_id
         WHERE s.id=?1",
        [suggestion_id],
        |row| {
            Ok(ReviewScope {
                assessment_version_id: row.get(0)?,
                max_score: row.get(18)?,
                suggestion: ObjectiveGradeSuggestion {
                    id: row.get(1)?,
                    public_id: row.get(2)?,
                    observation_revision_id: row.get(3)?,
                    attempt_id: row.get(4)?,
                    assessment_item_id: row.get(5)?,
                    answer_key_version_id: row.get(6)?,
                    outcome: row.get(7)?,
                    suggested_score: row.get(8)?,
                    result_json: row.get(9)?,
                    batch_eligible: row.get(10)?,
                    exclusion_reason: row.get(11)?,
                    state: row.get(12)?,
                },
                observation_state: row.get(13)?,
                observation_result: row.get(14)?,
                confidence: row.get(15)?,
                region_state: row.get(16)?,
                region_decision: row.get(17)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound(format!("objective_suggestion#{suggestion_id}")))
}

fn active_review_reason(scope: &ReviewScope, threshold: Option<f64>) -> Option<String> {
    if scope.suggestion.state != "active" || scope.observation_state != "active" {
        return Some("SUPERSEDED_SUGGESTION".into());
    }
    if scope.region_state != "active" || scope.region_decision != "teacher_confirmed" {
        return Some("REGION_NO_LONGER_CONFIRMED".into());
    }
    if scope.suggestion.outcome == "unscored" || scope.suggestion.suggested_score.is_none() {
        return Some(
            match scope.observation_result.as_str() {
                "blank" => "BLANK",
                "altered" => "ALTERED",
                "failed" => "RECOGNITION_FAILED",
                _ => "UNSCORABLE",
            }
            .into(),
        );
    }
    if let Some(threshold) = threshold {
        if scope.observation_result != "recognized" || !scope.suggestion.batch_eligible {
            return Some(
                scope
                    .suggestion
                    .exclusion_reason
                    .as_deref()
                    .unwrap_or("NOT_BATCH_ELIGIBLE")
                    .into(),
            );
        }
        if scope.confidence.is_none_or(|value| value < threshold) {
            return Some("BELOW_BATCH_THRESHOLD".into());
        }
    }
    None
}

fn link_decision_source(
    conn: &Connection,
    decision_id: i64,
    suggestion_id: i64,
    review_mode: &str,
    batch_id: Option<i64>,
    reviewed_by: &str,
    now: &str,
) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO exam_grade_decision_objective_sources_v2
         (grade_decision_id,suggestion_id,review_mode,batch_id,reviewed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(grade_decision_id) DO NOTHING",
        (
            decision_id,
            suggestion_id,
            review_mode,
            batch_id,
            reviewed_by,
            now,
        ),
    )?;
    let existing: (i64, String, Option<i64>, String) = conn.query_row(
        "SELECT suggestion_id,review_mode,batch_id,reviewed_by
         FROM exam_grade_decision_objective_sources_v2 WHERE grade_decision_id=?1",
        [decision_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if existing
        != (
            suggestion_id,
            review_mode.to_owned(),
            batch_id,
            reviewed_by.to_owned(),
        )
    {
        return Err(CoreError::Invalid(
            "评分 revision 已绑定不同的客观题建议来源".into(),
        ));
    }
    Ok(())
}

pub fn accept_objective_suggestion(
    conn: &Connection,
    suggestion_id: i64,
    reviewed_by: &str,
) -> CoreResult<GradeDecision> {
    required(reviewed_by, "客观题终审人")?;
    let tx = conn.unchecked_transaction()?;
    let scope = review_scope(&tx, suggestion_id)?;
    if let Some(reason) = active_review_reason(&scope, None) {
        return Err(CoreError::Invalid(format!(
            "该机器建议不能直接接受：{reason}"
        )));
    }
    let decision = decide_grade_in_transaction(
        &tx,
        &NewGradeDecision {
            attempt_id: scope.suggestion.attempt_id,
            assessment_item_id: scope.suggestion.assessment_item_id,
            machine_grade_ai_run_id: None,
            teacher_score: scope.suggestion.suggested_score.expect("validated score"),
            point_results_json: &scope.suggestion.result_json,
            teacher_note: None,
            confirmation_level: "teacher_accepted",
            decided_by: reviewed_by,
        },
    )?;
    let now = time::utc_now_rfc3339();
    link_decision_source(
        &tx,
        decision.id,
        suggestion_id,
        "single",
        None,
        reviewed_by.trim(),
        &now,
    )?;
    tx.commit()?;
    Ok(decision)
}

/// 老师查看原始题区后对异常或机器建议进行人工记分。
/// 结果写入新的 `teacher_corrected` revision，并继续保留具体 suggestion 来源。
pub fn correct_objective_suggestion(
    conn: &Connection,
    suggestion_id: i64,
    teacher_score: f64,
    teacher_note: Option<&str>,
    reviewed_by: &str,
) -> CoreResult<GradeDecision> {
    required(reviewed_by, "客观题终审人")?;
    let teacher_note = teacher_note
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CoreError::Invalid("人工记分必须填写证据依据".into()))?;
    let tx = conn.unchecked_transaction()?;
    let scope = review_scope(&tx, suggestion_id)?;
    if scope.suggestion.state != "active" || scope.observation_state != "active" {
        return Err(CoreError::Invalid("客观题建议已被后续观察替代".into()));
    }
    if scope.region_state != "active" || scope.region_decision != "teacher_confirmed" {
        return Err(CoreError::Invalid(
            "答案区域已变化，请先重新核对页面证据".into(),
        ));
    }
    if !teacher_score.is_finite()
        || teacher_score < 0.0
        || teacher_score > scope.max_score + 0.000_001
    {
        return Err(CoreError::Invalid(format!(
            "人工得分必须位于 0~{} 分",
            scope.max_score
        )));
    }
    let machine_result: Value = serde_json::from_str(&scope.suggestion.result_json)
        .map_err(|error| CoreError::Parse(format!("客观题机器建议 JSON 无效：{error}")))?;
    let point_results = serde_json::json!({
        "schema_version": 1,
        "source": "objective_teacher_correction",
        "suggestion_id": suggestion_id,
        "machine_result": machine_result,
        "teacher_score": teacher_score
    })
    .to_string();
    let decision = decide_grade_in_transaction(
        &tx,
        &NewGradeDecision {
            attempt_id: scope.suggestion.attempt_id,
            assessment_item_id: scope.suggestion.assessment_item_id,
            machine_grade_ai_run_id: None,
            teacher_score,
            point_results_json: &point_results,
            teacher_note: Some(teacher_note),
            confirmation_level: "teacher_corrected",
            decided_by: reviewed_by,
        },
    )?;
    let now = time::utc_now_rfc3339();
    link_decision_source(
        &tx,
        decision.id,
        suggestion_id,
        "single",
        None,
        reviewed_by.trim(),
        &now,
    )?;
    tx.commit()?;
    Ok(decision)
}

fn get_batch(conn: &Connection, id: i64) -> CoreResult<ObjectiveReviewBatch> {
    let mut batch = conn.query_row(
        "SELECT id,public_id,assessment_version_id,idempotency_key,confidence_threshold,
                requested_count,confirmed_count,excluded_count,created_by
         FROM exam_objective_review_batches_v2 WHERE id=?1",
        [id],
        |row| {
            Ok(ObjectiveReviewBatch {
                id: row.get(0)?,
                public_id: row.get(1)?,
                assessment_version_id: row.get(2)?,
                idempotency_key: row.get(3)?,
                confidence_threshold: row.get(4)?,
                requested_count: row.get(5)?,
                confirmed_count: row.get(6)?,
                excluded_count: row.get(7)?,
                created_by: row.get(8)?,
                items: Vec::new(),
            })
        },
    )?;
    let mut stmt = conn.prepare(
        "SELECT suggestion_id,outcome,reason_code,grade_decision_id
         FROM exam_objective_review_batch_items_v2
         WHERE batch_id=?1 ORDER BY id",
    )?;
    let rows = stmt.query_map([id], |row| {
        Ok(BatchItemResult {
            suggestion_id: row.get(0)?,
            outcome: row.get(1)?,
            reason_code: row.get(2)?,
            grade_decision_id: row.get(3)?,
        })
    })?;
    batch.items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(batch)
}

pub fn strict_batch_accept(
    conn: &Connection,
    input: &StrictBatchReview<'_>,
) -> CoreResult<ObjectiveReviewBatch> {
    required(input.reviewed_by, "批量终审人")?;
    required(input.idempotency_key, "批量终审幂等键")?;
    if !(DEFAULT_STRICT_BATCH_CONFIDENCE..=1.0).contains(&input.confidence_threshold) {
        return Err(CoreError::Invalid("严格批量确认阈值不得低于 0.95".into()));
    }
    let suggestion_ids: Vec<i64> = input
        .suggestion_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if suggestion_ids.is_empty() {
        return Err(CoreError::Invalid("严格批量确认至少需要一条建议".into()));
    }
    let request_value = serde_json::json!({
        "schema_version": 1,
        "suggestion_ids": suggestion_ids,
        "confidence_micros": (input.confidence_threshold * 1_000_000.0).round() as i64,
        "reviewed_by": input.reviewed_by.trim()
    });
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&request_value)
            .map_err(|error| CoreError::Parse(format!("批量终审 hash 失败：{error}")))?,
    );
    if let Some((id, existing_hash)) = conn
        .query_row(
            "SELECT id,request_hash FROM exam_objective_review_batches_v2
             WHERE idempotency_key=?1",
            [input.idempotency_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid("批量终审幂等键已属于不同请求".into()));
        }
        return get_batch(conn, id);
    }

    let tx = conn.unchecked_transaction()?;
    let mut scopes = Vec::with_capacity(suggestion_ids.len());
    let mut assessment_version_id = None;
    for suggestion_id in &suggestion_ids {
        let scope = review_scope(&tx, *suggestion_id)?;
        match assessment_version_id {
            Some(id) if id != scope.assessment_version_id => {
                return Err(CoreError::Invalid(
                    "一次严格批量终审只能处理同一作业版本".into(),
                ))
            }
            None => assessment_version_id = Some(scope.assessment_version_id),
            _ => {}
        }
        scopes.push(scope);
    }
    let mut reasons = Vec::with_capacity(scopes.len());
    for scope in &scopes {
        let already_confirmed: bool = tx.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM exam_grade_decision_objective_sources_v2 src
               JOIN exam_grade_decisions_v2 d ON d.id=src.grade_decision_id
               WHERE src.suggestion_id=?1 AND d.state='active'
             )",
            [scope.suggestion.id],
            |row| row.get(0),
        )?;
        reasons.push(if already_confirmed {
            Some("ALREADY_CONFIRMED".into())
        } else {
            active_review_reason(scope, Some(input.confidence_threshold))
        });
    }
    let confirmed_count = reasons.iter().filter(|reason| reason.is_none()).count() as i64;
    let excluded_count = reasons.len() as i64 - confirmed_count;
    let now = time::utc_now_rfc3339();
    tx.execute(
        "INSERT INTO exam_objective_review_batches_v2
         (public_id,assessment_version_id,idempotency_key,request_hash,
          confidence_threshold,requested_count,confirmed_count,excluded_count,
          state,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'completed',?9,?10)",
        (
            ids::new_public_id(),
            assessment_version_id.expect("non-empty scopes"),
            input.idempotency_key.trim(),
            &request_hash,
            input.confidence_threshold,
            scopes.len() as i64,
            confirmed_count,
            excluded_count,
            input.reviewed_by.trim(),
            &now,
        ),
    )?;
    let batch_id = tx.last_insert_rowid();
    for (scope, reason) in scopes.iter().zip(reasons) {
        let decision = if reason.is_none() {
            let decision = decide_grade_in_transaction(
                &tx,
                &NewGradeDecision {
                    attempt_id: scope.suggestion.attempt_id,
                    assessment_item_id: scope.suggestion.assessment_item_id,
                    machine_grade_ai_run_id: None,
                    teacher_score: scope.suggestion.suggested_score.expect("validated score"),
                    point_results_json: &scope.suggestion.result_json,
                    teacher_note: None,
                    confirmation_level: "teacher_accepted",
                    decided_by: input.reviewed_by,
                },
            )?;
            link_decision_source(
                &tx,
                decision.id,
                scope.suggestion.id,
                "strict_batch",
                Some(batch_id),
                input.reviewed_by.trim(),
                &now,
            )?;
            Some(decision)
        } else {
            None
        };
        tx.execute(
            "INSERT INTO exam_objective_review_batch_items_v2
             (batch_id,suggestion_id,outcome,reason_code,grade_decision_id,created_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
            (
                batch_id,
                scope.suggestion.id,
                if decision.is_some() {
                    "confirmed"
                } else {
                    "excluded"
                },
                reason,
                decision.as_ref().map(|value| value.id),
                &now,
            ),
        )?;
    }
    let batch = get_batch(&tx, batch_id)?;
    tx.commit()?;
    Ok(batch)
}

#[cfg(test)]
#[path = "objective_tests.rs"]
mod tests;
