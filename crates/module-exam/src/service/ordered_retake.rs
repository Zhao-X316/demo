//! M2-B3a0 待重拍页面的单点替换。
//!
//! 原图、旧质量结论和旧激活快照全部保留；新照片追加为新的 ingest page，
//! 并通过 replacement revision 占据原学生的逻辑页位。只有该学生整组全部清楚后，
//! 才建立 attempt/page-match。本流程拒绝修改已评分或已发布的页面身份。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::{artifacts, audit};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ArchiveStatus, ArtifactKind, AuditActorType, PrivacyClass};

use super::ordered_activation::{self, GroupingActivation};
use super::ordered_intake;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderedPageReplacement {
    pub id: i64,
    pub public_id: String,
    pub ingest_batch_id: i64,
    pub grouping_decision_id: i64,
    pub original_page_id: i64,
    pub replaces_page_id: i64,
    pub replacement_page_id: i64,
    pub group_index: i64,
    pub student_id: i64,
    pub logical_page_no: i64,
    pub revision: i64,
    pub state: String,
    pub confirmed_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetakePageResult {
    pub replacement: OrderedPageReplacement,
    pub activation: GroupingActivation,
    pub activated_student: bool,
}

pub struct RetakeArtifactInput<'a> {
    pub sha256: &'a str,
    pub byte_size: i64,
    pub original_name: &'a str,
    pub original_path: &'a str,
    pub archived_path: &'a str,
    pub processing_version: &'a str,
}

pub struct ReplaceRejectedPageInput<'a> {
    pub ingest_batch_id: i64,
    pub rejected_page_id: i64,
    pub artifact: RetakeArtifactInput<'a>,
    pub confirmed_by: &'a str,
}

#[derive(Debug, Deserialize)]
struct AssignmentEnvelope {
    groups: Vec<AssignmentGroup>,
}

#[derive(Debug, Deserialize)]
struct AssignmentGroup {
    group_index: i64,
    student_id: i64,
    pages: Vec<AssignmentPage>,
}

#[derive(Debug, Deserialize)]
struct AssignmentPage {
    page_id: i64,
    page_no: i64,
}

fn replacement_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OrderedPageReplacement> {
    Ok(OrderedPageReplacement {
        id: row.get(0)?,
        public_id: row.get(1)?,
        ingest_batch_id: row.get(2)?,
        grouping_decision_id: row.get(3)?,
        original_page_id: row.get(4)?,
        replaces_page_id: row.get(5)?,
        replacement_page_id: row.get(6)?,
        group_index: row.get(7)?,
        student_id: row.get(8)?,
        logical_page_no: row.get(9)?,
        revision: row.get(10)?,
        state: row.get(11)?,
        confirmed_by: row.get(12)?,
    })
}

fn parse_rejected(json: &str) -> CoreResult<BTreeSet<i64>> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| CoreError::Parse(format!("待重拍页面快照读取失败：{error}")))?;
    value
        .get("page_ids")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| CoreError::Parse("待重拍页面快照缺少 page_ids".into()))?
        .iter()
        .map(|value| {
            value
                .as_i64()
                .filter(|id| *id > 0)
                .ok_or_else(|| CoreError::Parse("待重拍页面 ID 非法".into()))
        })
        .collect()
}

