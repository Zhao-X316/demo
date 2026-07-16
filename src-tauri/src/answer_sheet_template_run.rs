//! 空白答题卡模板候选的归档、AI run 与老师确认编排。
//!
//! 文件读取和网络调用都在 SQLite 锁外；AI 只写不可变候选。只有老师确认 ready
//! 候选后，才创建 active 模板 revision。

use std::path::{Path, PathBuf};

use module_exam::answer_sheet_template_recognition::{
    AnswerSheetTemplateItemSpec, AnswerSheetTemplateQuestionType,
    AnswerSheetTemplateRecognitionErrorCode, AnswerSheetTemplateRecognitionFailure,
    AnswerSheetTemplateRecognitionOutput, AnswerSheetTemplateRecognitionRequest,
    AnswerSheetTemplateRecognitionState, AnswerSheetTemplateRecognizerDescriptor,
    ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION,
};
use module_exam::service::answer_sheet::{
    self, AnswerSheetTemplateRevision, AnswerSheetTemplateSetStatus,
    ConfirmAnswerSheetTemplateInput,
};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRun, AiRunStatus, ArchiveStatus, Artifact, ArtifactKind, PrivacyClass};

use crate::exam_intake::{archive_bytes, register_artifact};

const BLANK_TEMPLATE_PROCESSING_VERSION: &str = "answer-sheet-blank-template-v1";

pub struct PreparedBlankTemplate {
    pub bytes: Vec<u8>,
    pub sha256: String,
    pub mime_type: String,
    pub original_name: String,
    pub original_path: PathBuf,
    pub archived: crate::exam_intake::ArchivedFile,
}

pub struct AnswerSheetTemplateRunInput {
    assessment_version_id: i64,
    page_no: i64,
    template_version: String,
    blank_artifact: Artifact,
    image_bytes: Vec<u8>,
    items: Vec<AnswerSheetTemplateItemSpec>,
    input_hash: String,
}

impl AnswerSheetTemplateRunInput {
    pub fn request(&self) -> AnswerSheetTemplateRecognitionRequest<'_> {
        AnswerSheetTemplateRecognitionRequest {
            assessment_version_id: self.assessment_version_id,
            page_no: self.page_no,
            template_version: &self.template_version,
            blank_artifact_id: self.blank_artifact.id,
            blank_artifact_sha256: &self.blank_artifact.sha256,
            mime_type: &self.blank_artifact.mime_type,
            image_bytes: &self.image_bytes,
            items: &self.items,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AnswerSheetTemplateRunResult {
    pub ai_run_id: i64,
    pub status: String,
    pub output: Option<AnswerSheetTemplateRecognitionOutput>,
    pub failure: Option<AnswerSheetTemplateRecognitionFailure>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSheetTemplateStatus {
    pub assessment_version_id: i64,
    pub page_no: i64,
    pub active_template: Option<AnswerSheetTemplateRevision>,
    pub template_set: AnswerSheetTemplateSetStatus,
}

pub enum BeginAnswerSheetTemplateRun {
    Execute { ai_run_id: i64 },
    Completed(Box<AnswerSheetTemplateRunResult>),
}

fn image_format(path: &Path) -> CoreResult<(&'static str, &'static str)> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => Ok(("image/jpeg", "jpg")),
        "png" => Ok(("image/png", "png")),
        "webp" => Ok(("image/webp", "webp")),
        _ => Err(CoreError::Invalid(
            "空白答题卡只支持 JPG、PNG 或 WebP 图片".into(),
        )),
    }
}

pub fn prepare_blank_template(path: &str, data_dir: &Path) -> CoreResult<PreparedBlankTemplate> {
    let original_path = PathBuf::from(path);
    if !original_path.is_file() {
        return Err(CoreError::Invalid("空白答题卡文件不存在".into()));
    }
    let (mime_type, extension) = image_format(&original_path)?;
    let bytes = std::fs::read(&original_path)
        .map_err(|error| CoreError::Io(format!("读取空白答题卡失败：{error}")))?;
    image::load_from_memory(&bytes)
        .map_err(|_| CoreError::Invalid("空白答题卡图片无法解码".into()))?;
    let sha256 = hashing::sha256_hex(&bytes);
    let archived = archive_bytes(
        &bytes,
        &sha256,
        extension,
        &data_dir.join("archive/exam/answer-sheet/templates"),
    )?;
    Ok(PreparedBlankTemplate {
        bytes,
        sha256,
        mime_type: mime_type.into(),
        original_name: original_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("blank-answer-sheet")
            .into(),
        original_path,
        archived,
    })
}

pub fn register_blank_template(
    conn: &Connection,
    prepared: &PreparedBlankTemplate,
) -> CoreResult<Artifact> {
    register_artifact(
        conn,
        &prepared.archived,
        &prepared.sha256,
        prepared.bytes.len() as i64,
        ArtifactKind::Image,
        &prepared.mime_type,
        Some(&prepared.original_name),
        Some(&prepared.original_path),
        None,
        None,
        BLANK_TEMPLATE_PROCESSING_VERSION,
        PrivacyClass::TeachingContent,
    )
}

fn page_scope(conn: &Connection, reference_page_id: i64) -> CoreResult<(i64, i64, String)> {
    if reference_page_id <= 0 {
        return Err(CoreError::Invalid("答题卡参考页面 id 必须为正数".into()));
    }
    conn.query_row(
        "SELECT at.assessment_version_id,m.page_no,v.template_version
         FROM exam_ingest_pages_v2 p
         JOIN exam_page_quality_revisions_v2 q
           ON q.page_id=p.id AND q.state='active' AND q.result='pass'
         JOIN exam_page_match_revisions_v2 m
           ON m.page_id=p.id AND m.state='active' AND m.decision='teacher_confirmed'
         JOIN exam_attempts_v2 at ON at.id=m.attempt_id AND at.state<>'voided'
         JOIN exam_assessment_versions_v2 v
           ON v.id=at.assessment_version_id AND v.state='confirmed'
         JOIN exam_material_type_revisions_v2 mt
           ON mt.ingest_batch_id=p.batch_id AND mt.state='active'
              AND mt.decision='teacher_confirmed' AND mt.material_type='answer_sheet'
         WHERE p.id=?1 AND p.state<>'voided'",
        [reference_page_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        },
    )
    .optional()?
    .ok_or_else(|| {
        CoreError::Invalid("空白模板只允许绑定质量通过、身份已确认且材料为答题卡的页面".into())
    })
    .and_then(|(assessment_version_id, page_no, template_version)| {
        let template_version = template_version
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| CoreError::Invalid("当前作业尚未固定答题卡模板版本".into()))?;
        Ok((assessment_version_id, page_no, template_version))
    })
}

