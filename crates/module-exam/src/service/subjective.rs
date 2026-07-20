//! 答题卡填空/简答题区的追加式手写 OCR 转写。
//!
//! 本层只保存“学生实际写了什么”，不加载标准答案、不判分。后续机器评分和老师终审
//! 必须引用这里的具体 active revision，避免答案反向污染 OCR 原文。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::{ai_runs, audit};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, AuditActorType};

use crate::dictation::DictationRecognitionState;
use crate::dictation_recognition::DictationOcrOutput;
use crate::short_answer_grading::{
    ShortAnswerGradeOutput, ShortAnswerGradeRequest, ShortAnswerGradeState,
    ShortAnswerRubricPointSpec,
};

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
pub struct ShortAnswerGradeAnalysis {
    pub id: i64,
    pub public_id: String,
    pub transcription_revision_id: i64,
    pub suggestion_id: i64,
    pub attempt_id: i64,
    pub assessment_item_id: i64,
    pub answer_key_version_id: i64,
    pub rubric_version_id: i64,
    pub machine_grade_ai_run_id: i64,
    pub outcome: String,
    pub suggested_score: f64,
    pub result_json: String,
    pub confidence: f64,
    pub exclusion_reason: String,
    pub state: String,
    pub created_at: String,
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
    pub answer_slots_json: String,
    pub rubric_points_json: String,
    pub suggestion_id: i64,
    pub short_answer_analysis_id: Option<i64>,
    pub machine_grade_ai_run_id: Option<i64>,
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
    pub teacher_components_json: String,
    pub accepted_answer_promotion_id: Option<i64>,
    pub accepted_answer_promoted_at: Option<String>,
    pub rubric_evidence_promotions_json: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveWorkbench {
    pub rows: Vec<SubjectiveWorkbenchRow>,
    pub attempts: Vec<ObjectiveAttemptSummary>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveComponentGradeInput {
    pub source_type: String,
    pub source_public_id: String,
    pub teacher_score: f64,
    pub evidence_text: Option<String>,
    pub teacher_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveComponentGradeResult {
    pub source_type: String,
    pub source_public_id: String,
    pub stable_id: String,
    pub order_index: i64,
    pub teacher_score: f64,
    pub max_score: f64,
    pub result_status: String,
    pub evidence_text: Option<String>,
    pub teacher_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcceptedAnswerPromotionResult {
    pub outcome: String,
    pub promotion_id: Option<i64>,
    pub accepted_text: String,
    pub adopted_assessment_version_id: i64,
    pub adopted_assessment_revision: i64,
    pub adopted_answer_key_version_id: i64,
    pub current_grade_unchanged: bool,
    pub current_publication_unchanged: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RubricEvidencePromotionResult {
    pub outcome: String,
    pub promotion_id: Option<i64>,
    pub rubric_point_stable_id: String,
    pub evidence_text: String,
    pub adopted_assessment_version_id: i64,
    pub adopted_assessment_revision: i64,
    pub adopted_rubric_version_id: i64,
    pub adopted_link_set_id: i64,
    pub carried_knowledge_link_count: i64,
    pub carried_ability_link_count: i64,
    pub current_grade_unchanged: bool,
    pub current_publication_unchanged: bool,
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
    short_answer_analysis_id: Option<i64>,
    max_score: f64,
    question_type: String,
    transcription_result_state: String,
    effective_text: Option<String>,
    transcription_state: String,
    region_state: String,
    region_decision: String,
}

#[derive(Debug)]
struct ExpectedSubjectiveComponent {
    source_type: &'static str,
    source_public_id: String,
    stable_id: String,
    order_index: i64,
    max_score: f64,
}

#[derive(Debug, Serialize)]
struct AcceptedAnswerAssessmentHashItem {
    item_id: i64,
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    order_index: i64,
    score_millis: i64,
}

#[derive(Debug)]
struct AcceptedAnswerSourceScope {
    grade_decision_id: i64,
    suggestion_id: i64,
    transcription_revision_id: i64,
    accepted_text: String,
    source_assessment_version_id: i64,
    source_assessment_item_id: i64,
    source_answer_key_version_id: i64,
    assessment_id: i64,
    question_version_id: i64,
}

#[derive(Debug)]
struct AcceptedAnswerBaseItem {
    id: i64,
    assessment_version_id: i64,
    answer_key_version_id: i64,
    answer_json: String,
}

#[derive(Debug)]
struct AcceptedAnswerSlot {
    stable_id: String,
    order_index: i64,
    canonical_answers_json: String,
    normalization_rules_json: Option<String>,
    max_score: f64,
}

#[derive(Debug)]
struct AssessmentItemClone {
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    order_index: i64,
    score: f64,
    option_order_json: Option<String>,
    presentation_snapshot_json: String,
}

#[derive(Debug)]
struct RubricEvidenceSourceScope {
    grade_decision_id: i64,
    component_id: i64,
    suggestion_id: i64,
    transcription_revision_id: i64,
    evidence_text: String,
    rubric_point_stable_id: String,
    source_assessment_version_id: i64,
    source_assessment_item_id: i64,
    source_rubric_version_id: i64,
    assessment_id: i64,
    question_version_id: i64,
}

#[derive(Debug)]
struct RubricEvidenceBaseItem {
    id: i64,
    assessment_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
}

#[derive(Debug)]
struct RubricPointClone {
    id: i64,
    public_id: String,
    stable_id: String,
    order_index: i64,
    canonical_text: String,
    allowed_paraphrases_json: Option<String>,
    required_concepts_json: Option<String>,
    max_score: f64,
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

fn short_answer_analysis_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<ShortAnswerGradeAnalysis> {
    Ok(ShortAnswerGradeAnalysis {
        id: row.get(0)?,
        public_id: row.get(1)?,
        transcription_revision_id: row.get(2)?,
        suggestion_id: row.get(3)?,
        attempt_id: row.get(4)?,
        assessment_item_id: row.get(5)?,
        answer_key_version_id: row.get(6)?,
        rubric_version_id: row.get(7)?,
        machine_grade_ai_run_id: row.get(8)?,
        outcome: row.get(9)?,
        suggested_score: row.get(10)?,
        result_json: row.get(11)?,
        confidence: row.get(12)?,
        exclusion_reason: row.get(13)?,
        state: row.get(14)?,
        created_at: row.get(15)?,
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

pub fn get_short_answer_analysis_by_run(
    conn: &Connection,
    ai_run_id: i64,
) -> CoreResult<Option<ShortAnswerGradeAnalysis>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,transcription_revision_id,suggestion_id,attempt_id,
                    assessment_item_id,answer_key_version_id,rubric_version_id,
                    machine_grade_ai_run_id,outcome,suggested_score,result_json,
                    confidence,exclusion_reason,state,created_at
             FROM exam_short_answer_grade_analyses_v2
             WHERE machine_grade_ai_run_id=?1",
            [ai_run_id],
            short_answer_analysis_row,
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
        "UPDATE exam_short_answer_grade_analyses_v2 SET state='superseded'
         WHERE attempt_id=?1 AND assessment_item_id=?2 AND state='active'",
        (scope.attempt_id, scope.assessment_item_id),
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

fn append_unique_string(value: &mut Value, field: &str, text: &str) -> CoreResult<()> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| CoreError::Invalid("答案版本必须是对象".into()))?;
    let entries = object
        .entry(field)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| CoreError::Invalid(format!("答案版本 {field} 必须是数组")))?;
    if !entries
        .iter()
        .filter_map(Value::as_str)
        .any(|value| normalize_fill_value(value) == normalize_fill_value(text))
    {
        entries.push(Value::String(text.to_owned()));
    }
    Ok(())
}

fn promotion_result_by_decision(
    conn: &Connection,
    grade_decision_id: i64,
    outcome: &str,
) -> CoreResult<Option<AcceptedAnswerPromotionResult>> {
    conn.query_row(
        "SELECT promotion.id,promotion.accepted_text,
                promotion.adopted_assessment_version_id,version.revision,
                promotion.adopted_answer_key_version_id
         FROM exam_accepted_answer_promotions_v2 promotion
         JOIN exam_assessment_versions_v2 version
           ON version.id=promotion.adopted_assessment_version_id
         WHERE promotion.grade_decision_id=?1",
        [grade_decision_id],
        |row| {
            Ok(AcceptedAnswerPromotionResult {
                outcome: outcome.to_owned(),
                promotion_id: row.get(0)?,
                accepted_text: row.get(1)?,
                adopted_assessment_version_id: row.get(2)?,
                adopted_assessment_revision: row.get(3)?,
                adopted_answer_key_version_id: row.get(4)?,
                current_grade_unchanged: true,
                current_publication_unchanged: true,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn promotion_result_by_identity(
    conn: &Connection,
    assessment_id: i64,
    question_version_id: i64,
    normalized_text: &str,
) -> CoreResult<Option<AcceptedAnswerPromotionResult>> {
    conn.query_row(
        "SELECT promotion.id,promotion.accepted_text,
                promotion.adopted_assessment_version_id,version.revision,
                promotion.adopted_answer_key_version_id
         FROM exam_accepted_answer_promotions_v2 promotion
         JOIN exam_assessment_versions_v2 version
           ON version.id=promotion.adopted_assessment_version_id
         WHERE promotion.assessment_id=?1 AND promotion.question_version_id=?2
           AND promotion.normalized_text=?3",
        (assessment_id, question_version_id, normalized_text),
        |row| {
            Ok(AcceptedAnswerPromotionResult {
                outcome: "already_promoted".into(),
                promotion_id: row.get(0)?,
                accepted_text: row.get(1)?,
                adopted_assessment_version_id: row.get(2)?,
                adopted_assessment_revision: row.get(3)?,
                adopted_answer_key_version_id: row.get(4)?,
                current_grade_unchanged: true,
                current_publication_unchanged: true,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn json_string_array(raw: Option<String>, field: &str) -> CoreResult<Vec<String>> {
    match raw {
        None => Ok(Vec::new()),
        Some(raw) => {
            let value: Value = serde_json::from_str(&raw)
                .map_err(|error| CoreError::Parse(format!("简答题{field} JSON 损坏：{error}")))?;
            let array = value
                .as_array()
                .ok_or_else(|| CoreError::Invalid(format!("简答题{field}必须是数组")))?;
            Ok(array
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .collect())
        }
    }
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

/// 加载一次简答题评分所需的完整、版本冻结输入。调用方可在释放数据库锁后交给 provider。
pub fn load_short_answer_grade_request(
    conn: &Connection,
    transcription_id: i64,
) -> CoreResult<ShortAnswerGradeRequest> {
    let (
        attempt_id,
        assessment_item_id,
        answer_region_revision_id,
        crop_artifact_id,
        question_stem,
        student_answer,
        answer_key_version_id,
        rubric_version_id,
        answer_json,
        max_score,
    ): (i64, i64, i64, i64, String, String, i64, i64, String, f64) = conn
        .query_row(
            "SELECT t.attempt_id,t.assessment_item_id,t.answer_region_revision_id,
                    region.crop_artifact_id,question.stem,
                    COALESCE(t.teacher_corrected_text,t.normalized_text),
                    item.answer_key_version_id,item.rubric_version_id,
                    answer.answer_json,item.score
             FROM exam_subjective_transcription_revisions_v2 t
             JOIN exam_answer_region_revisions_v2 region
               ON region.id=t.answer_region_revision_id
              AND region.state='active' AND region.decision='teacher_confirmed'
             JOIN exam_attempts_v2 attempt
               ON attempt.id=t.attempt_id AND attempt.state<>'voided'
             JOIN exam_assessment_items_v2 item
               ON item.id=t.assessment_item_id
              AND item.assessment_version_id=attempt.assessment_version_id
              AND item.state='active'
             JOIN k1_question_versions question
               ON question.id=item.question_version_id
              AND question.state='published' AND question.question_type='short_answer'
             JOIN k1_answer_key_versions answer
               ON answer.id=item.answer_key_version_id AND answer.state='confirmed'
             JOIN k1_rubric_versions rubric
               ON rubric.id=item.rubric_version_id AND rubric.state='confirmed'
             WHERE t.id=?1 AND t.state='active' AND t.question_type='short_answer'
               AND t.result_state='recognized' AND region.crop_artifact_id IS NOT NULL",
            [transcription_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid(
                "简答题评分只允许使用当前老师确认题区的 active recognized 转写".into(),
            )
        })?;

    let answer: Value = serde_json::from_str(&answer_json)
        .map_err(|error| CoreError::Parse(format!("简答题答案 JSON 损坏：{error}")))?;
    let reference_answer = answer
        .get("reference_answer")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| CoreError::Invalid("简答题已确认答案缺少 reference_answer".into()))?
        .to_string();

    let mut point_stmt = conn.prepare(
        "SELECT id,public_id,stable_id,order_index,canonical_text,
                allowed_paraphrases_json,required_concepts_json,max_score
         FROM k1_rubric_points
         WHERE rubric_version_id=?1 ORDER BY order_index,id",
    )?;
    let mut rubric_points = point_stmt
        .query_map([rubric_version_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, f64>(7)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(
            |(
                id,
                public_id,
                stable_id,
                order_index,
                canonical_text,
                allowed,
                required,
                point_score,
            )| {
                Ok(ShortAnswerRubricPointSpec {
                    rubric_point_id: id,
                    public_id,
                    stable_id,
                    order_index,
                    canonical_text,
                    allowed_paraphrases: json_string_array(allowed, "允许改述")?,
                    required_concepts: json_string_array(required, "必需概念")?,
                    contradiction_rules: Vec::new(),
                    max_score: point_score,
                })
            },
        )
        .collect::<CoreResult<Vec<_>>>()?;
    drop(point_stmt);

    let point_indexes = rubric_points
        .iter()
        .enumerate()
        .map(|(index, point)| (point.rubric_point_id, index))
        .collect::<std::collections::BTreeMap<_, _>>();
    let mut rule_stmt = conn.prepare(
        "SELECT rule.rubric_point_id,rule.rule_type,rule.rule_json
         FROM k1_contradiction_rules rule
         JOIN k1_rubric_points point ON point.id=rule.rubric_point_id
         WHERE point.rubric_version_id=?1 ORDER BY rule.id",
    )?;
    let rules = rule_stmt
        .query_map([rubric_version_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for (point_id, rule_type, raw_rule) in rules {
        let rule: Value = serde_json::from_str(&raw_rule)
            .map_err(|error| CoreError::Parse(format!("简答题矛盾规则 JSON 损坏：{error}")))?;
        let index = point_indexes
            .get(&point_id)
            .copied()
            .ok_or_else(|| CoreError::Invalid("简答题矛盾规则绑定了外部评分点".into()))?;
        rubric_points[index]
            .contradiction_rules
            .push(serde_json::json!({
                "rule_type": rule_type,
                "rule": rule
            }));
    }

    let request = ShortAnswerGradeRequest {
        transcription_revision_id: transcription_id,
        attempt_id,
        assessment_item_id,
        answer_region_revision_id,
        crop_artifact_id,
        question_stem,
        student_answer,
        reference_answer,
        answer_key_version_id,
        rubric_version_id,
        max_score,
        rubric_points,
    };
    request.validate()?;
    Ok(request)
}

/// 将成功的 answer_grade run 固化为追加式分析。这里只更新机器建议，不写老师成绩。
pub fn record_short_answer_grade_ai_run(
    conn: &mut Connection,
    ai_run_id: i64,
) -> CoreResult<ShortAnswerGradeAnalysis> {
    if let Some(existing) = get_short_answer_analysis_by_run(conn, ai_run_id)? {
        return Ok(existing);
    }
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    if run.run_type != "answer_grade"
        || run.source_module != "exam"
        || run.business_ref_type != "subjective_transcription_revision"
        || run.status != AiRunStatus::Succeeded
    {
        return Err(CoreError::Invalid(
            "当前 AI run 不是已成功的答题卡简答题评分".into(),
        ));
    }
    let transcription_id = run
        .business_ref_id
        .parse::<i64>()
        .map_err(|_| CoreError::Invalid("简答题评分业务引用无效".into()))?;
    let request = load_short_answer_grade_request(conn, transcription_id)?;
    if run.input_artifact_id != Some(request.crop_artifact_id)
        || run.input_hash != request.input_hash()?
    {
        return Err(CoreError::Invalid(
            "简答题评分 run 与当前转写/裁剪/版本不一致".into(),
        ));
    }
    let output_json = run
        .output_json
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("成功的简答题评分缺少输出".into()))?;
    let output: ShortAnswerGradeOutput = serde_json::from_str(output_json)
        .map_err(|error| CoreError::Parse(format!("简答题评分输出损坏：{error}")))?;
    output.validate_against(&request)?;
    if output.descriptor.provider != run.provider
        || output.descriptor.model_name != run.model_name
        || output.descriptor.model_version != run.model_version
        || output.descriptor.config_version != run.config_version
        || output.descriptor.rule_version != run.prompt_or_rule_version
    {
        return Err(CoreError::Invalid(
            "简答题评分输出的模型描述与 AI run 不一致".into(),
        ));
    }
    let suggestion = ensure_subjective_suggestion(conn, transcription_id)?;
    if suggestion.outcome != "unscored" {
        return Err(CoreError::Invalid("简答题基础建议状态异常".into()));
    }
    let outcome = if output.suggested_score <= 0.000_001 {
        "incorrect"
    } else if (output.suggested_score - request.max_score).abs() <= 0.000_001 {
        "correct"
    } else {
        "partial"
    };
    let exclusion_reason = match output.state {
        ShortAnswerGradeState::Ready => "SHORT_ANSWER_TEACHER_REVIEW_REQUIRED".to_string(),
        ShortAnswerGradeState::NeedsReview => format!(
            "SHORT_ANSWER_MACHINE_REVIEW_REQUIRED:{}",
            output.issue_codes.join(",")
        ),
    };
    let tx = conn.transaction()?;
    tx.execute(
        "UPDATE exam_short_answer_grade_analyses_v2 SET state='superseded'
         WHERE transcription_revision_id=?1 AND state='active'",
        [transcription_id],
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    tx.execute(
        "INSERT INTO exam_short_answer_grade_analyses_v2
         (public_id,transcription_revision_id,suggestion_id,attempt_id,assessment_item_id,
          answer_key_version_id,rubric_version_id,machine_grade_ai_run_id,outcome,
          suggested_score,result_json,confidence,exclusion_reason,state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'active',?14)",
        rusqlite::params![
            &public_id,
            transcription_id,
            suggestion.id,
            request.attempt_id,
            request.assessment_item_id,
            request.answer_key_version_id,
            request.rubric_version_id,
            ai_run_id,
            outcome,
            output.suggested_score,
            output_json,
            output.confidence,
            &exclusion_reason,
            &now,
        ],
    )?;
    let analysis_id = tx.last_insert_rowid();
    let analysis = tx
        .query_row(
            "SELECT id,public_id,transcription_revision_id,suggestion_id,attempt_id,
                    assessment_item_id,answer_key_version_id,rubric_version_id,
                    machine_grade_ai_run_id,outcome,suggested_score,result_json,
                    confidence,exclusion_reason,state,created_at
             FROM exam_short_answer_grade_analyses_v2 WHERE id=?1",
            [analysis_id],
            short_answer_analysis_row,
        )
        .map_err(CoreError::from)?;
    tx.commit()?;
    Ok(analysis)
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
                COALESCE(analysis.machine_grade_ai_run_id,s.machine_grade_ai_run_id),
                COALESCE(analysis.outcome,s.outcome),
                COALESCE(analysis.suggested_score,s.suggested_score),
                COALESCE(analysis.result_json,s.result_json),
                COALESCE(analysis.confidence,s.confidence),
                s.batch_eligible,COALESCE(analysis.exclusion_reason,s.exclusion_reason),s.state,
                analysis.id,item.score,t.question_type,t.result_state,
                COALESCE(t.teacher_corrected_text,t.normalized_text,t.raw_ocr_text),
                t.state,region.state,region.decision
         FROM exam_subjective_grade_suggestions_v2 s
         JOIN exam_subjective_transcription_revisions_v2 t
           ON t.id=s.transcription_revision_id
         LEFT JOIN exam_short_answer_grade_analyses_v2 analysis
           ON analysis.suggestion_id=s.id AND analysis.state='active'
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
                short_answer_analysis_id: row.get(15)?,
                max_score: row.get(16)?,
                question_type: row.get(17)?,
                transcription_result_state: row.get(18)?,
                effective_text: row.get(19)?,
                transcription_state: row.get(20)?,
                region_state: row.get(21)?,
                region_decision: row.get(22)?,
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
    if let Some(analysis_id) = scope.short_answer_analysis_id {
        let machine_grade_ai_run_id = scope
            .suggestion
            .machine_grade_ai_run_id
            .ok_or_else(|| CoreError::Invalid("简答题评分分析缺少 answer_grade run".into()))?;
        conn.execute(
            "INSERT INTO exam_grade_decision_short_answer_sources_v2
             (grade_decision_id,analysis_id,machine_grade_ai_run_id,reviewed_by,created_at)
             VALUES (?1,?2,?3,?4,?5)
             ON CONFLICT(grade_decision_id) DO NOTHING",
            (
                decision_id,
                analysis_id,
                machine_grade_ai_run_id,
                reviewed_by,
                created_at,
            ),
        )?;
        let linked: (i64, i64, String) = conn.query_row(
            "SELECT analysis_id,machine_grade_ai_run_id,reviewed_by
             FROM exam_grade_decision_short_answer_sources_v2
             WHERE grade_decision_id=?1",
            [decision_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
        if linked
            != (
                analysis_id,
                machine_grade_ai_run_id,
                reviewed_by.to_string(),
            )
        {
            return Err(CoreError::Invalid(
                "评分 revision 已绑定不同的简答题分析".into(),
            ));
        }
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

fn expected_subjective_components(
    conn: &Connection,
    scope: &ReviewScope,
) -> CoreResult<Vec<ExpectedSubjectiveComponent>> {
    let components = if scope.question_type == "fill_blank" {
        let mut stmt = conn.prepare(
            "SELECT public_id,stable_id,order_index,max_score
             FROM k1_answer_slots
             WHERE answer_key_version_id=?1
             ORDER BY order_index,id",
        )?;
        let rows = stmt.query_map([scope.suggestion.answer_key_version_id], |row| {
            Ok(ExpectedSubjectiveComponent {
                source_type: "answer_slot",
                source_public_id: row.get(0)?,
                stable_id: row.get(1)?,
                order_index: row.get(2)?,
                max_score: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    } else if scope.question_type == "short_answer" {
        let mut stmt = conn.prepare(
            "SELECT public_id,stable_id,order_index,max_score
             FROM k1_rubric_points
             WHERE rubric_version_id=?1
             ORDER BY order_index,id",
        )?;
        let rows = stmt.query_map([scope.suggestion.rubric_version_id], |row| {
            Ok(ExpectedSubjectiveComponent {
                source_type: "rubric_point",
                source_public_id: row.get(0)?,
                stable_id: row.get(1)?,
                order_index: row.get(2)?,
                max_score: row.get(3)?,
            })
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        return Err(CoreError::Invalid("当前题型不支持逐项人工终审".into()));
    };
    if components.is_empty() {
        return Err(CoreError::Invalid(
            "当前主观题没有已确认的槽位或评分点".into(),
        ));
    }
    let component_max: f64 = components.iter().map(|component| component.max_score).sum();
    if (component_max - scope.max_score).abs() > 0.000_001 {
        return Err(CoreError::Invalid(
            "槽位或评分点分值之和与题目总分不一致，请先修正答案规则".into(),
        ));
    }
    Ok(components)
}

fn normalized_optional(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn component_evidence_matches(
    question_type: &str,
    effective_text: &str,
    evidence_text: &str,
) -> bool {
    if question_type == "fill_blank" {
        normalize_fill_value(effective_text).contains(&normalize_fill_value(evidence_text))
    } else {
        effective_text.contains(evidence_text)
    }
}

/// 老师按填空槽位或简答评分点逐项终审。
///
/// 总分由逐项得分自动汇总，所有当前槽位/评分点都必须提交且只能提交一次。保存时先写
/// 追加式 grade decision，再绑定当前题区/转写/机器分析，最后写不可变逐项账本；任一步
/// 失败都会回滚。旧的整题人工总分接口继续兼容，但不会产生伪精确逐项学习证据。
pub fn correct_subjective_components(
    conn: &Connection,
    suggestion_id: i64,
    components: &[SubjectiveComponentGradeInput],
    teacher_note: &str,
    reviewed_by: &str,
) -> CoreResult<GradeDecision> {
    required(reviewed_by, "终审人")?;
    required(teacher_note, "人工判定依据")?;
    let tx = conn.unchecked_transaction()?;
    let scope = review_scope(&tx, suggestion_id)?;
    validate_active_review(&scope)?;
    let expected = expected_subjective_components(&tx, &scope)?;

    let mut supplied = std::collections::BTreeMap::new();
    for input in components {
        required(&input.source_type, "逐项来源类型")?;
        required(&input.source_public_id, "逐项来源")?;
        let key = (
            input.source_type.trim().to_owned(),
            input.source_public_id.trim().to_owned(),
        );
        if supplied.insert(key, input).is_some() {
            return Err(CoreError::Invalid("同一槽位或评分点不能重复提交".into()));
        }
    }

    let mut results = Vec::with_capacity(expected.len());
    for component in expected {
        let key = (
            component.source_type.to_owned(),
            component.source_public_id.clone(),
        );
        let input = supplied
            .remove(&key)
            .ok_or_else(|| CoreError::Invalid("必须完整提交当前全部槽位或评分点".into()))?;
        if !input.teacher_score.is_finite()
            || input.teacher_score < 0.0
            || input.teacher_score > component.max_score + 0.000_001
        {
            return Err(CoreError::Invalid(format!(
                "{} 的得分必须位于 0~{} 分",
                component.stable_id, component.max_score
            )));
        }
        let evidence_text = normalized_optional(input.evidence_text.as_deref());
        if input.teacher_score > 0.000_001 && evidence_text.is_none() {
            return Err(CoreError::Invalid(format!(
                "{} 给分时必须填写学生作答证据",
                component.stable_id
            )));
        }
        if scope.transcription_result_state == "recognized" {
            if let (Some(actual), Some(evidence)) =
                (scope.effective_text.as_deref(), evidence_text.as_deref())
            {
                if !component_evidence_matches(&scope.question_type, actual, evidence) {
                    return Err(CoreError::Invalid(format!(
                        "{} 的给分证据不是当前学生转写原文片段",
                        component.stable_id
                    )));
                }
            }
        }
        let result_status = if input.teacher_score <= 0.000_001 {
            "incorrect"
        } else if input.teacher_score >= component.max_score - 0.000_001 {
            "correct"
        } else {
            "partial"
        };
        results.push(SubjectiveComponentGradeResult {
            source_type: component.source_type.to_owned(),
            source_public_id: component.source_public_id,
            stable_id: component.stable_id,
            order_index: component.order_index,
            teacher_score: input.teacher_score,
            max_score: component.max_score,
            result_status: result_status.to_owned(),
            evidence_text,
            teacher_note: normalized_optional(input.teacher_note.as_deref()),
        });
    }
    if !supplied.is_empty() {
        return Err(CoreError::Invalid(
            "提交内容包含不属于当前答案版本的槽位或评分点".into(),
        ));
    }

    let teacher_score: f64 = results.iter().map(|result| result.teacher_score).sum();
    let machine_result: Value = serde_json::from_str(&scope.suggestion.result_json)
        .map_err(|error| CoreError::Parse(format!("主观题建议 JSON 损坏：{error}")))?;
    let point_results_json = serde_json::json!({
        "schema_version": 2,
        "source": "answer_sheet_subjective_teacher_component_correction",
        "suggestion_id": suggestion_id,
        "transcription_revision_id": scope.suggestion.transcription_revision_id,
        "question_type": scope.question_type,
        "machine_result": machine_result,
        "component_results": results
    })
    .to_string();
    let decision = decide_grade_in_transaction(
        &tx,
        &NewGradeDecision {
            attempt_id: scope.suggestion.attempt_id,
            assessment_item_id: scope.suggestion.assessment_item_id,
            machine_grade_ai_run_id: scope.suggestion.machine_grade_ai_run_id,
            teacher_score,
            point_results_json: &point_results_json,
            teacher_note: Some(teacher_note.trim()),
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
    for result in &results {
        tx.execute(
            "INSERT INTO exam_grade_decision_subjective_components_v2
             (public_id,grade_decision_id,source_type,source_public_id,stable_id,
              order_index,teacher_score,max_score,result_status,evidence_text,
              teacher_note,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(grade_decision_id,source_type,source_public_id) DO NOTHING",
            (
                ids::new_public_id(),
                decision.id,
                &result.source_type,
                &result.source_public_id,
                &result.stable_id,
                result.order_index,
                result.teacher_score,
                result.max_score,
                &result.result_status,
                result.evidence_text.as_deref(),
                result.teacher_note.as_deref(),
                &now,
            ),
        )?;
        let stored: (
            String,
            i64,
            f64,
            f64,
            String,
            Option<String>,
            Option<String>,
        ) = tx.query_row(
            "SELECT stable_id,order_index,teacher_score,max_score,result_status,
                        evidence_text,teacher_note
                 FROM exam_grade_decision_subjective_components_v2
                 WHERE grade_decision_id=?1 AND source_type=?2 AND source_public_id=?3",
            (decision.id, &result.source_type, &result.source_public_id),
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )?;
        let expected_stored = (
            result.stable_id.clone(),
            result.order_index,
            result.teacher_score,
            result.max_score,
            result.result_status.clone(),
            result.evidence_text.clone(),
            result.teacher_note.clone(),
        );
        if stored != expected_stored {
            return Err(CoreError::Invalid(
                "评分 revision 已绑定不同的逐项人工结论".into(),
            ));
        }
    }
    let component_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM exam_grade_decision_subjective_components_v2
         WHERE grade_decision_id=?1",
        [decision.id],
        |row| row.get(0),
    )?;
    if component_count != results.len() as i64 {
        return Err(CoreError::Invalid("逐项人工结论未完整写入".into()));
    }
    tx.commit()?;
    Ok(decision)
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

/// 将一次老师已判满分的填空写法加入未来答案版本。
///
/// 该操作只允许引用当前 active 的人工修正评分 revision；它会以当前作业的最新确认版本
/// 为基线，追加 K1 答案版本和 assessment version。当前 attempt、评分、发布和学习证据
/// 均不重新绑定，也不重新计算。
pub fn promote_fill_accepted_answer(
    conn: &mut Connection,
    grade_decision_id: i64,
    confirmed_by: &str,
) -> CoreResult<AcceptedAnswerPromotionResult> {
    required(confirmed_by, "答案版本确认人")?;
    if let Some(existing) =
        promotion_result_by_decision(conn, grade_decision_id, "already_promoted")?
    {
        return Ok(existing);
    }

    let source = conn
        .query_row(
            "SELECT decision.id,suggestion.id,transcription.id,
                    COALESCE(transcription.teacher_corrected_text,
                             transcription.normalized_text),
                    source_version.id,source_item.id,source_item.answer_key_version_id,
                    source_version.assessment_id,source_item.question_version_id
             FROM exam_grade_decisions_v2 decision
             JOIN exam_grade_decision_subjective_sources_v2 decision_source
               ON decision_source.grade_decision_id=decision.id
             JOIN exam_subjective_grade_suggestions_v2 suggestion
               ON suggestion.id=decision_source.suggestion_id AND suggestion.state='active'
             JOIN exam_subjective_transcription_revisions_v2 transcription
               ON transcription.id=decision_source.transcription_revision_id
              AND transcription.id=suggestion.transcription_revision_id
              AND transcription.state='active' AND transcription.result_state='recognized'
             JOIN exam_answer_region_revisions_v2 region
               ON region.id=transcription.answer_region_revision_id
              AND region.state='active' AND region.decision='teacher_confirmed'
             JOIN exam_attempts_v2 attempt
               ON attempt.id=decision.attempt_id AND attempt.state<>'voided'
             JOIN exam_assessment_items_v2 source_item
               ON source_item.id=decision.assessment_item_id
              AND source_item.assessment_version_id=attempt.assessment_version_id
              AND source_item.answer_key_version_id=suggestion.answer_key_version_id
              AND source_item.state='active'
             JOIN exam_assessment_versions_v2 source_version
               ON source_version.id=source_item.assessment_version_id
              AND source_version.state='confirmed'
             JOIN k1_question_versions question
               ON question.id=source_item.question_version_id
              AND question.question_type='fill_blank'
             WHERE decision.id=?1 AND decision.state='active'
               AND decision.confirmation_level='teacher_corrected'
               AND ABS(decision.teacher_score-source_item.score) <= 0.000001",
            [grade_decision_id],
            |row| {
                Ok(AcceptedAnswerSourceScope {
                    grade_decision_id: row.get(0)?,
                    suggestion_id: row.get(1)?,
                    transcription_revision_id: row.get(2)?,
                    accepted_text: row.get(3)?,
                    source_assessment_version_id: row.get(4)?,
                    source_assessment_item_id: row.get(5)?,
                    source_answer_key_version_id: row.get(6)?,
                    assessment_id: row.get(7)?,
                    question_version_id: row.get(8)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid(
                "只有老师查看原图后人工判满分的当前填空 revision 才能加入答案库".into(),
            )
        })?;
    let accepted_text = source.accepted_text.trim().to_owned();
    let normalized_text = normalize_fill_value(&accepted_text);
    if normalized_text.is_empty() {
        return Err(CoreError::Invalid("可接受写法不能为空".into()));
    }
    if let Some(existing) = promotion_result_by_identity(
        conn,
        source.assessment_id,
        source.question_version_id,
        &normalized_text,
    )? {
        return Ok(existing);
    }

    let (base_assessment_version_id, base_assessment_revision): (i64, i64) = conn.query_row(
        "SELECT id,revision FROM exam_assessment_versions_v2
             WHERE assessment_id=?1 AND state='confirmed'
             ORDER BY revision DESC,id DESC LIMIT 1",
        [source.assessment_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let base = conn
        .query_row(
            "SELECT item.id,item.assessment_version_id,item.answer_key_version_id,
                    answer.answer_json
             FROM exam_assessment_items_v2 item
             JOIN k1_answer_key_versions answer
               ON answer.id=item.answer_key_version_id AND answer.state='confirmed'
             WHERE item.assessment_version_id=?1 AND item.question_version_id=?2
               AND item.state='active'",
            (base_assessment_version_id, source.question_version_id),
            |row| {
                Ok(AcceptedAnswerBaseItem {
                    id: row.get(0)?,
                    assessment_version_id: row.get(1)?,
                    answer_key_version_id: row.get(2)?,
                    answer_json: row.get(3)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid("最新作业版本已不包含该填空题，请先在题库中人工维护".into())
        })?;
    let mut slot_stmt = conn.prepare(
        "SELECT stable_id,order_index,canonical_answers_json,
                normalization_rules_json,max_score
         FROM k1_answer_slots WHERE answer_key_version_id=?1
         ORDER BY order_index,id",
    )?;
    let mut slots = slot_stmt
        .query_map([base.answer_key_version_id], |row| {
            Ok(AcceptedAnswerSlot {
                stable_id: row.get(0)?,
                order_index: row.get(1)?,
                canonical_answers_json: row.get(2)?,
                normalization_rules_json: row.get(3)?,
                max_score: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(slot_stmt);
    if slots.len() != 1 {
        return Err(CoreError::Invalid(
            "当前只允许把单槽位填空写法加入答案库；多槽位题请在题库中逐槽维护".into(),
        ));
    }

    let mut answer_json: Value = serde_json::from_str(&base.answer_json)
        .map_err(|error| CoreError::Parse(format!("当前填空答案 JSON 损坏：{error}")))?;
    let mut slot_json: Value = serde_json::from_str(&slots[0].canonical_answers_json)
        .map_err(|error| CoreError::Parse(format!("当前填空槽位答案 JSON 损坏：{error}")))?;
    let mut existing_values = fill_answer_values(&answer_json);
    existing_values.extend(string_values(slot_json.get("answers")));
    existing_values.extend(string_values(slot_json.get("accepted_variants")));
    if existing_values
        .iter()
        .any(|value| normalize_fill_value(value) == normalized_text)
    {
        return Ok(AcceptedAnswerPromotionResult {
            outcome: "already_available".into(),
            promotion_id: None,
            accepted_text,
            adopted_assessment_version_id: base_assessment_version_id,
            adopted_assessment_revision: base_assessment_revision,
            adopted_answer_key_version_id: base.answer_key_version_id,
            current_grade_unchanged: true,
            current_publication_unchanged: true,
        });
    }
    append_unique_string(&mut answer_json, "accepted_variants", &accepted_text)?;
    if let Some(answer_slots) = answer_json.get_mut("slots").and_then(Value::as_array_mut) {
        if answer_slots.len() != 1 {
            return Err(CoreError::Invalid(
                "答案 JSON 含多个槽位，请在题库中逐槽维护可接受写法".into(),
            ));
        }
        append_unique_string(&mut answer_slots[0], "accepted_variants", &accepted_text)?;
    }
    append_unique_string(&mut slot_json, "accepted_variants", &accepted_text)?;
    slots[0].canonical_answers_json = slot_json.to_string();

    let now = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    let answer_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_answer_key_versions
         WHERE question_version_id=?1",
        [source.question_version_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO k1_answer_key_versions
         (public_id,question_version_id,revision,answer_json,state,
          supersedes_answer_key_id,created_at,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,'confirmed',?5,?6,?7,?6)",
        rusqlite::params![
            ids::new_public_id(),
            source.question_version_id,
            answer_revision,
            answer_json.to_string(),
            base.answer_key_version_id,
            &now,
            confirmed_by.trim(),
        ],
    )?;
    let adopted_answer_key_version_id = tx.last_insert_rowid();
    for slot in &slots {
        tx.execute(
            "INSERT INTO k1_answer_slots
             (public_id,stable_id,answer_key_version_id,order_index,
              canonical_answers_json,normalization_rules_json,max_score,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            rusqlite::params![
                ids::new_public_id(),
                &slot.stable_id,
                adopted_answer_key_version_id,
                slot.order_index,
                &slot.canonical_answers_json,
                slot.normalization_rules_json.as_deref(),
                slot.max_score,
                &now,
            ],
        )?;
    }

    let adopted_assessment_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_assessment_versions_v2
         WHERE assessment_id=?1",
        [source.assessment_id],
        |row| row.get(0),
    )?;
    let template_version: Option<String> = tx.query_row(
        "SELECT template_version FROM exam_assessment_versions_v2 WHERE id=?1",
        [base.assessment_version_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO exam_assessment_versions_v2
         (public_id,assessment_id,revision,template_version,state,
          supersedes_version_id,created_at)
         VALUES (?1,?2,?3,?4,'draft',?5,?6)",
        rusqlite::params![
            ids::new_public_id(),
            source.assessment_id,
            adopted_assessment_revision,
            template_version.as_deref(),
            base.assessment_version_id,
            &now,
        ],
    )?;
    let adopted_assessment_version_id = tx.last_insert_rowid();
    let mut clone_stmt = tx.prepare(
        "SELECT question_version_id,answer_key_version_id,rubric_version_id,link_set_id,
                order_index,score,option_order_json,presentation_snapshot_json
         FROM exam_assessment_items_v2
         WHERE assessment_version_id=?1 AND state='active'
         ORDER BY order_index,id",
    )?;
    let clone_items = clone_stmt
        .query_map([base.assessment_version_id], |row| {
            Ok(AssessmentItemClone {
                question_version_id: row.get(0)?,
                answer_key_version_id: row.get(1)?,
                rubric_version_id: row.get(2)?,
                link_set_id: row.get(3)?,
                order_index: row.get(4)?,
                score: row.get(5)?,
                option_order_json: row.get(6)?,
                presentation_snapshot_json: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(clone_stmt);
    let mut hash_items = Vec::with_capacity(clone_items.len());
    let mut adopted_assessment_item_id = None;
    for item in clone_items {
        let answer_key_version_id = if item.question_version_id == source.question_version_id {
            adopted_answer_key_version_id
        } else {
            item.answer_key_version_id
        };
        tx.execute(
            "INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,option_order_json,
              presentation_snapshot_json,state,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11)",
            rusqlite::params![
                ids::new_public_id(),
                adopted_assessment_version_id,
                item.question_version_id,
                answer_key_version_id,
                item.rubric_version_id,
                item.link_set_id,
                item.order_index,
                item.score,
                item.option_order_json.as_deref(),
                &item.presentation_snapshot_json,
                &now,
            ],
        )?;
        let inserted_item_id = tx.last_insert_rowid();
        if item.question_version_id == source.question_version_id {
            adopted_assessment_item_id = Some(inserted_item_id);
        }
        hash_items.push(AcceptedAnswerAssessmentHashItem {
            item_id: inserted_item_id,
            question_version_id: item.question_version_id,
            answer_key_version_id,
            rubric_version_id: item.rubric_version_id,
            link_set_id: item.link_set_id,
            order_index: item.order_index,
            score_millis: (item.score * 1000.0).round() as i64,
        });
    }
    let adopted_assessment_item_id = adopted_assessment_item_id
        .ok_or_else(|| CoreError::Invalid("新作业版本未复制目标填空题，已回滚".into()))?;
    let item_set_hash = hashing::sha256_hex(
        &serde_json::to_vec(&hash_items)
            .map_err(|error| CoreError::Parse(format!("新作业版本 hash 失败：{error}")))?,
    );
    tx.execute(
        "UPDATE exam_assessment_versions_v2
         SET item_set_hash=?1,state='confirmed',confirmed_by=?2,confirmed_at=?3
         WHERE id=?4 AND state='draft'",
        (
            &item_set_hash,
            confirmed_by.trim(),
            &now,
            adopted_assessment_version_id,
        ),
    )?;
    tx.execute(
        "UPDATE exam_assessments_v2 SET updated_at=?1 WHERE id=?2",
        (&now, source.assessment_id),
    )?;

    let promotion_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO exam_accepted_answer_promotions_v2
         (public_id,grade_decision_id,suggestion_id,transcription_revision_id,
          assessment_id,question_version_id,source_assessment_version_id,
          source_assessment_item_id,source_answer_key_version_id,
          base_assessment_version_id,base_assessment_item_id,base_answer_key_version_id,
          adopted_assessment_version_id,adopted_assessment_item_id,
          adopted_answer_key_version_id,answer_slot_stable_id,accepted_text,
          normalized_text,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        rusqlite::params![
            &promotion_public_id,
            source.grade_decision_id,
            source.suggestion_id,
            source.transcription_revision_id,
            source.assessment_id,
            source.question_version_id,
            source.source_assessment_version_id,
            source.source_assessment_item_id,
            source.source_answer_key_version_id,
            base.assessment_version_id,
            base.id,
            base.answer_key_version_id,
            adopted_assessment_version_id,
            adopted_assessment_item_id,
            adopted_answer_key_version_id,
            &slots[0].stable_id,
            &accepted_text,
            &normalized_text,
            confirmed_by.trim(),
            &now,
        ],
    )?;
    let promotion_id = tx.last_insert_rowid();
    let audit_meta = serde_json::json!({
        "schema_version": 1,
        "grade_decision_id": source.grade_decision_id,
        "source_assessment_version_id": source.source_assessment_version_id,
        "base_assessment_version_id": base.assessment_version_id,
        "adopted_assessment_version_id": adopted_assessment_version_id,
        "adopted_answer_key_version_id": adopted_answer_key_version_id,
        "current_grade_unchanged": true,
        "current_publication_unchanged": true,
    })
    .to_string();
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!("exam:accepted-answer-promotion:{promotion_public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "exam.accepted_answer.promoted",
            object_type: "exam_accepted_answer_promotion",
            object_id: &promotion_public_id,
            object_revision: Some(adopted_assessment_revision),
            note: None,
            meta_json: Some(&audit_meta),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    Ok(AcceptedAnswerPromotionResult {
        outcome: "created_new_version".into(),
        promotion_id: Some(promotion_id),
        accepted_text,
        adopted_assessment_version_id,
        adopted_assessment_revision,
        adopted_answer_key_version_id,
        current_grade_unchanged: true,
        current_publication_unchanged: true,
    })
}

fn normalize_rubric_evidence(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn rubric_evidence_promotion_result(
    conn: &Connection,
    predicate: &str,
    values: &[&dyn rusqlite::ToSql],
    outcome: &str,
) -> CoreResult<Option<RubricEvidencePromotionResult>> {
    let sql = format!(
        "SELECT promotion.id,promotion.rubric_point_stable_id,promotion.evidence_text,
                promotion.adopted_assessment_version_id,version.revision,
                promotion.adopted_rubric_version_id,promotion.adopted_link_set_id,
                (SELECT COUNT(*) FROM k1_knowledge_links
                 WHERE link_set_id=promotion.adopted_link_set_id),
                (SELECT COUNT(*) FROM k1_ability_links
                 WHERE link_set_id=promotion.adopted_link_set_id)
         FROM exam_rubric_evidence_promotions_v2 promotion
         JOIN exam_assessment_versions_v2 version
           ON version.id=promotion.adopted_assessment_version_id
         WHERE {predicate}"
    );
    conn.query_row(&sql, values, |row| {
        Ok(RubricEvidencePromotionResult {
            outcome: outcome.into(),
            promotion_id: row.get(0)?,
            rubric_point_stable_id: row.get(1)?,
            evidence_text: row.get(2)?,
            adopted_assessment_version_id: row.get(3)?,
            adopted_assessment_revision: row.get(4)?,
            adopted_rubric_version_id: row.get(5)?,
            adopted_link_set_id: row.get(6)?,
            carried_knowledge_link_count: row.get(7)?,
            carried_ability_link_count: row.get(8)?,
            current_grade_unchanged: true,
            current_publication_unchanged: true,
        })
    })
    .optional()
    .map_err(Into::into)
}

/// 将老师逐点评分时引用的一条学生原文，加入未来简答题评分点的允许改述。
///
/// 只有当前 active、老师修正且已逐评分点给分的 revision 可以触发。服务会从最新确认
/// 作业版本继续追加 rubric、link set 和 assessment version；当前学生成绩、历史发布和
/// learning evidence 均不改写。
pub fn promote_short_answer_rubric_evidence(
    conn: &mut Connection,
    grade_decision_id: i64,
    source_public_id: &str,
    confirmed_by: &str,
) -> CoreResult<RubricEvidencePromotionResult> {
    required(source_public_id, "评分点")?;
    required(confirmed_by, "评分规则确认人")?;

    if let Some(existing) = rubric_evidence_promotion_result(
        conn,
        "promotion.grade_decision_id=?1
         AND EXISTS (
           SELECT 1 FROM exam_grade_decision_subjective_components_v2 component
           WHERE component.id=promotion.component_id
             AND component.source_public_id=?2
         )",
        &[&grade_decision_id, &source_public_id.trim()],
        "already_promoted",
    )? {
        return Ok(existing);
    }

    let source = conn
        .query_row(
            "SELECT decision.id,component.id,suggestion.id,transcription.id,
                    component.evidence_text,component.stable_id,
                    source_version.id,source_item.id,source_item.rubric_version_id,
                    source_version.assessment_id,source_item.question_version_id
             FROM exam_grade_decisions_v2 decision
             JOIN exam_grade_decision_subjective_sources_v2 decision_source
               ON decision_source.grade_decision_id=decision.id
             JOIN exam_grade_decision_subjective_components_v2 component
               ON component.grade_decision_id=decision.id
              AND component.source_type='rubric_point'
              AND component.source_public_id=?2
              AND component.teacher_score > 0.000001
              AND component.evidence_text IS NOT NULL
             JOIN exam_subjective_grade_suggestions_v2 suggestion
               ON suggestion.id=decision_source.suggestion_id AND suggestion.state='active'
             JOIN exam_subjective_transcription_revisions_v2 transcription
               ON transcription.id=decision_source.transcription_revision_id
              AND transcription.id=suggestion.transcription_revision_id
              AND transcription.state='active' AND transcription.result_state='recognized'
             JOIN exam_attempts_v2 attempt
               ON attempt.id=decision.attempt_id AND attempt.state<>'voided'
             JOIN exam_assessment_items_v2 source_item
               ON source_item.id=decision.assessment_item_id
              AND source_item.assessment_version_id=attempt.assessment_version_id
              AND source_item.rubric_version_id=suggestion.rubric_version_id
              AND source_item.state='active'
             JOIN exam_assessment_versions_v2 source_version
               ON source_version.id=source_item.assessment_version_id
              AND source_version.state='confirmed'
             JOIN k1_question_versions question
               ON question.id=source_item.question_version_id
              AND question.question_type='short_answer'
             WHERE decision.id=?1 AND decision.state='active'
               AND decision.confirmation_level='teacher_corrected'",
            (grade_decision_id, source_public_id.trim()),
            |row| {
                Ok(RubricEvidenceSourceScope {
                    grade_decision_id: row.get(0)?,
                    component_id: row.get(1)?,
                    suggestion_id: row.get(2)?,
                    transcription_revision_id: row.get(3)?,
                    evidence_text: row.get(4)?,
                    rubric_point_stable_id: row.get(5)?,
                    source_assessment_version_id: row.get(6)?,
                    source_assessment_item_id: row.get(7)?,
                    source_rubric_version_id: row.get(8)?,
                    assessment_id: row.get(9)?,
                    question_version_id: row.get(10)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid(
                "只有老师查看原图、逐评分点给分并引用学生原文的当前简答题 revision 才能更新评分规则"
                    .into(),
            )
        })?;
    let evidence_text = source.evidence_text.trim().to_owned();
    let normalized_text = normalize_rubric_evidence(&evidence_text);
    if normalized_text.is_empty() {
        return Err(CoreError::Invalid("评分点证据不能为空".into()));
    }
    if let Some(existing) = rubric_evidence_promotion_result(
        conn,
        "promotion.assessment_id=?1 AND promotion.question_version_id=?2
         AND promotion.rubric_point_stable_id=?3 AND promotion.normalized_text=?4",
        &[
            &source.assessment_id,
            &source.question_version_id,
            &source.rubric_point_stable_id,
            &normalized_text,
        ],
        "already_promoted",
    )? {
        return Ok(existing);
    }

    let (base_assessment_version_id, base_assessment_revision): (i64, i64) = conn.query_row(
        "SELECT id,revision FROM exam_assessment_versions_v2
         WHERE assessment_id=?1 AND state='confirmed'
         ORDER BY revision DESC,id DESC LIMIT 1",
        [source.assessment_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let base = conn
        .query_row(
            "SELECT item.id,item.assessment_version_id,item.rubric_version_id,item.link_set_id
             FROM exam_assessment_items_v2 item
             JOIN k1_rubric_versions rubric
               ON rubric.id=item.rubric_version_id AND rubric.state='confirmed'
             JOIN k1_link_sets links
               ON links.id=item.link_set_id AND links.state='confirmed'
             WHERE item.assessment_version_id=?1 AND item.question_version_id=?2
               AND item.state='active'",
            (base_assessment_version_id, source.question_version_id),
            |row| {
                Ok(RubricEvidenceBaseItem {
                    id: row.get(0)?,
                    assessment_version_id: row.get(1)?,
                    rubric_version_id: row.get(2)?,
                    link_set_id: row.get(3)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid("最新作业版本已不包含该简答题，请先在题库中人工维护".into())
        })?;

    let mut point_stmt = conn.prepare(
        "SELECT id,public_id,stable_id,order_index,canonical_text,
                allowed_paraphrases_json,required_concepts_json,max_score
         FROM k1_rubric_points WHERE rubric_version_id=?1
         ORDER BY order_index,id",
    )?;
    let mut points = point_stmt
        .query_map([base.rubric_version_id], |row| {
            Ok(RubricPointClone {
                id: row.get(0)?,
                public_id: row.get(1)?,
                stable_id: row.get(2)?,
                order_index: row.get(3)?,
                canonical_text: row.get(4)?,
                allowed_paraphrases_json: row.get(5)?,
                required_concepts_json: row.get(6)?,
                max_score: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(point_stmt);
    let target_point = points
        .iter_mut()
        .find(|point| point.stable_id == source.rubric_point_stable_id)
        .ok_or_else(|| {
            CoreError::Invalid("最新评分规则已不存在该评分点，请老师在题库中人工维护".into())
        })?;
    let mut allowed = json_string_array(target_point.allowed_paraphrases_json.clone(), "允许改述")?;
    let already_available = normalize_rubric_evidence(&target_point.canonical_text)
        == normalized_text
        || allowed
            .iter()
            .any(|value| normalize_rubric_evidence(value) == normalized_text);
    if already_available {
        return Ok(RubricEvidencePromotionResult {
            outcome: "already_available".into(),
            promotion_id: None,
            rubric_point_stable_id: source.rubric_point_stable_id,
            evidence_text,
            adopted_assessment_version_id: base.assessment_version_id,
            adopted_assessment_revision: base_assessment_revision,
            adopted_rubric_version_id: base.rubric_version_id,
            adopted_link_set_id: base.link_set_id,
            carried_knowledge_link_count: 0,
            carried_ability_link_count: 0,
            current_grade_unchanged: true,
            current_publication_unchanged: true,
        });
    }
    allowed.push(evidence_text.clone());
    target_point.allowed_paraphrases_json = Some(
        serde_json::to_string(&allowed)
            .map_err(|error| CoreError::Parse(format!("评分点允许改述序列化失败：{error}")))?,
    );

    let now = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    let rubric_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_rubric_versions
         WHERE question_version_id=?1",
        [source.question_version_id],
        |row| row.get(0),
    )?;
    let rubric_max_score: f64 = points.iter().map(|point| point.max_score).sum();
    tx.execute(
        "INSERT INTO k1_rubric_versions
         (public_id,question_version_id,revision,max_score,state,supersedes_rubric_id,
          created_at,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,'confirmed',?5,?6,?7,?6)",
        rusqlite::params![
            ids::new_public_id(),
            source.question_version_id,
            rubric_revision,
            rubric_max_score,
            base.rubric_version_id,
            &now,
            confirmed_by.trim(),
        ],
    )?;
    let adopted_rubric_version_id = tx.last_insert_rowid();
    let mut point_id_map = std::collections::BTreeMap::new();
    let mut point_public_id_map = std::collections::BTreeMap::new();
    for point in &points {
        let public_id = ids::new_public_id();
        tx.execute(
            "INSERT INTO k1_rubric_points
             (public_id,stable_id,rubric_version_id,order_index,canonical_text,
              allowed_paraphrases_json,required_concepts_json,max_score,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            rusqlite::params![
                &public_id,
                &point.stable_id,
                adopted_rubric_version_id,
                point.order_index,
                &point.canonical_text,
                point.allowed_paraphrases_json.as_deref(),
                point.required_concepts_json.as_deref(),
                point.max_score,
                &now,
            ],
        )?;
        point_id_map.insert(point.id, tx.last_insert_rowid());
        point_public_id_map.insert(point.public_id.clone(), public_id);
    }
    let mut rule_stmt = tx.prepare(
        "SELECT rubric_point_id,rule_type,rule_json
         FROM k1_contradiction_rules
         WHERE rubric_point_id IN (
           SELECT id FROM k1_rubric_points WHERE rubric_version_id=?1
         ) ORDER BY id",
    )?;
    let rules = rule_stmt
        .query_map([base.rubric_version_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(rule_stmt);
    for (old_point_id, rule_type, rule_json) in rules {
        let new_point_id = point_id_map
            .get(&old_point_id)
            .copied()
            .ok_or_else(|| CoreError::Invalid("评分点矛盾规则无法对应新版本".into()))?;
        tx.execute(
            "INSERT INTO k1_contradiction_rules
             (public_id,rubric_point_id,rule_type,rule_json,created_at)
             VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![
                ids::new_public_id(),
                new_point_id,
                &rule_type,
                &rule_json,
                &now,
            ],
        )?;
    }

    let (knowledge_map_id, base_link_state): (i64, String) = tx.query_row(
        "SELECT knowledge_map_id,state FROM k1_link_sets WHERE id=?1",
        [base.link_set_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if base_link_state != "confirmed" {
        return Err(CoreError::Invalid(
            "当前知识链接集尚未确认，不能沿用".into(),
        ));
    }
    let link_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_link_sets
         WHERE question_version_id=?1",
        [source.question_version_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO k1_link_sets
         (public_id,question_version_id,knowledge_map_id,revision,state,
          supersedes_link_set_id,created_at,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,'confirmed',?5,?6,?7,?6)",
        rusqlite::params![
            ids::new_public_id(),
            source.question_version_id,
            knowledge_map_id,
            link_revision,
            base.link_set_id,
            &now,
            confirmed_by.trim(),
        ],
    )?;
    let adopted_link_set_id = tx.last_insert_rowid();
    let mut carried_knowledge_link_count = 0;
    let mut knowledge_stmt = tx.prepare(
        "SELECT source_type,source_public_id,knowledge_node_id,relation_type,
                confirmation_level
         FROM k1_knowledge_links WHERE link_set_id=?1 ORDER BY id",
    )?;
    let knowledge_rows = knowledge_stmt
        .query_map([base.link_set_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(knowledge_stmt);
    for (source_type, old_source_public_id, node_id, relation_type, level) in knowledge_rows {
        let source_public_id = if source_type == "rubric_point" {
            point_public_id_map
                .get(&old_source_public_id)
                .cloned()
                .ok_or_else(|| CoreError::Invalid("知识链接无法对应新评分点".into()))?
        } else {
            old_source_public_id
        };
        let teacher_confirmed = level == "teacher_confirmed";
        tx.execute(
            "INSERT INTO k1_knowledge_links
             (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
              relation_type,confirmation_level,verified_by,verified_at,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            rusqlite::params![
                ids::new_public_id(),
                adopted_link_set_id,
                &source_type,
                &source_public_id,
                node_id,
                &relation_type,
                &level,
                teacher_confirmed.then_some(confirmed_by.trim()),
                teacher_confirmed.then_some(now.as_str()),
                &now,
            ],
        )?;
        carried_knowledge_link_count += 1;
    }
    let mut carried_ability_link_count = 0;
    let mut ability_stmt = tx.prepare(
        "SELECT source_type,source_public_id,ability_dimension_id,evidence_strength,
                response_mode,confirmation_level
         FROM k1_ability_links WHERE link_set_id=?1 ORDER BY id",
    )?;
    let ability_rows = ability_stmt
        .query_map([base.link_set_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(ability_stmt);
    for (source_type, old_source_public_id, dimension_id, strength, mode, level) in ability_rows {
        let source_public_id = if source_type == "rubric_point" {
            point_public_id_map
                .get(&old_source_public_id)
                .cloned()
                .ok_or_else(|| CoreError::Invalid("能力链接无法对应新评分点".into()))?
        } else {
            old_source_public_id
        };
        let teacher_confirmed = level == "teacher_confirmed";
        tx.execute(
            "INSERT INTO k1_ability_links
             (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
              evidence_strength,response_mode,confirmation_level,verified_by,verified_at,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![
                ids::new_public_id(),
                adopted_link_set_id,
                &source_type,
                &source_public_id,
                dimension_id,
                strength,
                &mode,
                &level,
                teacher_confirmed.then_some(confirmed_by.trim()),
                teacher_confirmed.then_some(now.as_str()),
                &now,
            ],
        )?;
        carried_ability_link_count += 1;
    }

    let adopted_assessment_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_assessment_versions_v2
         WHERE assessment_id=?1",
        [source.assessment_id],
        |row| row.get(0),
    )?;
    let template_version: Option<String> = tx.query_row(
        "SELECT template_version FROM exam_assessment_versions_v2 WHERE id=?1",
        [base.assessment_version_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO exam_assessment_versions_v2
         (public_id,assessment_id,revision,template_version,state,
          supersedes_version_id,created_at)
         VALUES (?1,?2,?3,?4,'draft',?5,?6)",
        rusqlite::params![
            ids::new_public_id(),
            source.assessment_id,
            adopted_assessment_revision,
            template_version.as_deref(),
            base.assessment_version_id,
            &now,
        ],
    )?;
    let adopted_assessment_version_id = tx.last_insert_rowid();
    let mut clone_stmt = tx.prepare(
        "SELECT question_version_id,answer_key_version_id,rubric_version_id,link_set_id,
                order_index,score,option_order_json,presentation_snapshot_json
         FROM exam_assessment_items_v2
         WHERE assessment_version_id=?1 AND state='active'
         ORDER BY order_index,id",
    )?;
    let clone_items = clone_stmt
        .query_map([base.assessment_version_id], |row| {
            Ok(AssessmentItemClone {
                question_version_id: row.get(0)?,
                answer_key_version_id: row.get(1)?,
                rubric_version_id: row.get(2)?,
                link_set_id: row.get(3)?,
                order_index: row.get(4)?,
                score: row.get(5)?,
                option_order_json: row.get(6)?,
                presentation_snapshot_json: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(clone_stmt);
    let mut hash_items = Vec::with_capacity(clone_items.len());
    let mut adopted_assessment_item_id = None;
    for item in clone_items {
        let is_target = item.question_version_id == source.question_version_id;
        let rubric_version_id = if is_target {
            adopted_rubric_version_id
        } else {
            item.rubric_version_id
        };
        let link_set_id = if is_target {
            adopted_link_set_id
        } else {
            item.link_set_id
        };
        tx.execute(
            "INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,option_order_json,
              presentation_snapshot_json,state,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11)",
            rusqlite::params![
                ids::new_public_id(),
                adopted_assessment_version_id,
                item.question_version_id,
                item.answer_key_version_id,
                rubric_version_id,
                link_set_id,
                item.order_index,
                item.score,
                item.option_order_json.as_deref(),
                &item.presentation_snapshot_json,
                &now,
            ],
        )?;
        let inserted_item_id = tx.last_insert_rowid();
        if is_target {
            adopted_assessment_item_id = Some(inserted_item_id);
        }
        hash_items.push(AcceptedAnswerAssessmentHashItem {
            item_id: inserted_item_id,
            question_version_id: item.question_version_id,
            answer_key_version_id: item.answer_key_version_id,
            rubric_version_id,
            link_set_id,
            order_index: item.order_index,
            score_millis: (item.score * 1000.0).round() as i64,
        });
    }
    let adopted_assessment_item_id = adopted_assessment_item_id
        .ok_or_else(|| CoreError::Invalid("新作业版本未复制目标简答题，已回滚".into()))?;
    let item_set_hash = hashing::sha256_hex(
        &serde_json::to_vec(&hash_items)
            .map_err(|error| CoreError::Parse(format!("新作业版本 hash 失败：{error}")))?,
    );
    tx.execute(
        "UPDATE exam_assessment_versions_v2
         SET item_set_hash=?1,state='confirmed',confirmed_by=?2,confirmed_at=?3
         WHERE id=?4 AND state='draft'",
        (
            &item_set_hash,
            confirmed_by.trim(),
            &now,
            adopted_assessment_version_id,
        ),
    )?;
    tx.execute(
        "UPDATE exam_assessments_v2 SET updated_at=?1 WHERE id=?2",
        (&now, source.assessment_id),
    )?;

    let promotion_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO exam_rubric_evidence_promotions_v2
         (public_id,grade_decision_id,component_id,suggestion_id,
          transcription_revision_id,assessment_id,question_version_id,
          source_assessment_version_id,source_assessment_item_id,source_rubric_version_id,
          base_assessment_version_id,base_assessment_item_id,base_rubric_version_id,
          base_link_set_id,adopted_assessment_version_id,adopted_assessment_item_id,
          adopted_rubric_version_id,adopted_link_set_id,rubric_point_stable_id,
          evidence_text,normalized_text,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,
                 ?17,?18,?19,?20,?21,?22,?23)",
        rusqlite::params![
            &promotion_public_id,
            source.grade_decision_id,
            source.component_id,
            source.suggestion_id,
            source.transcription_revision_id,
            source.assessment_id,
            source.question_version_id,
            source.source_assessment_version_id,
            source.source_assessment_item_id,
            source.source_rubric_version_id,
            base.assessment_version_id,
            base.id,
            base.rubric_version_id,
            base.link_set_id,
            adopted_assessment_version_id,
            adopted_assessment_item_id,
            adopted_rubric_version_id,
            adopted_link_set_id,
            &source.rubric_point_stable_id,
            &evidence_text,
            &normalized_text,
            confirmed_by.trim(),
            &now,
        ],
    )?;
    let promotion_id = tx.last_insert_rowid();
    let audit_meta = serde_json::json!({
        "schema_version": 1,
        "grade_decision_id": source.grade_decision_id,
        "rubric_point_stable_id": source.rubric_point_stable_id,
        "source_assessment_version_id": source.source_assessment_version_id,
        "base_assessment_version_id": base.assessment_version_id,
        "adopted_assessment_version_id": adopted_assessment_version_id,
        "adopted_rubric_version_id": adopted_rubric_version_id,
        "adopted_link_set_id": adopted_link_set_id,
        "current_grade_unchanged": true,
        "current_publication_unchanged": true,
    })
    .to_string();
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!("exam:rubric-evidence-promotion:{promotion_public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "exam.rubric_evidence.promoted",
            object_type: "exam_rubric_evidence_promotion",
            object_id: &promotion_public_id,
            object_revision: Some(adopted_assessment_revision),
            note: None,
            meta_json: Some(&audit_meta),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    Ok(RubricEvidencePromotionResult {
        outcome: "created_new_version".into(),
        promotion_id: Some(promotion_id),
        rubric_point_stable_id: source.rubric_point_stable_id,
        evidence_text,
        adopted_assessment_version_id,
        adopted_assessment_revision,
        adopted_rubric_version_id,
        adopted_link_set_id,
        carried_knowledge_link_count,
        carried_ability_link_count,
        current_grade_unchanged: true,
        current_publication_unchanged: true,
    })
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
                    'answer_slots',json_group_array(json_object(
                      'source_public_id',slot.public_id,
                      'stable_id',slot.stable_id,
                      'order_index',slot.order_index,
                      'canonical_answers_json',slot.canonical_answers_json,
                      'max_score',slot.max_score)))
                  FROM k1_answer_slots slot
                  WHERE slot.answer_key_version_id=item.answer_key_version_id),
                  '{\"schema_version\":1,\"answer_slots\":[]}'),
                COALESCE((SELECT json_object(
                    'schema_version',1,
                    'rubric_points',json_group_array(json_object(
                      'source_public_id',point.public_id,
                      'stable_id',point.stable_id,
                      'order_index',point.order_index,
                      'canonical_text',point.canonical_text,
                      'max_score',point.max_score)))
                  FROM k1_rubric_points point
                  WHERE point.rubric_version_id=item.rubric_version_id),
                  '{\"schema_version\":1,\"rubric_points\":[]}'),
                suggestion.id,analysis.id,
                COALESCE(analysis.machine_grade_ai_run_id,suggestion.machine_grade_ai_run_id),
                COALESCE(analysis.outcome,suggestion.outcome),
                COALESCE(analysis.suggested_score,suggestion.suggested_score),
                COALESCE(analysis.result_json,suggestion.result_json),
                suggestion.batch_eligible,
                COALESCE(analysis.exclusion_reason,suggestion.exclusion_reason),
                decision.id,decision.revision,decision.teacher_score,
                decision.confirmation_level,source.review_mode,
                CASE WHEN source.suggestion_id=suggestion.id
                       AND (analysis.id IS NULL OR short_source.analysis_id=analysis.id)
                     THEN 1 ELSE 0 END,
                decision.decided_at,
                COALESCE((SELECT json_object(
                    'schema_version',1,
                    'component_results',json_group_array(json_object(
                      'source_type',component.source_type,
                      'source_public_id',component.source_public_id,
                      'stable_id',component.stable_id,
                      'order_index',component.order_index,
                      'teacher_score',component.teacher_score,
                      'max_score',component.max_score,
                      'result_status',component.result_status,
                      'evidence_text',component.evidence_text,
                      'teacher_note',component.teacher_note)))
                  FROM exam_grade_decision_subjective_components_v2 component
                  WHERE component.grade_decision_id=decision.id),
                  '{\"schema_version\":1,\"component_results\":[]}'),
                promotion.id,promotion.created_at,
                COALESCE((SELECT json_object(
                    'schema_version',1,
                    'promotions',json_group_array(json_object(
                      'promotion_id',rubric_promotion.id,
                      'source_public_id',rubric_component.source_public_id,
                      'rubric_point_stable_id',rubric_promotion.rubric_point_stable_id,
                      'evidence_text',rubric_promotion.evidence_text,
                      'adopted_assessment_version_id',
                        rubric_promotion.adopted_assessment_version_id,
                      'adopted_rubric_version_id',
                        rubric_promotion.adopted_rubric_version_id,
                      'adopted_link_set_id',rubric_promotion.adopted_link_set_id,
                      'created_at',rubric_promotion.created_at)))
                  FROM exam_rubric_evidence_promotions_v2 rubric_promotion
                  JOIN exam_grade_decision_subjective_components_v2 rubric_component
                    ON rubric_component.id=rubric_promotion.component_id
                  WHERE rubric_promotion.grade_decision_id=decision.id),
                  '{\"schema_version\":1,\"promotions\":[]}')
         FROM exam_subjective_grade_suggestions_v2 suggestion
         JOIN exam_subjective_transcription_revisions_v2 transcription
           ON transcription.id=suggestion.transcription_revision_id
          AND transcription.state='active'
         LEFT JOIN exam_short_answer_grade_analyses_v2 analysis
           ON analysis.suggestion_id=suggestion.id AND analysis.state='active'
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
         LEFT JOIN exam_grade_decision_short_answer_sources_v2 short_source
           ON short_source.grade_decision_id=decision.id
         LEFT JOIN exam_accepted_answer_promotions_v2 promotion
           ON promotion.grade_decision_id=decision.id
          AND source.suggestion_id=suggestion.id
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
                answer_slots_json: row.get(25)?,
                rubric_points_json: row.get(26)?,
                suggestion_id: row.get(27)?,
                short_answer_analysis_id: row.get(28)?,
                machine_grade_ai_run_id: row.get(29)?,
                suggestion_outcome: row.get(30)?,
                suggested_score: row.get(31)?,
                suggestion_result_json: row.get(32)?,
                batch_eligible: row.get(33)?,
                exclusion_reason: row.get(34)?,
                grade_decision_id: row.get(35)?,
                grade_decision_revision: row.get(36)?,
                teacher_score: row.get(37)?,
                confirmation_level: row.get(38)?,
                review_mode: row.get(39)?,
                current_suggestion_confirmed: row.get(40)?,
                decided_at: row.get(41)?,
                teacher_components_json: row.get(42)?,
                accepted_answer_promotion_id: row.get(43)?,
                accepted_answer_promoted_at: row.get(44)?,
                rubric_evidence_promotions_json: row.get(45)?,
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