fn effective_page_id(conn: &Connection, original_page_id: i64) -> CoreResult<i64> {
    Ok(conn
        .query_row(
            "SELECT replacement_page_id FROM exam_ordered_page_replacements_v2
             WHERE original_page_id=?1 AND state='active'",
            [original_page_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(original_page_id))
}

fn next_attempt_no(
    conn: &Connection,
    assessment_version_id: i64,
    student_id: i64,
) -> CoreResult<i64> {
    Ok(conn.query_row(
        "SELECT COALESCE(MAX(attempt_no),0)+1 FROM exam_attempts_v2
         WHERE assessment_version_id=?1 AND student_id=?2",
        (assessment_version_id, student_id),
        |row| row.get(0),
    )?)
}

struct AuditAppend<'a> {
    key: &'a str,
    action: &'a str,
    object_type: &'a str,
    object_id: &'a str,
    revision: i64,
}

fn append_teacher_audit(
    conn: &Connection,
    event: &AuditAppend<'_>,
    teacher: &str,
    now: &str,
) -> CoreResult<()> {
    audit::append(
        conn,
        &audit::NewAuditEvent {
            idempotency_key: event.key,
            actor_type: AuditActorType::Teacher,
            actor_id: Some(teacher),
            action: event.action,
            object_type: event.object_type,
            object_id: event.object_id,
            object_revision: Some(event.revision),
            note: None,
            meta_json: Some(r#"{"schema_version":1}"#),
            occurred_at: now,
        },
    )?;
    Ok(())
}

fn insert_confirmed_match(
    conn: &Connection,
    page_id: i64,
    page_no: i64,
    attempt_id: i64,
    teacher: &str,
    now: &str,
) -> CoreResult<i64> {
    let active: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_page_match_revisions_v2
         WHERE page_id=?1 AND state='active')",
        [page_id],
        |row| row.get(0),
    )?;
    if active {
        return Err(CoreError::Invalid(
            "待重拍学生页组已存在正式页面归属，不能走重拍激活".into(),
        ));
    }
    let revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_page_match_revisions_v2 WHERE page_id=?1",
        [page_id],
        |row| row.get(0),
    )?;
    let public_id = ids::new_public_id();
    conn.execute(
        "INSERT INTO exam_page_match_revisions_v2
         (public_id,page_id,revision,attempt_id,page_no,student_confidence,
          page_no_confidence,template_confidence,decision,reason_code,
          confirmed_by,state,created_at)
         VALUES (?1,?2,?3,?4,?5,1.0,1.0,NULL,'teacher_confirmed',
                 'ordered_retake_teacher_confirmed',?6,'active',?7)",
        (
            &public_id, page_id, revision, attempt_id, page_no, teacher, now,
        ),
    )?;
    conn.execute(
        "UPDATE exam_ingest_pages_v2 SET state='matched',updated_at=?1 WHERE id=?2",
        (now, page_id),
    )?;
    append_teacher_audit(
        conn,
        &AuditAppend {
            key: &format!("exam:page-match:{public_id}:active"),
            action: "exam.page_match.activated",
            object_type: "exam_page_match",
            object_id: &public_id,
            revision,
        },
        teacher,
        now,
    )?;
    Ok(conn.last_insert_rowid())
}

fn activation_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GroupingActivation> {
    Ok(GroupingActivation {
        id: row.get(0)?,
        public_id: row.get(1)?,
        ingest_batch_id: row.get(2)?,
        grouping_decision_id: row.get(3)?,
        revision: row.get(4)?,
        snapshot_hash: row.get(5)?,
        rejected_page_ids_json: row.get(6)?,
        attempt_ids_json: row.get(7)?,
        page_match_ids_json: row.get(8)?,
        mapped_group_count: row.get(9)?,
        rejected_group_count: row.get(10)?,
        state: row.get(11)?,
        confirmed_by: row.get(12)?,
    })
}

