//! M6.1-4 老师确认的班级教学行动草稿。
//!
//! 预览只读取不可变班级快照；确认后冻结来源节点、目标学生和 K1 内容版本。
//! 本模块不直接修改成绩、学习证据、掌握快照或背诵任务。练习草稿只有在
//! 老师再次显式确认 materialization 时，才由外层编排调用 M2 作业事务。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use crate::class_profile::{self, ClassProfileNodeMetric};
use crate::profile::ProfileStudent;

pub const CLASS_ACTION_SCHEMA_VERSION: i64 = 1;
pub const CLASS_ACTION_RULE_VERSION: &str = "m6.1-teacher-confirmed-action-drafts-v1";
pub const CLASS_ACTION_PRACTICE_TEMPLATE_VERSION: &str = "m6.1-class-action-practice-v1";
const ACTION_KINDS: &[&str] = &["reteach", "practice", "recitation", "temporary_group"];
const MAX_TARGETS: usize = 200;
const MAX_CANDIDATES: usize = 10;

#[derive(Debug, Clone)]
pub struct PreviewClassActionInput<'a> {
    pub snapshot_public_id: &'a str,
    pub node_metric_public_id: &'a str,
    pub action_kind: &'a str,
}

#[derive(Debug, Clone)]
pub struct ConfirmClassActionInput<'a> {
    pub request_key: &'a str,
    pub snapshot_public_id: &'a str,
    pub node_metric_public_id: &'a str,
    pub action_kind: &'a str,
    pub expected_snapshot_payload_sha256: &'a str,
    pub title: &'a str,
    pub rationale: &'a str,
    pub estimated_minutes: i64,
    pub target_student_ids: &'a [i64],
    pub candidate_question_version_public_ids: &'a [String],
    pub confirmed_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassActionTarget {
    pub student: ProfileStudent,
    pub source_status: String,
    pub recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassActionCandidate {
    pub question_version_public_id: String,
    pub answer_key_version_public_id: String,
    pub rubric_version_public_id: String,
    pub link_set_public_id: String,
    pub title: String,
    pub question_type: String,
    pub max_score: f64,
    pub active_assignment_count: i64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassActionPreview {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub class_id: i64,
    pub class_name: String,
    pub snapshot_public_id: String,
    pub snapshot_revision: i64,
    pub snapshot_payload_sha256: String,
    pub snapshot_source_watermark: String,
    pub node_metric_public_id: String,
    pub target_type: String,
    pub target_public_id: String,
    pub target_title: String,
    pub action_kind: String,
    pub suggested_title: String,
    pub suggested_rationale: String,
    pub suggested_estimated_minutes: i64,
    pub destination_module: String,
    pub destination_view: String,
    pub targets: Vec<ClassActionTarget>,
    pub candidates: Vec<ClassActionCandidate>,
    pub can_confirm: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassActionMaterialization {
    pub public_id: String,
    pub destination_type: String,
    pub destination_public_id: String,
    pub destination_version_public_id: String,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassActionDraft {
    pub public_id: String,
    pub class_id: i64,
    pub class_name: String,
    pub snapshot_public_id: String,
    pub snapshot_revision: i64,
    pub node_metric_public_id: String,
    pub target_type: String,
    pub target_public_id: String,
    pub target_title: String,
    pub action_kind: String,
    pub title: String,
    pub rationale: String,
    pub estimated_minutes: i64,
    pub destination_module: String,
    pub destination_view: String,
    pub snapshot_payload_sha256: String,
    pub payload_sha256: String,
    pub state: String,
    pub confirmed_by: String,
    pub confirmed_at: String,
    pub targets: Vec<ClassActionTarget>,
    pub candidates: Vec<ClassActionCandidate>,
    pub materialization: Option<ClassActionMaterialization>,
}

#[derive(Debug, Clone)]
pub struct PracticeMaterializationPlan {
    pub draft_id: i64,
    pub draft_public_id: String,
    pub class_id: i64,
    pub title: String,
    pub target_student_ids: Vec<i64>,
    pub candidates: Vec<ClassActionCandidate>,
}

#[derive(Debug, Clone)]
pub struct RecordPracticeMaterializationInput<'a> {
    pub request_key: &'a str,
    pub draft_id: i64,
    pub draft_public_id: &'a str,
    pub destination_public_id: &'a str,
    pub destination_version_public_id: &'a str,
    pub created_by: &'a str,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn validate_action_kind(value: &str) -> CoreResult<()> {
    if ACTION_KINDS.contains(&value) {
        Ok(())
    } else {
        Err(CoreError::Invalid("不支持的教学行动类型".into()))
    }
}

fn action_defaults(
    action_kind: &str,
    target_title: &str,
) -> (&'static str, String, i64, &'static str, &'static str) {
    match action_kind {
        "reteach" => (
            "针对性再讲解",
            format!("围绕“{target_title}”安排一次短讲与当堂核对"),
            15,
            "class_dashboard",
            "dashboard",
        ),
        "practice" => (
            "针对性题目练习",
            format!("使用已确认链接到“{target_title}”的题目形成一次练习"),
            15,
            "exam",
            "exam",
        ),
        "recitation" => (
            "背诵巩固",
            format!("围绕“{target_title}”安排背诵巩固"),
            10,
            "recitation",
            "today",
        ),
        "temporary_group" => (
            "临时支持小组",
            format!("按当前快照为“{target_title}”建立一次临时支持小组"),
            15,
            "class_dashboard",
            "dashboard",
        ),
        _ => unreachable!("validated action kind"),
    }
}

fn find_node<'a>(
    snapshot: &'a class_profile::ClassProfileSnapshot,
    public_id: &str,
) -> Option<&'a ClassProfileNodeMetric> {
    snapshot
        .knowledge_metrics
        .iter()
        .chain(snapshot.ability_metrics.iter())
        .find(|node| node.public_id == public_id)
}

fn action_targets(node: &ClassProfileNodeMetric) -> Vec<ClassActionTarget> {
    node.cells
        .iter()
        .filter(|cell| {
            matches!(
                cell.status.as_str(),
                "needs_support" | "developing" | "stable"
            )
        })
        .map(|cell| ClassActionTarget {
            student: cell.student.clone(),
            source_status: cell.status.clone(),
            recommended: cell.status == "needs_support",
        })
        .collect()
}

fn candidate_row(row: &Row<'_>, target_type: &str) -> rusqlite::Result<ClassActionCandidate> {
    Ok(ClassActionCandidate {
        question_version_public_id: row.get(0)?,
        answer_key_version_public_id: row.get(1)?,
        rubric_version_public_id: row.get(2)?,
        link_set_public_id: row.get(3)?,
        title: row.get(4)?,
        question_type: row.get(5)?,
        max_score: row.get(6)?,
        active_assignment_count: row.get(7)?,
        reason: match target_type {
            "knowledge_node" => "题目版本已由老师确认直接考查或作为评分点依据。".into(),
            "ability_dimension" => "题目版本已由老师确认提供该能力维度证据。".into(),
            _ => "老师确认行动时冻结的题目、答案、评分点与知识链接版本。".into(),
        },
    })
}

fn question_candidates(
    conn: &Connection,
    class_id: i64,
    target_type: &str,
    target_public_id: &str,
) -> CoreResult<Vec<ClassActionCandidate>> {
    let link_join = if target_type == "knowledge_node" {
        "JOIN k1_knowledge_links target_link
           ON target_link.link_set_id=links.id
          AND target_link.confirmation_level='teacher_confirmed'
          AND target_link.relation_type IN ('direct_assessment','rubric_basis')
         JOIN k1_knowledge_nodes target
           ON target.id=target_link.knowledge_node_id
          AND target.public_id=?2"
    } else {
        "JOIN k1_ability_links target_link
           ON target_link.link_set_id=links.id
          AND target_link.confirmation_level='teacher_confirmed'
          AND target_link.evidence_strength>=0.5
         JOIN k1_ability_dimensions target
           ON target.id=target_link.ability_dimension_id
          AND target.public_id=?2"
    };
    let sql = format!(
        "SELECT question.public_id,answer.public_id,rubric.public_id,links.public_id,
                question.stem,question.question_type,question.max_score,
                (SELECT COUNT(DISTINCT assigned_version.id)
                 FROM exam_assessment_items_v2 assigned_item
                 JOIN exam_assessment_versions_v2 assigned_version
                   ON assigned_version.id=assigned_item.assessment_version_id
                  AND assigned_version.state='confirmed'
                 JOIN exam_assessments_v2 assigned
                   ON assigned.id=assigned_version.assessment_id
                  AND assigned.state='active'
                 WHERE assigned.class_id=?1
                   AND assigned_item.question_version_id=question.id
                   AND assigned_item.state='active') AS active_assignment_count
         FROM k1_question_versions question
         JOIN k1_answer_key_versions answer
           ON answer.question_version_id=question.id
          AND answer.state='confirmed'
          AND answer.revision=(
            SELECT MAX(latest_answer.revision)
            FROM k1_answer_key_versions latest_answer
            WHERE latest_answer.question_version_id=question.id
              AND latest_answer.state='confirmed')
         JOIN k1_rubric_versions rubric
           ON rubric.question_version_id=question.id
          AND rubric.state='confirmed'
          AND rubric.revision=(
            SELECT MAX(latest_rubric.revision)
            FROM k1_rubric_versions latest_rubric
            WHERE latest_rubric.question_version_id=question.id
              AND latest_rubric.state='confirmed')
         JOIN k1_link_sets links
           ON links.question_version_id=question.id
          AND links.state='confirmed'
          AND links.revision=(
            SELECT MAX(latest_links.revision)
            FROM k1_link_sets latest_links
            WHERE latest_links.question_version_id=question.id
              AND latest_links.state='confirmed')
         {link_join}
         WHERE question.state='published'
           AND question.quality_level IN ('L2','L3','L4')
           AND question.revision=(
             SELECT MAX(latest_question.revision)
             FROM k1_question_versions latest_question
             WHERE latest_question.question_id=question.question_id
               AND latest_question.state='published')
         GROUP BY question.id,answer.id,rubric.id,links.id
         ORDER BY active_assignment_count,question.question_type,question.stem,question.public_id
         LIMIT 8"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![class_id, target_public_id], |row| {
        candidate_row(row, target_type)
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn preview_class_action(
    conn: &Connection,
    input: &PreviewClassActionInput<'_>,
) -> CoreResult<ClassActionPreview> {
    required(input.snapshot_public_id, "班级快照")?;
    required(input.node_metric_public_id, "掌握节点")?;
    validate_action_kind(input.action_kind)?;
    let snapshot = class_profile::get_class_profile(conn, input.snapshot_public_id)?
        .ok_or_else(|| CoreError::Invalid("班级掌握快照不存在".into()))?;
    let node = find_node(&snapshot, input.node_metric_public_id)
        .cloned()
        .ok_or_else(|| CoreError::Invalid("掌握节点不属于该班级快照".into()))?;
    let targets = action_targets(&node);
    let candidates = if input.action_kind == "practice" {
        question_candidates(
            conn,
            snapshot.class.id,
            &node.target_type,
            &node.target_public_id,
        )?
    } else {
        Vec::new()
    };
    let (action_label, rationale, estimated_minutes, destination_module, destination_view) =
        action_defaults(input.action_kind, &node.target_title);
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    if snapshot.is_stale {
        blockers.push(
            snapshot
                .stale_reason
                .clone()
                .unwrap_or_else(|| "来源班级快照已有新输入，请先重新生成。".into()),
        );
    }
    if !node.sample_sufficient {
        blockers.push("该节点班级样本不足，不能据此生成教学行动。".into());
    }
    if !targets.iter().any(|target| target.recommended) {
        blockers.push("该节点没有“需要支持”的目标学生。".into());
    }
    if input.action_kind == "temporary_group"
        && targets.iter().filter(|target| target.recommended).count() < 2
    {
        blockers.push("临时小组至少需要 2 名当前快照中“需要支持”的学生。".into());
    }
    if input.action_kind == "practice" && candidates.is_empty() {
        blockers.push("题库中没有已发布 L2+ 且由老师确认链接到该节点的题目。".into());
    }
    if input.action_kind == "recitation" {
        blockers.push(
            "背诵内容尚未建立到 K1 知识节点的可靠版本映射；本阶段不按标题相似度猜测内容。".into(),
        );
    }
    if candidates
        .iter()
        .any(|candidate| candidate.active_assignment_count > 0)
    {
        warnings.push("部分候选题已出现在当前进行中作业，请老师确认是否重复使用。".into());
    }
    Ok(ClassActionPreview {
        schema_version: CLASS_ACTION_SCHEMA_VERSION,
        rule_version: CLASS_ACTION_RULE_VERSION.into(),
        calculated_at: time::utc_now_rfc3339(),
        class_id: snapshot.class.id,
        class_name: snapshot.class.name,
        snapshot_public_id: snapshot.public_id,
        snapshot_revision: snapshot.revision,
        snapshot_payload_sha256: snapshot.payload_sha256,
        snapshot_source_watermark: snapshot.source_watermark,
        node_metric_public_id: node.public_id.clone(),
        target_type: node.target_type.clone(),
        target_public_id: node.target_public_id.clone(),
        target_title: node.target_title.clone(),
        action_kind: input.action_kind.into(),
        suggested_title: format!("{action_label}：{}", node.target_title),
        suggested_rationale: rationale,
        suggested_estimated_minutes: estimated_minutes,
        destination_module: destination_module.into(),
        destination_view: destination_view.into(),
        targets,
        candidates,
        can_confirm: blockers.is_empty(),
        blockers,
        warnings,
        boundary_note:
            "行动草稿不会修改成绩、掌握快照、学生标签或背诵排程；题目练习只有老师明确确认后才建立作业。"
                .into(),
    })
}

fn canonical_ids(values: &[i64], label: &str) -> CoreResult<Vec<i64>> {
    let mut unique = BTreeSet::new();
    for value in values {
        if *value <= 0 || !unique.insert(*value) {
            return Err(CoreError::Invalid(format!("{label}包含非法或重复 ID")));
        }
    }
    Ok(unique.into_iter().collect())
}

fn canonical_public_ids(values: &[String], label: &str) -> CoreResult<Vec<String>> {
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::with_capacity(values.len());
    for value in values {
        required(value, label)?;
        let normalized = value.trim().to_string();
        if !seen.insert(normalized.clone()) {
            return Err(CoreError::Invalid(format!("{label}不能重复")));
        }
        ordered.push(normalized);
    }
    Ok(ordered)
}

#[derive(Serialize)]
struct DraftHashInput<'a> {
    schema_version: i64,
    snapshot_public_id: &'a str,
    snapshot_payload_sha256: &'a str,
    node_metric_public_id: &'a str,
    action_kind: &'a str,
    title: &'a str,
    rationale: &'a str,
    estimated_minutes: i64,
    target_student_ids: &'a [i64],
    candidate_question_version_public_ids: &'a [String],
}

fn draft_hash(input: &DraftHashInput<'_>) -> CoreResult<String> {
    let bytes = serde_json::to_vec(input).map_err(|error| CoreError::Invalid(error.to_string()))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn draft_id_by_public_id(conn: &Connection, public_id: &str) -> CoreResult<Option<i64>> {
    Ok(conn
        .query_row(
            "SELECT id FROM class_action_drafts WHERE public_id=?1",
            [public_id],
            |row| row.get(0),
        )
        .optional()?)
}

fn load_targets(conn: &Connection, draft_id: i64) -> CoreResult<Vec<ClassActionTarget>> {
    let mut stmt = conn.prepare(
        "SELECT student.id,student.class_id,student.student_no,student.name,
                target.source_cell_status
         FROM class_action_draft_targets target
         JOIN students student ON student.id=target.student_id
         WHERE target.draft_id=?1
         ORDER BY
           CASE WHEN student.student_no GLOB '[0-9]*' THEN 0 ELSE 1 END,
           CAST(student.student_no AS INTEGER),student.student_no,student.id",
    )?;
    let rows = stmt.query_map([draft_id], |row| {
        let status: String = row.get(4)?;
        Ok(ClassActionTarget {
            student: ProfileStudent {
                id: row.get(0)?,
                class_id: row.get(1)?,
                student_no: row.get(2)?,
                name: row.get(3)?,
            },
            recommended: status == "needs_support",
            source_status: status,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_candidates(conn: &Connection, draft_id: i64) -> CoreResult<Vec<ClassActionCandidate>> {
    let mut stmt = conn.prepare(
        "SELECT question_version_public_id,answer_key_version_public_id,
                rubric_version_public_id,link_set_public_id,title,question_type,max_score,
                active_assignment_count
         FROM class_action_draft_candidates
         WHERE draft_id=?1
         ORDER BY order_index,question_version_public_id",
    )?;
    let rows = stmt.query_map([draft_id], |row| candidate_row(row, "stored"))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_materialization(
    conn: &Connection,
    draft_id: i64,
) -> CoreResult<Option<ClassActionMaterialization>> {
    Ok(conn
        .query_row(
            "SELECT public_id,destination_type,destination_public_id,
                    destination_version_public_id,created_by,created_at
             FROM class_action_materializations WHERE draft_id=?1",
            [draft_id],
            |row| {
                Ok(ClassActionMaterialization {
                    public_id: row.get(0)?,
                    destination_type: row.get(1)?,
                    destination_public_id: row.get(2)?,
                    destination_version_public_id: row.get(3)?,
                    created_by: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        )
        .optional()?)
}

fn load_draft_by_id(conn: &Connection, draft_id: i64) -> CoreResult<ClassActionDraft> {
    let base = conn
        .query_row(
            "SELECT draft.public_id,draft.class_id,class.name,snapshot.public_id,
                    snapshot.revision,metric.public_id,metric.target_type,
                    metric.target_public_id,metric.target_title,draft.action_kind,draft.title,
                    draft.rationale,draft.estimated_minutes,draft.destination_module,
                    draft.destination_view,draft.snapshot_payload_sha256,draft.payload_sha256,
                    draft.state,draft.confirmed_by,draft.confirmed_at
             FROM class_action_drafts draft
             JOIN classes class ON class.id=draft.class_id
             JOIN class_profile_snapshots snapshot ON snapshot.id=draft.class_profile_snapshot_id
             JOIN class_profile_node_metrics metric ON metric.id=draft.class_profile_node_metric_id
             WHERE draft.id=?1",
            [draft_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, String>(15)?,
                    row.get::<_, String>(16)?,
                    row.get::<_, String>(17)?,
                    row.get::<_, String>(18)?,
                    row.get::<_, String>(19)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("教学行动草稿不存在".into()))?;
    Ok(ClassActionDraft {
        public_id: base.0,
        class_id: base.1,
        class_name: base.2,
        snapshot_public_id: base.3,
        snapshot_revision: base.4,
        node_metric_public_id: base.5,
        target_type: base.6,
        target_public_id: base.7,
        target_title: base.8,
        action_kind: base.9,
        title: base.10,
        rationale: base.11,
        estimated_minutes: base.12,
        destination_module: base.13,
        destination_view: base.14,
        snapshot_payload_sha256: base.15,
        payload_sha256: base.16,
        state: base.17,
        confirmed_by: base.18,
        confirmed_at: base.19,
        targets: load_targets(conn, draft_id)?,
        candidates: load_candidates(conn, draft_id)?,
        materialization: load_materialization(conn, draft_id)?,
    })
}

pub fn confirm_class_action(
    conn: &mut Connection,
    input: &ConfirmClassActionInput<'_>,
) -> CoreResult<ClassActionDraft> {
    required(input.request_key, "请求标识")?;
    required(input.confirmed_by, "确认人")?;
    required(input.title, "行动标题")?;
    required(input.rationale, "行动依据")?;
    if input.title.chars().count() > 100 {
        return Err(CoreError::Invalid("行动标题不能超过 100 个字符".into()));
    }
    if input.rationale.chars().count() > 1000 {
        return Err(CoreError::Invalid("行动依据不能超过 1000 个字符".into()));
    }
    if !(1..=240).contains(&input.estimated_minutes) {
        return Err(CoreError::Invalid(
            "预计时长必须在 1 至 240 分钟之间".into(),
        ));
    }
    let target_ids = canonical_ids(input.target_student_ids, "目标学生")?;
    let candidate_ids =
        canonical_public_ids(input.candidate_question_version_public_ids, "候选题目版本")?;
    if target_ids.is_empty() || target_ids.len() > MAX_TARGETS {
        return Err(CoreError::Invalid(
            "目标学生数量必须在 1 至 200 人之间".into(),
        ));
    }
    if candidate_ids.len() > MAX_CANDIDATES {
        return Err(CoreError::Invalid("单个行动草稿最多选择 10 道题".into()));
    }
    let preview = preview_class_action(
        conn,
        &PreviewClassActionInput {
            snapshot_public_id: input.snapshot_public_id,
            node_metric_public_id: input.node_metric_public_id,
            action_kind: input.action_kind,
        },
    )?;
    if !preview.can_confirm {
        return Err(CoreError::Invalid(preview.blockers.join("；")));
    }
    if preview.snapshot_payload_sha256 != input.expected_snapshot_payload_sha256 {
        return Err(CoreError::Invalid(
            "班级快照内容与预览时不一致，请刷新后重试".into(),
        ));
    }
    let target_by_id: BTreeMap<_, _> = preview
        .targets
        .iter()
        .map(|target| (target.student.id, target))
        .collect();
    if target_ids.iter().any(|id| !target_by_id.contains_key(id)) {
        return Err(CoreError::Invalid(
            "目标学生不属于当前快照该节点的可选范围".into(),
        ));
    }
    if input.action_kind == "temporary_group" && target_ids.len() < 2 {
        return Err(CoreError::Invalid("临时小组至少需要 2 名学生".into()));
    }
    let candidate_by_id: BTreeMap<_, _> = preview
        .candidates
        .iter()
        .map(|candidate| (candidate.question_version_public_id.as_str(), candidate))
        .collect();
    if input.action_kind == "practice" {
        if candidate_ids.is_empty() {
            return Err(CoreError::Invalid("题目练习至少选择 1 道题".into()));
        }
        if candidate_ids
            .iter()
            .any(|id| !candidate_by_id.contains_key(id.as_str()))
        {
            return Err(CoreError::Invalid(
                "候选题目版本已变化，请刷新后重新选择".into(),
            ));
        }
    } else if !candidate_ids.is_empty() {
        return Err(CoreError::Invalid(
            "只有题目练习行动可以冻结题目候选".into(),
        ));
    }
    let hash_input = DraftHashInput {
        schema_version: CLASS_ACTION_SCHEMA_VERSION,
        snapshot_public_id: &preview.snapshot_public_id,
        snapshot_payload_sha256: &preview.snapshot_payload_sha256,
        node_metric_public_id: &preview.node_metric_public_id,
        action_kind: input.action_kind,
        title: input.title.trim(),
        rationale: input.rationale.trim(),
        estimated_minutes: input.estimated_minutes,
        target_student_ids: &target_ids,
        candidate_question_version_public_ids: &candidate_ids,
    };
    let payload_sha256 = draft_hash(&hash_input)?;
    let existing = conn
        .query_row(
            "SELECT id,payload_sha256 FROM class_action_drafts WHERE request_key=?1",
            [input.request_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    if let Some((draft_id, stored_hash)) = existing {
        if stored_hash != payload_sha256 {
            return Err(CoreError::Invalid(
                "同一请求标识已用于不同教学行动内容".into(),
            ));
        }
        return load_draft_by_id(conn, draft_id);
    }
    let (snapshot_id, node_metric_id): (i64, i64) = conn.query_row(
        "SELECT snapshot.id,metric.id
         FROM class_profile_snapshots snapshot
         JOIN class_profile_node_metrics metric
           ON metric.snapshot_id=snapshot.id AND metric.public_id=?2
         WHERE snapshot.public_id=?1",
        params![preview.snapshot_public_id, preview.node_metric_public_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let public_id = ids::new_public_id();
    let confirmed_at = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO class_action_drafts
          (public_id,request_key,class_profile_snapshot_id,class_profile_node_metric_id,
           class_id,action_kind,title,rationale,estimated_minutes,destination_module,
           destination_view,target_count,candidate_count,snapshot_payload_sha256,
           payload_sha256,state,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,
                 'teacher_confirmed',?16,?17)",
        params![
            public_id,
            input.request_key.trim(),
            snapshot_id,
            node_metric_id,
            preview.class_id,
            input.action_kind,
            input.title.trim(),
            input.rationale.trim(),
            input.estimated_minutes,
            preview.destination_module,
            preview.destination_view,
            target_ids.len() as i64,
            candidate_ids.len() as i64,
            preview.snapshot_payload_sha256,
            payload_sha256,
            input.confirmed_by.trim(),
            confirmed_at,
        ],
    )?;
    let draft_id = tx.last_insert_rowid();
    for student_id in &target_ids {
        let target = target_by_id
            .get(student_id)
            .expect("target was checked above");
        tx.execute(
            "INSERT INTO class_action_draft_targets
              (draft_id,student_id,source_cell_status) VALUES (?1,?2,?3)",
            params![draft_id, student_id, target.source_status],
        )?;
    }
    for (order_index, question_public_id) in candidate_ids.iter().enumerate() {
        let candidate = candidate_by_id
            .get(question_public_id.as_str())
            .expect("candidate was checked above");
        tx.execute(
            "INSERT INTO class_action_draft_candidates
              (draft_id,order_index,candidate_type,question_version_public_id,
               answer_key_version_public_id,rubric_version_public_id,link_set_public_id,
               title,question_type,max_score,active_assignment_count)
             VALUES (?1,?2,'question_version',?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                draft_id,
                order_index as i64,
                candidate.question_version_public_id,
                candidate.answer_key_version_public_id,
                candidate.rubric_version_public_id,
                candidate.link_set_public_id,
                candidate.title,
                candidate.question_type,
                candidate.max_score,
                candidate.active_assignment_count,
            ],
        )?;
    }
    let event_payload = serde_json::json!({
        "schema_version": CLASS_ACTION_SCHEMA_VERSION,
        "draft_public_id": public_id,
        "class_profile_snapshot_public_id": preview.snapshot_public_id,
        "node_metric_public_id": preview.node_metric_public_id,
        "action_kind": input.action_kind,
        "target_count": target_ids.len(),
        "candidate_count": candidate_ids.len(),
        "task_created": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("profile:outbox:class-action:{public_id}"),
            event_type: "class_action_draft_confirmed",
            event_version: 1,
            aggregate_type: "class_action_draft",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &confirmed_at,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("profile:audit:class-action:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.confirmed_by.trim()),
            action: "profile.class_action.confirmed",
            object_type: "class_action_draft",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("行动草稿已由老师确认；尚未自动创建成绩、证据或任务"),
            meta_json: Some(&event_payload),
            occurred_at: &confirmed_at,
        },
    )?;
    tx.commit()?;
    load_draft_by_id(conn, draft_id)
}

pub fn list_class_action_drafts(
    conn: &Connection,
    class_id: i64,
    limit: i64,
) -> CoreResult<Vec<ClassActionDraft>> {
    if !(1..=100).contains(&limit) {
        return Err(CoreError::Invalid(
            "行动草稿读取数量必须在 1 至 100 之间".into(),
        ));
    }
    let mut stmt = conn.prepare(
        "SELECT id FROM class_action_drafts
         WHERE class_id=?1 ORDER BY confirmed_at DESC,id DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![class_id, limit], |row| row.get::<_, i64>(0))?;
    rows.map(|row| load_draft_by_id(conn, row?)).collect()
}

pub fn get_class_action_draft(
    conn: &Connection,
    public_id: &str,
) -> CoreResult<Option<ClassActionDraft>> {
    required(public_id, "行动草稿")?;
    draft_id_by_public_id(conn, public_id)?
        .map(|draft_id| load_draft_by_id(conn, draft_id))
        .transpose()
}

pub fn prepare_practice_materialization(
    conn: &Connection,
    draft_public_id: &str,
) -> CoreResult<PracticeMaterializationPlan> {
    required(draft_public_id, "行动草稿")?;
    let draft_id = draft_id_by_public_id(conn, draft_public_id)?
        .ok_or_else(|| CoreError::Invalid("教学行动草稿不存在".into()))?;
    let draft = load_draft_by_id(conn, draft_id)?;
    if draft.action_kind != "practice" {
        return Err(CoreError::Invalid(
            "只有题目练习草稿可以建立 M2 作业".into(),
        ));
    }
    if draft.materialization.is_some() {
        return Err(CoreError::Invalid("该题目练习已经建立作业".into()));
    }
    let snapshot = class_profile::get_class_profile(conn, &draft.snapshot_public_id)?
        .ok_or_else(|| CoreError::Invalid("来源班级快照不存在".into()))?;
    if snapshot.is_stale {
        return Err(CoreError::Invalid(snapshot.stale_reason.unwrap_or_else(
            || "来源班级快照已有新输入，请重新生成行动草稿。".into(),
        )));
    }
    if snapshot.payload_sha256 != draft.snapshot_payload_sha256 {
        return Err(CoreError::Invalid("来源班级快照校验失败".into()));
    }
    if draft.candidates.is_empty() || draft.targets.is_empty() {
        return Err(CoreError::Invalid("题目练习缺少冻结题目或目标学生".into()));
    }
    Ok(PracticeMaterializationPlan {
        draft_id,
        draft_public_id: draft.public_id,
        class_id: draft.class_id,
        title: draft.title,
        target_student_ids: draft
            .targets
            .into_iter()
            .map(|target| target.student.id)
            .collect(),
        candidates: draft.candidates,
    })
}

fn materialization_hash(
    draft_public_id: &str,
    destination_public_id: &str,
    destination_version_public_id: &str,
) -> CoreResult<String> {
    let value = serde_json::json!({
        "schema_version": 1,
        "draft_public_id": draft_public_id,
        "destination_type": "exam_assessment",
        "destination_public_id": destination_public_id,
        "destination_version_public_id": destination_version_public_id
    });
    let bytes =
        serde_json::to_vec(&value).map_err(|error| CoreError::Invalid(error.to_string()))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn validate_materialization_destination(
    conn: &Connection,
    input: &RecordPracticeMaterializationInput<'_>,
) -> CoreResult<()> {
    let destination_scope: Option<(i64, i64)> = conn
        .query_row(
            "SELECT assessment.class_id,version.id
             FROM exam_assessments_v2 assessment
             JOIN exam_assessment_versions_v2 version
              ON version.assessment_id=assessment.id
              AND version.public_id=?2
              AND version.state='confirmed'
              AND version.template_version=?3
             WHERE assessment.public_id=?1
               AND assessment.state='active'
               AND assessment.audience_kind='explicit'",
            params![
                input.destination_public_id,
                input.destination_version_public_id,
                CLASS_ACTION_PRACTICE_TEMPLATE_VERSION
            ],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((destination_class_id, destination_version_id)) = destination_scope else {
        return Err(CoreError::Invalid(
            "目标作业不是已确认的 M6.1 显式定向练习".into(),
        ));
    };
    let draft_scope: (i64, String) = conn.query_row(
        "SELECT class_id,action_kind FROM class_action_drafts
         WHERE id=?1 AND public_id=?2 AND state='teacher_confirmed'",
        params![input.draft_id, input.draft_public_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if destination_class_id != draft_scope.0 || draft_scope.1 != "practice" {
        return Err(CoreError::Invalid(
            "目标作业与教学行动的班级或类型不一致".into(),
        ));
    }
    let selected_targets: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT student_id FROM class_action_draft_targets
             WHERE draft_id=?1 ORDER BY student_id",
        )?;
        let rows = stmt.query_map([input.draft_id], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let destination_targets: Vec<i64> = {
        let mut stmt = conn.prepare(
            "SELECT student_id FROM exam_assessment_targets_v2
             WHERE assessment_version_id=?1 ORDER BY student_id",
        )?;
        let rows = stmt.query_map([destination_version_id], |row| row.get(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if selected_targets != destination_targets {
        return Err(CoreError::Invalid(
            "目标作业学生范围与老师确认的行动草稿不一致".into(),
        ));
    }
    let selected_candidates: Vec<(String, String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT question_version_public_id,answer_key_version_public_id,
                    rubric_version_public_id,link_set_public_id
             FROM class_action_draft_candidates
             WHERE draft_id=?1 ORDER BY order_index,question_version_public_id",
        )?;
        let rows = stmt.query_map([input.draft_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let destination_candidates: Vec<(String, String, String, String)> = {
        let mut stmt = conn.prepare(
            "SELECT question.public_id,answer.public_id,rubric.public_id,links.public_id
             FROM exam_assessment_items_v2 item
             JOIN k1_question_versions question ON question.id=item.question_version_id
             JOIN k1_answer_key_versions answer ON answer.id=item.answer_key_version_id
             JOIN k1_rubric_versions rubric ON rubric.id=item.rubric_version_id
             JOIN k1_link_sets links ON links.id=item.link_set_id
             WHERE item.assessment_version_id=?1 AND item.state='active'
             ORDER BY item.order_index,item.id",
        )?;
        let rows = stmt.query_map([destination_version_id], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if selected_candidates != destination_candidates {
        return Err(CoreError::Invalid(
            "目标作业题目版本集合与老师确认的行动草稿不一致".into(),
        ));
    }
    Ok(())
}

pub fn record_practice_materialization_in_transaction(
    conn: &Connection,
    input: &RecordPracticeMaterializationInput<'_>,
) -> CoreResult<ClassActionMaterialization> {
    required(input.request_key, "请求标识")?;
    required(input.draft_public_id, "行动草稿")?;
    required(input.destination_public_id, "目标作业")?;
    required(input.destination_version_public_id, "目标作业版本")?;
    required(input.created_by, "操作人")?;
    validate_materialization_destination(conn, input)?;
    let digest = materialization_hash(
        input.draft_public_id,
        input.destination_public_id,
        input.destination_version_public_id,
    )?;
    let by_request = conn
        .query_row(
            "SELECT public_id,destination_type,destination_public_id,
                    destination_version_public_id,created_by,created_at,payload_sha256,draft_id
             FROM class_action_materializations WHERE request_key=?1",
            [input.request_key.trim()],
            |row| {
                Ok((
                    ClassActionMaterialization {
                        public_id: row.get(0)?,
                        destination_type: row.get(1)?,
                        destination_public_id: row.get(2)?,
                        destination_version_public_id: row.get(3)?,
                        created_by: row.get(4)?,
                        created_at: row.get(5)?,
                    },
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )
        .optional()?;
    if let Some((existing, stored_hash, draft_id)) = by_request {
        if stored_hash != digest || draft_id != input.draft_id {
            return Err(CoreError::Invalid(
                "同一请求标识已用于不同作业建立操作".into(),
            ));
        }
        return Ok(existing);
    }
    if let Some(existing) = load_materialization(conn, input.draft_id)? {
        return Ok(existing);
    }
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO class_action_materializations
          (public_id,request_key,draft_id,destination_type,destination_public_id,
           destination_version_public_id,payload_sha256,created_by,created_at)
         VALUES (?1,?2,?3,'exam_assessment',?4,?5,?6,?7,?8)",
        params![
            public_id,
            input.request_key.trim(),
            input.draft_id,
            input.destination_public_id,
            input.destination_version_public_id,
            digest,
            input.created_by.trim(),
            created_at,
        ],
    )?;
    let event_payload = serde_json::json!({
        "schema_version": 1,
        "draft_public_id": input.draft_public_id,
        "destination_type": "exam_assessment",
        "destination_public_id": input.destination_public_id,
        "destination_version_public_id": input.destination_version_public_id
    })
    .to_string();
    outbox::create_event(
        conn,
        &NewOutboxEvent {
            idempotency_key: &format!("profile:outbox:class-action-materialized:{public_id}"),
            event_type: "class_action_materialized",
            event_version: 1,
            aggregate_type: "class_action_draft",
            aggregate_id: input.draft_public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &created_at,
        },
    )?;
    audit::append(
        conn,
        &NewAuditEvent {
            idempotency_key: &format!("profile:audit:class-action-materialized:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.created_by.trim()),
            action: "profile.class_action.materialized",
            object_type: "class_action_draft",
            object_id: input.draft_public_id,
            object_revision: Some(1),
            note: Some("老师显式确认后建立固定版本 M2 练习作业"),
            meta_json: Some(&event_payload),
            occurred_at: &created_at,
        },
    )?;
    Ok(ClassActionMaterialization {
        public_id,
        destination_type: "exam_assessment".into(),
        destination_public_id: input.destination_public_id.into(),
        destination_version_public_id: input.destination_version_public_id.into(),
        created_by: input.created_by.trim().into(),
        created_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile;
    use module_exam::service::assessment::{
        create_confirmed_multi_item_targeted_in_transaction, NewMultiItemTargetedAssessment,
        NewTargetedAssessmentItem,
    };
    use module_knowledge::db::content::{
        add_knowledge_link, create_answer_key_version, create_link_set, create_question,
        create_question_version, create_rubric_version, promote_question_version,
        NewAnswerKeyVersion, NewKnowledgeLink, NewQuestion, NewQuestionVersion, NewRubricPoint,
        NewRubricVersion,
    };
    use suite_core::db::repo::learning_evidence::{create_or_get, NewLearningEvidence};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{
        AssessmentContext, ConfirmationLevel, EvidenceKind, EvidenceSourceModule,
    };

    struct Fixture {
        conn: Connection,
        students: Vec<i64>,
        map_public_id: String,
        knowledge_public_id: String,
        class_snapshot: class_profile::ClassProfileSnapshot,
        question_version_id: i64,
        question_version_public_id: String,
        answer_key_version_id: i64,
        rubric_version_id: i64,
        link_set_id: i64,
    }

    fn add_evidence(
        conn: &Connection,
        student_id: i64,
        knowledge_scope: (&str, &str),
        source_scope: (&str, &str),
        occurred_at: &str,
        value: f64,
    ) {
        let (map_public_id, knowledge_public_id) = knowledge_scope;
        let (key, source) = source_scope;
        create_or_get(
            conn,
            &NewLearningEvidence {
                idempotency_key: key,
                student_id,
                source_module: EvidenceSourceModule::Grading,
                source_type: "rubric_point",
                source_ref_type: "assessment_item",
                source_ref_id: source,
                source_revision: 1,
                decision_ref_type: Some("grade_decision"),
                decision_ref_id: Some(key),
                decision_revision: Some(1),
                knowledge_node_id: Some(knowledge_public_id),
                ability_dimension_id: None,
                evidence_kind: EvidenceKind::Accuracy,
                value,
                confirmation_level: ConfirmationLevel::TeacherCorrected,
                evidence_quality: 1.0,
                assessment_context: AssessmentContext::ClosedBook,
                occurred_at,
                rule_version: "exam-v1",
                knowledge_map_version: &format!("{map_public_id}:r1"),
            },
        )
        .unwrap();
    }

    fn setup() -> Fixture {
        let mut conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, module_exam::exam_migrations()).unwrap();
        run_migrations(&conn, crate::profile_migrations()).unwrap();
        conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])
            .unwrap();
        let subject_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO classes(name,term,textbook)
             VALUES ('八年级一班','2026','中国历史八上')",
            [],
        )
        .unwrap();
        let class_id = conn.last_insert_rowid();
        let mut students = Vec::new();
        for index in 1..=3 {
            conn.execute(
                "INSERT INTO students(student_no,name,class_id,enabled)
                 VALUES (?1,?2,?3,1)",
                params![format!("{index:02}"), format!("学生{index}"), class_id],
            )
            .unwrap();
            students.push(conn.last_insert_rowid());
        }
        conn.execute(
            "INSERT INTO k1_textbook_editions
              (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
             VALUES ('edition-action',?1,'pep','2026','八上历史','8','upper','active',
                     '2026-07-01T00:00:00.000Z')",
            [subject_id],
        )
        .unwrap();
        let edition_id = conn.last_insert_rowid();
        let map_public_id = "map-action".to_string();
        conn.execute(
            "INSERT INTO k1_knowledge_maps
              (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
             VALUES (?1,?2,1,'confirmed','2026-07-01T00:00:00.000Z',
                     '2026-07-01T00:00:00.000Z')",
            (&map_public_id, edition_id),
        )
        .unwrap();
        let map_id = conn.last_insert_rowid();
        let knowledge_public_id = "knowledge-action".to_string();
        conn.execute(
            "INSERT INTO k1_knowledge_nodes
              (public_id,stable_id,knowledge_map_id,title,order_index,state,created_at)
             VALUES (?1,'stable-action',?2,'洋务运动失败原因',1,'active',
                     '2026-07-01T00:00:00.000Z')",
            (&knowledge_public_id, map_id),
        )
        .unwrap();
        let knowledge_id = conn.last_insert_rowid();

        let question = create_question(
            &conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: "teacher-1",
                question_family_id: None,
                rights_status: "unknown",
                sharing_allowed: false,
            },
        )
        .unwrap();
        let question_version = create_question_version(
            &conn,
            &NewQuestionVersion {
                question_id: question.id,
                revision: 1,
                question_type: "true_false",
                stem: "洋务运动失败的根本原因是只学习技术而未改变封建制度。",
                material_text: None,
                max_score: 2.0,
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "L0",
                state: "draft",
                options: &[],
            },
        )
        .unwrap();
        let answer = create_answer_key_version(
            &conn,
            &NewAnswerKeyVersion {
                question_version_id: question_version.id,
                revision: 1,
                answer_json: r#"{"schema_version":1,"correct":true}"#,
                state: "confirmed",
                confirmed_by: Some("teacher-1"),
                supersedes_answer_key_id: None,
                slots: &[],
            },
        )
        .unwrap();
        let rubric = create_rubric_version(
            &conn,
            &NewRubricVersion {
                question_version_id: question_version.id,
                revision: 1,
                max_score: 2.0,
                state: "confirmed",
                confirmed_by: Some("teacher-1"),
                supersedes_rubric_id: None,
                points: &[NewRubricPoint {
                    stable_id: Some("action-point-1"),
                    order_index: 0,
                    canonical_text: "判断为正确",
                    allowed_paraphrases_json: None,
                    required_concepts_json: None,
                    max_score: 2.0,
                }],
            },
        )
        .unwrap();
        let link_set = create_link_set(
            &conn,
            question_version.id,
            map_id,
            1,
            "confirmed",
            Some("teacher-1"),
            None,
        )
        .unwrap();
        add_knowledge_link(
            &conn,
            &NewKnowledgeLink {
                link_set_id: link_set.id,
                source_type: "question",
                source_public_id: &question_version.public_id,
                knowledge_node_id: knowledge_id,
                relation_type: "direct_assessment",
                confirmation_level: "teacher_confirmed",
                verified_by: Some("teacher-1"),
            },
        )
        .unwrap();
        promote_question_version(&conn, question_version.id, "L2", "teacher-1", None).unwrap();

        for (student_index, student_id) in students.iter().enumerate() {
            for (date_index, occurred_at) in [
                "2026-07-05T00:00:00.000Z",
                "2026-07-12T00:00:00.000Z",
                "2026-07-16T00:00:00.000Z",
            ]
            .iter()
            .enumerate()
            {
                add_evidence(
                    &conn,
                    *student_id,
                    (&map_public_id, &knowledge_public_id),
                    (
                        &format!("action-evidence-{student_index}-{date_index}"),
                        &format!("action-question-{student_index}-{date_index}"),
                    ),
                    occurred_at,
                    0.0,
                );
            }
            profile::generate_student_profile(
                &mut conn,
                &profile::GenerateStudentProfileInput {
                    scope: profile::StudentProfileScope {
                        class_id,
                        student_id: *student_id,
                        range_start: "2026-07-01",
                        range_end: "2026-07-31",
                    },
                    confirmed_by: "teacher-1",
                },
            )
            .unwrap();
        }
        let class_scope = class_profile::ClassProfileScope {
            class_id,
            range_start: "2026-07-01",
            range_end: "2026-07-31",
        };
        let preview = class_profile::preview_class_profile(&conn, &class_scope).unwrap();
        let class_snapshot = class_profile::generate_class_profile(
            &mut conn,
            &class_profile::GenerateClassProfileInput {
                scope: class_scope,
                expected_source_watermark: &preview.source_watermark,
                confirmed_by: "teacher-1",
            },
        )
        .unwrap();
        Fixture {
            conn,
            students,
            map_public_id,
            knowledge_public_id,
            class_snapshot,
            question_version_id: question_version.id,
            question_version_public_id: question_version.public_id,
            answer_key_version_id: answer.id,
            rubric_version_id: rubric.id,
            link_set_id: link_set.id,
        }
    }

    fn node_public_id(fixture: &Fixture) -> &str {
        &fixture.class_snapshot.knowledge_metrics[0].public_id
    }

    fn practice_preview(fixture: &Fixture) -> ClassActionPreview {
        preview_class_action(
            &fixture.conn,
            &PreviewClassActionInput {
                snapshot_public_id: &fixture.class_snapshot.public_id,
                node_metric_public_id: node_public_id(fixture),
                action_kind: "practice",
            },
        )
        .unwrap()
    }

    fn confirm_practice(fixture: &mut Fixture, request_key: &str) -> ClassActionDraft {
        let preview = practice_preview(fixture);
        let snapshot_public_id = fixture.class_snapshot.public_id.clone();
        let node_metric_public_id = node_public_id(fixture).to_string();
        let target_student_ids = fixture.students.clone();
        let candidate_ids = vec![fixture.question_version_public_id.clone()];
        confirm_class_action(
            &mut fixture.conn,
            &ConfirmClassActionInput {
                request_key,
                snapshot_public_id: &snapshot_public_id,
                node_metric_public_id: &node_metric_public_id,
                action_kind: "practice",
                expected_snapshot_payload_sha256: &preview.snapshot_payload_sha256,
                title: "洋务运动失败原因定向练习",
                rationale: "三名学生在三次独立证据中均需要支持",
                estimated_minutes: 15,
                target_student_ids: &target_student_ids,
                candidate_question_version_public_ids: &candidate_ids,
                confirmed_by: "teacher-1",
            },
        )
        .unwrap()
    }

    #[test]
    fn preview_is_read_only_and_recitation_fails_closed_without_mapping() {
        let fixture = setup();
        let before: i64 = fixture
            .conn
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap();
        let practice = practice_preview(&fixture);
        let recitation = preview_class_action(
            &fixture.conn,
            &PreviewClassActionInput {
                snapshot_public_id: &fixture.class_snapshot.public_id,
                node_metric_public_id: node_public_id(&fixture),
                action_kind: "recitation",
            },
        )
        .unwrap();
        let after: i64 = fixture
            .conn
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap();
        assert_eq!(before, after);
        assert!(practice.can_confirm);
        assert_eq!(practice.targets.len(), 3);
        assert!(practice.targets.iter().all(|target| target.recommended));
        assert_eq!(practice.candidates.len(), 1);
        assert_eq!(
            practice.candidates[0].question_version_public_id,
            fixture.question_version_public_id
        );
        assert!(!recitation.can_confirm);
        assert!(recitation
            .blockers
            .iter()
            .any(|message| message.contains("不按标题相似度猜测")));
    }

    #[test]
    fn confirm_is_idempotent_immutable_and_has_no_automatic_task_or_grade_effects() {
        let mut fixture = setup();
        let evidence_before: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM learning_evidence", [], |row| {
                row.get(0)
            })
            .unwrap();
        let task_before: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .unwrap();
        let first = confirm_practice(&mut fixture, "action-confirm-1");
        let repeated = confirm_practice(&mut fixture, "action-confirm-1");
        assert_eq!(first.public_id, repeated.public_id);
        assert_eq!(first.targets.len(), 3);
        assert_eq!(first.candidates.len(), 1);
        assert!(first.materialization.is_none());
        let counts: (i64, i64, i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM class_action_drafts),
                   (SELECT COUNT(*) FROM class_action_draft_targets),
                   (SELECT COUNT(*) FROM class_action_draft_candidates),
                   (SELECT COUNT(*) FROM audit_events
                    WHERE action='profile.class_action.confirmed'),
                   (SELECT COUNT(*) FROM outbox_events
                    WHERE event_type='class_action_draft_confirmed')",
                [],
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
            .unwrap();
        assert_eq!(counts, (1, 3, 1, 1, 1));
        let assessment_count: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM exam_assessments_v2", [], |row| {
                row.get(0)
            })
            .unwrap();
        let evidence_after: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM learning_evidence", [], |row| {
                row.get(0)
            })
            .unwrap();
        let task_after: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM tasks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(assessment_count, 0);
        assert_eq!(evidence_before, evidence_after);
        assert_eq!(task_before, task_after);
        assert!(fixture
            .conn
            .execute(
                "UPDATE class_action_drafts SET title='不可覆盖' WHERE public_id=?1",
                [&first.public_id],
            )
            .is_err());

        let preview = practice_preview(&fixture);
        let snapshot_public_id = fixture.class_snapshot.public_id.clone();
        let node_metric_public_id = node_public_id(&fixture).to_string();
        let target_student_ids = fixture.students.clone();
        let candidate_ids = vec![fixture.question_version_public_id.clone()];
        let conflict = confirm_class_action(
            &mut fixture.conn,
            &ConfirmClassActionInput {
                request_key: "action-confirm-1",
                snapshot_public_id: &snapshot_public_id,
                node_metric_public_id: &node_metric_public_id,
                action_kind: "practice",
                expected_snapshot_payload_sha256: &preview.snapshot_payload_sha256,
                title: "另一份行动",
                rationale: "不能复用请求标识",
                estimated_minutes: 15,
                target_student_ids: &target_student_ids,
                candidate_question_version_public_ids: &candidate_ids,
                confirmed_by: "teacher-1",
            },
        );
        assert!(conflict
            .unwrap_err()
            .to_string()
            .contains("同一请求标识已用于不同"));
    }

    #[test]
    fn materialization_is_atomic_and_matches_frozen_students_and_versions() {
        let mut fixture = setup();
        let draft = confirm_practice(&mut fixture, "action-confirm-materialize");
        let plan = prepare_practice_materialization(&fixture.conn, &draft.public_id).unwrap();
        let evidence_before: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM learning_evidence", [], |row| {
                row.get(0)
            })
            .unwrap();
        let snapshot_before: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM class_profile_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        let item = NewTargetedAssessmentItem {
            question_version_id: fixture.question_version_id,
            answer_key_version_id: fixture.answer_key_version_id,
            rubric_version_id: fixture.rubric_version_id,
            link_set_id: fixture.link_set_id,
            score: 2.0,
            option_order_json: None,
            presentation_snapshot_json: r#"{"schema_version":1,"source":"m6.1_class_action"}"#,
        };
        let transaction = fixture.conn.transaction().unwrap();
        let assessment = create_confirmed_multi_item_targeted_in_transaction(
            &transaction,
            &NewMultiItemTargetedAssessment {
                title: &plan.title,
                class_id: plan.class_id,
                target_student_ids: &plan.target_student_ids,
                assessment_context: "classwork",
                evidence_policy: "include",
                template_version: CLASS_ACTION_PRACTICE_TEMPLATE_VERSION,
                created_by: "teacher-1",
                items: &[item],
            },
        )
        .unwrap();
        let materialized = record_practice_materialization_in_transaction(
            &transaction,
            &RecordPracticeMaterializationInput {
                request_key: "action-materialize-1",
                draft_id: plan.draft_id,
                draft_public_id: &plan.draft_public_id,
                destination_public_id: &assessment.assessment_public_id,
                destination_version_public_id: &assessment.assessment_version_public_id,
                created_by: "teacher-1",
            },
        )
        .unwrap();
        transaction.commit().unwrap();
        let repeated = record_practice_materialization_in_transaction(
            &fixture.conn,
            &RecordPracticeMaterializationInput {
                request_key: "action-materialize-1",
                draft_id: plan.draft_id,
                draft_public_id: &plan.draft_public_id,
                destination_public_id: &assessment.assessment_public_id,
                destination_version_public_id: &assessment.assessment_version_public_id,
                created_by: "teacher-1",
            },
        )
        .unwrap();
        assert_eq!(materialized.public_id, repeated.public_id);
        let scope: (String, String, i64, i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT assessment.state,assessment.audience_kind,
                        (SELECT COUNT(*) FROM exam_assessment_targets_v2
                         WHERE assessment_version_id=version.id),
                        (SELECT COUNT(*) FROM exam_assessment_items_v2
                         WHERE assessment_version_id=version.id),
                        (SELECT COUNT(*) FROM exam_attempts_v2
                         WHERE assessment_version_id=version.id),
                        (SELECT COUNT(*) FROM class_action_materializations
                         WHERE draft_id=?2)
                 FROM exam_assessments_v2 assessment
                 JOIN exam_assessment_versions_v2 version
                   ON version.assessment_id=assessment.id
                 WHERE assessment.public_id=?1",
                params![assessment.assessment_public_id, plan.draft_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(scope, ("active".into(), "explicit".into(), 3, 1, 0, 1));
        let evidence_after: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM learning_evidence", [], |row| {
                row.get(0)
            })
            .unwrap();
        let snapshot_after: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM class_profile_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(evidence_before, evidence_after);
        assert_eq!(snapshot_before, snapshot_after);
        let loaded = get_class_action_draft(&fixture.conn, &draft.public_id)
            .unwrap()
            .unwrap();
        assert_eq!(
            loaded
                .materialization
                .as_ref()
                .map(|value| value.destination_public_id.as_str()),
            Some(assessment.assessment_public_id.as_str())
        );
    }

    #[test]
    fn stale_source_snapshot_rejects_confirmation_without_partial_write() {
        let mut fixture = setup();
        let preview = practice_preview(&fixture);
        add_evidence(
            &fixture.conn,
            fixture.students[0],
            (&fixture.map_public_id, &fixture.knowledge_public_id),
            ("action-evidence-drift", "action-question-drift"),
            "2026-07-17T00:00:00.000Z",
            1.0,
        );
        let snapshot_public_id = fixture.class_snapshot.public_id.clone();
        let node_metric_public_id = node_public_id(&fixture).to_string();
        let target_student_ids = fixture.students.clone();
        let candidate_ids = vec![fixture.question_version_public_id.clone()];
        let result = confirm_class_action(
            &mut fixture.conn,
            &ConfirmClassActionInput {
                request_key: "action-stale",
                snapshot_public_id: &snapshot_public_id,
                node_metric_public_id: &node_metric_public_id,
                action_kind: "practice",
                expected_snapshot_payload_sha256: &preview.snapshot_payload_sha256,
                title: "不应建立",
                rationale: "来源已变化",
                estimated_minutes: 15,
                target_student_ids: &target_student_ids,
                candidate_question_version_public_ids: &candidate_ids,
                confirmed_by: "teacher-1",
            },
        );
        assert!(result.unwrap_err().to_string().contains("建议重新生成"));
        let count: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM class_action_drafts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}
