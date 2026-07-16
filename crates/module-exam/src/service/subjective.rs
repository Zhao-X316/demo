//! 答题卡填空/简答题区的追加式手写 OCR 转写。
//!
//! 本层只保存“学生实际写了什么”，不加载标准答案、不判分。后续机器评分和老师终审
//! 必须引用这里的具体 active revision，避免答案反向污染 OCR 原文。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::{ai_runs, audit};
use suite_core::domain::{ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, AuditActorType};

use crate::dictation::DictationRecognitionState;
use crate::dictation_recognition::DictationOcrOutput;

use super::assessment::{decide_grade_in_transaction, GradeDecision, NewGradeDecision};
use super::objective::ObjectiveAttemptSummary;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveTranscriptionRevision {
    pub id: i64,
    pub public_id: String,
    pub attempt_id: i64,
    pub assessment_item_id: i64,
    pub answer_region_revision_id: i64,
    pub question_type: String,
    pub revision: i64,
    pub source_ai_run_id: Option<i64>,
    pub result_state: String,
    pub raw_ocr_text: Option<String>,
    pub normalized_text: Option<String>,
    pub teacher_corrected_text: Option<String>,
    pub confidence: Option<f64>,
    pub failure_meta_json: Option<String>,
    pub corrected_by: Option<String>,
    pub corrected_at: Option<String>,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubjectiveRegionScope {
    pub attempt_id: i64,
    pub assessment_item_id: i64,
    pub answer_region_revision_id: i64,
    pub question_type: String,
    pub crop_artifact_id: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveGradeSuggestion {
    pub id: i64,
    pub public_id: String,
    pub transcription_revision_id: i64,
    pub attempt_id: i64,
    pub assessment_item_id: i64,
    pub answer_key_version_id: i64,
    pub rubric_version_id: i64,
    pub machine_grade_ai_run_id: Option<i64>,
    pub outcome: String,
    pub suggested_score: Option<f64>,
    pub result_json: String,
    pub confidence: Option<f64>,
    pub batch_eligible: bool,
    pub exclusion_reason: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveWorkbenchRow {
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
    pub question_no: String,
    pub question_type: String,
    pub question_stem: String,
    pub max_score: f64,
    pub answer_region_revision_id: i64,
    pub crop_path: Option<String>,
    pub transcription_revision_id: i64,
    pub transcription_revision: i64,
    pub result_state: String,
    pub raw_ocr_text: Option<String>,
    pub normalized_text: Option<String>,
    pub teacher_corrected_text: Option<String>,
    pub confidence: Option<f64>,
    pub answer_json: String,
    pub rubric_points_json: String,
    pub suggestion_id: i64,
    pub suggestion_outcome: String,
    pub suggested_score: Option<f64>,
    pub suggestion_result_json: String,
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
pub struct SubjectiveWorkbench {
    pub rows: Vec<SubjectiveWorkbenchRow>,
    pub attempts: Vec<ObjectiveAttemptSummary>,
}

#[derive(Debug)]
struct SuggestionScope {
    transcription: SubjectiveTranscriptionRevision,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    answer_json: String,
    max_score: f64,
}

#[derive(Debug)]
struct ReviewScope {
    suggestion: SubjectiveGradeSuggestion,
    max_score: f64,
    transcription_state: String,
    region_state: String,
    region_decision: String,
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("主观题{field}不能为空")))
    } else {
        Ok(())
    }
}

fn row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SubjectiveTranscriptionRevision> {
    Ok(SubjectiveTranscriptionRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        attempt_id: row.get(2)?,
        assessment_item_id: row.get(3)?,
        answer_region_revision_id: row.get(4)?,
        question_type: row.get(5)?,
        revision: row.get(6)?,
        source_ai_run_id: row.get(7)?,
        result_state: row.get(8)?,
        raw_ocr_text: row.get(9)?,
        normalized_text: row.get(10)?,
        teacher_corrected_text: row.get(11)?,
        confidence: row.get(12)?,
        failure_meta_json: row.get(13)?,
        corrected_by: row.get(14)?,
        corrected_at: row.get(15)?,
        state: row.get(16)?,
        created_at: row.get(17)?,
    })
}

fn suggestion_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SubjectiveGradeSuggestion> {
    Ok(SubjectiveGradeSuggestion {
        id: row.get(0)?,
        public_id: row.get(1)?,
        transcription_revision_id: row.get(2)?,
        attempt_id: row.get(3)?,
        assessment_item_id: row.get(4)?,
        answer_key_version_id: row.get(5)?,
        rubric_version_id: row.get(6)?,
        machine_grade_ai_run_id: row.get(7)?,
        outcome: row.get(8)?,
        suggested_score: row.get(9)?,
        result_json: row.get(10)?,
        confidence: row.get(11)?,
        batch_eligible: row.get(12)?,
        exclusion_reason: row.get(13)?,
        state: row.get(14)?,
    })
}

