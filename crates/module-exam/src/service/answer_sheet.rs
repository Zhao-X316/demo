//! 固定答题卡模板确认。
//!
//! 系统可以自动生成模板候选，但只有老师确认后才成为 active revision。模板只固定
//! 空白卡、页码、锚点、题号和格位，不产生学生作答、分数或发布。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit;
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ArchiveStatus, ArtifactKind, AuditActorType, PrivacyClass};

use crate::answer_sheet_recognition::AnswerSheetTemplateDefinition;

pub struct ConfirmAnswerSheetTemplateInput<'a> {
    pub definition: &'a AnswerSheetTemplateDefinition,
    pub source_ai_run_id: Option<i64>,
    pub confirmed_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSheetTemplateRevision {
    pub id: i64,
    pub public_id: String,
    pub assessment_version_id: i64,
    pub revision: i64,
    pub template_version: String,
    pub page_no: i64,
    pub blank_artifact_id: i64,
    pub template_hash: String,
    pub template_json: String,
    pub source_ai_run_id: Option<i64>,
    pub confirmed_by: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSheetTemplatePageStatus {
    pub page_no: i64,
    pub expected_item_count: usize,
    pub objective_item_count: usize,
    pub subjective_item_count: usize,
    pub active_template_revision_id: Option<i64>,
    pub ready: bool,
    pub issue_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerSheetTemplateSetStatus {
    pub assessment_version_id: i64,
    pub template_version: String,
    pub ready: bool,
    pub template_set_hash: Option<String>,
    pub pages: Vec<AnswerSheetTemplatePageStatus>,
    pub issue_codes: Vec<String>,
}

fn template_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AnswerSheetTemplateRevision> {
    Ok(AnswerSheetTemplateRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        assessment_version_id: row.get(2)?,
        revision: row.get(3)?,
        template_version: row.get(4)?,
        page_no: row.get(5)?,
        blank_artifact_id: row.get(6)?,
        template_hash: row.get(7)?,
        template_json: row.get(8)?,
        source_ai_run_id: row.get(9)?,
        confirmed_by: row.get(10)?,
        state: row.get(11)?,
        created_at: row.get(12)?,
    })
}

pub fn get_active_answer_sheet_template(
    conn: &Connection,
    assessment_version_id: i64,
    page_no: i64,
) -> CoreResult<Option<AnswerSheetTemplateRevision>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,assessment_version_id,revision,template_version,page_no,
                    blank_artifact_id,template_hash,template_json,source_ai_run_id,
                    confirmed_by,state,created_at
             FROM exam_answer_sheet_template_revisions_v2
             WHERE assessment_version_id=?1 AND page_no=?2 AND state='active'",
            (assessment_version_id, page_no),
            template_row,
        )
        .optional()?)
}

fn definition_item_map(
    definition: &AnswerSheetTemplateDefinition,
) -> CoreResult<BTreeMap<i64, String>> {
    let mut actual = BTreeMap::new();
    for item in &definition.items {
        if actual
            .insert(
                item.assessment_item_id,
                item.question_type.as_str().to_string(),
            )
            .is_some()
        {
            return Err(CoreError::Invalid(
                "答题卡每道客观题必须且只能映射一个题区".into(),
            ));
        }
    }
    for region in &definition.subjective_regions {
        if actual
            .insert(
                region.assessment_item_id,
                region.question_type.as_db().to_string(),
            )
            .is_some()
        {
            return Err(CoreError::Invalid(
                "答题卡每道题必须且只能映射一个客观或主观题区".into(),
            ));
        }
    }
    Ok(actual)
}

