//! 固定普通试卷整页分析 run 编排。
//!
//! 数据库锁只用于读取已确认身份链、抢占和完成 `ai_run`；原图读取与外部网络均在
//! 锁外执行。当前批只持久化不可变机器建议，不写老师质量、配准、题区或分数。

use std::path::PathBuf;

use module_exam::ordinary_paper_recognition::{
    OrdinaryPaperItemSpec, OrdinaryPaperQuestionType, OrdinaryPaperRecognitionErrorCode,
    OrdinaryPaperRecognitionFailure, OrdinaryPaperRecognitionOutput,
    OrdinaryPaperRecognitionRequest, OrdinaryPaperRecognizerDescriptor,
    ORDINARY_PAPER_SCHEMA_VERSION,
};
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use suite_core::db::repo::{ai_runs, artifacts};
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRun, AiRunStatus, ArchiveStatus};

pub struct OrdinaryPaperRunMetadata {
    page_id: i64,
    input_artifact_id: i64,
    input_artifact_sha256: String,
    mime_type: String,
    archived_path: PathBuf,
    expected_page_no: i64,
    template_version: String,
    items: Vec<OrdinaryPaperItemSpec>,
}

pub struct OrdinaryPaperRunInput {
    page_id: i64,
    input_artifact_id: i64,
    input_artifact_sha256: String,
    mime_type: String,
    image_bytes: Vec<u8>,
    expected_page_no: i64,
    template_version: String,
    items: Vec<OrdinaryPaperItemSpec>,
    input_hash: String,
}