fn load_items(
    conn: &Connection,
    assessment_version_id: i64,
    page_no: i64,
) -> CoreResult<Vec<AnswerSheetTemplateItemSpec>> {
    let mut stmt = conn.prepare(
        "SELECT i.id,i.order_index,q.question_type
         FROM exam_assessment_items_v2 i
         JOIN k1_question_versions q ON q.id=i.question_version_id
         WHERE i.assessment_version_id=?1 AND i.state='active'
           AND COALESCE(CAST(json_extract(i.presentation_snapshot_json,'$.page_no') AS INTEGER),1)=?2
         ORDER BY i.order_index,i.id",
    )?;
    let rows = stmt.query_map((assessment_version_id, page_no), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (assessment_item_id, order_index, question_type) = row?;
        let question_type = AnswerSheetTemplateQuestionType::from_db(&question_type)
            .ok_or_else(|| CoreError::Invalid("固定答题卡包含不支持的题型".into()))?;
        items.push(AnswerSheetTemplateItemSpec {
            assessment_item_id,
            order_index,
            question_type,
        });
    }
    if items.is_empty() {
        return Err(CoreError::Invalid("当前答题卡页面没有可绑定的题目".into()));
    }
    Ok(items)
}

pub fn load_input(
    conn: &Connection,
    reference_page_id: i64,
    blank_artifact_id: i64,
) -> CoreResult<AnswerSheetTemplateRunInput> {
    let (assessment_version_id, page_no, template_version) = page_scope(conn, reference_page_id)?;
    let blank_artifact = artifacts::get_by_id(conn, blank_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{blank_artifact_id}")))?;
    if blank_artifact.archive_status != ArchiveStatus::Ready
        || blank_artifact.privacy_class != PrivacyClass::TeachingContent
        || !matches!(
            blank_artifact.kind,
            ArtifactKind::Image | ArtifactKind::Page
        )
    {
        return Err(CoreError::Invalid(
            "空白答题卡必须是已归档的教学内容图片".into(),
        ));
    }
    let image_bytes = std::fs::read(&blank_artifact.archived_path)
        .map_err(|_| CoreError::Invalid("空白答题卡归档缺失或不可读".into()))?;
    if hashing::sha256_hex(&image_bytes) != blank_artifact.sha256 {
        return Err(CoreError::Invalid(
            "空白答题卡归档与登记 hash 不一致".into(),
        ));
    }
    let items = load_items(conn, assessment_version_id, page_no)?;
    let mut input = AnswerSheetTemplateRunInput {
        assessment_version_id,
        page_no,
        template_version,
        blank_artifact,
        image_bytes,
        items,
        input_hash: String::new(),
    };
    input.input_hash = input.request().input_hash()?;
    Ok(input)
}

pub fn status(conn: &Connection, reference_page_id: i64) -> CoreResult<AnswerSheetTemplateStatus> {
    let (assessment_version_id, page_no, _) = page_scope(conn, reference_page_id)?;
    Ok(AnswerSheetTemplateStatus {
        assessment_version_id,
        page_no,
        active_template: answer_sheet::get_active_answer_sheet_template(
            conn,
            assessment_version_id,
            page_no,
        )?,
        template_set: answer_sheet::answer_sheet_template_set_status(conn, assessment_version_id)?,
    })
}

pub fn begin(
    conn: &Connection,
    input: &AnswerSheetTemplateRunInput,
    descriptor: &AnswerSheetTemplateRecognizerDescriptor,
    idempotency_key: &str,
) -> CoreResult<BeginAnswerSheetTemplateRun> {
    descriptor.validate()?;
    if idempotency_key.trim().is_empty() {
        return Err(CoreError::Invalid("答题卡模板分析幂等键不能为空".into()));
    }
    let business_ref_id = format!("{}:{}", input.assessment_version_id, input.page_no);
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: idempotency_key.trim(),
            run_type: "answer_sheet_template",
            source_module: "exam",
            business_ref_type: "assessment_page",
            business_ref_id: &business_ref_id,
            input_artifact_id: Some(input.blank_artifact.id),
            provider: &descriptor.provider,
            model_name: &descriptor.model_name,
            model_version: &descriptor.model_version,
            config_version: &descriptor.config_version,
            prompt_or_rule_version: &descriptor.rule_version,
            input_hash: &input.input_hash,
            retry_of_ai_run_id: None,
        },
    )?;
    match run.status {
        AiRunStatus::Pending => {
            ai_runs::start(conn, run.id, &time::utc_now_rfc3339(), None)?;
            Ok(BeginAnswerSheetTemplateRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => Ok(BeginAnswerSheetTemplateRun::Completed(
            Box::new(result_from_run(input, &run)?),
        )),
        AiRunStatus::Processing => Err(CoreError::Invalid(
            "该空白答题卡正在分析，请勿重复提交".into(),
        )),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "该答题卡模板分析已作废，请重新选择后重试".into(),
        )),
    }
}