pub fn get_subjective_suggestion(
    conn: &Connection,
    id: i64,
) -> CoreResult<Option<SubjectiveGradeSuggestion>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,transcription_revision_id,attempt_id,
                    assessment_item_id,answer_key_version_id,rubric_version_id,
                    machine_grade_ai_run_id,outcome,suggested_score,result_json,
                    confidence,batch_eligible,exclusion_reason,state
             FROM exam_subjective_grade_suggestions_v2 WHERE id=?1",
            [id],
            suggestion_row,
        )
        .optional()?)
}

pub fn load_region_scope(
    conn: &Connection,
    answer_region_revision_id: i64,
) -> CoreResult<SubjectiveRegionScope> {
    if answer_region_revision_id <= 0 {
        return Err(CoreError::Invalid("主观题区 id 必须为正数".into()));
    }
    conn.query_row(
        "SELECT attempt.id,region.assessment_item_id,region.id,
                route.question_type,region.crop_artifact_id
         FROM exam_answer_sheet_region_routes_v2 route
         JOIN exam_answer_sheet_page_materializations_v2 materialization
           ON materialization.id=route.materialization_id
         JOIN exam_answer_region_revisions_v2 region
           ON region.id=route.answer_region_revision_id
          AND region.state='active' AND region.decision='teacher_confirmed'
         JOIN exam_page_alignment_revisions_v2 alignment
           ON alignment.id=region.alignment_revision_id
          AND alignment.id=materialization.alignment_revision_id
          AND alignment.state='active' AND alignment.decision='teacher_confirmed'
         JOIN exam_page_match_revisions_v2 page_match
           ON page_match.id=alignment.match_revision_id
          AND page_match.state='active' AND page_match.decision='teacher_confirmed'
         JOIN exam_attempts_v2 attempt
           ON attempt.id=page_match.attempt_id AND attempt.state<>'voided'
         JOIN exam_assessment_items_v2 item
           ON item.id=region.assessment_item_id
          AND item.assessment_version_id=attempt.assessment_version_id
          AND item.state='active'
         JOIN k1_question_versions question
           ON question.id=item.question_version_id AND question.state='published'
         JOIN exam_ingest_pages_v2 page
           ON page.id=region.page_id AND page.state<>'voided'
         JOIN exam_material_type_revisions_v2 material_type
           ON material_type.ingest_batch_id=page.batch_id
          AND material_type.state='active'
          AND material_type.decision='teacher_confirmed'
          AND material_type.material_type='answer_sheet'
         WHERE region.id=?1 AND route.recognition_route='handwriting_ocr'
           AND route.question_type IN ('fill_blank','short_answer')
           AND question.question_type=route.question_type
           AND region.crop_artifact_id IS NOT NULL",
        [answer_region_revision_id],
        |row| {
            Ok(SubjectiveRegionScope {
                attempt_id: row.get(0)?,
                assessment_item_id: row.get(1)?,
                answer_region_revision_id: row.get(2)?,
                question_type: row.get(3)?,
                crop_artifact_id: row.get(4)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| {
        CoreError::Invalid("手写 OCR 只允许处理当前老师已确认的答题卡填空/简答题区".into())
    })
}

pub fn get_active_transcription(
    conn: &Connection,
    answer_region_revision_id: i64,
) -> CoreResult<Option<SubjectiveTranscriptionRevision>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,attempt_id,assessment_item_id,answer_region_revision_id,
                    question_type,revision,source_ai_run_id,result_state,raw_ocr_text,
                    normalized_text,teacher_corrected_text,confidence,failure_meta_json,
                    corrected_by,corrected_at,state,created_at
             FROM exam_subjective_transcription_revisions_v2
             WHERE answer_region_revision_id=?1 AND state='active'",
            [answer_region_revision_id],
            row,
        )
        .optional()?)
}

fn state_name(value: DictationRecognitionState) -> &'static str {
    match value {
        DictationRecognitionState::Recognized => "recognized",
        DictationRecognitionState::NotWritten => "not_written",
        DictationRecognitionState::Unreadable => "unreadable",
        DictationRecognitionState::AmbiguousFinal => "ambiguous_final",
        DictationRecognitionState::RecognizeFailed => "recognize_failed",
    }
}

struct NewTranscription<'a> {
    source_ai_run_id: Option<i64>,
    recognition_state: DictationRecognitionState,
    raw_ocr_text: Option<&'a str>,
    normalized_text: Option<&'a str>,
    teacher_corrected_text: Option<&'a str>,
    confidence: Option<f64>,
    failure_meta_json: Option<&'a str>,
    corrected_by: Option<&'a str>,
}