/// 整套答题卡只有在每个实际页码都有 active 模板、且客观/主观题逐项覆盖时才 ready。
///
/// 该状态只冻结模板集合，不授权评分、老师确认或发布。
pub fn answer_sheet_template_set_status(
    conn: &Connection,
    assessment_version_id: i64,
) -> CoreResult<AnswerSheetTemplateSetStatus> {
    if assessment_version_id <= 0 {
        return Err(CoreError::Invalid("答题卡作业版本 id 必须为正数".into()));
    }
    let scope: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT state,template_version FROM exam_assessment_versions_v2 WHERE id=?1",
            [assessment_version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((state, template_version)) = scope else {
        return Err(CoreError::NotFound("答题卡关联的作业版本".into()));
    };
    let template_version = template_version
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| CoreError::Invalid("答题卡作业尚未固定模板版本".into()))?;
    if state != "confirmed" {
        return Err(CoreError::Invalid(
            "只有已确认作业版本可以检查整套答题卡模板".into(),
        ));
    }

    let mut expected_by_page: BTreeMap<i64, BTreeMap<i64, String>> = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT i.id,q.question_type,
                COALESCE(CAST(json_extract(i.presentation_snapshot_json,'$.page_no') AS INTEGER),1)
         FROM exam_assessment_items_v2 i
         JOIN k1_question_versions q ON q.id=i.question_version_id
         WHERE i.assessment_version_id=?1 AND i.state='active'
         ORDER BY i.order_index,i.id",
    )?;
    let rows = stmt.query_map([assessment_version_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)?,
        ))
    })?;
    for row in rows {
        let (item_id, question_type, page_no) = row?;
        if page_no <= 0
            || !matches!(
                question_type.as_str(),
                "single" | "multiple" | "true_false" | "fill_blank" | "short_answer"
            )
        {
            return Err(CoreError::Invalid("答题卡题目页码或题型不受支持".into()));
        }
        expected_by_page
            .entry(page_no)
            .or_default()
            .insert(item_id, question_type);
    }
    if expected_by_page.is_empty() {
        return Err(CoreError::Invalid("答题卡作业没有可建模题目".into()));
    }

    let mut active_by_page = BTreeMap::new();
    let mut stmt = conn.prepare(
        "SELECT id,public_id,assessment_version_id,revision,template_version,page_no,
                blank_artifact_id,template_hash,template_json,source_ai_run_id,
                confirmed_by,state,created_at
         FROM exam_answer_sheet_template_revisions_v2
         WHERE assessment_version_id=?1 AND state='active'
         ORDER BY page_no",
    )?;
    let rows = stmt.query_map([assessment_version_id], template_row)?;
    for row in rows {
        let revision = row?;
        active_by_page.insert(revision.page_no, revision);
    }

    let max_page = *expected_by_page
        .keys()
        .max()
        .ok_or_else(|| CoreError::Invalid("答题卡缺少页码".into()))?;
    let mut issue_codes = Vec::new();
    let mut pages = Vec::new();
    let mut hash_pages = Vec::new();
    for page_no in 1..=max_page {
        let expected = expected_by_page.get(&page_no);
        let mut page_issues = Vec::new();
        if expected.is_none() {
            page_issues.push("PAGE_SEQUENCE_GAP".to_string());
        }
        let active = active_by_page.get(&page_no);
        let mut objective_item_count = 0;
        let mut subjective_item_count = 0;
        if let (Some(expected), Some(active)) = (expected, active) {
            let definition: AnswerSheetTemplateDefinition =
                serde_json::from_str(&active.template_json).map_err(|error| {
                    CoreError::Parse(format!("答题卡第 {page_no} 页模板损坏：{error}"))
                })?;
            definition.validate()?;
            objective_item_count = definition.items.len();
            subjective_item_count = definition.subjective_regions.len();
            if definition.template_hash()? != active.template_hash
                || definition.assessment_version_id != assessment_version_id
                || definition.page_no != page_no
                || definition.template_version.trim() != template_version.trim()
            {
                page_issues.push("TEMPLATE_SCOPE_MISMATCH".to_string());
            }
            if &definition_item_map(&definition)? != expected {
                page_issues.push("ITEM_COVERAGE_MISMATCH".to_string());
            }
            hash_pages.push(serde_json::json!({
                "page_no": page_no,
                "template_revision_id": active.id,
                "template_hash": active.template_hash,
            }));
        } else if expected.is_some() {
            page_issues.push("MISSING_TEMPLATE".to_string());
        }
        for code in &page_issues {
            issue_codes.push(format!("PAGE_{page_no}_{code}"));
        }
        pages.push(AnswerSheetTemplatePageStatus {
            page_no,
            expected_item_count: expected.map(BTreeMap::len).unwrap_or(0),
            objective_item_count,
            subjective_item_count,
            active_template_revision_id: active.map(|revision| revision.id),
            ready: page_issues.is_empty(),
            issue_codes: page_issues,
        });
    }
    for page_no in active_by_page.keys() {
        if !expected_by_page.contains_key(page_no) {
            issue_codes.push(format!("PAGE_{page_no}_UNEXPECTED_TEMPLATE"));
        }
    }
    let ready = issue_codes.is_empty();
    let template_set_hash = if ready {
        let bytes = serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "assessment_version_id": assessment_version_id,
            "template_version": template_version.trim(),
            "pages": hash_pages,
        }))
        .map_err(|error| CoreError::Parse(format!("答题卡模板集序列化失败：{error}")))?;
        Some(hashing::sha256_hex(&bytes))
    } else {
        None
    };
    Ok(AnswerSheetTemplateSetStatus {
        assessment_version_id,
        template_version,
        ready,
        template_set_hash,
        pages,
        issue_codes,
    })
}