impl OrdinaryPaperRunInput {
    pub fn request(&self) -> OrdinaryPaperRecognitionRequest<'_> {
        OrdinaryPaperRecognitionRequest {
            page_id: self.page_id,
            input_artifact_id: self.input_artifact_id,
            input_artifact_sha256: &self.input_artifact_sha256,
            mime_type: &self.mime_type,
            image_bytes: &self.image_bytes,
            expected_page_no: self.expected_page_no,
            template_version: &self.template_version,
            items: &self.items,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct OrdinaryPaperRunResult {
    pub ai_run_id: i64,
    pub status: String,
    pub output: Option<OrdinaryPaperRecognitionOutput>,
    pub failure: Option<OrdinaryPaperRecognitionFailure>,
}

pub enum BeginOrdinaryPaperRun {
    Execute { ai_run_id: i64 },
    Completed(Box<OrdinaryPaperRunResult>),
}

pub fn load_metadata(conn: &Connection, page_id: i64) -> CoreResult<OrdinaryPaperRunMetadata> {
    if page_id <= 0 {
        return Err(CoreError::Invalid("普通试卷页面 id 必须为正数".into()));
    }
    let (artifact_id, page_no, assessment_version_id, template_version) = conn
        .query_row(
            "SELECT p.source_artifact_id,m.page_no,at.assessment_version_id,v.template_version
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
                  AND mt.decision='teacher_confirmed' AND mt.material_type='ordinary_paper'
             WHERE p.id=?1 AND p.state<>'voided'",
            [page_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid(
                "普通卷整页分析只允许当前质量通过、身份已确认且材料为普通试卷的页面".into(),
            )
        })?;
    let template_version = template_version
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CoreError::Invalid("当前作业尚未固定普通卷模板版本".into()))?;
    let artifact = artifacts::get_by_id(conn, artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{artifact_id}")))?;
    if artifact.archive_status != ArchiveStatus::Ready {
        return Err(CoreError::Invalid("普通试卷原图当前不可读取".into()));
    }

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
        let question_type =
            OrdinaryPaperQuestionType::from_db(&question_type).ok_or_else(|| {
                CoreError::Invalid(
                    "B3a1 当前只分析固定普通卷的选择题和判断题，其他题型必须单独路由".into(),
                )
            })?;
        items.push(OrdinaryPaperItemSpec {
            assessment_item_id,
            order_index,
            question_type,
        });
    }
    if items.is_empty() {
        return Err(CoreError::Invalid(
            "当前普通试卷页面没有可安全绑定的选择或判断题".into(),
        ));
    }

    Ok(OrdinaryPaperRunMetadata {
        page_id,
        input_artifact_id: artifact.id,
        input_artifact_sha256: artifact.sha256,
        mime_type: artifact.mime_type,
        archived_path: artifact.archived_path.into(),
        expected_page_no: page_no,
        template_version,
        items,
    })
}

/// 在数据库锁外读取不可变原图，并再次核对内容 hash。
pub fn load_input(metadata: OrdinaryPaperRunMetadata) -> CoreResult<OrdinaryPaperRunInput> {
    let image_bytes = std::fs::read(&metadata.archived_path)
        .map_err(|_| CoreError::Invalid("普通试卷原图缺失或不可读".into()))?;
    if hashing::sha256_hex(&image_bytes) != metadata.input_artifact_sha256 {
        return Err(CoreError::Invalid(
            "普通试卷原图与已登记 artifact hash 不一致".into(),
        ));
    }
    let mut input = OrdinaryPaperRunInput {
        page_id: metadata.page_id,
        input_artifact_id: metadata.input_artifact_id,
        input_artifact_sha256: metadata.input_artifact_sha256,
        mime_type: metadata.mime_type,
        image_bytes,
        expected_page_no: metadata.expected_page_no,
        template_version: metadata.template_version,
        items: metadata.items,
        input_hash: String::new(),
    };
    input.input_hash = input.request().input_hash()?;
    Ok(input)
}

pub fn begin(
    conn: &Connection,
    input: &OrdinaryPaperRunInput,
    descriptor: &OrdinaryPaperRecognizerDescriptor,
    idempotency_key: &str,
) -> CoreResult<BeginOrdinaryPaperRun> {
    descriptor.validate()?;
    if idempotency_key.trim().is_empty() {
        return Err(CoreError::Invalid("普通试卷分析幂等键不能为空".into()));
    }
    let business_ref_id = input.page_id.to_string();
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: idempotency_key.trim(),
            run_type: "ordinary_paper_structure",
            source_module: "exam",
            business_ref_type: "ingest_page",
            business_ref_id: &business_ref_id,
            input_artifact_id: Some(input.input_artifact_id),
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
            Ok(BeginOrdinaryPaperRun::Execute { ai_run_id: run.id })
        }
        AiRunStatus::Succeeded | AiRunStatus::Failed => Ok(BeginOrdinaryPaperRun::Completed(
            Box::new(result_from_run(input, &run)?),
        )),
        AiRunStatus::Processing => Err(CoreError::Invalid(
            "该普通试卷页面正在分析，请勿重复提交".into(),
        )),
        AiRunStatus::Voided => Err(CoreError::Invalid(
            "该普通试卷分析已作废，请使用新的幂等键重试".into(),
        )),
    }
}

pub fn finish(
    conn: &Connection,
    input: &OrdinaryPaperRunInput,
    ai_run_id: i64,
    result: Result<OrdinaryPaperRecognitionOutput, OrdinaryPaperRecognitionFailure>,
) -> CoreResult<OrdinaryPaperRunResult> {
    let finished_at = time::utc_now_rfc3339();
    match result {
        Ok(output) => {
            let output_json = match output.to_json_against(&input.request()) {
                Ok(value) => value,
                Err(_) => {
                    let failure = OrdinaryPaperRecognitionFailure {
                        schema_version: ORDINARY_PAPER_SCHEMA_VERSION,
                        code: OrdinaryPaperRecognitionErrorCode::InvalidOutput,
                        safe_message: "普通试卷分析结果未通过安全校验，已转入老师复核".into(),
                        retryable: false,
                    };
                    ai_runs::finalize_failed(conn, ai_run_id, &failure.to_json()?, &finished_at)?;
                    let run = ai_runs::get_by_id(conn, ai_run_id)?
                        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
                    return result_from_run(input, &run);
                }
            };
            ai_runs::finalize_succeeded(
                conn,
                ai_run_id,
                &hashing::sha256_hex(output_json.as_bytes()),
                Some(output.confidence),
                &output_json,
                &finished_at,
            )?;
        }
        Err(failure) => {
            let failure_json = failure.to_json()?;
            ai_runs::finalize_failed(conn, ai_run_id, &failure_json, &finished_at)?;
        }
    }
    let run = ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    result_from_run(input, &run)
}

