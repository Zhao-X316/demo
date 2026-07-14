//! T6 客观题确定性观察与老师终审。
//!
//! 固定 fixture/OMR 只写 observation 和 machine suggestion；只有老师显式接受单题，
//! 或执行满足严格门槛的批量终审，才会调用 B0 的追加式 grade decision。

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};

use super::assessment::{decide_grade_in_transaction, GradeDecision, NewGradeDecision};

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
    let limit = limit.clamp(1, 1000);
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
         ORDER BY v.id DESC,i.order_index,st.student_no,at.attempt_no
         LIMIT ?2",
    )?;
    let rows = row_stmt
        .query_map((assessment_version_id, limit), |row| {
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
         ORDER BY v.id DESC,st.student_no,at.attempt_no
         LIMIT ?2",
    )?;
    let attempts = attempt_stmt
        .query_map((assessment_version_id, limit), |row| {
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
        "SELECT m.attempt_id, r.assessment_item_id, q.question_type,
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
                question_type: row.get(2)?,
                answer_key_version_id: row.get(3)?,
                answer_json: row.get(4)?,
                max_score: row.get(5)?,
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
    if let Some(run_id) = input.ai_run_id {
        let valid: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM ai_runs
             WHERE id=?1 AND run_type='omr' AND status='succeeded')",
            [run_id],
            |row| row.get(0),
        )?;
        if !valid {
            return Err(CoreError::Invalid("OMR run 尚未成功或类型不符".into()));
        }
    }
    if let Some(value) = observed.as_ref() {
        match scope.question_type.as_str() {
            "single" => {
                let labels = normalize_labels(value, "selected_labels")?;
                if labels.len() != 1 {
                    return Err(CoreError::Invalid("单选题观察必须恰好包含一个选项".into()));
                }
            }
            "multiple" => {
                normalize_labels(value, "selected_labels")?;
            }
            "true_false" => {
                value
                    .get("selected")
                    .and_then(Value::as_bool)
                    .ok_or_else(|| CoreError::Invalid("判断题观察缺少 selected".into()))?;
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
mod tests {
    use super::*;
    use suite_core::db::repo::artifacts::{create_or_get, NewArtifact};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

    struct Fixture {
        conn: Connection,
        region_one: i64,
        region_two: i64,
        attempt_one: i64,
        attempt_two: i64,
    }

    fn artifact(
        conn: &Connection,
        kind: ArtifactKind,
        digit: char,
        parent_artifact_id: Option<i64>,
        derivative_type: Option<&str>,
    ) -> i64 {
        create_or_get(
            conn,
            &NewArtifact {
                kind,
                sha256: &digit.to_string().repeat(64),
                mime_type: "image/png",
                byte_size: 128,
                original_name: None,
                original_path: None,
                archived_path: &format!("/fixture/objective-{digit}.png"),
                parent_artifact_id,
                derivative_type,
                processing_version: "objective-fixture-v1",
                privacy_class: PrivacyClass::StudentSensitive,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap()
        .id
    }

    fn add_page_chain(conn: &Connection, attempt_id: i64, index: i64, digit: char) -> i64 {
        let page_artifact = artifact(conn, ArtifactKind::Page, digit, None, None);
        let aligned_artifact = artifact(
            conn,
            ArtifactKind::Page,
            char::from_u32(digit as u32 + 1).unwrap(),
            Some(page_artifact),
            Some("page_alignment"),
        );
        let crop_artifact = artifact(
            conn,
            ArtifactKind::Crop,
            char::from_u32(digit as u32 + 2).unwrap(),
            Some(aligned_artifact),
            Some("answer_region"),
        );
        let now = "2026-07-14T08:00:00.000Z";
        conn.execute(
            "INSERT INTO exam_ingest_batches_v2
             (public_id,assessment_version_id,source_kind,idempotency_key,state,
              created_by,created_at,updated_at)
             VALUES (?1,1,'fixed_fixture',?2,'ready','teacher',?3,?3)",
            (
                format!("objective-batch-{index}"),
                format!("batch-{index}"),
                now,
            ),
        )
        .unwrap();
        let batch_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO exam_ingest_pages_v2
             (public_id,batch_id,source_artifact_id,import_index,expected_page_no,state,
              created_at,updated_at)
             VALUES (?1,?2,?3,0,1,'segmented',?4,?4)",
            (
                format!("objective-page-{index}"),
                batch_id,
                page_artifact,
                now,
            ),
        )
        .unwrap();
        let page_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO exam_page_match_revisions_v2
             (public_id,page_id,revision,attempt_id,page_no,student_confidence,
              page_no_confidence,template_confidence,decision,confirmed_by,state,created_at)
             VALUES (?1,?2,1,?3,1,0.99,0.99,0.99,'teacher_confirmed','teacher','active',?4)",
            (format!("objective-match-{index}"), page_id, attempt_id, now),
        )
        .unwrap();
        let match_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO exam_page_alignment_revisions_v2
             (public_id,page_id,revision,match_revision_id,template_version,transform_json,
              confidence,aligned_artifact_id,decision,confirmed_by,state,created_at)
             VALUES (?1,?2,1,?3,'fixture-v1',
                     '{\"schema_version\":1,\"matrix\":[1,0,0,1,0,0]}',0.99,?4,
                     'teacher_confirmed','teacher','active',?5)",
            (
                format!("objective-alignment-{index}"),
                page_id,
                match_id,
                aligned_artifact,
                now,
            ),
        )
        .unwrap();
        let alignment_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO exam_answer_region_revisions_v2
             (public_id,page_id,assessment_item_id,region_index,revision,
              alignment_revision_id,bbox_json,crop_artifact_id,mapping_confidence,
              decision,confirmed_by,state,created_at)
             VALUES (?1,?2,1,0,1,?3,
                     '{\"schema_version\":1,\"x\":0.1,\"y\":0.1,\"width\":0.2,\"height\":0.1}',
                     ?4,0.99,'teacher_confirmed','teacher','active',?5)",
            (
                format!("objective-region-{index}"),
                page_id,
                alignment_id,
                crop_artifact,
                now,
            ),
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn setup() -> Fixture {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        let hash = "a".repeat(64);
        conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO students(student_no,name,class_id) VALUES ('S001','小林',1);
               INSERT INTO students(student_no,name,class_id) VALUES ('S002','小周',1);
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('objective-edition',1,'PEP','2024','八上','8','upper','active',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('objective-map',1,1,'confirmed','2026-07-14T08:00:00.000Z',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('objective-question','personal','teacher','unknown',0,
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('objective-question-v1',1,1,'true_false','鸦片战争爆发于1840年。',1,
                         '{hash}','L2','published','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('objective-answer-v1',1,1,'{{"schema_version":1,"correct":true}}',
                         'confirmed','2026-07-14T08:00:00.000Z','teacher',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('objective-rubric-v1',1,1,1,'confirmed',
                         '2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('objective-link-set',1,1,1,'confirmed',
                         '2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,
                  created_by,created_at,updated_at)
                 VALUES ('objective-assessment','判断题测验',1,'quiz','include','active','teacher',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('objective-assessment-v1',1,1,'{hash}','confirmed',
                         '2026-07-14T08:00:00.000Z','teacher','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
                 VALUES ('objective-item',1,1,1,1,1,0,1,
                         '{{"schema_version":1,"question_no":"1"}}','active',
                         '2026-07-14T08:00:00.000Z');
               INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES ('objective-attempt-1',1,1,1,'image','first','ingesting',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');
               INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES ('objective-attempt-2',1,2,1,'image','first','ingesting',
                         '2026-07-14T08:00:00.000Z','2026-07-14T08:00:00.000Z');"#
        ))
        .unwrap();
        let region_one = add_page_chain(&conn, 1, 1, '1');
        let region_two = add_page_chain(&conn, 2, 2, '4');
        Fixture {
            conn,
            region_one,
            region_two,
            attempt_one: 1,
            attempt_two: 2,
        }
    }

    fn observed<'a>(region_id: i64, key: &'a str, confidence: f64) -> NewObjectiveObservation<'a> {
        NewObjectiveObservation {
            answer_region_revision_id: region_id,
            source_kind: "fixed_fixture",
            result_state: if confidence >= DEFAULT_STRICT_BATCH_CONFIDENCE {
                "recognized"
            } else {
                "low_confidence"
            },
            observed_answer_json: Some(r#"{"schema_version":1,"selected":true}"#),
            confidence: Some(confidence),
            ai_run_id: None,
            failure_meta_json: None,
            idempotency_key: key,
        }
    }

    #[test]
    fn observation_is_idempotent_and_never_confirms_or_publishes_by_itself() {
        let fixture = setup();
        let first = record_objective_observation(
            &fixture.conn,
            &observed(fixture.region_one, "observe-one", 0.99),
        )
        .unwrap();
        let repeated = record_objective_observation(
            &fixture.conn,
            &observed(fixture.region_one, "observe-one", 0.99),
        )
        .unwrap();
        assert_eq!(first.observation.id, repeated.observation.id);
        assert_eq!(first.suggestion.outcome, "correct");
        assert!(first.suggestion.batch_eligible);
        let before: (i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(before, (0, 0));

        let decision =
            accept_objective_suggestion(&fixture.conn, first.suggestion.id, "teacher").unwrap();
        let repeated_decision =
            accept_objective_suggestion(&fixture.conn, first.suggestion.id, "teacher").unwrap();
        assert_eq!(decision.id, repeated_decision.id);
        assert_eq!(decision.teacher_score, 1.0);
        let publication_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_grade_publications_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(publication_count, 0, "老师单题确认后仍需显式发布");
    }

    #[test]
    fn teacher_correction_completes_an_unscored_exception_idempotently() {
        let fixture = setup();
        let blank = record_objective_observation(
            &fixture.conn,
            &NewObjectiveObservation {
                answer_region_revision_id: fixture.region_one,
                source_kind: "fixed_fixture",
                result_state: "blank",
                observed_answer_json: None,
                confidence: None,
                ai_run_id: None,
                failure_meta_json: None,
                idempotency_key: "teacher-correct-blank",
            },
        )
        .unwrap();
        assert_eq!(blank.suggestion.outcome, "unscored");

        let first = correct_objective_suggestion(
            &fixture.conn,
            blank.suggestion.id,
            0.5,
            Some("已查看原始题区，按部分作答记分"),
            "teacher",
        )
        .unwrap();
        let repeated = correct_objective_suggestion(
            &fixture.conn,
            blank.suggestion.id,
            0.5,
            Some("已查看原始题区，按部分作答记分"),
            "teacher",
        )
        .unwrap();
        assert_eq!(first.id, repeated.id);
        assert_eq!(first.teacher_score, 0.5);
        assert_eq!(first.confirmation_level, "teacher_corrected");

        let workbench = list_objective_workbench(&fixture.conn, Some(1), 100).unwrap();
        assert!(workbench.rows[0].current_suggestion_confirmed);
        assert_eq!(workbench.rows[0].teacher_score, Some(0.5));
        assert_eq!(
            workbench.rows[0].confirmation_level.as_deref(),
            Some("teacher_corrected")
        );
        assert!(workbench.attempts[0].can_publish);
    }

    #[test]
    fn strict_batch_confirms_only_high_confidence_and_records_exclusions() {
        let fixture = setup();
        let high = record_objective_observation(
            &fixture.conn,
            &observed(fixture.region_one, "observe-high", 0.99),
        )
        .unwrap();
        let low = record_objective_observation(
            &fixture.conn,
            &observed(fixture.region_two, "observe-low", 0.72),
        )
        .unwrap();
        assert_eq!(low.suggestion.outcome, "correct");
        assert!(!low.suggestion.batch_eligible);
        let review = StrictBatchReview {
            suggestion_ids: &[high.suggestion.id, low.suggestion.id],
            confidence_threshold: 0.95,
            reviewed_by: "teacher",
            idempotency_key: "strict-review-1",
        };
        let batch = strict_batch_accept(&fixture.conn, &review).unwrap();
        let repeated = strict_batch_accept(&fixture.conn, &review).unwrap();
        assert_eq!(batch.id, repeated.id);
        assert_eq!(batch.confirmed_count, 1);
        assert_eq!(batch.excluded_count, 1);
        assert_eq!(batch.items[0].outcome, "confirmed");
        assert_eq!(
            batch.items[1].reason_code.as_deref(),
            Some("LOW_CONFIDENCE")
        );
        let states: (String, String) = fixture
            .conn
            .query_row(
                "SELECT (SELECT state FROM exam_attempts_v2 WHERE id=?1),
                        (SELECT state FROM exam_attempts_v2 WHERE id=?2)",
                (fixture.attempt_one, fixture.attempt_two),
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(states.0, "ready_to_publish");
        assert_eq!(states.1, "ingesting");
        let publication_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_grade_publications_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(publication_count, 0);
    }

    #[test]
    fn blank_and_altered_are_unscored_and_atomic_batch_failure_leaves_no_half_state() {
        let fixture = setup();
        let blank = record_objective_observation(
            &fixture.conn,
            &NewObjectiveObservation {
                answer_region_revision_id: fixture.region_one,
                source_kind: "fixed_fixture",
                result_state: "blank",
                observed_answer_json: None,
                confidence: Some(0.99),
                ai_run_id: None,
                failure_meta_json: None,
                idempotency_key: "blank-one",
            },
        )
        .unwrap();
        assert_eq!(blank.suggestion.outcome, "unscored");
        assert!(
            accept_objective_suggestion(&fixture.conn, blank.suggestion.id, "teacher").is_err()
        );
        let altered = record_objective_observation(
            &fixture.conn,
            &NewObjectiveObservation {
                answer_region_revision_id: fixture.region_two,
                source_kind: "fixed_fixture",
                result_state: "altered",
                observed_answer_json: Some(r#"{"schema_version":1,"selected":true}"#),
                confidence: Some(0.98),
                ai_run_id: None,
                failure_meta_json: None,
                idempotency_key: "altered-two",
            },
        )
        .unwrap();
        assert_eq!(
            altered.suggestion.exclusion_reason.as_deref(),
            Some("ALTERED")
        );

        let fixture = setup();
        let one = record_objective_observation(
            &fixture.conn,
            &observed(fixture.region_one, "atomic-one", 0.99),
        )
        .unwrap();
        let two = record_objective_observation(
            &fixture.conn,
            &observed(fixture.region_two, "atomic-two", 0.99),
        )
        .unwrap();
        fixture
            .conn
            .execute_batch(&format!(
                "CREATE TRIGGER fail_second_batch_item
                 BEFORE INSERT ON exam_objective_review_batch_items_v2
                 WHEN NEW.suggestion_id={}
                 BEGIN SELECT RAISE(ABORT, 'injected batch failure'); END;",
                two.suggestion.id
            ))
            .unwrap();
        assert!(strict_batch_accept(
            &fixture.conn,
            &StrictBatchReview {
                suggestion_ids: &[one.suggestion.id, two.suggestion.id],
                confidence_threshold: 0.95,
                reviewed_by: "teacher",
                idempotency_key: "atomic-review",
            },
        )
        .is_err());
        let counts: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_objective_review_batches_v2),
                        (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_decision_objective_sources_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0, 0));
    }

    #[test]
    fn workbench_read_model_tracks_review_totals_and_explicit_publication() {
        let fixture = setup();
        let high = record_objective_observation(
            &fixture.conn,
            &observed(fixture.region_one, "workbench-high", 0.99),
        )
        .unwrap();
        record_objective_observation(
            &fixture.conn,
            &observed(fixture.region_two, "workbench-low", 0.72),
        )
        .unwrap();

        let initial = list_objective_workbench(&fixture.conn, None, 100).unwrap();
        assert_eq!(initial.rows.len(), 2);
        assert_eq!(initial.attempts.len(), 2);
        assert_eq!(initial.rows[0].question_no, "1");
        assert_eq!(initial.rows[0].observation_state, "recognized");
        assert!(!initial.rows[0].current_suggestion_confirmed);
        assert_eq!(
            initial.rows[1].exclusion_reason.as_deref(),
            Some("LOW_CONFIDENCE")
        );

        accept_objective_suggestion(&fixture.conn, high.suggestion.id, "teacher").unwrap();
        let reviewed = list_objective_workbench(&fixture.conn, Some(1), 100).unwrap();
        let reviewed_row = reviewed
            .rows
            .iter()
            .find(|row| row.suggestion_id == high.suggestion.id)
            .unwrap();
        assert!(reviewed_row.current_suggestion_confirmed);
        assert_eq!(reviewed_row.teacher_score, Some(1.0));
        let reviewed_attempt = reviewed
            .attempts
            .iter()
            .find(|attempt| attempt.attempt_id == fixture.attempt_one)
            .unwrap();
        assert_eq!(reviewed_attempt.confirmed_count, 1);
        assert_eq!(reviewed_attempt.teacher_total_score, 1.0);
        assert!(reviewed_attempt.can_publish);
        assert_eq!(reviewed_attempt.published_total_score, None);

        super::super::assessment::publish_attempt(&fixture.conn, fixture.attempt_one, "teacher")
            .unwrap();
        let published = list_objective_workbench(&fixture.conn, Some(1), 100).unwrap();
        let published_attempt = published
            .attempts
            .iter()
            .find(|attempt| attempt.attempt_id == fixture.attempt_one)
            .unwrap();
        assert_eq!(published_attempt.attempt_state, "published");
        assert!(!published_attempt.can_publish);
        assert_eq!(published_attempt.published_total_score, Some(1.0));
    }
}