fn append_transcription(
    conn: &Connection,
    scope: &SubjectiveRegionScope,
    input: &NewTranscription<'_>,
) -> CoreResult<SubjectiveTranscriptionRevision> {
    let revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1
         FROM exam_subjective_transcription_revisions_v2
         WHERE attempt_id=?1 AND assessment_item_id=?2 AND answer_region_revision_id=?3",
        (
            scope.attempt_id,
            scope.assessment_item_id,
            scope.answer_region_revision_id,
        ),
        |row| row.get(0),
    )?;
    conn.execute(
        "UPDATE exam_subjective_grade_suggestions_v2 SET state='superseded'
         WHERE attempt_id=?1 AND assessment_item_id=?2 AND state='active'",
        (scope.attempt_id, scope.assessment_item_id),
    )?;
    conn.execute(
        "UPDATE exam_subjective_transcription_revisions_v2 SET state='superseded'
         WHERE attempt_id=?1 AND assessment_item_id=?2
           AND answer_region_revision_id=?3 AND state='active'",
        (
            scope.attempt_id,
            scope.assessment_item_id,
            scope.answer_region_revision_id,
        ),
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO exam_subjective_transcription_revisions_v2
         (public_id,attempt_id,assessment_item_id,answer_region_revision_id,question_type,
          revision,source_ai_run_id,result_state,raw_ocr_text,normalized_text,
          teacher_corrected_text,confidence,failure_meta_json,corrected_by,corrected_at,
          state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,'active',?16)",
        rusqlite::params![
            &public_id,
            scope.attempt_id,
            scope.assessment_item_id,
            scope.answer_region_revision_id,
            &scope.question_type,
            revision,
            input.source_ai_run_id,
            state_name(input.recognition_state),
            input.raw_ocr_text,
            input.normalized_text,
            input.teacher_corrected_text,
            input.confidence,
            input.failure_meta_json,
            input.corrected_by,
            input.corrected_by.map(|_| now.as_str()),
            &now,
        ],
    )?;
    conn.query_row(
        "SELECT id,public_id,attempt_id,assessment_item_id,answer_region_revision_id,
                question_type,revision,source_ai_run_id,result_state,raw_ocr_text,
                normalized_text,teacher_corrected_text,confidence,failure_meta_json,
                corrected_by,corrected_at,state,created_at
         FROM exam_subjective_transcription_revisions_v2 WHERE id=?1",
        [conn.last_insert_rowid()],
        row,
    )
    .map_err(Into::into)
}

fn normalize_fill_value(raw: &str) -> String {
    raw.trim()
        .chars()
        .filter(|value| !value.is_whitespace())
        .map(|value| match value {
            '０'..='９' => char::from_u32(value as u32 - '０' as u32 + '0' as u32).unwrap(),
            'Ａ'..='Ｚ' => char::from_u32(value as u32 - 'Ａ' as u32 + 'A' as u32).unwrap(),
            'ａ'..='ｚ' => char::from_u32(value as u32 - 'ａ' as u32 + 'a' as u32).unwrap(),
            _ => value,
        })
        .collect::<String>()
        .to_lowercase()
}