fn result_from_run(
    input: &OrdinaryPaperRunInput,
    run: &AiRun,
) -> CoreResult<OrdinaryPaperRunResult> {
    match run.status {
        AiRunStatus::Succeeded => {
            let output: OrdinaryPaperRecognitionOutput = serde_json::from_str(
                run.output_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("普通试卷成功 run 缺少输出".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("普通试卷 run 输出无法解析：{error}")))?;
            output.validate_against(&input.request())?;
            Ok(OrdinaryPaperRunResult {
                ai_run_id: run.id,
                status: "succeeded".into(),
                output: Some(output),
                failure: None,
            })
        }
        AiRunStatus::Failed => {
            let failure: OrdinaryPaperRecognitionFailure = serde_json::from_str(
                run.error_meta_json
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("普通试卷失败 run 缺少错误信息".into()))?,
            )
            .map_err(|error| CoreError::Parse(format!("普通试卷 run 错误无法解析：{error}")))?;
            failure.validate()?;
            Ok(OrdinaryPaperRunResult {
                ai_run_id: run.id,
                status: "failed".into(),
                output: None,
                failure: Some(failure),
            })
        }
        _ => Err(CoreError::Invalid("普通试卷 run 尚未形成可返回结果".into())),
    }
}

#[cfg(test)]
mod tests {
    use module_exam::ordinary_paper_recognition::{
        NormalizedRect, OrdinaryPaperAlignment, OrdinaryPaperMarkCell, OrdinaryPaperQuality,
        OrdinaryPaperQualityResult, OrdinaryPaperRecognitionState, OrdinaryPaperRegionProposal,
    };
    use suite_core::db::repo::artifacts;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

    use super::*;

