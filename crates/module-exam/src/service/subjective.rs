//! 答题卡填空/简答题区的追加式手写 OCR 转写。
//!
//! 本层只保存“学生实际写了什么”，不加载标准答案、不判分。后续机器评分和老师终审
//! 必须引用这里的具体 active revision，避免答案反向污染 OCR 原文。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::{ai_runs, audit};
use suite_core::domain::{ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, AuditActorType};

use crate::dictation::DictationRecognitionState;
use crate::dictation_recognition::DictationOcrOutput;

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