fn string_values(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn fill_answer_values(answer: &Value) -> Vec<String> {
    let mut values = string_values(answer.get("values"));
    values.extend(string_values(answer.get("canonical_answers")));
    values.extend(string_values(answer.get("accepted_variants")));
    if values.is_empty() {
        if let Some(slots) = answer.get("slots").and_then(Value::as_array) {
            if slots.len() == 1 {
                values.extend(string_values(slots[0].get("canonical_answers")));
                values.extend(string_values(slots[0].get("accepted_variants")));
                values.extend(string_values(slots[0].get("accepted_answers")));
            }
        }
    }
    values.sort();
    values.dedup();
    values
}

fn suggestion_scope(conn: &Connection, transcription_id: i64) -> CoreResult<SuggestionScope> {
    conn.query_row(
        "SELECT t.id,t.public_id,t.attempt_id,t.assessment_item_id,
                t.answer_region_revision_id,t.question_type,t.revision,t.source_ai_run_id,
                t.result_state,t.raw_ocr_text,t.normalized_text,t.teacher_corrected_text,
                t.confidence,t.failure_meta_json,t.corrected_by,t.corrected_at,t.state,t.created_at,
                i.answer_key_version_id,i.rubric_version_id,answer.answer_json,i.score
         FROM exam_subjective_transcription_revisions_v2 t
         JOIN exam_attempts_v2 attempt
           ON attempt.id=t.attempt_id AND attempt.state<>'voided'
         JOIN exam_assessment_items_v2 i
           ON i.id=t.assessment_item_id
          AND i.assessment_version_id=attempt.assessment_version_id AND i.state='active'
         JOIN k1_answer_key_versions answer
           ON answer.id=i.answer_key_version_id AND answer.state='confirmed'
         JOIN k1_rubric_versions rubric
           ON rubric.id=i.rubric_version_id AND rubric.state='confirmed'
         WHERE t.id=?1 AND t.state='active'",
        [transcription_id],
        |row| {
            Ok(SuggestionScope {
                transcription: SubjectiveTranscriptionRevision {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    attempt_id: row.get(2)?,
                    assessment_item_id: row.get(3)?,
                    answer_region_revision_id: row.get(4)?,
                    question_type: row.get(5)?,
                    revision: row.get(6)?,
                    source_ai_run_id: row.get(7)?,
                    result_state: row.get(8)?,
                    raw_ocr_text: row.get(9)?,
                    normalized_text: row.get(10)?,
                    teacher_corrected_text: row.get(11)?,
                    confidence: row.get(12)?,
                    failure_meta_json: row.get(13)?,
                    corrected_by: row.get(14)?,
                    corrected_at: row.get(15)?,
                    state: row.get(16)?,
                    created_at: row.get(17)?,
                },
                answer_key_version_id: row.get(18)?,
                rubric_version_id: row.get(19)?,
                answer_json: row.get(20)?,
                max_score: row.get(21)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("主观题转写未绑定当前已确认答案与评分规则".into()))
}

fn ensure_subjective_suggestion(
    conn: &Connection,
    transcription_id: i64,
) -> CoreResult<SubjectiveGradeSuggestion> {
    if let Some(existing) = conn
        .query_row(
            "SELECT id,public_id,transcription_revision_id,attempt_id,
                    assessment_item_id,answer_key_version_id,rubric_version_id,
                    machine_grade_ai_run_id,outcome,suggested_score,result_json,
                    confidence,batch_eligible,exclusion_reason,state
             FROM exam_subjective_grade_suggestions_v2
             WHERE transcription_revision_id=?1",
            [transcription_id],
            suggestion_row,
        )
        .optional()?
    {
        return Ok(existing);
    }
    let scope = suggestion_scope(conn, transcription_id)?;
    let effective_text = scope
        .transcription
        .teacher_corrected_text
        .as_deref()
        .or(scope.transcription.normalized_text.as_deref());
    let (outcome, suggested_score, result_json, batch_eligible, exclusion_reason) =
        if scope.transcription.question_type == "short_answer" {
            (
                "unscored",
                None,
                serde_json::json!({
                    "schema_version": 1,
                    "source": "answer_sheet_short_answer_pending",
                    "transcription_revision_id": transcription_id,
                    "reason": "SHORT_ANSWER_AI_REQUIRED"
                })
                .to_string(),
                false,
                Some("SHORT_ANSWER_AI_REQUIRED"),
            )
        } else if scope.transcription.result_state != "recognized" {
            let reason = match scope.transcription.result_state.as_str() {
                "not_written" => "NOT_WRITTEN",
                "unreadable" => "UNREADABLE",
                "ambiguous_final" => "AMBIGUOUS_FINAL",
                "recognize_failed" => "RECOGNIZE_FAILED",
                _ => "TRANSCRIPTION_NOT_GRADABLE",
            };
            (
                "unscored",
                None,
                serde_json::json!({
                    "schema_version": 1,
                    "source": "answer_sheet_fill_exact",
                    "transcription_revision_id": transcription_id,
                    "reason": reason
                })
                .to_string(),
                false,
                Some(reason),
            )
        } else {
            let answer: Value = serde_json::from_str(&scope.answer_json)
                .map_err(|error| CoreError::Parse(format!("填空题答案 JSON 损坏：{error}")))?;
            let accepted = fill_answer_values(&answer);
            if accepted.is_empty() {
                (
                    "unscored",
                    None,
                    serde_json::json!({
                        "schema_version": 1,
                        "source": "answer_sheet_fill_exact",
                        "transcription_revision_id": transcription_id,
                        "reason": "ANSWER_FORMAT_REVIEW_REQUIRED"
                    })
                    .to_string(),
                    false,
                    Some("ANSWER_FORMAT_REVIEW_REQUIRED"),
                )
            } else if let Some(actual) = effective_text {
                let normalized_actual = normalize_fill_value(actual);
                let matched = accepted
                    .iter()
                    .find(|value| normalize_fill_value(value) == normalized_actual);
                if let Some(matched_answer) = matched {
                    let batch_eligible = scope.transcription.confidence.unwrap_or(0.0) >= 0.95;
                    (
                        "correct",
                        Some(scope.max_score),
                        serde_json::json!({
                            "schema_version": 1,
                            "source": "answer_sheet_fill_exact",
                            "transcription_revision_id": transcription_id,
                            "actual_text": actual,
                            "matched_answer": matched_answer,
                            "result": "exact"
                        })
                        .to_string(),
                        batch_eligible,
                        if batch_eligible {
                            None
                        } else {
                            Some("LOW_CONFIDENCE")
                        },
                    )
                } else {
                    (
                        "incorrect",
                        Some(0.0),
                        serde_json::json!({
                            "schema_version": 1,
                            "source": "answer_sheet_fill_exact",
                            "transcription_revision_id": transcription_id,
                            "actual_text": actual,
                            "result": "mismatch"
                        })
                        .to_string(),
                        false,
                        Some("ANSWER_MISMATCH_REQUIRES_REVIEW"),
                    )
                }
            } else {
                (
                    "unscored",
                    None,
                    serde_json::json!({
                        "schema_version": 1,
                        "source": "answer_sheet_fill_exact",
                        "transcription_revision_id": transcription_id,
                        "reason": "TRANSCRIPTION_TEXT_MISSING"
                    })
                    .to_string(),
                    false,
                    Some("TRANSCRIPTION_TEXT_MISSING"),
                )
            }
        };
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO exam_subjective_grade_suggestions_v2
         (public_id,transcription_revision_id,attempt_id,assessment_item_id,
          answer_key_version_id,rubric_version_id,machine_grade_ai_run_id,outcome,
          suggested_score,result_json,confidence,batch_eligible,exclusion_reason,state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,NULL,?7,?8,?9,?10,?11,?12,'active',?13)",
        rusqlite::params![
            &public_id,
            transcription_id,
            scope.transcription.attempt_id,
            scope.transcription.assessment_item_id,
            scope.answer_key_version_id,
            scope.rubric_version_id,
            outcome,
            suggested_score,
            &result_json,
            scope.transcription.confidence,
            batch_eligible,
            exclusion_reason,
            &now,
        ],
    )?;
    get_subjective_suggestion(conn, conn.last_insert_rowid())?
        .ok_or_else(|| CoreError::NotFound("刚创建的主观题评分建议".into()))
}

pub fn record_ocr_ai_run_transcription(
    conn: &mut Connection,
    ai_run_id: i64,
) -> CoreResult<SubjectiveTranscriptionRevision> {
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    if run.run_type != "handwriting_ocr" || run.business_ref_type != "answer_region_revision" {
        return Err(CoreError::Invalid("当前 AI run 不是答题卡手写 OCR".into()));
    }
    let answer_region_revision_id = run
        .business_ref_id
        .parse::<i64>()
        .map_err(|_| CoreError::Invalid("手写 OCR 业务引用无效".into()))?;
    let scope = load_region_scope(conn, answer_region_revision_id)?;
    if run.input_artifact_id != Some(scope.crop_artifact_id) {
        return Err(CoreError::Invalid("手写 OCR run 与当前裁剪不一致".into()));
    }
    if let Some(existing) = get_active_transcription(conn, answer_region_revision_id)? {
        if existing.source_ai_run_id == Some(ai_run_id) {
            ensure_subjective_suggestion(conn, existing.id)?;
            return Ok(existing);
        }
    }
    let tx = conn.transaction()?;
    let result = match run.status {
        AiRunStatus::Succeeded => {
            let output: DictationOcrOutput = serde_json::from_str(
                run.output_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("成功的手写 OCR 缺少输出".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("手写 OCR 输出损坏：{error}")))?;
            if output.answer_region_revision_id != answer_region_revision_id
                || output.crop_artifact_id != scope.crop_artifact_id
                || output.input_hash != run.input_hash
            {
                return Err(CoreError::Invalid("手写 OCR 输出作用域不一致".into()));
            }
            append_transcription(
                &tx,
                &scope,
                &NewTranscription {
                    source_ai_run_id: Some(ai_run_id),
                    recognition_state: output.state,
                    raw_ocr_text: output.raw_text.as_deref(),
                    normalized_text: output.normalized_text.as_deref(),
                    teacher_corrected_text: None,
                    confidence: output.confidence,
                    failure_meta_json: None,
                    corrected_by: None,
                },
            )?
        }
        AiRunStatus::Failed => append_transcription(
            &tx,
            &scope,
            &NewTranscription {
                source_ai_run_id: Some(ai_run_id),
                recognition_state: DictationRecognitionState::RecognizeFailed,
                raw_ocr_text: None,
                normalized_text: None,
                teacher_corrected_text: None,
                confidence: None,
                failure_meta_json: Some(
                    run.error_meta_json
                        .as_deref()
                        .ok_or_else(|| CoreError::Invalid("失败的手写 OCR 缺少错误信息".into()))?,
                ),
                corrected_by: None,
            },
        )?,
        _ => return Err(CoreError::Invalid("手写 OCR 尚未形成最终结果".into())),
    };
    ensure_subjective_suggestion(&tx, result.id)?;
    tx.commit()?;
    Ok(result)
}

pub fn teacher_correct_transcription(
    conn: &mut Connection,
    answer_region_revision_id: i64,
    corrected_text: &str,
    corrected_by: &str,
) -> CoreResult<SubjectiveTranscriptionRevision> {
    required(corrected_text, "老师校正文本")?;
    required(corrected_by, "校正人")?;
    let scope = load_region_scope(conn, answer_region_revision_id)?;
    let active = get_active_transcription(conn, answer_region_revision_id)?
        .ok_or_else(|| CoreError::NotFound("当前主观题区尚无转写结果".into()))?;
    if active.result_state != "recognized" {
        return Err(CoreError::Invalid(
            "未写、无法辨认、涂改歧义或识别失败请直接进入老师判定；不能伪造 OCR 原文".into(),
        ));
    }
    let tx = conn.transaction()?;
    let result = append_transcription(
        &tx,
        &scope,
        &NewTranscription {
            source_ai_run_id: active.source_ai_run_id,
            recognition_state: DictationRecognitionState::Recognized,
            raw_ocr_text: active.raw_ocr_text.as_deref(),
            normalized_text: active.normalized_text.as_deref(),
            teacher_corrected_text: Some(corrected_text.trim()),
            confidence: active.confidence,
            failure_meta_json: None,
            corrected_by: Some(corrected_by.trim()),
        },
    )?;
    ensure_subjective_suggestion(&tx, result.id)?;
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!(
                "exam:subjective-region:{answer_region_revision_id}:transcription:{}:corrected",
                result.revision
            ),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(corrected_by.trim()),
            action: "exam.subjective_transcription.corrected",
            object_type: "exam_answer_region_revision",
            object_id: &answer_region_revision_id.to_string(),
            object_revision: Some(result.revision),
            note: None,
            meta_json: Some(r#"{"schema_version":1}"#),
            occurred_at: &result.created_at,
        },
    )?;
    tx.commit()?;
    Ok(result)
}

fn review_scope(conn: &Connection, suggestion_id: i64) -> CoreResult<ReviewScope> {
    conn.query_row(
        "SELECT s.id,s.public_id,s.transcription_revision_id,s.attempt_id,
                s.assessment_item_id,s.answer_key_version_id,s.rubric_version_id,
                s.machine_grade_ai_run_id,s.outcome,s.suggested_score,s.result_json,
                s.confidence,s.batch_eligible,s.exclusion_reason,s.state,
                item.score,t.state,region.state,region.decision
         FROM exam_subjective_grade_suggestions_v2 s
         JOIN exam_subjective_transcription_revisions_v2 t
           ON t.id=s.transcription_revision_id
         JOIN exam_answer_region_revisions_v2 region
           ON region.id=t.answer_region_revision_id
         JOIN exam_attempts_v2 attempt
           ON attempt.id=s.attempt_id AND attempt.state<>'voided'
         JOIN exam_assessment_items_v2 item
           ON item.id=s.assessment_item_id
          AND item.assessment_version_id=attempt.assessment_version_id
          AND item.answer_key_version_id=s.answer_key_version_id
          AND item.rubric_version_id=s.rubric_version_id
          AND item.state='active'
         WHERE s.id=?1",
        [suggestion_id],
        |row| {
            Ok(ReviewScope {
                suggestion: SubjectiveGradeSuggestion {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    transcription_revision_id: row.get(2)?,
                    attempt_id: row.get(3)?,
                    assessment_item_id: row.get(4)?,
                    answer_key_version_id: row.get(5)?,
                    rubric_version_id: row.get(6)?,
                    machine_grade_ai_run_id: row.get(7)?,
                    outcome: row.get(8)?,
                    suggested_score: row.get(9)?,
                    result_json: row.get(10)?,
                    confidence: row.get(11)?,
                    batch_eligible: row.get(12)?,
                    exclusion_reason: row.get(13)?,
                    state: row.get(14)?,
                },
                max_score: row.get(15)?,
                transcription_state: row.get(16)?,
                region_state: row.get(17)?,
                region_decision: row.get(18)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound(format!("主观题评分建议#{suggestion_id}")))
}

fn link_decision_source(
    conn: &Connection,
    decision_id: i64,
    scope: &ReviewScope,
    review_mode: &str,
    reviewed_by: &str,
    created_at: &str,
) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO exam_grade_decision_subjective_sources_v2
         (grade_decision_id,suggestion_id,transcription_revision_id,review_mode,
          reviewed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6)
         ON CONFLICT(grade_decision_id) DO NOTHING",
        (
            decision_id,
            scope.suggestion.id,
            scope.suggestion.transcription_revision_id,
            review_mode,
            reviewed_by,
            created_at,
        ),
    )?;
    let existing: (i64, i64, String, String) = conn.query_row(
        "SELECT suggestion_id,transcription_revision_id,review_mode,reviewed_by
         FROM exam_grade_decision_subjective_sources_v2 WHERE grade_decision_id=?1",
        [decision_id],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;
    if existing
        != (
            scope.suggestion.id,
            scope.suggestion.transcription_revision_id,
            review_mode.to_owned(),
            reviewed_by.to_owned(),
        )
    {
        return Err(CoreError::Invalid(
            "评分 revision 已绑定不同的答题卡主观题证据".into(),
        ));
    }
    Ok(())
}

fn validate_active_review(scope: &ReviewScope) -> CoreResult<()> {
    if scope.suggestion.state != "active" || scope.transcription_state != "active" {
        return Err(CoreError::Invalid("主观题建议已被后续转写替代".into()));
    }
    if scope.region_state != "active" || scope.region_decision != "teacher_confirmed" {
        return Err(CoreError::Invalid(
            "答案区域已变化，请先重新核对页面证据".into(),
        ));
    }
    Ok(())
}

pub fn accept_subjective_suggestion(
    conn: &Connection,
    suggestion_id: i64,
    reviewed_by: &str,
) -> CoreResult<GradeDecision> {
    required(reviewed_by, "终审人")?;
    let tx = conn.unchecked_transaction()?;
    let scope = review_scope(&tx, suggestion_id)?;
    validate_active_review(&scope)?;
    let score = scope
        .suggestion
        .suggested_score
        .ok_or_else(|| CoreError::Invalid("该主观题没有可直接接受的机器得分".into()))?;
    let decision = decide_grade_in_transaction(
        &tx,
        &NewGradeDecision {
            attempt_id: scope.suggestion.attempt_id,
            assessment_item_id: scope.suggestion.assessment_item_id,
            machine_grade_ai_run_id: scope.suggestion.machine_grade_ai_run_id,
            teacher_score: score,
            point_results_json: &scope.suggestion.result_json,
            teacher_note: None,
            confirmation_level: "teacher_accepted",
            decided_by: reviewed_by,
        },
    )?;
    let now = time::utc_now_rfc3339();
    link_decision_source(&tx, decision.id, &scope, "single", reviewed_by.trim(), &now)?;
    tx.commit()?;
    Ok(decision)
}

pub fn correct_subjective_suggestion(
    conn: &Connection,
    suggestion_id: i64,
    teacher_score: f64,
    teacher_note: Option<&str>,
    reviewed_by: &str,
) -> CoreResult<GradeDecision> {
    required(reviewed_by, "终审人")?;
    let teacher_note = teacher_note
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CoreError::Invalid("人工记分必须填写查看原图后的判定依据".into()))?;
    let tx = conn.unchecked_transaction()?;
    let scope = review_scope(&tx, suggestion_id)?;
    validate_active_review(&scope)?;
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
        .map_err(|error| CoreError::Parse(format!("主观题建议 JSON 损坏：{error}")))?;
    let point_results = serde_json::json!({
        "schema_version": 1,
        "source": "answer_sheet_subjective_teacher_correction",
        "suggestion_id": suggestion_id,
        "transcription_revision_id": scope.suggestion.transcription_revision_id,
        "machine_result": machine_result,
        "teacher_score": teacher_score
    })
    .to_string();
    let decision = decide_grade_in_transaction(
        &tx,
        &NewGradeDecision {
            attempt_id: scope.suggestion.attempt_id,
            assessment_item_id: scope.suggestion.assessment_item_id,
            machine_grade_ai_run_id: scope.suggestion.machine_grade_ai_run_id,
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
        &scope,
        "teacher_corrected",
        reviewed_by.trim(),
        &now,
    )?;
    tx.commit()?;
    Ok(decision)
}

pub fn list_subjective_workbench(
    conn: &Connection,
    assessment_version_id: Option<i64>,
    limit: i64,
) -> CoreResult<SubjectiveWorkbench> {
    let limit = limit.clamp(1, 2000);
    let mut stmt = conn.prepare(
        "SELECT assessment.id,version.id,assessment.title,
                attempt.id,attempt.state,attempt.active_publication_id,
                student.id,student.student_no,student.name,
                item.id,item.order_index,
                COALESCE(CAST(json_extract(item.presentation_snapshot_json,'$.question_no') AS TEXT),
                         CAST(item.order_index + 1 AS TEXT)),
                question.question_type,question.stem,item.score,
                region.id,artifact.archived_path,
                transcription.id,transcription.revision,transcription.result_state,
                transcription.raw_ocr_text,transcription.normalized_text,
                transcription.teacher_corrected_text,transcription.confidence,
                answer.answer_json,
                COALESCE((SELECT json_object(
                    'schema_version',1,
                    'rubric_points',json_group_array(json_object(
                      'stable_id',point.stable_id,
                      'order_index',point.order_index,
                      'canonical_text',point.canonical_text,
                      'max_score',point.max_score)))
                  FROM k1_rubric_points point
                  WHERE point.rubric_version_id=item.rubric_version_id),
                  '{\"schema_version\":1,\"rubric_points\":[]}'),
                suggestion.id,suggestion.outcome,suggestion.suggested_score,
                suggestion.result_json,suggestion.batch_eligible,suggestion.exclusion_reason,
                decision.id,decision.revision,decision.teacher_score,
                decision.confirmation_level,source.review_mode,
                CASE WHEN source.suggestion_id=suggestion.id THEN 1 ELSE 0 END,
                decision.decided_at
         FROM exam_subjective_grade_suggestions_v2 suggestion
         JOIN exam_subjective_transcription_revisions_v2 transcription
           ON transcription.id=suggestion.transcription_revision_id
          AND transcription.state='active'
         JOIN exam_answer_region_revisions_v2 region
           ON region.id=transcription.answer_region_revision_id AND region.state='active'
         LEFT JOIN artifacts artifact ON artifact.id=region.crop_artifact_id
         JOIN exam_attempts_v2 attempt
           ON attempt.id=suggestion.attempt_id AND attempt.state<>'voided'
         JOIN students student ON student.id=attempt.student_id
         JOIN exam_assessment_items_v2 item
           ON item.id=suggestion.assessment_item_id AND item.state='active'
         JOIN exam_assessment_versions_v2 version
           ON version.id=item.assessment_version_id
         JOIN exam_assessments_v2 assessment ON assessment.id=version.assessment_id
         JOIN k1_question_versions question ON question.id=item.question_version_id
         JOIN k1_answer_key_versions answer ON answer.id=suggestion.answer_key_version_id
         LEFT JOIN exam_grade_decisions_v2 decision
           ON decision.attempt_id=attempt.id
          AND decision.assessment_item_id=item.id AND decision.state='active'
         LEFT JOIN exam_grade_decision_subjective_sources_v2 source
           ON source.grade_decision_id=decision.id
         WHERE suggestion.state='active' AND (?1 IS NULL OR version.id=?1)
         ORDER BY version.id DESC,item.order_index,student.student_no,attempt.attempt_no
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map((assessment_version_id, limit), |row| {
            Ok(SubjectiveWorkbenchRow {
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
                question_no: row.get(11)?,
                question_type: row.get(12)?,
                question_stem: row.get(13)?,
                max_score: row.get(14)?,
                answer_region_revision_id: row.get(15)?,
                crop_path: row.get(16)?,
                transcription_revision_id: row.get(17)?,
                transcription_revision: row.get(18)?,
                result_state: row.get(19)?,
                raw_ocr_text: row.get(20)?,
                normalized_text: row.get(21)?,
                teacher_corrected_text: row.get(22)?,
                confidence: row.get(23)?,
                answer_json: row.get(24)?,
                rubric_points_json: row.get(25)?,
                suggestion_id: row.get(26)?,
                suggestion_outcome: row.get(27)?,
                suggested_score: row.get(28)?,
                suggestion_result_json: row.get(29)?,
                batch_eligible: row.get(30)?,
                exclusion_reason: row.get(31)?,
                grade_decision_id: row.get(32)?,
                grade_decision_revision: row.get(33)?,
                teacher_score: row.get(34)?,
                confirmation_level: row.get(35)?,
                review_mode: row.get(36)?,
                current_suggestion_confirmed: row.get(37)?,
                decided_at: row.get(38)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    let mut attempt_stmt = conn.prepare(
        "SELECT assessment.id,version.id,assessment.title,
                attempt.id,attempt.state,attempt.active_publication_id,
                student.id,student.student_no,student.name,
                (SELECT COUNT(*) FROM exam_assessment_items_v2 item
                 WHERE item.assessment_version_id=version.id AND item.state='active'),
                (SELECT COUNT(*) FROM exam_subjective_grade_suggestions_v2 suggestion
                 WHERE suggestion.attempt_id=attempt.id AND suggestion.state='active'),
                (SELECT COUNT(*) FROM exam_grade_decisions_v2 decision
                 JOIN exam_assessment_items_v2 item ON item.id=decision.assessment_item_id
                 WHERE decision.attempt_id=attempt.id AND decision.state='active'
                   AND item.assessment_version_id=version.id AND item.state='active'),
                (SELECT COALESCE(SUM(decision.teacher_score),0.0)
                 FROM exam_grade_decisions_v2 decision
                 JOIN exam_assessment_items_v2 item ON item.id=decision.assessment_item_id
                 WHERE decision.attempt_id=attempt.id AND decision.state='active'
                   AND item.assessment_version_id=version.id AND item.state='active'),
                (SELECT COALESCE(SUM(item.score),0.0)
                 FROM exam_assessment_items_v2 item
                 WHERE item.assessment_version_id=version.id AND item.state='active'),
                publication_item.total_score,
                CASE WHEN attempt.state='ready_to_publish' THEN 1 ELSE 0 END
         FROM exam_attempts_v2 attempt
         JOIN students student ON student.id=attempt.student_id
         JOIN exam_assessment_versions_v2 version
           ON version.id=attempt.assessment_version_id
         JOIN exam_assessments_v2 assessment ON assessment.id=version.assessment_id
         LEFT JOIN exam_grade_publication_items_v2 publication_item
           ON publication_item.publication_id=attempt.active_publication_id
          AND publication_item.attempt_id=attempt.id
         WHERE attempt.state<>'voided' AND (?1 IS NULL OR version.id=?1)
           AND EXISTS(SELECT 1 FROM exam_subjective_grade_suggestions_v2 suggestion
                      WHERE suggestion.attempt_id=attempt.id AND suggestion.state='active')
         ORDER BY version.id DESC,student.student_no,attempt.attempt_no
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
    Ok(SubjectiveWorkbench { rows, attempts })
}