/// 用一张已归档 JPEG 替换当前激活快照中的单个待重拍页。
pub fn replace_rejected_page(
    conn: &Connection,
    input: &ReplaceRejectedPageInput<'_>,
) -> CoreResult<RetakePageResult> {
    let teacher = input.confirmed_by.trim();
    if teacher.is_empty() {
        return Err(CoreError::Invalid("重拍确认必须记录老师".into()));
    }
    let current = ordered_activation::current_activation(conn, input.ingest_batch_id)?
        .ok_or_else(|| CoreError::Invalid("请先完成页面质量确认".into()))?;
    let mut rejected = parse_rejected(&current.rejected_page_ids_json)?;
    if !rejected.contains(&input.rejected_page_id) {
        return Err(CoreError::Invalid(
            "只能替换当前仍标记为需重拍的页面".into(),
        ));
    }
    let decision = ordered_intake::current_grouping_decision(conn, input.ingest_batch_id)?
        .ok_or_else(|| CoreError::Invalid("缺少老师确认的照片分组".into()))?;
    if decision.id != current.grouping_decision_id {
        return Err(CoreError::Invalid("照片分组已变化，请重新打开批次".into()));
    }
    let envelope: AssignmentEnvelope = serde_json::from_str(&decision.assignments_json)
        .map_err(|error| CoreError::Parse(format!("学生页组映射读取失败：{error}")))?;

    let mut selected: Option<(&AssignmentGroup, &AssignmentPage, i64)> = None;
    for group in &envelope.groups {
        for page in &group.pages {
            let effective = effective_page_id(conn, page.page_id)?;
            if effective == input.rejected_page_id {
                selected = Some((group, page, effective));
                break;
            }
        }
    }
    let (group, slot, replaces_page_id) =
        selected.ok_or_else(|| CoreError::Invalid("待重拍页不属于当前有效学生页组".into()))?;

    let batch_scope: (i64, String) = conn
        .query_row(
            "SELECT assessment_version_id,state FROM exam_ingest_batches_v2 WHERE id=?1",
            [input.ingest_batch_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("ingest_batch#{}", input.ingest_batch_id)))?;
    if matches!(batch_scope.1.as_str(), "failed" | "voided") {
        return Err(CoreError::Invalid("失败或作废批次不能补拍".into()));
    }
    let has_downstream: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM exam_page_match_revisions_v2 m
           JOIN exam_ingest_pages_v2 p ON p.id=m.page_id
           WHERE p.batch_id=?1 AND m.state='active' AND m.attempt_id IN (
             SELECT a.id FROM exam_attempts_v2 a WHERE a.student_id=?2
               AND a.assessment_version_id=?3 AND a.state<>'voided'
           )
         )",
        (input.ingest_batch_id, group.student_id, batch_scope.0),
        |row| row.get(0),
    )?;
    if has_downstream {
        return Err(CoreError::Invalid(
            "该学生已建立正式页面归属；评分或发布后的纠错不能使用重拍入口".into(),
        ));
    }

    let tx = conn.unchecked_transaction()?;
    let now = time::utc_now_rfc3339();
    let artifact = artifacts::create_or_get(
        &tx,
        &artifacts::NewArtifact {
            kind: ArtifactKind::Image,
            sha256: input.artifact.sha256,
            mime_type: "image/jpeg",
            byte_size: input.artifact.byte_size,
            original_name: Some(input.artifact.original_name),
            original_path: Some(input.artifact.original_path),
            archived_path: input.artifact.archived_path,
            parent_artifact_id: None,
            derivative_type: None,
            processing_version: input.artifact.processing_version,
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )?;
    let used_in_batch: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_ingest_pages_v2
         WHERE batch_id=?1 AND source_artifact_id=?2)",
        (input.ingest_batch_id, artifact.id),
        |row| row.get(0),
    )?;
    if used_in_batch {
        return Err(CoreError::Invalid(
            "这张照片已在本批使用，请选择新的重拍照片".into(),
        ));
    }
    let import_index: i64 = tx.query_row(
        "SELECT COALESCE(MAX(import_index),-1)+1 FROM exam_ingest_pages_v2 WHERE batch_id=?1",
        [input.ingest_batch_id],
        |row| row.get(0),
    )?;
    let page_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO exam_ingest_pages_v2
         (public_id,batch_id,source_artifact_id,import_index,expected_page_no,state,created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,'quality_checked',?6,?6)",
        (
            &page_public_id,
            input.ingest_batch_id,
            artifact.id,
            import_index,
            slot.page_no,
            &now,
        ),
    )?;
    let replacement_page_id = tx.last_insert_rowid();

    let page_type_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO exam_page_type_revisions_v2
         (public_id,page_id,revision,page_type_key,confidence,evidence_json,decision,state,
          created_by_type,created_by,confirmed_by,created_at)
         VALUES (?1,?2,1,?3,1.0,?4,'teacher_confirmed','active','teacher',?5,?5,?6)",
        (
            &page_type_public_id,
            replacement_page_id,
            format!("page_{}", slot.page_no),
            r#"{"schema_version":1,"source":"ordered_retake_logical_slot_v1"}"#,
            teacher,
            &now,
        ),
    )?;
    let quality_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO exam_page_quality_revisions_v2
         (public_id,page_id,revision,blur_score,glare_score,brightness_score,
          perspective_score,rotation_degrees,crop_complete,result,issue_codes_json,
          checked_by_type,checked_by,state,created_at)
         VALUES (?1,?2,1,0.0,0.0,0.5,1.0,0.0,1,'pass',?3,'teacher',?4,'active',?5)",
        (
            &quality_public_id,
            replacement_page_id,
            r#"{"schema_version":1,"codes":[],"metrics_source":"teacher_retake_confirmation_v1"}"#,
            teacher,
            &now,
        ),
    )?;

    let original_page_id = slot.page_id;
    let replacement_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_ordered_page_replacements_v2
         WHERE original_page_id=?1",
        [original_page_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_ordered_page_replacements_v2 SET state='superseded'
         WHERE original_page_id=?1 AND state='active'",
        [original_page_id],
    )?;
    let replacement_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO exam_ordered_page_replacements_v2
         (public_id,ingest_batch_id,grouping_decision_id,original_page_id,replaces_page_id,
          replacement_page_id,group_index,student_id,logical_page_no,revision,state,
          confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11,?12)",
        params![
            &replacement_public_id,
            input.ingest_batch_id,
            decision.id,
            original_page_id,
            replaces_page_id,
            replacement_page_id,
            group.group_index,
            group.student_id,
            slot.page_no,
            replacement_revision,
            teacher,
            &now,
        ],
    )?;
    let replacement_id = tx.last_insert_rowid();
    tx.execute(
        "UPDATE exam_ingest_pages_v2 SET state='voided',updated_at=?1 WHERE id=?2",
        (&now, replaces_page_id),
    )?;
    tx.execute(
        "UPDATE exam_pipeline_issues_v2 SET state='resolved',resolved_by=?1,resolved_at=?2,
         resolution_note='ordered_retake_replaced',updated_at=?2
         WHERE target_type='page' AND target_public_id=(
           SELECT public_id FROM exam_ingest_pages_v2 WHERE id=?3
         ) AND issue_code='PAGE_QUALITY_GATE' AND state='open'",
        (teacher, &now, replaces_page_id),
    )?;
    append_teacher_audit(
        &tx,
        &AuditAppend {
            key: &format!("exam:ordered-page-replacement:{replacement_public_id}:active"),
            action: "exam.ordered_page.replaced",
            object_type: "exam_ordered_page_replacement",
            object_id: &replacement_public_id,
            revision: replacement_revision,
        },
        teacher,
        &now,
    )?;

    rejected.remove(&replaces_page_id);
    let mut effective_by_original = BTreeMap::new();
    for assignment_group in &envelope.groups {
        for page in &assignment_group.pages {
            effective_by_original.insert(page.page_id, effective_page_id(&tx, page.page_id)?);
        }
    }
    let group_still_rejected = group.pages.iter().any(|page| {
        effective_by_original
            .get(&page.page_id)
            .is_some_and(|effective| rejected.contains(effective))
    });
    let activated_student = !group_still_rejected;
    if activated_student {
        let attempt_no = next_attempt_no(&tx, batch_scope.0, group.student_id)?;
        let attempt_public_id = ids::new_public_id();
        let attempt_kind =
            super::assessment::attempt_kind_for_assessment_version(&tx, batch_scope.0, attempt_no)?;
        tx.execute(
            "INSERT INTO exam_attempts_v2
             (public_id,assessment_version_id,student_id,attempt_no,source_kind,
              attempt_kind,state,created_at,updated_at)
             VALUES (?1,?2,?3,?4,'image',?5,'grading',?6,?6)",
            (
                &attempt_public_id,
                batch_scope.0,
                group.student_id,
                attempt_no,
                attempt_kind,
                &now,
            ),
        )?;
        let attempt_id = tx.last_insert_rowid();
        for page in &group.pages {
            let effective = effective_by_original[&page.page_id];
            insert_confirmed_match(&tx, effective, page.page_no, attempt_id, teacher, &now)?;
        }
    }

    let mut rejected_group_count = 0_i64;
    for assignment_group in &envelope.groups {
        let any_rejected = assignment_group.pages.iter().any(|page| {
            effective_by_original
                .get(&page.page_id)
                .is_some_and(|effective| rejected.contains(effective))
        });
        if any_rejected {
            rejected_group_count += 1;
        }
    }
    let mapped_group_count = envelope.groups.len() as i64 - rejected_group_count;
    tx.execute(
        "UPDATE exam_ingest_batches_v2 SET state=?1,updated_at=?2 WHERE id=?3",
        (
            if rejected_group_count == 0 {
                "ready"
            } else {
                "needs_review"
            },
            &now,
            input.ingest_batch_id,
        ),
    )?;

    let mut attempt_ids = BTreeMap::<i64, i64>::new();
    let mut attempt_stmt = tx.prepare(
        "SELECT DISTINCT a.student_id,a.id
         FROM exam_page_match_revisions_v2 m
         JOIN exam_ingest_pages_v2 p ON p.id=m.page_id
         JOIN exam_attempts_v2 a ON a.id=m.attempt_id
         WHERE p.batch_id=?1 AND m.state='active' AND a.state<>'voided'
         ORDER BY a.student_id,a.id",
    )?;
    for row in attempt_stmt.query_map([input.ingest_batch_id], |row| {
        Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
    })? {
        let (student_id, attempt_id) = row?;
        attempt_ids.insert(student_id, attempt_id);
    }
    drop(attempt_stmt);
    let mut match_stmt = tx.prepare(
        "SELECT m.id FROM exam_page_match_revisions_v2 m
         JOIN exam_ingest_pages_v2 p ON p.id=m.page_id
         WHERE p.batch_id=?1 AND m.state='active' ORDER BY m.id",
    )?;
    let match_ids = match_stmt
        .query_map([input.ingest_batch_id], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    drop(match_stmt);

    tx.execute(
        "UPDATE exam_ordered_grouping_activations_v2 SET state='superseded'
         WHERE id=?1 AND state='active'",
        [current.id],
    )?;
    let activation_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_ordered_grouping_activations_v2
         WHERE ingest_batch_id=?1",
        [input.ingest_batch_id],
        |row| row.get(0),
    )?;
    let activation_snapshot = serde_json::json!({
        "schema_version": 2,
        "grouping_decision_id": decision.id,
        "previous_activation_id": current.id,
        "replacement_id": replacement_id,
        "effective_page_ids_by_original": effective_by_original,
        "rejected_page_ids": rejected.iter().collect::<Vec<_>>()
    });
    let snapshot_hash = hashing::sha256_hex(
        &serde_json::to_vec(&activation_snapshot)
            .map_err(|error| CoreError::Parse(format!("重拍激活快照失败：{error}")))?,
    );
    let rejected_json = serde_json::json!({
        "schema_version": 2,
        "page_ids": rejected.iter().collect::<Vec<_>>()
    })
    .to_string();
    let attempt_json = serde_json::json!({
        "schema_version": 2,
        "by_student_id": attempt_ids
    })
    .to_string();
    let matches_json = serde_json::json!({
        "schema_version": 2,
        "ids": match_ids
    })
    .to_string();
    let activation_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO exam_ordered_grouping_activations_v2
         (public_id,ingest_batch_id,grouping_decision_id,revision,snapshot_hash,
          rejected_page_ids_json,attempt_ids_json,page_match_ids_json,mapped_group_count,
          rejected_group_count,state,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11,?12)",
        params![
            &activation_public_id,
            input.ingest_batch_id,
            decision.id,
            activation_revision,
            &snapshot_hash,
            &rejected_json,
            &attempt_json,
            &matches_json,
            mapped_group_count,
            rejected_group_count,
            teacher,
            &now,
        ],
    )?;
    let activation_id = tx.last_insert_rowid();
    append_teacher_audit(
        &tx,
        &AuditAppend {
            key: &format!("exam:ordered-grouping-activation:{activation_public_id}:active"),
            action: "exam.ordered_grouping.retake_activated",
            object_type: "exam_ordered_grouping_activation",
            object_id: &activation_public_id,
            revision: activation_revision,
        },
        teacher,
        &now,
    )?;
    tx.commit()?;

    let replacement = conn.query_row(
        "SELECT id,public_id,ingest_batch_id,grouping_decision_id,original_page_id,
                replaces_page_id,replacement_page_id,group_index,student_id,logical_page_no,
                revision,state,confirmed_by
         FROM exam_ordered_page_replacements_v2 WHERE id=?1",
        [replacement_id],
        replacement_row,
    )?;
    let activation = conn.query_row(
        "SELECT id,public_id,ingest_batch_id,grouping_decision_id,revision,snapshot_hash,
                rejected_page_ids_json,attempt_ids_json,page_match_ids_json,
                mapped_group_count,rejected_group_count,state,confirmed_by
         FROM exam_ordered_grouping_activations_v2 WHERE id=?1",
        [activation_id],
        activation_row,
    )?;
    Ok(RetakePageResult {
        replacement,
        activation,
        activated_student,
    })
}