pub fn finish(
    conn: &Connection,
    input: &AnswerSheetTemplateRunInput,
    ai_run_id: i64,
    result: Result<AnswerSheetTemplateRecognitionOutput, AnswerSheetTemplateRecognitionFailure>,
) -> CoreResult<AnswerSheetTemplateRunResult> {
    let finished_at = time::utc_now_rfc3339();
    match result {
        Ok(output) => match output.to_json_against(&input.request()) {
            Ok(output_json) => {
                ai_runs::finalize_succeeded(
                    conn,
                    ai_run_id,
                    &hashing::sha256_hex(output_json.as_bytes()),
                    Some(output.confidence),
                    &output_json,
                    &finished_at,
                )?;
            }
            Err(_) => {
                let failure = AnswerSheetTemplateRecognitionFailure {
                    schema_version: ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION,
                    code: AnswerSheetTemplateRecognitionErrorCode::InvalidOutput,
                    safe_message: "答题卡模板未通过安全校验，已转入老师复核".into(),
                    retryable: false,
                };
                ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
            }
        },
        Err(failure) => {
            ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
        }
    }
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    result_from_run(input, &run)
}

fn result_from_run(
    input: &AnswerSheetTemplateRunInput,
    run: &AiRun,
) -> CoreResult<AnswerSheetTemplateRunResult> {
    match run.status {
        AiRunStatus::Succeeded => {
            let output: AnswerSheetTemplateRecognitionOutput = serde_json::from_str(
                run.output_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("答题卡成功 run 缺少输出".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("答题卡模板输出无法解析：{error}")))?;
            output.validate_against(&input.request())?;
            Ok(AnswerSheetTemplateRunResult {
                ai_run_id: run.id,
                status: "succeeded".into(),
                output: Some(output),
                failure: None,
            })
        }
        AiRunStatus::Failed => {
            let failure: AnswerSheetTemplateRecognitionFailure = serde_json::from_str(
                run.error_meta_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("答题卡失败 run 缺少错误信息".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("答题卡模板错误无法解析：{error}")))?;
            failure.validate()?;
            Ok(AnswerSheetTemplateRunResult {
                ai_run_id: run.id,
                status: "failed".into(),
                output: None,
                failure: Some(failure),
            })
        }
        _ => Err(CoreError::Invalid(
            "答题卡模板 run 尚未形成可返回结果".into(),
        )),
    }
}

