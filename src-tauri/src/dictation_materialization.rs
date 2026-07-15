//! 固定版式默写学生页的题区裁剪与证据物化。
//!
//! 首版要求整页拍摄、方向正确且页面比例与空白模板一致；超出容差直接要求重拍，
//! 不让系统在透视严重时猜题区。裁剪和模板映射只产生题区证据，不产生分数。

use std::path::Path;

use image::GenericImageView;
use module_exam::dictation_recognition::DictationRegionProposal;
use module_exam::service::dictation_pipeline::{
    self, DictationPageMaterializationResult, DictationRegionArtifact,
    MaterializeDictationPageInput,
};
use module_exam::service::ordinary_structure::crop_normalized_jpeg;
use rusqlite::{Connection, OptionalExtension};
use suite_core::db::repo::artifacts;
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

use crate::exam_intake::{archive_bytes, register_artifact, ArchivedFile};

const MATERIALIZATION_VERSION: &str = "fixed-dictation-materialization-v1";
const ASPECT_RATIO_TOLERANCE: f64 = 0.08;

struct PreparedRegion {
    assessment_item_id: i64,
    region_index: i64,
    hash: String,
    bytes_len: i64,
    derivative_type: String,
    archived: ArchivedFile,
}

struct PreparedPage {
    source_artifact_id: i64,
    template_revision_id: i64,
    aligned_bytes: Vec<u8>,
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

#[derive(serde::Deserialize)]
struct StoredTemplate {
    canvas_width: u32,
    canvas_height: u32,
    regions: Vec<DictationRegionProposal>,
}

fn read_verified(path: &str, expected_hash: &str) -> CoreResult<Vec<u8>> {
    let bytes =
        std::fs::read(path).map_err(|_| CoreError::Invalid("默写学生原图缺失或不可读取".into()))?;
    if hashing::sha256_hex(&bytes) != expected_hash {
        return Err(CoreError::Invalid(
            "默写学生原图与登记 artifact hash 不一致".into(),
        ));
    }
    Ok(bytes)
}

fn prepare_files(conn: &Connection, data_dir: &Path, page_id: i64) -> CoreResult<PreparedPage> {
    let template = dictation_pipeline::get_active_template_for_page(conn, page_id)?;
    let source_artifact_id = conn
        .query_row(
            "SELECT source_artifact_id FROM exam_ingest_pages_v2
             WHERE id=?1 AND state<>'voided'",
            [page_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("默写页面#{page_id}")))?;
    let source = artifacts::get_by_id(conn, source_artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{source_artifact_id}")))?;
    if source.archive_status != ArchiveStatus::Ready
        || !matches!(source.kind, ArtifactKind::Image | ArtifactKind::Page)
    {
        return Err(CoreError::Invalid("默写学生原图尚不可用".into()));
    }
    let source_bytes = read_verified(&source.archived_path, &source.sha256)?;
    let decoded = image::load_from_memory(&source_bytes)
        .map_err(|_| CoreError::Invalid("默写学生原图无法解码".into()))?;
    let stored: StoredTemplate = serde_json::from_str(&template.template_json)
        .map_err(|error| CoreError::Parse(format!("默写模板 JSON 损坏：{error}")))?;
    if stored.canvas_width == 0 || stored.canvas_height == 0 || stored.regions.is_empty() {
        return Err(CoreError::Invalid("默写模板缺少画布或题区".into()));
    }
    let (width, height) = decoded.dimensions();
    let page_ratio = width as f64 / height as f64;
    let template_ratio = stored.canvas_width as f64 / stored.canvas_height as f64;
    if ((page_ratio - template_ratio) / template_ratio).abs() > ASPECT_RATIO_TOLERANCE {
        return Err(CoreError::Invalid(
            "默写照片未完整覆盖页面或方向不一致，请按空白模板重新拍摄".into(),
        ));
    }

    let full_page = module_exam::ordinary_paper_recognition::NormalizedRect {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };
    let aligned_bytes = crop_normalized_jpeg(&source_bytes, &full_page)?;
    let aligned_hash = hashing::sha256_hex(&aligned_bytes);
    let aligned_derivative_type = format!(
        "dictation_aligned_input:page:{}:template:{}",
        page_id, template.id
    );
    let aligned = archive_bytes(
        &aligned_bytes,
        &aligned_hash,
        "jpg",
        &data_dir.join("archive/exam/dictation/aligned"),
    )?;

    let mut regions: Vec<PreparedRegion> = Vec::with_capacity(stored.regions.len());
    for proposal in stored.regions {
        let bytes = match crop_normalized_jpeg(&aligned_bytes, &proposal.bbox) {
            Ok(value) => value,
            Err(error) => {
                aligned.rollback_new_file();
                for region in &regions {
                    region.archived.rollback_new_file();
                }
                return Err(error);
            }
        };
        let hash = hashing::sha256_hex(&bytes);
        let derivative_type = format!(
            "dictation_answer_region:page:{}:item:{}:region:{}",
            page_id, proposal.assessment_item_id, proposal.region_index
        );
        let archived = match archive_bytes(
            &bytes,
            &hash,
            "jpg",
            &data_dir.join("archive/exam/dictation/crops"),
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
            assessment_item_id: proposal.assessment_item_id,
            region_index: proposal.region_index,
            hash,
            bytes_len: bytes.len() as i64,
            derivative_type,
            archived,
        });
    }
    Ok(PreparedPage {
        source_artifact_id,
        template_revision_id: template.id,
        aligned_bytes,
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
    confirmed_by: &str,
) -> CoreResult<DictationPageMaterializationResult> {
    let tx = conn.unchecked_transaction()?;
    let aligned = register_artifact(
        &tx,
        &prepared.aligned,
        &prepared.aligned_hash,
        prepared.aligned_bytes.len() as i64,
        ArtifactKind::Page,
        "image/jpeg",
        None,
        None,
        Some(prepared.source_artifact_id),
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
        region_artifacts.push(DictationRegionArtifact {
            assessment_item_id: region.assessment_item_id,
            region_index: region.region_index,
            crop_artifact_id: crop.id,
        });
    }
    let result = dictation_pipeline::materialize_in_transaction(
        &tx,
        &MaterializeDictationPageInput {
            page_id,
            template_revision_id: prepared.template_revision_id,
            aligned_artifact_id: aligned.id,
            regions: &region_artifacts,
            confirmed_by,
        },
    )?;
    tx.commit()?;
    Ok(result)
}

pub fn materialize_page(
    conn: &Connection,
    data_dir: &Path,
    page_id: i64,
    confirmed_by: &str,
) -> CoreResult<DictationPageMaterializationResult> {
    let active = dictation_pipeline::get_active_template_for_page(conn, page_id)?;
    if let Some(existing) = dictation_pipeline::get_page_materialization(conn, page_id, active.id)?
    {
        let region_ids = existing.region_revision_ids.clone();
        let mut regions = Vec::with_capacity(region_ids.len());
        for id in region_ids {
            regions.push(conn.query_row(
                "SELECT id,public_id,page_id,assessment_item_id,region_index,revision,
                        alignment_revision_id,bbox_json,crop_artifact_id,mapping_confidence,
                        decision,reason_code,confirmed_by,state
                 FROM exam_answer_region_revisions_v2 WHERE id=?1",
                [id],
                |row| {
                    Ok(module_exam::service::papers::AnswerRegionRevision {
                        id: row.get(0)?,
                        public_id: row.get(1)?,
                        page_id: row.get(2)?,
                        assessment_item_id: row.get(3)?,
                        region_index: row.get(4)?,
                        revision: row.get(5)?,
                        alignment_revision_id: row.get(6)?,
                        bbox_json: row.get(7)?,
                        crop_artifact_id: row.get(8)?,
                        mapping_confidence: row.get(9)?,
                        decision: row.get(10)?,
                        reason_code: row.get(11)?,
                        confirmed_by: row.get(12)?,
                        state: row.get(13)?,
                    })
                },
            )?);
        }
        let alignment = conn.query_row(
            "SELECT id,public_id,page_id,revision,match_revision_id,template_version,
                    transform_json,confidence,aligned_artifact_id,decision,reason_code,
                    confirmed_by,state
             FROM exam_page_alignment_revisions_v2 WHERE id=?1",
            [existing.alignment_revision_id],
            |row| {
                Ok(module_exam::service::papers::PageAlignmentRevision {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    page_id: row.get(2)?,
                    revision: row.get(3)?,
                    match_revision_id: row.get(4)?,
                    template_version: row.get(5)?,
                    transform_json: row.get(6)?,
                    confidence: row.get(7)?,
                    aligned_artifact_id: row.get(8)?,
                    decision: row.get(9)?,
                    reason_code: row.get(10)?,
                    confirmed_by: row.get(11)?,
                    state: row.get(12)?,
                })
            },
        )?;
        return Ok(DictationPageMaterializationResult {
            materialization: existing,
            alignment,
            regions,
        });
    }
    let prepared = prepare_files(conn, data_dir, page_id)?;
    match materialize(conn, &prepared, page_id, confirmed_by) {
        Ok(result) => Ok(result),
        Err(error) => {
            prepared.rollback_new_files();
            Err(error)
        }
    }
}
