//! 固定答题卡的一站式本地处理：四角校正、题区裁剪、结构落账和本地 OMR。
//!
//! 老师触发本命令表示接受“把已确认模板应用到当前已确认页面”；本地 OMR 结果仍只是
//! observation/评分建议，必须继续进入既有老师终审和显式发布链路。

use std::collections::BTreeMap;
use std::path::Path;

use module_exam::answer_sheet_recognition::{
    align_answer_sheet_page, validate_blank_template_canvas, LocalAnswerSheetOmr, SheetRect,
};
use module_exam::objective_recognition::ObjectiveRecognizer;
use module_exam::ordinary_paper_recognition::NormalizedRect;
use module_exam::service::answer_sheet_page::{
    self, AnswerSheetPageMaterializationResult, AnswerSheetRegionArtifact,
    MaterializeAnswerSheetPageInput,
};
use module_exam::service::objective::ObjectiveObservationResult;
use module_exam::service::ordinary_structure::crop_normalized_jpeg;
use module_exam::service::subjective::SubjectiveTranscriptionRevision;
use rusqlite::Connection;
use serde::Serialize;
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ArtifactKind, PrivacyClass};

use crate::exam_intake::{archive_bytes, register_artifact, ArchivedFile};
use crate::objective_run::{self, BeginObjectiveRun};

