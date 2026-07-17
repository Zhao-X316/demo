//! M2-B3a0 老师查看归组原图后的质量确认与正式页面归属激活。
//!
//! 一个批次只接受一次 active 激活：质量 revision、attempt、page match 和激活快照
//! 在同一 SQLite transaction 中提交。被标记需重拍的任一页面只扣住该学生页组，
//! 不让后续学生整体错位，也不会创建评分、发布或学习证据。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit;
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use super::ordered_intake;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupedPageEvidence {
    pub group_index: i64,
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub pages: Vec<PageEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageEvidence {
    pub page_id: i64,
    pub replaced_page_id: Option<i64>,
    pub page_no: i64,
    pub import_index: i64,
    pub archived_path: String,
    pub original_name: Option<String>,
    pub page_state: String,
    pub quality_result: Option<String>,
    pub match_decision: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupingActivation {
    pub id: i64,
    pub public_id: String,
    pub ingest_batch_id: i64,
    pub grouping_decision_id: i64,
    pub revision: i64,
    pub snapshot_hash: String,
    pub rejected_page_ids_json: String,
    pub attempt_ids_json: String,
    pub page_match_ids_json: String,
    pub mapped_group_count: i64,
    pub rejected_group_count: i64,
    pub state: String,
    pub confirmed_by: String,
}

pub struct ConfirmGroupingQualityInput<'a> {
    pub ingest_batch_id: i64,
    pub rejected_page_ids: &'a [i64],
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
    student_no: String,
    student_name: String,
    pages: Vec<AssignmentPage>,
}

#[derive(Debug, Deserialize)]
struct AssignmentPage {
    page_id: i64,
    import_index: i64,
    page_no: i64,
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

pub fn current_activation(
    conn: &Connection,
    ingest_batch_id: i64,
) -> CoreResult<Option<GroupingActivation>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,grouping_decision_id,revision,snapshot_hash,
                    rejected_page_ids_json,attempt_ids_json,page_match_ids_json,
                    mapped_group_count,rejected_group_count,state,confirmed_by
             FROM exam_ordered_grouping_activations_v2
             WHERE ingest_batch_id=?1 AND state='active'",
            [ingest_batch_id],
            activation_row,
        )
        .optional()?)
}

fn parse_assignments(json: &str) -> CoreResult<Vec<AssignmentGroup>> {
    let envelope: AssignmentEnvelope = serde_json::from_str(json)
        .map_err(|error| CoreError::Parse(format!("学生页组映射读取失败：{error}")))?;
    if envelope.groups.is_empty() || envelope.groups.iter().any(|group| group.pages.is_empty()) {
        return Err(CoreError::Invalid("学生页组映射不能为空".into()));
    }
    let mut page_ids = BTreeSet::new();
    for group in &envelope.groups {
        if group.student_id < 1
            || group.group_index < 0
            || group.pages.iter().any(|page| {
                page.page_id < 1
                    || page.page_no < 1
                    || page.import_index < 0
                    || !page_ids.insert(page.page_id)
            })
        {
            return Err(CoreError::Invalid("学生页组映射包含重复或非法页面".into()));
        }
    }
    Ok(envelope.groups)
}

pub fn grouping_evidence(
    conn: &Connection,
    ingest_batch_id: i64,
) -> CoreResult<Vec<GroupedPageEvidence>> {
    let decision = ordered_intake::current_grouping_decision(conn, ingest_batch_id)?
        .ok_or_else(|| CoreError::Invalid("请先确认照片与学生顺序".into()))?;
    let assignments = parse_assignments(&decision.assignments_json)?;
    assignments
        .into_iter()
        .map(|group| {
            let pages = group
                .pages
                .into_iter()
                .map(|page| {
                    let effective_page_id = conn
                        .query_row(
                            "SELECT replacement_page_id
                             FROM exam_ordered_page_replacements_v2
                             WHERE original_page_id=?1 AND state='active'",
                            [page.page_id],
                            |row| row.get(0),
                        )
                        .optional()?
                        .unwrap_or(page.page_id);
                    conn.query_row(
                        "SELECT p.import_index,a.archived_path,a.original_name,p.state,
                                q.result,m.decision
                         FROM exam_ingest_pages_v2 p
                         JOIN artifacts a ON a.id=p.source_artifact_id
                         LEFT JOIN exam_page_quality_revisions_v2 q
                           ON q.page_id=p.id AND q.state='active'
                         LEFT JOIN exam_page_match_revisions_v2 m
                           ON m.page_id=p.id AND m.state='active'
                         WHERE p.id=?1 AND p.batch_id=?2",
                        (effective_page_id, ingest_batch_id),
                        |row| {
                            Ok(PageEvidence {
                                page_id: effective_page_id,
                                replaced_page_id: (effective_page_id != page.page_id)
                                    .then_some(page.page_id),
                                page_no: page.page_no,
                                import_index: row.get(0)?,
                                archived_path: row.get(1)?,
                                original_name: row.get(2)?,
                                page_state: row.get(3)?,
                                quality_result: row.get(4)?,
                                match_decision: row.get(5)?,
                            })
                        },
                    )
                    .optional()?
                    .ok_or_else(|| {
                        CoreError::Invalid(format!(
                            "页组中的当前页面 {effective_page_id} 不属于当前批次"
                        ))
                    })
                })
                .collect::<CoreResult<Vec<_>>>()?;
            Ok(GroupedPageEvidence {
                group_index: group.group_index,
                student_id: group.student_id,
                student_no: group.student_no,
                student_name: group.student_name,
                pages,
            })
        })
        .collect()
}