pub fn confirm_answer_sheet_template(
    conn: &mut Connection,
    input: &ConfirmAnswerSheetTemplateInput<'_>,
) -> CoreResult<AnswerSheetTemplateRevision> {
    input.definition.validate()?;
    if input.confirmed_by.trim().is_empty() {
        return Err(CoreError::Invalid("答题卡模板确认人不能为空".into()));
    }
    let template_hash = input.definition.template_hash()?;
    if let Some(existing) = get_active_answer_sheet_template(
        conn,
        input.definition.assessment_version_id,
        input.definition.page_no,
    )? {
        if existing.template_hash == template_hash
            && existing.confirmed_by == input.confirmed_by.trim()
        {
            return Ok(existing);
        }
    }

    validate_scope(conn, input)?;
    let template_json = input.definition.to_json()?;
    let tx = conn.transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1
         FROM exam_answer_sheet_template_revisions_v2
         WHERE assessment_version_id=?1 AND page_no=?2",
        (
            input.definition.assessment_version_id,
            input.definition.page_no,
        ),
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_answer_sheet_template_revisions_v2 SET state='superseded'
         WHERE assessment_version_id=?1 AND page_no=?2 AND state='active'",
        (
            input.definition.assessment_version_id,
            input.definition.page_no,
        ),
    )?;
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    tx.execute(
        "INSERT INTO exam_answer_sheet_template_revisions_v2
         (public_id,assessment_version_id,revision,template_version,page_no,
          blank_artifact_id,template_hash,template_json,source_ai_run_id,
          confirmed_by,state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11)",
        (
            &public_id,
            input.definition.assessment_version_id,
            revision,
            input.definition.template_version.trim(),
            input.definition.page_no,
            input.definition.blank_artifact_id,
            &template_hash,
            &template_json,
            input.source_ai_run_id,
            input.confirmed_by.trim(),
            &created_at,
        ),
    )?;
    let id = tx.last_insert_rowid();
    let meta_json = serde_json::json!({
        "schema_version": 1,
        "page_no": input.definition.page_no,
        "blank_artifact_id": input.definition.blank_artifact_id,
        "item_count": input.definition.region_count(),
        "objective_region_count": input.definition.items.len(),
        "subjective_region_count": input.definition.subjective_regions.len(),
        "source_ai_run_id": input.source_ai_run_id,
    })
    .to_string();
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!(
                "exam:answer-sheet-template:{}:{}:{}",
                input.definition.assessment_version_id, input.definition.page_no, revision
            ),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.confirmed_by.trim()),
            action: "exam.answer_sheet_template.confirmed",
            object_type: "exam_assessment_version",
            object_id: &input.definition.assessment_version_id.to_string(),
            object_revision: Some(revision),
            note: None,
            meta_json: Some(&meta_json),
            occurred_at: &created_at,
        },
    )?;
    let created = tx
        .query_row(
            "SELECT id,public_id,assessment_version_id,revision,template_version,page_no,
                    blank_artifact_id,template_hash,template_json,source_ai_run_id,
                    confirmed_by,state,created_at
             FROM exam_answer_sheet_template_revisions_v2 WHERE id=?1",
            [id],
            template_row,
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("新建答题卡模板 revision".into()))?;
    tx.commit()?;
    Ok(created)
}