const MATERIALIZATION_VERSION: &str = "answer-sheet-materialization-v1";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSheetPageProcessingResult {
    pub structure: AnswerSheetPageMaterializationResult,
    pub observations: Vec<ObjectiveObservationResult>,
    pub subjective_regions: Vec<AnswerSheetSubjectiveRegionProcessingResult>,
    pub subjective_transcriptions: Vec<SubjectiveTranscriptionRevision>,
    pub subjective_failures: Vec<AnswerSheetSubjectiveRegionFailure>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSheetSubjectiveRegionProcessingResult {
    pub answer_region_revision_id: i64,
    pub assessment_item_id: i64,
    pub region_index: i64,
    pub crop_artifact_id: i64,
    pub state: &'static str,
    pub next_action: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSheetSubjectiveRegionFailure {
    pub answer_region_revision_id: i64,
    pub safe_message: String,
}

struct PreparedRegion {
    assessment_item_id: i64,
    region_index: i64,
    hash: String,
    bytes_len: i64,
    derivative_type: String,
    archived: ArchivedFile,
    blank_crop: Option<Vec<u8>>,
}

struct PreparedPage {
    source_id: i64,
    definition: module_exam::answer_sheet_recognition::AnswerSheetTemplateDefinition,
    alignment: module_exam::answer_sheet_recognition::AnswerSheetAlignmentResult,
    aligned_hash: String,
    aligned_derivative_type: String,
    aligned: ArchivedFile,
    regions: Vec<PreparedRegion>,
}

impl PreparedPage {
    fn rollback_new_files(&self) {
        self.aligned.rollback_new_file();
        for region in &self.regions {
            region.archived.rollback_new_file();
        }
    }
}

fn normalized(rect: &SheetRect) -> NormalizedRect {
    NormalizedRect {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height,
    }
}

fn read_verified(path: &str, expected_hash: &str, label: &str) -> CoreResult<Vec<u8>> {
    let bytes = std::fs::read(path)
        .map_err(|_| CoreError::Invalid(format!("{label}文件缺失或不可读取")))?;
    if hashing::sha256_hex(&bytes) != expected_hash {
        return Err(CoreError::Invalid(format!(
            "{label}与登记 artifact hash 不一致"
        )));
    }
    Ok(bytes)
}

fn prepare_files(
    conn: &Connection,
    data_dir: &Path,
    page_id: i64,
    template_revision_id: i64,
) -> CoreResult<PreparedPage> {
    let scope =
        answer_sheet_page::validate_materialization_source(conn, page_id, template_revision_id)?;
    let source_bytes = read_verified(
        &scope.source_artifact.archived_path,
        &scope.source_artifact.sha256,
        "答题卡学生原图",
    )?;
    let blank_bytes = read_verified(
        &scope.blank_artifact.archived_path,
        &scope.blank_artifact.sha256,
        "答题卡空白模板",
    )?;
    validate_blank_template_canvas(&scope.definition, &blank_bytes)?;
    let alignment = align_answer_sheet_page(&scope.definition, &source_bytes)
        .map_err(|failure| CoreError::Invalid(failure.safe_message))?;
    let aligned_hash = hashing::sha256_hex(&alignment.aligned_jpeg);
    let aligned_derivative_type = format!(
        "answer_sheet_aligned_input:page:{}:template:{}",
        page_id, template_revision_id
    );
    let aligned = archive_bytes(
        &alignment.aligned_jpeg,
        &aligned_hash,
        "jpg",
        &data_dir.join("archive/exam/answer-sheet/aligned"),
    )?;

    let mut regions: Vec<PreparedRegion> = Vec::with_capacity(scope.definition.region_count());
    for item in &scope.definition.items {
        let student_crop =
            match crop_normalized_jpeg(&alignment.aligned_jpeg, &normalized(&item.region)) {
                Ok(value) => value,
                Err(error) => {
                    aligned.rollback_new_file();
                    for region in &regions {
                        region.archived.rollback_new_file();
                    }
                    return Err(error);
                }
            };
        let blank_crop = match crop_normalized_jpeg(&blank_bytes, &normalized(&item.region)) {
            Ok(value) => value,
            Err(error) => {
                aligned.rollback_new_file();
                for region in &regions {
                    region.archived.rollback_new_file();
                }
                return Err(error);
            }
        };
        let hash = hashing::sha256_hex(&student_crop);
        let derivative_type = format!(
            "answer_sheet_answer_region:page:{}:item:{}:region:{}",
            page_id, item.assessment_item_id, item.region_index
        );
        let archived = match archive_bytes(
            &student_crop,
            &hash,
            "jpg",
            &data_dir.join("archive/exam/answer-sheet/crops"),
        ) {
            Ok(value) => value,
            Err(error) => {
                aligned.rollback_new_file();
                for region in &regions {
                    region.archived.rollback_new_file();
                }
                return Err(error);
            }
        };
        regions.push(PreparedRegion {
            assessment_item_id: item.assessment_item_id,
            region_index: item.region_index,
            hash,
            bytes_len: student_crop.len() as i64,
            derivative_type,
            archived,
            blank_crop: Some(blank_crop),
        });
    }
    for item in &scope.definition.subjective_regions {
        let student_crop =
            match crop_normalized_jpeg(&alignment.aligned_jpeg, &normalized(&item.region)) {
                Ok(value) => value,
                Err(error) => {
                    aligned.rollback_new_file();
                    for region in &regions {
                        region.archived.rollback_new_file();
                    }
                    return Err(error);
                }
            };
        let hash = hashing::sha256_hex(&student_crop);
        let derivative_type = format!(
            "answer_sheet_subjective_region:page:{}:item:{}:region:{}",
            page_id, item.assessment_item_id, item.region_index
        );
        let archived = match archive_bytes(
            &student_crop,
            &hash,
            "jpg",
            &data_dir.join("archive/exam/answer-sheet/subjective-crops"),
        ) {
            Ok(value) => value,
            Err(error) => {
                aligned.rollback_new_file();
                for region in &regions {
                    region.archived.rollback_new_file();
                }
                return Err(error);
            }
        };
        regions.push(PreparedRegion {
            assessment_item_id: item.assessment_item_id,
            region_index: item.region_index,
            hash,
            bytes_len: student_crop.len() as i64,
            derivative_type,
            archived,
            blank_crop: None,
        });
    }
    Ok(PreparedPage {
        source_id: scope.source_artifact.id,
        definition: scope.definition,
        alignment,
        aligned_hash,
        aligned_derivative_type,
        aligned,
        regions,
    })
}

fn materialize(
    conn: &Connection,
    prepared: &PreparedPage,
    page_id: i64,
    template_revision_id: i64,
    confirmed_by: &str,
) -> CoreResult<AnswerSheetPageMaterializationResult> {
    let tx = conn.unchecked_transaction()?;
    let aligned = register_artifact(
        &tx,
        &prepared.aligned,
        &prepared.aligned_hash,
        prepared.alignment.aligned_jpeg.len() as i64,
        ArtifactKind::Page,
        "image/jpeg",
        None,
        None,
        Some(prepared.source_id),
        Some(&prepared.aligned_derivative_type),
        MATERIALIZATION_VERSION,
        PrivacyClass::StudentSensitive,
    )?;
    let mut region_artifacts = Vec::with_capacity(prepared.regions.len());
    for region in &prepared.regions {
        let crop = register_artifact(
            &tx,
            &region.archived,
            &region.hash,
            region.bytes_len,
            ArtifactKind::Crop,
            "image/jpeg",
            None,
            None,
            Some(aligned.id),
            Some(&region.derivative_type),
            MATERIALIZATION_VERSION,
            PrivacyClass::StudentSensitive,
        )?;
        region_artifacts.push(AnswerSheetRegionArtifact {
            assessment_item_id: region.assessment_item_id,
            region_index: region.region_index,
            crop_artifact_id: crop.id,
        });
    }
    let result = answer_sheet_page::materialize_in_transaction(
        &tx,
        &MaterializeAnswerSheetPageInput {
            page_id,
            template_revision_id,
            aligned_artifact_id: aligned.id,
            region_artifacts: &region_artifacts,
            template_to_source: prepared.alignment.template_to_source,
            detected_anchors: &prepared.alignment.detected_anchors,
            alignment_confidence: prepared.alignment.confidence,
            confirmed_by,
        },
    )?;
    tx.commit()?;
    Ok(result)
}

fn recognize_regions(
    conn: &Connection,
    page_id: i64,
    template_revision_id: i64,
    prepared: &PreparedPage,
    structure: &AnswerSheetPageMaterializationResult,
) -> CoreResult<Vec<ObjectiveObservationResult>> {
    let blank_by_key = prepared
        .regions
        .iter()
        .map(|region| {
            (
                (region.assessment_item_id, region.region_index),
                region.blank_crop.clone(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut observations = Vec::with_capacity(structure.regions.len());
    for region in &structure.regions {
        let Some(blank_crop) = blank_by_key
            .get(&(region.assessment_item_id, region.region_index))
            .cloned()
            .flatten()
        else {
            continue;
        };
        let recognizer = LocalAnswerSheetOmr::new(blank_crop, prepared.definition.policy.clone())?;
        let descriptor = recognizer.descriptor();
        let metadata = objective_run::load_metadata(conn, region.id)?;
        let input = objective_run::load_input(metadata)?;
        let idempotency_key = format!(
            "answer-sheet-local-omr:page:{page_id}:template:{template_revision_id}:region:{}",
            region.id
        );
        match objective_run::begin(conn, &input, &descriptor, &idempotency_key)? {
            BeginObjectiveRun::Completed(result) => observations.push(*result),
            BeginObjectiveRun::Execute { ai_run_id } => {
                let provider_result = recognizer.recognize(&input.request());
                observations.push(objective_run::finish(
                    conn,
                    ai_run_id,
                    &idempotency_key,
                    provider_result,
                )?);
            }
        }
    }
    Ok(observations)
}

pub fn process_page(
    conn: &Connection,
    data_dir: &Path,
    page_id: i64,
    confirmed_by: &str,
) -> CoreResult<AnswerSheetPageProcessingResult> {
    let template_revision = answer_sheet_page::get_active_template_for_page(conn, page_id)?;
    let template_set = module_exam::service::answer_sheet::answer_sheet_template_set_status(
        conn,
        template_revision.assessment_version_id,
    )?;
    if !template_set.ready {
        return Err(CoreError::Invalid(format!(
            "整套答题卡模板还不完整：{}",
            template_set.issue_codes.join("、")
        )));
    }
    let template_revision_id = template_revision.id;
    let prepared = prepare_files(conn, data_dir, page_id, template_revision_id)?;
    let structure = materialize(conn, &prepared, page_id, template_revision_id, confirmed_by);
    let structure = match structure {
        Ok(value) => value,
        Err(error) => {
            prepared.rollback_new_files();
            return Err(error);
        }
    };
    let observations =
        recognize_regions(conn, page_id, template_revision_id, &prepared, &structure)?;
    let subjective_region_ids = structure
        .routes
        .iter()
        .filter(|route| route.recognition_route == "handwriting_ocr")
        .map(|route| route.answer_region_revision_id)
        .collect::<std::collections::BTreeSet<_>>();
    let subjective_regions = structure
        .regions
        .iter()
        .filter(|region| subjective_region_ids.contains(&region.id))
        .map(|region| {
            Ok(AnswerSheetSubjectiveRegionProcessingResult {
                answer_region_revision_id: region.id,
                assessment_item_id: region.assessment_item_id,
                region_index: region.region_index,
                crop_artifact_id: region
                    .crop_artifact_id
                    .ok_or_else(|| CoreError::Invalid("答题卡主观题区缺少已归档裁图".into()))?,
                state: "awaiting_handwriting_recognition",
                next_action: "进入手写 OCR 与评分点复核，不使用 OMR",
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(AnswerSheetPageProcessingResult {
        structure,
        observations,
        subjective_regions,
        subjective_transcriptions: Vec::new(),
        subjective_failures: Vec::new(),
    })
}