struct AuditAppend<'a> {
    idempotency_key: &'a str,
    action: &'a str,
    object_type: &'a str,
    object_id: &'a str,
    revision: i64,
}

fn append_audit(
    conn: &Connection,
    event: &AuditAppend<'_>,
    teacher: &str,
    now: &str,
) -> CoreResult<()> {
    audit::append(
        conn,
        &audit::NewAuditEvent {
            idempotency_key: event.idempotency_key,
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

fn resolve_issue(
    conn: &Connection,
    target_public_id: &str,
    issue_code: &str,
    teacher: &str,
    now: &str,
) -> CoreResult<()> {
    conn.execute(
        "UPDATE exam_pipeline_issues_v2
         SET state='resolved',resolved_by=?1,resolved_at=?2,
             resolution_note='teacher_group_quality_confirmation',updated_at=?2
         WHERE target_type='page' AND target_public_id=?3 AND issue_code=?4 AND state='open'",
        (teacher, now, target_public_id, issue_code),
    )?;
    Ok(())
}

fn open_quality_issue(
    conn: &Connection,
    target_public_id: &str,
    page_id: i64,
    now: &str,
) -> CoreResult<()> {
    let details = serde_json::json!({
        "schema_version": 1,
        "codes": ["TEACHER_RETAKE_REQUIRED"],
        "page_id": page_id
    })
    .to_string();
    conn.execute(
        "INSERT OR IGNORE INTO exam_pipeline_issues_v2
         (public_id,target_type,target_public_id,issue_code,severity,details_json,state,
          created_at,updated_at)
         VALUES (?1,'page',?2,'PAGE_QUALITY_GATE','blocking',?3,'open',?4,?4)",
        (ids::new_public_id(), target_public_id, details, now),
    )?;
    Ok(())
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

/// 老师看过联系表后一次提交：被点为“需重拍”的页组不建 attempt/page match，
/// 其他页组以老师确认来源一次性建立正式归属。
pub fn confirm_grouping_quality(
    conn: &Connection,
    input: &ConfirmGroupingQualityInput<'_>,
) -> CoreResult<GroupingActivation> {
    let teacher = input.confirmed_by.trim();
    if teacher.is_empty() {
        return Err(CoreError::Invalid("质量确认必须记录老师".into()));
    }
    let decision = ordered_intake::current_grouping_decision(conn, input.ingest_batch_id)?
        .ok_or_else(|| CoreError::Invalid("请先确认照片与学生顺序".into()))?;
    let assignments = parse_assignments(&decision.assignments_json)?;
    let all_page_ids = assignments
        .iter()
        .flat_map(|group| group.pages.iter().map(|page| page.page_id))
        .collect::<BTreeSet<_>>();
    let rejected = input
        .rejected_page_ids
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();
    if rejected.len() != input.rejected_page_ids.len()
        || rejected
            .iter()
            .any(|page_id| !all_page_ids.contains(page_id))
    {
        return Err(CoreError::Invalid(
            "需重拍页面不能重复，且必须属于当前老师确认的照片组".into(),
        ));
    }
    let snapshot = serde_json::json!({
        "schema_version": 1,
        "grouping_decision_id": decision.id,
        "grouping_snapshot_hash": decision.snapshot_hash,
        "rejected_page_ids": rejected.iter().collect::<Vec<_>>()
    });
    let snapshot_hash = hashing::sha256_hex(
        &serde_json::to_vec(&snapshot)
            .map_err(|error| CoreError::Parse(format!("质量确认快照失败：{error}")))?,
    );
    if let Some(existing) = current_activation(conn, input.ingest_batch_id)? {
        if existing.grouping_decision_id == decision.id && existing.snapshot_hash == snapshot_hash {
            return Ok(existing);
        }
        return Err(CoreError::Invalid(
            "本批已完成质量和正式归属确认；如需改学生或重拍，请走单点纠正流程".into(),
        ));
    }
    let assessment_version_id: i64 = conn.query_row(
        "SELECT assessment_version_id FROM exam_ingest_batches_v2 WHERE id=?1 AND state<>'voided'",
        [input.ingest_batch_id],
        |row| row.get(0),
    )?;

    let tx = conn.unchecked_transaction()?;
    let now = time::utc_now_rfc3339();
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_ordered_grouping_activations_v2
         WHERE ingest_batch_id=?1",
        [input.ingest_batch_id],
        |row| row.get(0),
    )?;
    let mut attempt_ids = BTreeMap::<i64, i64>::new();
    let mut match_ids = Vec::<i64>::new();
    let mut rejected_group_count = 0_i64;

    for group in &assignments {
        let group_rejected = group
            .pages
            .iter()
            .any(|page| rejected.contains(&page.page_id));
        if group_rejected {
            rejected_group_count += 1;
        }
        for page in &group.pages {
            let page_public_id: String = tx
                .query_row(
                    "SELECT public_id FROM exam_ingest_pages_v2
                     WHERE id=?1 AND batch_id=?2 AND state<>'voided'",
                    (page.page_id, input.ingest_batch_id),
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| {
                    CoreError::Invalid(format!("页组页面 {} 已失效或不属于当前批次", page.page_id))
                })?;
            let quality_revision: i64 = tx.query_row(
                "SELECT COALESCE(MAX(revision),0)+1 FROM exam_page_quality_revisions_v2
                 WHERE page_id=?1",
                [page.page_id],
                |row| row.get(0),
            )?;
            tx.execute(
                "UPDATE exam_page_quality_revisions_v2 SET state='superseded'
                 WHERE page_id=?1 AND state='active'",
                [page.page_id],
            )?;
            let rejected_page = rejected.contains(&page.page_id);
            let quality_public_id = ids::new_public_id();
            let issue_codes = if rejected_page {
                r#"{"schema_version":1,"codes":["TEACHER_RETAKE_REQUIRED"],"metrics_source":"teacher_overall_confirmation_v1"}"#
            } else {
                r#"{"schema_version":1,"codes":[],"metrics_source":"teacher_overall_confirmation_v1"}"#
            };
            tx.execute(
                "INSERT INTO exam_page_quality_revisions_v2
                 (public_id,page_id,revision,blur_score,glare_score,brightness_score,
                  perspective_score,rotation_degrees,crop_complete,result,issue_codes_json,
                  checked_by_type,checked_by,state,created_at)
                 VALUES (?1,?2,?3,?4,?4,0.5,?5,0.0,?6,?7,?8,'teacher',?9,'active',?10)",
                params![
                    &quality_public_id,
                    page.page_id,
                    quality_revision,
                    if rejected_page { 1.0 } else { 0.0 },
                    if rejected_page { 0.0 } else { 1.0 },
                    i64::from(!rejected_page),
                    if rejected_page { "reject" } else { "pass" },
                    issue_codes,
                    teacher,
                    &now,
                ],
            )?;
            resolve_issue(&tx, &page_public_id, "PAGE_QUALITY_GATE", teacher, &now)?;
            if rejected_page {
                open_quality_issue(&tx, &page_public_id, page.page_id, &now)?;
            }
            tx.execute(
                "UPDATE exam_ingest_pages_v2 SET state=?1,updated_at=?2 WHERE id=?3",
                (
                    if rejected_page {
                        "needs_review"
                    } else {
                        "quality_checked"
                    },
                    &now,
                    page.page_id,
                ),
            )?;
            append_audit(
                &tx,
                &AuditAppend {
                    idempotency_key: &format!("exam:page-quality:{quality_public_id}:active"),
                    action: "exam.page_quality.activated",
                    object_type: "exam_page_quality",
                    object_id: &quality_public_id,
                    revision: quality_revision,
                },
                teacher,
                &now,
            )?;
        }
        if group_rejected {
            continue;
        }

        let attempt_no = next_attempt_no(&tx, assessment_version_id, group.student_id)?;
        let attempt_public_id = ids::new_public_id();
        let attempt_kind = super::assessment::attempt_kind_for_assessment_version(
            &tx,
            assessment_version_id,
            attempt_no,
        )?;
        tx.execute(
            "INSERT INTO exam_attempts_v2
             (public_id,assessment_version_id,student_id,attempt_no,source_kind,
              attempt_kind,state,created_at,updated_at)
             VALUES (?1,?2,?3,?4,'image',?5,'grading',?6,?6)",
            (
                &attempt_public_id,
                assessment_version_id,
                group.student_id,
                attempt_no,
                attempt_kind,
                &now,
            ),
        )?;
        let attempt_id = tx.last_insert_rowid();
        attempt_ids.insert(group.student_id, attempt_id);
        for page in &group.pages {
            let match_revision: i64 = tx.query_row(
                "SELECT COALESCE(MAX(revision),0)+1 FROM exam_page_match_revisions_v2
                 WHERE page_id=?1",
                [page.page_id],
                |row| row.get(0),
            )?;
            tx.execute(
                "UPDATE exam_page_match_revisions_v2 SET state='superseded'
                 WHERE page_id=?1 AND state='active'",
                [page.page_id],
            )?;
            tx.execute(
                "UPDATE exam_answer_region_revisions_v2 SET state='superseded'
                 WHERE page_id=?1 AND state='active'",
                [page.page_id],
            )?;
            tx.execute(
                "UPDATE exam_page_alignment_revisions_v2 SET state='superseded'
                 WHERE page_id=?1 AND state='active'",
                [page.page_id],
            )?;
            let match_public_id = ids::new_public_id();
            tx.execute(
                "INSERT INTO exam_page_match_revisions_v2
                 (public_id,page_id,revision,attempt_id,page_no,student_confidence,
                  page_no_confidence,template_confidence,decision,reason_code,
                  confirmed_by,state,created_at)
                 VALUES (?1,?2,?3,?4,?5,1.0,1.0,NULL,'teacher_confirmed',
                         'ordered_grouping_teacher_confirmed',?6,'active',?7)",
                (
                    &match_public_id,
                    page.page_id,
                    match_revision,
                    attempt_id,
                    page.page_no,
                    teacher,
                    &now,
                ),
            )?;
            match_ids.push(tx.last_insert_rowid());
            let page_public_id: String = tx.query_row(
                "SELECT public_id FROM exam_ingest_pages_v2 WHERE id=?1",
                [page.page_id],
                |row| row.get(0),
            )?;
            resolve_issue(&tx, &page_public_id, "PAGE_MATCH_REVIEW", teacher, &now)?;
            tx.execute(
                "UPDATE exam_ingest_pages_v2 SET state='matched',updated_at=?1 WHERE id=?2",
                (&now, page.page_id),
            )?;
            append_audit(
                &tx,
                &AuditAppend {
                    idempotency_key: &format!("exam:page-match:{match_public_id}:active"),
                    action: "exam.page_match.activated",
                    object_type: "exam_page_match",
                    object_id: &match_public_id,
                    revision: match_revision,
                },
                teacher,
                &now,
            )?;
        }
    }

    let mapped_group_count = assignments.len() as i64 - rejected_group_count;
    tx.execute(
        "UPDATE exam_ingest_batches_v2 SET state=?1,updated_at=?2 WHERE id=?3",
        (
            if rejected_group_count > 0 {
                "needs_review"
            } else {
                "ready"
            },
            &now,
            input.ingest_batch_id,
        ),
    )?;
    let public_id = ids::new_public_id();
    let rejected_page_ids_json = serde_json::json!({
        "schema_version": 1,
        "page_ids": rejected.iter().collect::<Vec<_>>()
    })
    .to_string();
    let attempt_ids_json = serde_json::json!({
        "schema_version": 1,
        "by_student_id": attempt_ids
    })
    .to_string();
    let page_match_ids_json = serde_json::json!({
        "schema_version": 1,
        "ids": match_ids
    })
    .to_string();
    tx.execute(
        "INSERT INTO exam_ordered_grouping_activations_v2
         (public_id,ingest_batch_id,grouping_decision_id,revision,snapshot_hash,
          rejected_page_ids_json,attempt_ids_json,page_match_ids_json,mapped_group_count,
          rejected_group_count,state,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11,?12)",
        params![
            &public_id,
            input.ingest_batch_id,
            decision.id,
            revision,
            &snapshot_hash,
            &rejected_page_ids_json,
            &attempt_ids_json,
            &page_match_ids_json,
            mapped_group_count,
            rejected_group_count,
            teacher,
            &now,
        ],
    )?;
    let id = tx.last_insert_rowid();
    append_audit(
        &tx,
        &AuditAppend {
            idempotency_key: &format!("exam:ordered-grouping-activation:{public_id}:active"),
            action: "exam.ordered_grouping.quality_confirmed",
            object_type: "exam_ordered_grouping_activation",
            object_id: &public_id,
            revision,
        },
        teacher,
        &now,
    )?;
    tx.commit()?;
    conn.query_row(
        "SELECT id,public_id,ingest_batch_id,grouping_decision_id,revision,snapshot_hash,
                rejected_page_ids_json,attempt_ids_json,page_match_ids_json,
                mapped_group_count,rejected_group_count,state,confirmed_by
         FROM exam_ordered_grouping_activations_v2 WHERE id=?1",
        [id],
        activation_row,
    )
    .map_err(Into::into)
}