pub fn confirm(
    conn: &mut Connection,
    reference_page_id: i64,
    ai_run_id: i64,
    confirmed_by: &str,
) -> CoreResult<AnswerSheetTemplateRevision> {
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    if run.status != AiRunStatus::Succeeded || run.run_type != "answer_sheet_template" {
        return Err(CoreError::Invalid("只能确认已成功的答题卡模板候选".into()));
    }
    let blank_artifact_id = run
        .input_artifact_id
        .ok_or_else(|| CoreError::Invalid("答题卡模板 run 缺少空白图 artifact".into()))?;
    let input = load_input(conn, reference_page_id, blank_artifact_id)?;
    let output: AnswerSheetTemplateRecognitionOutput = serde_json::from_str(
        run.output_json
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("答题卡模板 run 缺少候选输出".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("答题卡模板候选无法解析：{error}")))?;
    output.validate_against(&input.request())?;
    if output.state != AnswerSheetTemplateRecognitionState::Ready {
        return Err(CoreError::Invalid(
            "有分歧或受阻的答题卡模板不能直接确认".into(),
        ));
    }
    answer_sheet::confirm_answer_sheet_template(
        conn,
        &ConfirmAnswerSheetTemplateInput {
            definition: &output.definition(),
            source_ai_run_id: Some(ai_run_id),
            confirmed_by,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use image::{DynamicImage, ImageOutputFormat};
    use suite_core::db::repo::artifacts;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    use super::*;

    fn input(conn: &Connection) -> AnswerSheetTemplateRunInput {
        let mut bytes = Vec::new();
        DynamicImage::new_rgb8(1200, 1800)
            .write_to(&mut Cursor::new(&mut bytes), ImageOutputFormat::Jpeg(90))
            .unwrap();
        let sha256 = hashing::sha256_hex(&bytes);
        let artifact = artifacts::create_or_get(
            conn,
            &artifacts::NewArtifact {
                kind: ArtifactKind::Image,
                sha256: &sha256,
                mime_type: "image/jpeg",
                byte_size: bytes.len() as i64,
                original_name: Some("blank.jpg"),
                original_path: None,
                archived_path: "/tmp/answer-sheet-blank.jpg",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: BLANK_TEMPLATE_PROCESSING_VERSION,
                privacy_class: PrivacyClass::TeachingContent,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        let mut input = AnswerSheetTemplateRunInput {
            assessment_version_id: 7,
            page_no: 1,
            template_version: "sheet-v1".into(),
            blank_artifact: artifact,
            image_bytes: bytes,
            items: vec![AnswerSheetTemplateItemSpec {
                assessment_item_id: 11,
                order_index: 0,
                question_type: AnswerSheetTemplateQuestionType::Single,
            }],
            input_hash: String::new(),
        };
        input.input_hash = input.request().input_hash().unwrap();
        input
    }

    fn descriptor() -> AnswerSheetTemplateRecognizerDescriptor {
        AnswerSheetTemplateRecognizerDescriptor {
            provider: "fixture".into(),
            model_name: "fixture-vision".into(),
            model_version: "v1".into(),
            config_version: "v1".into(),
            rule_version: "v1".into(),
        }
    }

    #[test]
    fn failed_template_run_is_idempotently_reloaded_without_paths() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let input = input(&conn);
        let ai_run_id = match begin(&conn, &input, &descriptor(), "sheet-template:7:1").unwrap() {
            BeginAnswerSheetTemplateRun::Execute { ai_run_id } => ai_run_id,
            BeginAnswerSheetTemplateRun::Completed(_) => panic!("first run must execute"),
        };
        let failure = AnswerSheetTemplateRecognitionFailure {
            schema_version: ANSWER_SHEET_TEMPLATE_RUN_SCHEMA_VERSION,
            code: AnswerSheetTemplateRecognitionErrorCode::Timeout,
            safe_message: "超时".into(),
            retryable: true,
        };
        let result = finish(&conn, &input, ai_run_id, Err(failure)).unwrap();
        assert_eq!(result.status, "failed");
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("/tmp/"));
        assert!(!json.contains("image_bytes"));

        let cached = match begin(&conn, &input, &descriptor(), "sheet-template:7:1").unwrap() {
            BeginAnswerSheetTemplateRun::Completed(result) => result,
            BeginAnswerSheetTemplateRun::Execute { .. } => panic!("failed run must be reused"),
        };
        assert_eq!(cached.ai_run_id, ai_run_id);
        assert_eq!(cached.status, "failed");
    }
}
