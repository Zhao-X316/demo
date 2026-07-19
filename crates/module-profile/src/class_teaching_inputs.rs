//! M6-4 班级共性教学输入。
//!
//! 本模块只把当前、未过期班级掌握快照中已经达到班级双门槛的共同支持节点，
//! 整理成老师可删减、可改写的课堂重点预览。老师明确确认后冻结一份不可变
//! 教学输入；它不建立作业、不修改成绩、学习证据、学生标签或背诵排程。

use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use crate::class_profile::{self, ClassProfileNodeMetric, ClassProfileSnapshot};

pub const CLASS_TEACHING_INPUT_SCHEMA_VERSION: i64 = 1;
pub const CLASS_TEACHING_INPUT_RULE_VERSION: &str =
    "m6-class-common-teaching-input-teacher-confirmed-v1";
const MAX_SELECTED_ITEMS: usize = 100;

#[derive(Debug, Clone)]
pub struct PreviewClassTeachingInput<'a> {
    pub snapshot_public_id: &'a str,
}

#[derive(Debug, Clone)]
pub struct ConfirmClassTeachingInput<'a> {
    pub request_key: &'a str,
    pub snapshot_public_id: &'a str,
    pub expected_snapshot_payload_sha256: &'a str,
    pub title: &'a str,
    pub teaching_note: &'a str,
    pub estimated_minutes: i64,
    pub selected_node_metric_public_ids: &'a [String],
    pub confirmed_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassTeachingInputItem {
    pub node_metric_public_id: String,
    pub target_type: String,
    pub target_public_id: String,
    pub target_title: String,
    pub confidence_level: String,
    pub total_student_count: i64,
    pub eligible_student_count: i64,
    pub needs_support_count: i64,
    pub needs_support_ratio: f64,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassTeachingInputPreview {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub class_id: i64,
    pub class_name: String,
    pub snapshot_public_id: String,
    pub snapshot_revision: i64,
    pub snapshot_payload_sha256: String,
    pub range_start: String,
    pub range_end: String,
    pub suggested_title: String,
    pub suggested_teaching_note: String,
    pub suggested_estimated_minutes: i64,
    pub items: Vec<ClassTeachingInputItem>,
    pub can_confirm: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub denominator_note: String,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassTeachingInputDraft {
    pub public_id: String,
    pub class_id: i64,
    pub class_name: String,
    pub snapshot_public_id: String,
    pub snapshot_revision: i64,
    pub title: String,
    pub teaching_note: String,
    pub estimated_minutes: i64,
    pub source_snapshot_payload_sha256: String,
    pub schema_version: i64,
    pub rule_version: String,
    pub payload_sha256: String,
    pub state: String,
    pub confirmed_by: String,
    pub confirmed_at: String,
    pub items: Vec<ClassTeachingInputItem>,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn item_from_metric(metric: &ClassProfileNodeMetric) -> CoreResult<ClassTeachingInputItem> {
    let needs_support_ratio = metric.needs_support_ratio.ok_or_else(|| {
        CoreError::Invalid(format!(
            "共同支持节点“{}”缺少可解释分母",
            metric.target_title
        ))
    })?;
    Ok(ClassTeachingInputItem {
        node_metric_public_id: metric.public_id.clone(),
        target_type: metric.target_type.clone(),
        target_public_id: metric.target_public_id.clone(),
        target_title: metric.target_title.clone(),
        confidence_level: metric.confidence_level.clone(),
        total_student_count: metric.total_student_count,
        eligible_student_count: metric.eligible_student_count,
        needs_support_count: metric.needs_support_count,
        needs_support_ratio,
        explanation: metric.explanation.clone(),
    })
}

fn common_items(snapshot: &ClassProfileSnapshot) -> CoreResult<Vec<ClassTeachingInputItem>> {
    snapshot
        .knowledge_metrics
        .iter()
        .chain(snapshot.ability_metrics.iter())
        .filter(|metric| metric.class_status == "common_needs_support")
        .filter(|metric| metric.sample_sufficient)
        .map(item_from_metric)
        .collect()
}

fn current_snapshot_blocker(
    conn: &Connection,
    snapshot: &ClassProfileSnapshot,
) -> CoreResult<Option<String>> {
    let latest_public_id = conn
        .query_row(
            "SELECT public_id FROM class_profile_snapshots
             WHERE class_id=?1 ORDER BY revision DESC LIMIT 1",
            [snapshot.class.id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(match latest_public_id {
        Some(public_id) if public_id == snapshot.public_id => None,
        Some(_) => Some("当前查看的不是该班最新掌握快照，请刷新后再生成教学重点。".into()),
        None => Some("该班还没有可用的班级掌握快照。".into()),
    })
}

fn suggested_note(items: &[ClassTeachingInputItem]) -> String {
    let mut lines = vec!["本次优先处理：".to_string()];
    for (index, item) in items.iter().enumerate() {
        let kind = if item.target_type == "knowledge_node" {
            "知识点"
        } else {
            "能力项"
        };
        lines.push(format!(
            "{}. {}：{}（需要支持 {}/{} 人；合格样本 {}/{} 人）",
            index + 1,
            kind,
            item.target_title,
            item.needs_support_count,
            item.eligible_student_count,
            item.eligible_student_count,
            item.total_student_count
        ));
    }
    lines.push("课堂后请用一次简短核对或练习收集新证据，再由老师决定是否布置后续任务。".into());
    lines.join("\n")
}

pub fn preview_class_teaching_input(
    conn: &Connection,
    input: &PreviewClassTeachingInput<'_>,
) -> CoreResult<ClassTeachingInputPreview> {
    required(input.snapshot_public_id, "班级掌握快照")?;
    let snapshot = class_profile::get_class_profile(conn, input.snapshot_public_id)?
        .ok_or_else(|| CoreError::Invalid("班级掌握快照不存在".into()))?;
    let items = common_items(&snapshot)?;
    let mut blockers = Vec::new();
    let mut warnings = Vec::new();
    if snapshot.state != "teacher_confirmed" {
        blockers.push("班级掌握快照尚未由老师确认。".into());
    }
    if snapshot.is_stale {
        blockers.push(
            snapshot
                .stale_reason
                .clone()
                .unwrap_or_else(|| "来源班级快照已有新输入，请先重新生成。".into()),
        );
    }
    if let Some(blocker) = current_snapshot_blocker(conn, &snapshot)? {
        blockers.push(blocker);
    }
    if items.is_empty() {
        blockers.push("当前快照没有同时达到样本门槛和共同支持比例门槛的知识点或能力项。".into());
    }
    if items.len() > MAX_SELECTED_ITEMS {
        blockers.push(format!(
            "当前共同支持节点超过 {MAX_SELECTED_ITEMS} 个，请缩小教材或时间范围后再生成。"
        ));
    }
    if items.iter().any(|item| item.confidence_level == "none") {
        blockers.push("共同支持节点存在无法解释的置信度，请重新生成班级快照。".into());
    }
    let data_gap_count = snapshot
        .inputs
        .iter()
        .filter(|item| item.inclusion_status != "included")
        .count();
    if data_gap_count > 0 {
        warnings.push(format!(
            "本次仍有 {data_gap_count} 名学生因个人快照缺失、范围不符或已过期未进入有效样本。"
        ));
    }
    let suggested_estimated_minutes = (10 + 5 * items.len() as i64).min(45);
    let class_name = snapshot.class.name.clone();
    let range_start = snapshot.range_start.clone();
    let range_end = snapshot.range_end.clone();
    Ok(ClassTeachingInputPreview {
        schema_version: CLASS_TEACHING_INPUT_SCHEMA_VERSION,
        rule_version: CLASS_TEACHING_INPUT_RULE_VERSION.into(),
        calculated_at: time::utc_now_rfc3339(),
        class_id: snapshot.class.id,
        class_name: class_name.clone(),
        snapshot_public_id: snapshot.public_id,
        snapshot_revision: snapshot.revision,
        snapshot_payload_sha256: snapshot.payload_sha256,
        range_start,
        range_end,
        suggested_title: format!("{class_name}阶段复习重点"),
        suggested_teaching_note: suggested_note(&items),
        suggested_estimated_minutes,
        items,
        can_confirm: blockers.is_empty(),
        blockers,
        warnings,
        denominator_note:
            "只把“合格样本中需要支持的比例达到班级门槛”的节点列为共同重点；未评估、证据不足、个人快照缺失、范围不符或已过期都保留在分母说明中，不算作薄弱。"
                .into(),
        boundary_note:
            "确认后只保存老师本次课堂输入，不会自动建立作业、修改成绩、学习证据、学生标签或背诵排程。"
                .into(),
    })
}

fn canonical_selected_items(
    preview: &ClassTeachingInputPreview,
    selected_public_ids: &[String],
) -> CoreResult<Vec<ClassTeachingInputItem>> {
    if selected_public_ids.is_empty() || selected_public_ids.len() > MAX_SELECTED_ITEMS {
        return Err(CoreError::Invalid(format!(
            "教学重点必须选择 1 至 {MAX_SELECTED_ITEMS} 个节点"
        )));
    }
    let mut selected = BTreeSet::new();
    for public_id in selected_public_ids {
        required(public_id, "教学重点节点")?;
        if !selected.insert(public_id.trim().to_string()) {
            return Err(CoreError::Invalid("教学重点节点不能重复".into()));
        }
    }
    if selected.iter().any(|public_id| {
        !preview
            .items
            .iter()
            .any(|item| &item.node_metric_public_id == public_id)
    }) {
        return Err(CoreError::Invalid(
            "所选节点不是当前快照已达到双门槛的共同支持节点".into(),
        ));
    }
    Ok(preview
        .items
        .iter()
        .filter(|item| selected.contains(&item.node_metric_public_id))
        .cloned()
        .collect())
}

#[derive(Serialize)]
struct TeachingInputHash<'a> {
    schema_version: i64,
    rule_version: &'a str,
    snapshot_public_id: &'a str,
    snapshot_payload_sha256: &'a str,
    title: &'a str,
    teaching_note: &'a str,
    estimated_minutes: i64,
    selected_node_metric_public_ids: Vec<&'a str>,
}

fn payload_hash(
    preview: &ClassTeachingInputPreview,
    title: &str,
    teaching_note: &str,
    estimated_minutes: i64,
    items: &[ClassTeachingInputItem],
) -> CoreResult<String> {
    let value = TeachingInputHash {
        schema_version: CLASS_TEACHING_INPUT_SCHEMA_VERSION,
        rule_version: CLASS_TEACHING_INPUT_RULE_VERSION,
        snapshot_public_id: &preview.snapshot_public_id,
        snapshot_payload_sha256: &preview.snapshot_payload_sha256,
        title,
        teaching_note,
        estimated_minutes,
        selected_node_metric_public_ids: items
            .iter()
            .map(|item| item.node_metric_public_id.as_str())
            .collect(),
    };
    let bytes = serde_json::to_vec(&value).map_err(|error| CoreError::Parse(error.to_string()))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn load_items(conn: &Connection, draft_id: i64) -> CoreResult<Vec<ClassTeachingInputItem>> {
    let mut stmt = conn.prepare(
        "SELECT metric.public_id,item.target_type,item.target_public_id,item.target_title,
                item.confidence_level,item.total_student_count,item.eligible_student_count,
                item.needs_support_count,item.needs_support_ratio,item.explanation
         FROM class_teaching_input_items item
         JOIN class_profile_node_metrics metric
           ON metric.id=item.class_profile_node_metric_id
         WHERE item.draft_id=?1
         ORDER BY item.order_index,item.class_profile_node_metric_id",
    )?;
    let rows = stmt.query_map([draft_id], |row| {
        Ok(ClassTeachingInputItem {
            node_metric_public_id: row.get(0)?,
            target_type: row.get(1)?,
            target_public_id: row.get(2)?,
            target_title: row.get(3)?,
            confidence_level: row.get(4)?,
            total_student_count: row.get(5)?,
            eligible_student_count: row.get(6)?,
            needs_support_count: row.get(7)?,
            needs_support_ratio: row.get(8)?,
            explanation: row.get(9)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_draft_by_id(conn: &Connection, draft_id: i64) -> CoreResult<ClassTeachingInputDraft> {
    let base = conn
        .query_row(
            "SELECT draft.public_id,draft.class_id,class.name,snapshot.public_id,
                    snapshot.revision,draft.title,draft.teaching_note,draft.estimated_minutes,
                    draft.source_snapshot_payload_sha256,draft.schema_version,draft.rule_version,
                    draft.payload_sha256,draft.state,draft.confirmed_by,draft.confirmed_at
             FROM class_teaching_input_drafts draft
             JOIN classes class ON class.id=draft.class_id
             JOIN class_profile_snapshots snapshot
               ON snapshot.id=draft.class_profile_snapshot_id
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
                    row.get::<_, i64>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, String>(14)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("班级教学重点不存在".into()))?;
    Ok(ClassTeachingInputDraft {
        public_id: base.0,
        class_id: base.1,
        class_name: base.2,
        snapshot_public_id: base.3,
        snapshot_revision: base.4,
        title: base.5,
        teaching_note: base.6,
        estimated_minutes: base.7,
        source_snapshot_payload_sha256: base.8,
        schema_version: base.9,
        rule_version: base.10,
        payload_sha256: base.11,
        state: base.12,
        confirmed_by: base.13,
        confirmed_at: base.14,
        items: load_items(conn, draft_id)?,
    })
}

pub fn confirm_class_teaching_input(
    conn: &mut Connection,
    input: &ConfirmClassTeachingInput<'_>,
) -> CoreResult<ClassTeachingInputDraft> {
    required(input.request_key, "请求标识")?;
    required(input.expected_snapshot_payload_sha256, "快照校验值")?;
    required(input.title, "教学重点标题")?;
    required(input.teaching_note, "课堂说明")?;
    required(input.confirmed_by, "确认人")?;
    if input.expected_snapshot_payload_sha256.len() != 64 {
        return Err(CoreError::Invalid("班级快照校验值无效".into()));
    }
    if input.title.trim().chars().count() > 100 {
        return Err(CoreError::Invalid("教学重点标题不能超过 100 个字符".into()));
    }
    if input.teaching_note.trim().chars().count() > 4000 {
        return Err(CoreError::Invalid("课堂说明不能超过 4000 个字符".into()));
    }
    if !(1..=240).contains(&input.estimated_minutes) {
        return Err(CoreError::Invalid(
            "预计时长必须在 1 至 240 分钟之间".into(),
        ));
    }
    let preview = preview_class_teaching_input(
        conn,
        &PreviewClassTeachingInput {
            snapshot_public_id: input.snapshot_public_id,
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
    let selected_items = canonical_selected_items(&preview, input.selected_node_metric_public_ids)?;
    let title = input.title.trim();
    let teaching_note = input.teaching_note.trim();
    let digest = payload_hash(
        &preview,
        title,
        teaching_note,
        input.estimated_minutes,
        &selected_items,
    )?;
    let existing = conn
        .query_row(
            "SELECT id,request_payload_sha256
             FROM class_teaching_input_drafts WHERE request_key=?1",
            [input.request_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    if let Some((draft_id, stored_hash)) = existing {
        if stored_hash != digest {
            return Err(CoreError::Invalid(
                "同一请求标识已用于不同班级教学重点".into(),
            ));
        }
        return load_draft_by_id(conn, draft_id);
    }
    let snapshot_id: i64 = conn.query_row(
        "SELECT id FROM class_profile_snapshots WHERE public_id=?1",
        [&preview.snapshot_public_id],
        |row| row.get(0),
    )?;
    let metric_ids = selected_items
        .iter()
        .map(|item| {
            conn.query_row(
                "SELECT id FROM class_profile_node_metrics
                 WHERE snapshot_id=?1 AND public_id=?2",
                params![snapshot_id, item.node_metric_public_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(CoreError::from)
        })
        .collect::<CoreResult<Vec<_>>>()?;
    let public_id = ids::new_public_id();
    let confirmed_at = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO class_teaching_input_drafts
          (public_id,request_key,request_payload_sha256,class_profile_snapshot_id,class_id,
           title,teaching_note,estimated_minutes,selected_node_count,
           source_snapshot_payload_sha256,schema_version,rule_version,payload_sha256,
           state,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,
                 'teacher_confirmed',?14,?15)",
        params![
            public_id,
            input.request_key.trim(),
            digest,
            snapshot_id,
            preview.class_id,
            title,
            teaching_note,
            input.estimated_minutes,
            selected_items.len() as i64,
            preview.snapshot_payload_sha256,
            CLASS_TEACHING_INPUT_SCHEMA_VERSION,
            CLASS_TEACHING_INPUT_RULE_VERSION,
            digest,
            input.confirmed_by.trim(),
            confirmed_at,
        ],
    )?;
    let draft_id = tx.last_insert_rowid();
    for (order_index, (metric_id, item)) in metric_ids.iter().zip(selected_items.iter()).enumerate()
    {
        tx.execute(
            "INSERT INTO class_teaching_input_items
              (draft_id,order_index,class_profile_node_metric_id,target_type,target_public_id,
               target_title,confidence_level,total_student_count,eligible_student_count,
               needs_support_count,needs_support_ratio,explanation)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                draft_id,
                order_index as i64,
                metric_id,
                item.target_type,
                item.target_public_id,
                item.target_title,
                item.confidence_level,
                item.total_student_count,
                item.eligible_student_count,
                item.needs_support_count,
                item.needs_support_ratio,
                item.explanation,
            ],
        )?;
    }
    let event_payload = serde_json::json!({
        "schema_version": CLASS_TEACHING_INPUT_SCHEMA_VERSION,
        "draft_public_id": public_id,
        "class_profile_snapshot_public_id": preview.snapshot_public_id,
        "class_id": preview.class_id,
        "selected_node_count": selected_items.len(),
        "task_created": false,
        "student_results_changed": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("profile:outbox:class-teaching-input:{public_id}"),
            event_type: "class_teaching_input_confirmed",
            event_version: 1,
            aggregate_type: "class_teaching_input",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &confirmed_at,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("profile:audit:class-teaching-input:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.confirmed_by.trim()),
            action: "profile.class_teaching_input.confirmed",
            object_type: "class_teaching_input",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("老师确认班级共性教学输入；未建立作业或修改学生结果"),
            meta_json: Some(&event_payload),
            occurred_at: &confirmed_at,
        },
    )?;
    tx.commit()?;
    load_draft_by_id(conn, draft_id)
}

pub fn list_class_teaching_inputs(
    conn: &Connection,
    class_id: i64,
    limit: i64,
) -> CoreResult<Vec<ClassTeachingInputDraft>> {
    if class_id <= 0 {
        return Err(CoreError::Invalid("班级无效".into()));
    }
    let limit = limit.clamp(1, 100);
    let mut stmt = conn.prepare(
        "SELECT id FROM class_teaching_input_drafts
         WHERE class_id=?1
         ORDER BY confirmed_at DESC,id DESC LIMIT ?2",
    )?;
    let ids = stmt
        .query_map(params![class_id, limit], |row| row.get::<_, i64>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ids.into_iter()
        .map(|draft_id| load_draft_by_id(conn, draft_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{class_profile, profile};
    use suite_core::db::repo::learning_evidence::{create_or_get, NewLearningEvidence};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{
        AssessmentContext, ConfirmationLevel, EvidenceKind, EvidenceSourceModule,
    };

    struct Fixture {
        conn: Connection,
        class_id: i64,
        students: Vec<i64>,
        map_public_id: String,
        knowledge_public_id: String,
        ability_public_id: String,
        class_snapshot: ClassProfileSnapshot,
    }

    #[allow(clippy::too_many_arguments)]
    fn add_evidence(
        conn: &Connection,
        student_id: i64,
        map_public_id: &str,
        knowledge_public_id: &str,
        ability_public_id: &str,
        key: &str,
        occurred_at: &str,
        value: f64,
    ) {
        create_or_get(
            conn,
            &NewLearningEvidence {
                idempotency_key: key,
                student_id,
                source_module: EvidenceSourceModule::Grading,
                source_type: "question_rubric_point",
                source_ref_type: "rubric_point",
                source_ref_id: key,
                source_revision: 1,
                decision_ref_type: Some("grade_decision"),
                decision_ref_id: Some(key),
                decision_revision: Some(1),
                knowledge_node_id: Some(knowledge_public_id),
                ability_dimension_id: Some(ability_public_id),
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
        run_migrations(&conn, module_wrongbook::wrongbook_migrations()).unwrap();
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
             VALUES ('edition-teaching-input',?1,'pep','2026','八上历史','8','upper','active',
                     '2026-07-01T00:00:00.000Z')",
            [subject_id],
        )
        .unwrap();
        let edition_id = conn.last_insert_rowid();
        let map_public_id = "map-teaching-input".to_string();
        conn.execute(
            "INSERT INTO k1_knowledge_maps
              (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
             VALUES (?1,?2,1,'confirmed','2026-07-01T00:00:00.000Z',
                     '2026-07-01T00:00:00.000Z')",
            (&map_public_id, edition_id),
        )
        .unwrap();
        let map_id = conn.last_insert_rowid();
        let knowledge_public_id = "knowledge-teaching-input".to_string();
        conn.execute(
            "INSERT INTO k1_knowledge_nodes
              (public_id,stable_id,knowledge_map_id,title,order_index,state,created_at)
             VALUES (?1,'stable-teaching-input',?2,'洋务运动失败原因',1,'active',
                     '2026-07-01T00:00:00.000Z')",
            (&knowledge_public_id, map_id),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO k1_knowledge_nodes
              (public_id,stable_id,knowledge_map_id,title,order_index,state,created_at)
             VALUES ('knowledge-unassessed','stable-unassessed',?1,'未考查知识点',2,'active',
                     '2026-07-01T00:00:00.000Z')",
            [map_id],
        )
        .unwrap();
        let ability_public_id = "ability-teaching-input".to_string();
        conn.execute(
            "INSERT INTO k1_ability_dimensions
              (public_id,stable_id,subject_id,revision,code,title,state,created_at)
             VALUES (?1,'ability-stable-teaching-input',?2,1,'cause_analysis',
                     '因果分析','active','2026-07-01T00:00:00.000Z')",
            (&ability_public_id, subject_id),
        )
        .unwrap();
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
                    &map_public_id,
                    &knowledge_public_id,
                    &ability_public_id,
                    &format!("teaching-input-{student_index}-{date_index}"),
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
                    confirmed_by: "local_teacher",
                },
            )
            .unwrap();
        }
        let scope = class_profile::ClassProfileScope {
            class_id,
            range_start: "2026-07-01",
            range_end: "2026-07-31",
        };
        let preview = class_profile::preview_class_profile(&conn, &scope).unwrap();
        let class_snapshot = class_profile::generate_class_profile(
            &mut conn,
            &class_profile::GenerateClassProfileInput {
                scope,
                expected_source_watermark: &preview.source_watermark,
                confirmed_by: "local_teacher",
            },
        )
        .unwrap();
        Fixture {
            conn,
            class_id,
            students,
            map_public_id,
            knowledge_public_id,
            ability_public_id,
            class_snapshot,
        }
    }

    fn confirm_input(
        fixture: &mut Fixture,
        request_key: &str,
        selected: &[String],
    ) -> CoreResult<ClassTeachingInputDraft> {
        let snapshot_public_id = fixture.class_snapshot.public_id.clone();
        let snapshot_sha = fixture.class_snapshot.payload_sha256.clone();
        confirm_class_teaching_input(
            &mut fixture.conn,
            &ConfirmClassTeachingInput {
                request_key,
                snapshot_public_id: &snapshot_public_id,
                expected_snapshot_payload_sha256: &snapshot_sha,
                title: "八年级一班阶段复习重点",
                teaching_note: "先核对洋务运动失败原因，再安排一次材料题口头归纳。",
                estimated_minutes: 20,
                selected_node_metric_public_ids: selected,
                confirmed_by: "local_teacher",
            },
        )
    }

    #[test]
    fn preview_only_lists_current_common_support_nodes() {
        let fixture = setup();
        let preview = preview_class_teaching_input(
            &fixture.conn,
            &PreviewClassTeachingInput {
                snapshot_public_id: &fixture.class_snapshot.public_id,
            },
        )
        .unwrap();
        assert!(preview.can_confirm);
        assert_eq!(preview.items.len(), 2);
        assert!(preview
            .items
            .iter()
            .any(|item| item.target_type == "knowledge_node"));
        assert!(preview
            .items
            .iter()
            .any(|item| item.target_type == "ability_dimension"));
        assert!(!preview
            .items
            .iter()
            .any(|item| item.target_public_id == "knowledge-unassessed"));
        assert!(preview.denominator_note.contains("不算作薄弱"));
        assert!(preview.boundary_note.contains("不会自动建立作业"));
    }

    #[test]
    fn confirmed_input_is_immutable_idempotent_and_audited() {
        let mut fixture = setup();
        let preview = preview_class_teaching_input(
            &fixture.conn,
            &PreviewClassTeachingInput {
                snapshot_public_id: &fixture.class_snapshot.public_id,
            },
        )
        .unwrap();
        let selected = preview
            .items
            .iter()
            .map(|item| item.node_metric_public_id.clone())
            .collect::<Vec<_>>();
        let first = confirm_input(&mut fixture, "teaching-input-1", &selected).unwrap();
        let second = confirm_input(&mut fixture, "teaching-input-1", &selected).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.items.len(), 2);
        assert!(fixture
            .conn
            .execute(
                "UPDATE class_teaching_input_drafts SET title='篡改' WHERE public_id=?1",
                [&first.public_id],
            )
            .is_err());
        let conflict = confirm_class_teaching_input(
            &mut fixture.conn,
            &ConfirmClassTeachingInput {
                request_key: "teaching-input-1",
                snapshot_public_id: &fixture.class_snapshot.public_id,
                expected_snapshot_payload_sha256: &fixture.class_snapshot.payload_sha256,
                title: "不同标题",
                teaching_note: "不同内容",
                estimated_minutes: 10,
                selected_node_metric_public_ids: &selected,
                confirmed_by: "local_teacher",
            },
        );
        assert!(conflict.is_err());
        let audit_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM audit_events
                 WHERE action='profile.class_teaching_input.confirmed' AND object_id=?1",
                [&first.public_id],
                |row| row.get(0),
            )
            .unwrap();
        let outbox_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM outbox_events
                 WHERE event_type='class_teaching_input_confirmed' AND aggregate_id=?1",
                [&first.public_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_count, 1);
        assert_eq!(outbox_count, 1);
        assert_eq!(
            list_class_teaching_inputs(&fixture.conn, fixture.class_id, 20)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn excluded_or_duplicate_node_is_rejected_without_partial_write() {
        let mut fixture = setup();
        let excluded = fixture
            .class_snapshot
            .knowledge_metrics
            .iter()
            .find(|item| item.target_public_id == "knowledge-unassessed")
            .unwrap()
            .public_id
            .clone();
        assert!(confirm_input(&mut fixture, "excluded", &[excluded]).is_err());
        let selected = vec![
            fixture.class_snapshot.knowledge_metrics[0]
                .public_id
                .clone(),
            fixture.class_snapshot.knowledge_metrics[0]
                .public_id
                .clone(),
        ];
        assert!(confirm_input(&mut fixture, "duplicate", &selected).is_err());
        let count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM class_teaching_input_drafts",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn stale_or_non_latest_snapshot_cannot_be_confirmed() {
        let mut fixture = setup();
        let old_snapshot = fixture.class_snapshot.clone();
        let scope = class_profile::ClassProfileScope {
            class_id: fixture.class_id,
            range_start: "2026-07-01",
            range_end: "2026-07-31",
        };
        let preview = class_profile::preview_class_profile(&fixture.conn, &scope).unwrap();
        fixture.class_snapshot = class_profile::generate_class_profile(
            &mut fixture.conn,
            &class_profile::GenerateClassProfileInput {
                scope,
                expected_source_watermark: &preview.source_watermark,
                confirmed_by: "local_teacher",
            },
        )
        .unwrap();
        let old_preview = preview_class_teaching_input(
            &fixture.conn,
            &PreviewClassTeachingInput {
                snapshot_public_id: &old_snapshot.public_id,
            },
        )
        .unwrap();
        assert!(!old_preview.can_confirm);
        assert!(old_preview
            .blockers
            .iter()
            .any(|item| item.contains("不是该班最新")));

        add_evidence(
            &fixture.conn,
            fixture.students[0],
            &fixture.map_public_id,
            &fixture.knowledge_public_id,
            &fixture.ability_public_id,
            "teaching-input-stale",
            "2026-07-25T00:00:00.000Z",
            1.0,
        );
        let stale_preview = preview_class_teaching_input(
            &fixture.conn,
            &PreviewClassTeachingInput {
                snapshot_public_id: &fixture.class_snapshot.public_id,
            },
        )
        .unwrap();
        assert!(!stale_preview.can_confirm);
        assert!(stale_preview
            .blockers
            .iter()
            .any(|item| item.contains("建议重新生成") || item.contains("已经变化")));
    }
}