    fn input(conn: &Connection) -> OrdinaryPaperRunInput {
        let bytes = b"ordinary-run-page".to_vec();
        let sha256 = hashing::sha256_hex(&bytes);
        let artifact = artifacts::create_or_get(
            conn,
            &artifacts::NewArtifact {
                kind: ArtifactKind::Image,
                sha256: &sha256,
                mime_type: "image/jpeg",
                byte_size: bytes.len() as i64,
                original_name: Some("page.jpg"),
                original_path: None,
                archived_path: "/tmp/ordinary-run-page.jpg",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "ordinary-run-test-v1",
                privacy_class: PrivacyClass::StudentSensitive,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        let items = vec![OrdinaryPaperItemSpec {
            assessment_item_id: 11,
            order_index: 0,
            question_type: OrdinaryPaperQuestionType::Single,
        }];
        let mut input = OrdinaryPaperRunInput {
            page_id: 7,
            input_artifact_id: artifact.id,
            input_artifact_sha256: sha256,
            mime_type: "image/jpeg".into(),
            image_bytes: bytes,
            expected_page_no: 1,
            template_version: "ordinary-v1".into(),
            items,
            input_hash: String::new(),
        };
        input.input_hash = input.request().input_hash().unwrap();
        input
    }

    fn descriptor() -> OrdinaryPaperRecognizerDescriptor {
        OrdinaryPaperRecognizerDescriptor {
            provider: "fixture".into(),
            model_name: "fixture-vision".into(),
            model_version: "fixture-v1".into(),
            config_version: "ordinary-v1".into(),
            rule_version: "ordinary-json-v1".into(),
        }
    }

    fn output(input: &OrdinaryPaperRunInput) -> OrdinaryPaperRecognitionOutput {
        OrdinaryPaperRecognitionOutput {
            schema_version: ORDINARY_PAPER_SCHEMA_VERSION,
            page_id: input.page_id,
            input_artifact_id: input.input_artifact_id,
            input_artifact_sha256: input.input_artifact_sha256.clone(),
            input_hash: input.input_hash.clone(),
            expected_page_no: 1,
            descriptor: descriptor(),
            state: OrdinaryPaperRecognitionState::Ready,
            quality: OrdinaryPaperQuality {
                blur_score: 0.01,
                glare_score: 0.01,
                brightness_score: 0.8,
                perspective_score: 0.99,
                rotation_degrees: 0.0,
                crop_complete: true,
                result: OrdinaryPaperQualityResult::Pass,
                issue_codes: vec![],
            },
            alignment: Some(OrdinaryPaperAlignment {
                template_version: "ordinary-v1".into(),
                matrix: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
                confidence: 0.99,
            }),
            regions: vec![OrdinaryPaperRegionProposal {
                assessment_item_id: 11,
                region_index: 0,
                bbox: NormalizedRect {
                    x: 0.1,
                    y: 0.1,
                    width: 0.8,
                    height: 0.3,
                },
                mapping_confidence: 0.99,
                mark_cells: vec![
                    OrdinaryPaperMarkCell {
                        label: "A".into(),
                        rect: NormalizedRect {
                            x: 0.05,
                            y: 0.1,
                            width: 0.2,
                            height: 0.3,
                        },
                    },
                    OrdinaryPaperMarkCell {
                        label: "B".into(),
                        rect: NormalizedRect {
                            x: 0.35,
                            y: 0.1,
                            width: 0.2,
                            height: 0.3,
                        },
                    },
                ],
            }],
            printed_questions: vec![],
            confidence: 0.99,
            issue_codes: vec![],
        }
    }

    #[test]
    fn result_dto_does_not_expose_image_bytes_or_paths() {
        let result = OrdinaryPaperRunResult {
            ai_run_id: 7,
            status: "failed".into(),
            output: None,
            failure: Some(OrdinaryPaperRecognitionFailure {
                schema_version: 1,
                code: module_exam::ordinary_paper_recognition::OrdinaryPaperRecognitionErrorCode::Timeout,
                safe_message: "超时".into(),
                retryable: true,
            }),
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("archived_path"));
        assert!(!json.contains("image_bytes"));
    }

    #[test]
    fn succeeded_run_is_idempotently_reloaded_from_ledger() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let input = input(&conn);
        let ai_run_id = match begin(&conn, &input, &descriptor(), "ordinary:page:7").unwrap() {
            BeginOrdinaryPaperRun::Execute { ai_run_id } => ai_run_id,
            BeginOrdinaryPaperRun::Completed(_) => panic!("first run must execute"),
        };
        let result = finish(&conn, &input, ai_run_id, Ok(output(&input))).unwrap();
        assert_eq!(result.status, "succeeded");
        assert!(result.output.is_some());

        let cached = match begin(&conn, &input, &descriptor(), "ordinary:page:7").unwrap() {
            BeginOrdinaryPaperRun::Completed(result) => result,
            BeginOrdinaryPaperRun::Execute { .. } => panic!("completed run must be reused"),
        };
        assert_eq!(cached.ai_run_id, ai_run_id);
        assert_eq!(cached.status, "succeeded");
    }

    #[test]
    fn invalid_provider_output_is_finalized_as_safe_failure() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let input = input(&conn);
        let ai_run_id = match begin(&conn, &input, &descriptor(), "ordinary:invalid").unwrap() {
            BeginOrdinaryPaperRun::Execute { ai_run_id } => ai_run_id,
            BeginOrdinaryPaperRun::Completed(_) => panic!("first run must execute"),
        };
        let mut invalid = output(&input);
        invalid.regions[0].assessment_item_id = 999;
        let result = finish(&conn, &input, ai_run_id, Ok(invalid)).unwrap();
        assert_eq!(result.status, "failed");
        assert_eq!(
            result.failure.unwrap().code,
            OrdinaryPaperRecognitionErrorCode::InvalidOutput
        );
        let run = ai_runs::get_by_id(&conn, ai_run_id).unwrap().unwrap();
        assert_eq!(run.status, AiRunStatus::Failed);
    }
}