fn validate_scope(
    conn: &Connection,
    input: &ConfirmAnswerSheetTemplateInput<'_>,
) -> CoreResult<()> {
    let version: Option<(String, Option<String>)> = conn
        .query_row(
            "SELECT state,template_version FROM exam_assessment_versions_v2 WHERE id=?1",
            [input.definition.assessment_version_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((state, template_version)) = version else {
        return Err(CoreError::NotFound("答题卡关联的作业版本".into()));
    };
    if state != "confirmed"
        || template_version.as_deref().map(str::trim)
            != Some(input.definition.template_version.trim())
    {
        return Err(CoreError::Invalid(
            "答题卡模板必须绑定当前已确认作业模板版本".into(),
        ));
    }

    let artifact: Option<(String, String, String, String, String)> = conn
        .query_row(
            "SELECT kind,sha256,mime_type,privacy_class,archive_status
             FROM artifacts WHERE id=?1",
            [input.definition.blank_artifact_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .optional()?;
    let Some((kind, sha256, mime_type, privacy_class, archive_status)) = artifact else {
        return Err(CoreError::NotFound("答题卡空白模板 artifact".into()));
    };
    let kind = ArtifactKind::from_db(&kind);
    let privacy = PrivacyClass::from_db(&privacy_class);
    let archive = ArchiveStatus::from_db(&archive_status);
    if !matches!(kind, Some(ArtifactKind::Image | ArtifactKind::Page))
        || !mime_type.starts_with("image/")
        || !matches!(
            privacy,
            Some(PrivacyClass::TeachingContent | PrivacyClass::PublicSafe)
        )
        || archive != Some(ArchiveStatus::Ready)
        || sha256
            != input
                .definition
                .blank_artifact_sha256
                .trim()
                .to_ascii_lowercase()
    {
        return Err(CoreError::Invalid(
            "答题卡空白模板必须是已归档、非学生敏感且 hash 一致的图片".into(),
        ));
    }

    if let Some(ai_run_id) = input.source_ai_run_id {
        let valid_run: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM ai_runs
             WHERE id=?1 AND source_module='exam' AND run_type='answer_sheet_template'
               AND status='succeeded' AND input_artifact_id=?2)",
            (ai_run_id, input.definition.blank_artifact_id),
            |row| row.get(0),
        )?;
        if !valid_run {
            return Err(CoreError::Invalid(
                "答题卡模板候选 run 未成功或未绑定当前空白卡".into(),
            ));
        }
    }

    let mut stmt = conn.prepare(
        "SELECT i.id,q.question_type
         FROM exam_assessment_items_v2 i
         JOIN k1_question_versions q ON q.id=i.question_version_id
         WHERE i.assessment_version_id=?1 AND i.state='active'
           AND COALESCE(CAST(json_extract(i.presentation_snapshot_json,'$.page_no') AS INTEGER),1)=?2
         ORDER BY i.order_index,i.id",
    )?;
    let rows = stmt.query_map(
        (
            input.definition.assessment_version_id,
            input.definition.page_no,
        ),
        |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
    )?;
    let mut expected = BTreeMap::new();
    for row in rows {
        let (item_id, question_type) = row?;
        if !matches!(
            question_type.as_str(),
            "single" | "multiple" | "true_false" | "fill_blank" | "short_answer"
        ) {
            return Err(CoreError::Invalid("答题卡包含不支持的题型".into()));
        }
        expected.insert(item_id, question_type);
    }
    let actual = definition_item_map(input.definition)?;
    if expected.is_empty() || expected != actual {
        return Err(CoreError::Invalid(
            "答题卡客观格和主观作答区必须完整覆盖当前页全部题目且题型一致".into(),
        ));
    }
    let unique_regions = input
        .definition
        .items
        .iter()
        .map(|item| (item.assessment_item_id, item.region_index))
        .collect::<BTreeSet<_>>();
    if unique_regions.len() != input.definition.items.len() {
        return Err(CoreError::Invalid("答题卡题区映射重复".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;
    use suite_core::db::repo::artifacts;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

    use super::*;
    use crate::answer_sheet_recognition::{
        AnswerSheetAnchor, AnswerSheetItemTemplate, AnswerSheetSubjectiveKind,
        AnswerSheetSubjectiveRegionTemplate, LocalOmrPolicy, SheetRect,
        ANSWER_SHEET_TEMPLATE_SCHEMA_VERSION,
    };
    use crate::objective_recognition::ObjectiveMarkCell;
    use crate::objective_recognition::ObjectiveQuestionType;

    fn setup() -> (Connection, i64, String) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        let hash = "a".repeat(64);
        conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,
                  created_by,created_at,updated_at)
                 VALUES ('assessment-a','答题卡',1,'quiz','include','active','teacher',
                         '2026-07-15T09:00:00.000Z','2026-07-15T09:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,
                  created_at,confirmed_by,confirmed_at)
                 VALUES ('assessment-version-a',1,1,'{hash}','sheet-v1','confirmed',
                         '2026-07-15T09:00:00.000Z','teacher','2026-07-15T09:00:00.000Z');
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('edition-a',1,'PEP','2024','中国历史八上','8','upper','active',
                         '2026-07-15T09:00:00.000Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('map-a',1,1,'confirmed','2026-07-15T09:00:00.000Z',
                         '2026-07-15T09:00:00.000Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('question-a','personal','teacher','unknown',0,
                         '2026-07-15T09:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('question-version-a',1,1,'single','第1题',1,'{hash}',
                         'L3','published','2026-07-15T09:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-a',1,1,'{{"schema_version":1,"selected_labels":["B"]}}',
                         'confirmed','2026-07-15T09:00:00.000Z','teacher',
                         '2026-07-15T09:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('rubric-a',1,1,1,'confirmed','2026-07-15T09:00:00.000Z',
                         'teacher','2026-07-15T09:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('link-a',1,1,1,'confirmed','2026-07-15T09:00:00.000Z',
                         'teacher','2026-07-15T09:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
                 VALUES ('item-a',1,1,1,1,1,0,1,
                         '{{"schema_version":1,"question_no":"1","page_no":1}}',
                         'active','2026-07-15T09:00:00.000Z');"#
        ))
        .unwrap();
        let artifact_hash = "b".repeat(64);
        let artifact = artifacts::create_or_get(
            &conn,
            &artifacts::NewArtifact {
                kind: ArtifactKind::Image,
                sha256: &artifact_hash,
                mime_type: "image/png",
                byte_size: 100,
                original_name: Some("blank-answer-sheet.png"),
                original_path: None,
                archived_path: "/archive/blank-answer-sheet.png",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "fixture-v1",
                privacy_class: PrivacyClass::TeachingContent,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        (conn, artifact.id, artifact_hash)
    }

    fn rect(x: f64, y: f64) -> SheetRect {
        SheetRect {
            x,
            y,
            width: 0.05,
            height: 0.05,
        }
    }

    fn definition(artifact_id: i64, hash: String) -> AnswerSheetTemplateDefinition {
        AnswerSheetTemplateDefinition {
            schema_version: 1,
            assessment_version_id: 1,
            template_version: "sheet-v1".into(),
            page_no: 1,
            canvas_width: 1000,
            canvas_height: 1400,
            blank_artifact_id: artifact_id,
            blank_artifact_sha256: hash,
            anchors: vec![
                AnswerSheetAnchor {
                    key: "top_left".into(),
                    expected: rect(0.02, 0.02),
                    search: rect(0.0, 0.0),
                },
                AnswerSheetAnchor {
                    key: "top_right".into(),
                    expected: rect(0.93, 0.02),
                    search: rect(0.90, 0.0),
                },
                AnswerSheetAnchor {
                    key: "bottom_left".into(),
                    expected: rect(0.02, 0.93),
                    search: rect(0.0, 0.90),
                },
                AnswerSheetAnchor {
                    key: "bottom_right".into(),
                    expected: rect(0.93, 0.93),
                    search: rect(0.90, 0.90),
                },
            ],
            items: vec![AnswerSheetItemTemplate {
                assessment_item_id: 1,
                region_index: 0,
                question_type: ObjectiveQuestionType::Single,
                region: SheetRect {
                    x: 0.1,
                    y: 0.1,
                    width: 0.5,
                    height: 0.1,
                },
                cells: vec![
                    ObjectiveMarkCell {
                        label: "A".into(),
                        x: 0.0,
                        y: 0.0,
                        width: 0.5,
                        height: 1.0,
                    },
                    ObjectiveMarkCell {
                        label: "B".into(),
                        x: 0.5,
                        y: 0.0,
                        width: 0.5,
                        height: 1.0,
                    },
                ],
            }],
            subjective_regions: Vec::new(),
            policy: LocalOmrPolicy {
                blank_max_ratio: 0.02,
                marked_min_ratio: 0.20,
                pixel_delta_threshold: 40,
                cell_inset_ratio: 0.1,
            },
        }
    }

    fn add_second_page_short_answer(conn: &Connection) {
        let hash = "a".repeat(64);
        conn.execute_batch(&format!(
            r#"INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('question-b','personal','teacher','unknown',0,
                         '2026-07-15T09:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('question-version-b',2,1,'short_answer','第2题',4,'{hash}',
                         'L3','published','2026-07-15T09:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-b',2,1,'{{"schema_version":1,"reference":"要点"}}',
                         'confirmed','2026-07-15T09:00:00.000Z','teacher',
                         '2026-07-15T09:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('rubric-b',2,1,4,'confirmed','2026-07-15T09:00:00.000Z',
                         'teacher','2026-07-15T09:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('link-b',2,1,1,'confirmed','2026-07-15T09:00:00.000Z',
                         'teacher','2026-07-15T09:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                  state,created_at)
                 VALUES ('item-b',1,2,2,2,2,1,4,
                         '{{"schema_version":1,"question_no":"2","page_no":2}}',
                         'active','2026-07-15T09:00:00.000Z');"#,
        ))
        .unwrap();
    }

    fn subjective_page_two(artifact_id: i64, hash: String) -> AnswerSheetTemplateDefinition {
        let mut definition = definition(artifact_id, hash);
        definition.schema_version = ANSWER_SHEET_TEMPLATE_SCHEMA_VERSION;
        definition.page_no = 2;
        definition.items.clear();
        definition.subjective_regions = vec![AnswerSheetSubjectiveRegionTemplate {
            assessment_item_id: 2,
            region_index: 0,
            question_type: AnswerSheetSubjectiveKind::ShortAnswer,
            region: SheetRect {
                x: 0.1,
                y: 0.2,
                width: 0.8,
                height: 0.5,
            },
        }];
        definition
    }

    #[test]
    fn teacher_confirms_immutable_template_and_retry_is_idempotent() {
        let (mut conn, artifact_id, hash) = setup();
        let definition = definition(artifact_id, hash);
        let input = ConfirmAnswerSheetTemplateInput {
            definition: &definition,
            source_ai_run_id: None,
            confirmed_by: "teacher",
        };
        let first = confirm_answer_sheet_template(&mut conn, &input).unwrap();
        let second = confirm_answer_sheet_template(&mut conn, &input).unwrap();
        assert_eq!(first.id, second.id);
        let counts: (i64, i64, i64) = conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_answer_sheet_template_revisions_v2),
                   (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                   (SELECT COUNT(*) FROM exam_grade_publications_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(counts, (1, 0, 0));
    }

    #[test]
    fn template_rejects_student_sensitive_blank_card() {
        let (mut conn, _, _) = setup();
        let hash = "c".repeat(64);
        let artifact = artifacts::create_or_get(
            &conn,
            &artifacts::NewArtifact {
                kind: ArtifactKind::Image,
                sha256: &hash,
                mime_type: "image/png",
                byte_size: 100,
                original_name: Some("student-answer-sheet.png"),
                original_path: None,
                archived_path: "/archive/student-answer-sheet.png",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "fixture-v1",
                privacy_class: PrivacyClass::StudentSensitive,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        let definition = definition(artifact.id, hash);
        let error = confirm_answer_sheet_template(
            &mut conn,
            &ConfirmAnswerSheetTemplateInput {
                definition: &definition,
                source_ai_run_id: None,
                confirmed_by: "teacher",
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("非学生敏感"));
    }

    #[test]
    fn multi_page_template_set_blocks_until_objective_and_subjective_pages_are_complete() {
        let (mut conn, artifact_id, hash) = setup();
        add_second_page_short_answer(&conn);

        let initial = answer_sheet_template_set_status(&conn, 1).unwrap();
        assert!(!initial.ready);
        assert_eq!(initial.pages.len(), 2);
        assert!(initial
            .issue_codes
            .contains(&"PAGE_1_MISSING_TEMPLATE".into()));
        assert!(initial
            .issue_codes
            .contains(&"PAGE_2_MISSING_TEMPLATE".into()));

        let page_one = definition(artifact_id, hash.clone());
        confirm_answer_sheet_template(
            &mut conn,
            &ConfirmAnswerSheetTemplateInput {
                definition: &page_one,
                source_ai_run_id: None,
                confirmed_by: "teacher",
            },
        )
        .unwrap();
        let partial = answer_sheet_template_set_status(&conn, 1).unwrap();
        assert!(!partial.ready);
        assert!(partial
            .issue_codes
            .contains(&"PAGE_2_MISSING_TEMPLATE".into()));

        let page_two = subjective_page_two(artifact_id, hash);
        confirm_answer_sheet_template(
            &mut conn,
            &ConfirmAnswerSheetTemplateInput {
                definition: &page_two,
                source_ai_run_id: None,
                confirmed_by: "teacher",
            },
        )
        .unwrap();
        let complete = answer_sheet_template_set_status(&conn, 1).unwrap();
        assert!(complete.ready);
        assert!(complete.issue_codes.is_empty());
        assert_eq!(
            complete.template_set_hash.as_deref().map(str::len),
            Some(64)
        );
        assert_eq!(complete.pages[0].objective_item_count, 1);
        assert_eq!(complete.pages[0].subjective_item_count, 0);
        assert_eq!(complete.pages[1].objective_item_count, 0);
        assert_eq!(complete.pages[1].subjective_item_count, 1);
    }

    #[test]
    fn subjective_region_cannot_be_silently_bound_to_the_wrong_page_or_type() {
        let (mut conn, artifact_id, hash) = setup();
        add_second_page_short_answer(&conn);
        let mut wrong = subjective_page_two(artifact_id, hash);
        wrong.subjective_regions[0].question_type = AnswerSheetSubjectiveKind::FillBlank;
        let error = confirm_answer_sheet_template(
            &mut conn,
            &ConfirmAnswerSheetTemplateInput {
                definition: &wrong,
                source_ai_run_id: None,
                confirmed_by: "teacher",
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("完整覆盖"));
    }
}
