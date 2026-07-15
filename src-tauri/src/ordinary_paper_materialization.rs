//! 普通试卷 ready 结构候选的本地派生文件准备与事务确认。

use std::path::Path;

use module_exam::ordinary_paper_recognition::OrdinaryPaperRecognitionOutput;
use module_exam::service::ordinary_structure::{
    self, ConfirmOrdinaryStructureInput, OrdinaryRegionArtifact,
    OrdinaryStructureConfirmationResult,
};
use rusqlite::Connection;
use suite_core::db::repo::artifacts;
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{Artifact, ArtifactKind, PrivacyClass};

use crate::exam_intake::{archive_bytes, register_artifact, ArchivedFile};

const MATERIALIZATION_VERSION: &str = "ordinary-structure-materialization-v1";
const ALIGNED_DERIVATIVE: &str = "ordinary_aligned_input";
const CROP_DERIVATIVE: &str = "ordinary_answer_region";

struct CandidateMetadata {
    source: Artifact,
    output: OrdinaryPaperRecognitionOutput,
}

struct PreparedRegion {
    assessment_item_id: i64,
    region_index: i64,
    hash: String,
    bytes_len: i64,
    archived: ArchivedFile,
    existed_before: bool,
}

struct PreparedCandidate {
    source: Artifact,
    source_bytes: Vec<u8>,
    aligned: ArchivedFile,
    aligned_existed_before: bool,
    regions: Vec<PreparedRegion>,
}

impl PreparedCandidate {
    fn rollback_new_files(&self) {
        if !self.aligned_existed_before {
            self.aligned.rollback_new_file();
        }
        for region in &self.regions {
            if !region.existed_before {
                region.archived.rollback_new_file();
            }
        }
    }
}

fn extension_for_mime(mime_type: &str) -> CoreResult<&'static str> {
    match mime_type.trim().to_ascii_lowercase().as_str() {
        "image/jpeg" => Ok("jpg"),
        "image/png" => Ok("png"),
        "image/webp" => Ok("webp"),
        _ => Err(CoreError::Invalid(
            "普通试卷来源图片格式不支持结构确认".into(),
        )),
    }
}

fn load_candidate(
    conn: &Connection,
    page_id: i64,
    ai_run_id: i64,
) -> CoreResult<CandidateMetadata> {
    let (artifact_id, _, _, output) =
        ordinary_structure::validate_source_artifact(conn, page_id, ai_run_id)?;
    let source = artifacts::get_by_id(conn, artifact_id)?
        .ok_or_else(|| CoreError::NotFound(format!("artifact#{artifact_id}")))?;
    Ok(CandidateMetadata { source, output })
}

fn prepare_files(
    conn: &Connection,
    data_dir: &Path,
    metadata: CandidateMetadata,
) -> CoreResult<PreparedCandidate> {
    let source_bytes = std::fs::read(&metadata.source.archived_path)
        .map_err(|_| CoreError::Invalid("普通试卷原图缺失或不可读取".into()))?;
    if hashing::sha256_hex(&source_bytes) != metadata.source.sha256 {
        return Err(CoreError::Invalid(
            "普通试卷原图与登记 artifact hash 不一致".into(),
        ));
    }
    let aligned_existed_before = artifacts::get_by_identity(
        conn,
        &metadata.source.sha256,
        ArtifactKind::Page,
        Some(ALIGNED_DERIVATIVE),
        MATERIALIZATION_VERSION,
    )?
    .is_some();
    let aligned_dir = data_dir.join("archive/exam/ordinary/aligned");
    let aligned = archive_bytes(
        &source_bytes,
        &metadata.source.sha256,
        extension_for_mime(&metadata.source.mime_type)?,
        &aligned_dir,
    )?;

    let crop_dir = data_dir.join("archive/exam/ordinary/crops");
    let mut regions: Vec<PreparedRegion> = Vec::with_capacity(metadata.output.regions.len());
    for candidate in &metadata.output.regions {
        let bytes = ordinary_structure::crop_normalized_jpeg(&source_bytes, &candidate.bbox)?;
        let hash = hashing::sha256_hex(&bytes);
        let existed_before = artifacts::get_by_identity(
            conn,
            &hash,
            ArtifactKind::Crop,
            Some(CROP_DERIVATIVE),
            MATERIALIZATION_VERSION,
        )?
        .is_some();
        let archived = match archive_bytes(&bytes, &hash, "jpg", &crop_dir) {
            Ok(value) => value,
            Err(error) => {
                if !aligned_existed_before {
                    aligned.rollback_new_file();
                }
                for region in &regions {
                    if !region.existed_before {
                        region.archived.rollback_new_file();
                    }
                }
                return Err(error);
            }
        };
        regions.push(PreparedRegion {
            assessment_item_id: candidate.assessment_item_id,
            region_index: candidate.region_index,
            hash,
            bytes_len: bytes.len() as i64,
            archived,
            existed_before,
        });
    }
    Ok(PreparedCandidate {
        source: metadata.source,
        source_bytes,
        aligned,
        aligned_existed_before,
        regions,
    })
}

pub fn confirm_page_structure(
    conn: &Connection,
    data_dir: &Path,
    page_id: i64,
    ai_run_id: i64,
    confirmed_by: &str,
) -> CoreResult<OrdinaryStructureConfirmationResult> {
    if let Some(existing) = ordinary_structure::get_confirmation_by_ai_run(conn, ai_run_id)? {
        if existing.page_id != page_id {
            return Err(CoreError::Invalid(
                "普通试卷结构 run 已确认到另一页面".into(),
            ));
        }
        // 幂等重放仍由核心服务读取并验证完整账本。
        let tx = conn.unchecked_transaction()?;
        let result = ordinary_structure::confirm_in_transaction(
            &tx,
            &ConfirmOrdinaryStructureInput {
                page_id,
                ai_run_id,
                aligned_artifact_id: 1,
                region_artifacts: &[],
                confirmed_by,
            },
        )?;
        tx.commit()?;
        return Ok(result);
    }

    let metadata = load_candidate(conn, page_id, ai_run_id)?;
    let prepared = prepare_files(conn, data_dir, metadata)?;
    let result = (|| -> CoreResult<OrdinaryStructureConfirmationResult> {
        let tx = conn.unchecked_transaction()?;
        let aligned = register_artifact(
            &tx,
            &prepared.aligned,
            &prepared.source.sha256,
            prepared.source_bytes.len() as i64,
            ArtifactKind::Page,
            &prepared.source.mime_type,
            None,
            None,
            Some(prepared.source.id),
            Some(ALIGNED_DERIVATIVE),
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
                Some(CROP_DERIVATIVE),
                MATERIALIZATION_VERSION,
                PrivacyClass::StudentSensitive,
            )?;
            region_artifacts.push(OrdinaryRegionArtifact {
                assessment_item_id: region.assessment_item_id,
                region_index: region.region_index,
                crop_artifact_id: crop.id,
            });
        }
        let result = ordinary_structure::confirm_in_transaction(
            &tx,
            &ConfirmOrdinaryStructureInput {
                page_id,
                ai_run_id,
                aligned_artifact_id: aligned.id,
                region_artifacts: &region_artifacts,
                confirmed_by,
            },
        )?;
        tx.commit()?;
        Ok(result)
    })();
    if result.is_err() {
        prepared.rollback_new_files();
    }
    result
}
