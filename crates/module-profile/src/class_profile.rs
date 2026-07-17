//! M6.1-2 班级掌握快照与知识热力图。
//!
//! 班级聚合只引用当前启用学生的最新个人快照。最新快照必须与所选日期范围完全
//! 一致且未 stale；缺快照、范围不符和 stale 都作为独立分母展示，绝不解释为
//! “学生薄弱”。班级结论还要同时满足最小合格人数和全班覆盖比例。

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use crate::profile::{self, ProfileNodeMetric, ProfileStudent, StudentProfileSnapshot};

pub const CLASS_PROFILE_SCHEMA_VERSION: i64 = 1;
pub const CLASS_PROFILE_RULE_VERSION: &str = "m6.1-latest-student-snapshots-v1";
const MAX_RANGE_DAYS: i64 = 366;

#[derive(Debug, Clone)]
pub struct ClassProfileScope<'a> {
    pub class_id: i64,
    pub range_start: &'a str,
    pub range_end: &'a str,
}

#[derive(Debug, Clone)]
pub struct GenerateClassProfileInput<'a> {
    pub scope: ClassProfileScope<'a>,
    pub expected_source_watermark: &'a str,
    pub confirmed_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileClass {
    pub id: i64,
    pub name: String,
    pub term: Option<String>,
    pub textbook: Option<String>,
    pub enabled_student_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassProfilePolicy {
    pub public_id: String,
    pub revision: i64,
    pub min_eligible_students: i64,
    pub min_eligible_ratio: f64,
    pub common_support_ratio_at_or_above: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassProfilePreviewCounts {
    pub total_student_count: i64,
    pub snapshot_student_count: i64,
    pub eligible_student_count: i64,
    pub missing_snapshot_count: i64,
    pub scope_mismatch_count: i64,
    pub stale_snapshot_count: i64,
    pub knowledge_node_total: i64,
    pub knowledge_node_sample_sufficient: i64,
    pub ability_node_total: i64,
    pub ability_node_sample_sufficient: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassProfilePreview {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub class: ProfileClass,
    pub range_start: String,
    pub range_end: String,
    pub policy: ClassProfilePolicy,
    pub counts: ClassProfilePreviewCounts,
    pub source_watermark: String,
    pub can_generate: bool,
    pub blocker: Option<String>,
    pub denominator_note: String,
    pub scope_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassProfileStudentInputView {
    pub student: ProfileStudent,
    pub student_snapshot_public_id: Option<String>,
    pub inclusion_status: String,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassProfileCell {
    pub student: ProfileStudent,
    pub student_snapshot_public_id: Option<String>,
    pub student_metric_public_id: Option<String>,
    pub status: String,
    pub mastery_score: Option<f64>,
    pub confidence_level: String,
    pub last_evidence_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassProfileNodeMetric {
    pub public_id: String,
    pub target_type: String,
    pub target_public_id: String,
    pub target_title: String,
    pub average_mastery_score: Option<f64>,
    pub class_status: String,
    pub confidence_level: String,
    pub total_student_count: i64,
    pub snapshot_student_count: i64,
    pub assessed_student_count: i64,
    pub eligible_student_count: i64,
    pub needs_support_count: i64,
    pub developing_count: i64,
    pub stable_count: i64,
    pub insufficient_evidence_count: i64,
    pub unassessed_count: i64,
    pub missing_snapshot_count: i64,
    pub scope_mismatch_count: i64,
    pub stale_snapshot_count: i64,
    pub eligible_ratio: f64,
    pub needs_support_ratio: Option<f64>,
    pub sample_sufficient: bool,
    pub last_evidence_at: Option<String>,
    pub source_breakdown: BTreeMap<String, i64>,
    pub explanation: String,
    pub cells: Vec<ClassProfileCell>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassProfileSnapshot {
    pub public_id: String,
    pub revision: i64,
    pub class: ProfileClass,
    pub range_start: String,
    pub range_end: String,
    pub scope_kind: String,
    pub evidence_cutoff_at: String,
    pub policy: ClassProfilePolicy,
    pub source_watermark: String,
    pub total_student_count: i64,
    pub snapshot_student_count: i64,
    pub eligible_student_count: i64,
    pub knowledge_node_total: i64,
    pub knowledge_node_sample_sufficient: i64,
    pub ability_node_total: i64,
    pub ability_node_sample_sufficient: i64,
    pub state: String,
    pub payload_sha256: String,
    pub generated_by: String,
    pub generated_at: String,
    pub confirmed_by: String,
    pub confirmed_at: String,
    pub is_stale: bool,
    pub stale_reason: Option<String>,
    pub inputs: Vec<ClassProfileStudentInputView>,
    pub knowledge_metrics: Vec<ClassProfileNodeMetric>,
    pub ability_metrics: Vec<ClassProfileNodeMetric>,
}

#[derive(Debug, Clone)]
struct ValidatedClassScope {
    class: ProfileClass,
    students: Vec<ProfileStudent>,
    range_start: NaiveDate,
    range_end: NaiveDate,
}

#[derive(Debug, Clone)]
struct StudentInput {
    student: ProfileStudent,
    student_snapshot_id: Option<i64>,
    student_snapshot_public_id: Option<String>,
    snapshot: Option<StudentProfileSnapshot>,
    inclusion_status: String,
    detail: String,
}

#[derive(Debug, Clone)]
struct ComputedCell {
    student: ProfileStudent,
    student_snapshot_id: Option<i64>,
    student_metric_public_id: Option<String>,
    status: String,
    mastery_score: Option<f64>,
    confidence_level: String,
    last_evidence_at: Option<String>,
}

#[derive(Debug, Clone)]
struct ComputedNode {
    target_type: String,
    target_public_id: String,
    target_title: String,
    average_mastery_score: Option<f64>,
    class_status: String,
    confidence_level: String,
    total_student_count: i64,
    snapshot_student_count: i64,
    assessed_student_count: i64,
    eligible_student_count: i64,
    needs_support_count: i64,
    developing_count: i64,
    stable_count: i64,
    insufficient_evidence_count: i64,
    unassessed_count: i64,
    missing_snapshot_count: i64,
    scope_mismatch_count: i64,
    stale_snapshot_count: i64,
    eligible_ratio: f64,
    needs_support_ratio: Option<f64>,
    sample_sufficient: bool,
    last_evidence_at: Option<String>,
    source_breakdown: BTreeMap<String, i64>,
    explanation: String,
    cells: Vec<ComputedCell>,
}

#[derive(Debug, Clone)]
struct Computation {
    validated: ValidatedClassScope,
    policy_id: i64,
    policy: ClassProfilePolicy,
    inputs: Vec<StudentInput>,
    nodes: Vec<ComputedNode>,
    counts: ClassProfilePreviewCounts,
    source_watermark: String,
    evidence_cutoff_at: String,
    calculated_at: String,
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{field}不能为空")))
    } else {
        Ok(())
    }
}

fn parse_date(value: &str, field: &str) -> CoreResult<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid(format!("{field}必须是 YYYY-MM-DD")))
}

fn student_no_parts(value: &str) -> (u8, i64, String) {
    match value.trim().parse::<i64>() {
        Ok(number) => (0, number, String::new()),
        Err(_) => (1, 0, value.to_lowercase()),
    }
}

fn validate_scope(
    conn: &Connection,
    scope: &ClassProfileScope<'_>,
) -> CoreResult<ValidatedClassScope> {
    let range_start = parse_date(scope.range_start, "开始日期")?;
    let range_end = parse_date(scope.range_end, "结束日期")?;
    if range_start > range_end {
        return Err(CoreError::Invalid("开始日期不能晚于结束日期".into()));
    }
    if (range_end - range_start).num_days() > MAX_RANGE_DAYS {
        return Err(CoreError::Invalid("单次班级掌握范围不能超过 366 天".into()));
    }
    let class = conn
        .query_row(
            "SELECT id,name,term,textbook,
                    (SELECT COUNT(*) FROM students s WHERE s.class_id=c.id AND s.enabled=1)
             FROM classes c WHERE id=?1",
            [scope.class_id],
            |row| {
                Ok(ProfileClass {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    term: row.get(2)?,
                    textbook: row.get(3)?,
                    enabled_student_count: row.get(4)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("班级不存在".into()))?;
    let mut stmt = conn.prepare(
        "SELECT id,class_id,student_no,name
         FROM students WHERE class_id=?1 AND enabled=1",
    )?;
    let rows = stmt.query_map([scope.class_id], |row| {
        Ok(ProfileStudent {
            id: row.get(0)?,
            class_id: row.get(1)?,
            student_no: row.get(2)?,
            name: row.get(3)?,
        })
    })?;
    let mut students = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    students.sort_by(|left, right| {
        student_no_parts(&left.student_no)
            .cmp(&student_no_parts(&right.student_no))
            .then(left.id.cmp(&right.id))
    });
    Ok(ValidatedClassScope {
        class,
        students,
        range_start,
        range_end,
    })
}

fn active_policy(conn: &Connection) -> CoreResult<(i64, ClassProfilePolicy)> {
    conn.query_row(
        "SELECT id,public_id,revision,min_eligible_students,min_eligible_ratio,
                common_support_ratio_at_or_above
         FROM class_profile_policy_versions
         WHERE policy_key='default' AND state='active'
         ORDER BY revision DESC LIMIT 1",
        [],
        |row| {
            Ok((
                row.get(0)?,
                ClassProfilePolicy {
                    public_id: row.get(1)?,
                    revision: row.get(2)?,
                    min_eligible_students: row.get(3)?,
                    min_eligible_ratio: row.get(4)?,
                    common_support_ratio_at_or_above: row.get(5)?,
                },
            ))
        },
    )
    .map_err(Into::into)
}

fn load_student_inputs(
    conn: &Connection,
    validated: &ValidatedClassScope,
) -> CoreResult<Vec<StudentInput>> {
    let mut result = Vec::with_capacity(validated.students.len());
    let expected_start = validated.range_start.to_string();
    let expected_end = validated.range_end.to_string();
    for student in &validated.students {
        let latest = conn
            .query_row(
                "SELECT id,public_id,range_start,range_end
                 FROM profile_snapshots
                 WHERE class_id=?1 AND student_id=?2
                 ORDER BY revision DESC LIMIT 1",
                (validated.class.id, student.id),
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        let Some((snapshot_id, public_id, range_start, range_end)) = latest else {
            result.push(StudentInput {
                student: student.clone(),
                student_snapshot_id: None,
                student_snapshot_public_id: None,
                snapshot: None,
                inclusion_status: "missing_snapshot".into(),
                detail: "尚未生成个人掌握快照。".into(),
            });
            continue;
        };
        if range_start != expected_start || range_end != expected_end {
            result.push(StudentInput {
                student: student.clone(),
                student_snapshot_id: Some(snapshot_id),
                student_snapshot_public_id: Some(public_id),
                snapshot: None,
                inclusion_status: "scope_mismatch".into(),
                detail: format!(
                    "最新个人快照范围为 {range_start} 至 {range_end}，与本次班级范围不一致。"
                ),
            });
            continue;
        }
        let snapshot = profile::get_student_profile(conn, &public_id)?
            .ok_or_else(|| CoreError::Db("个人快照索引存在但无法读取".into()))?;
        if snapshot.is_stale {
            result.push(StudentInput {
                student: student.clone(),
                student_snapshot_id: Some(snapshot_id),
                student_snapshot_public_id: Some(public_id),
                snapshot: None,
                inclusion_status: "stale_snapshot".into(),
                detail: snapshot
                    .stale_reason
                    .unwrap_or_else(|| "个人快照已过期，需重新生成。".into()),
            });
            continue;
        }
        result.push(StudentInput {
            student: student.clone(),
            student_snapshot_id: Some(snapshot_id),
            student_snapshot_public_id: Some(public_id),
            snapshot: Some(snapshot),
            inclusion_status: "included".into(),
            detail: "已纳入：最新个人快照范围一致且未过期。".into(),
        });
    }
    Ok(result)
}

fn eligible_status(status: &str) -> bool {
    matches!(status, "needs_support" | "developing" | "stable")
}

fn input_status_cell(input: &StudentInput) -> ComputedCell {
    ComputedCell {
        student: input.student.clone(),
        student_snapshot_id: input.student_snapshot_id,
        student_metric_public_id: None,
        status: input.inclusion_status.clone(),
        mastery_score: None,
        confidence_level: "none".into(),
        last_evidence_at: None,
    }
}

fn metric_cell(input: &StudentInput, metric: Option<&ProfileNodeMetric>) -> ComputedCell {
    let Some(metric) = metric else {
        return ComputedCell {
            student: input.student.clone(),
            student_snapshot_id: input.student_snapshot_id,
            student_metric_public_id: None,
            status: "unassessed".into(),
            mastery_score: None,
            confidence_level: "none".into(),
            last_evidence_at: None,
        };
    };
    ComputedCell {
        student: input.student.clone(),
        student_snapshot_id: input.student_snapshot_id,
        student_metric_public_id: Some(metric.public_id.clone()),
        status: metric.status.clone(),
        mastery_score: metric.mastery_score,
        confidence_level: metric.confidence_level.clone(),
        last_evidence_at: metric.last_evidence_at.clone(),
    }
}

fn compute_node(
    target_type: &str,
    target_public_id: &str,
    target_title: &str,
    inputs: &[StudentInput],
    policy: &ClassProfilePolicy,
) -> ComputedNode {
    let mut cells = Vec::with_capacity(inputs.len());
    let mut source_breakdown = BTreeMap::new();
    let mut scores = Vec::new();
    for input in inputs {
        let Some(snapshot) = &input.snapshot else {
            cells.push(input_status_cell(input));
            continue;
        };
        let metrics = if target_type == "knowledge_node" {
            &snapshot.knowledge_metrics
        } else {
            &snapshot.ability_metrics
        };
        let metric = metrics
            .iter()
            .find(|item| item.target_public_id == target_public_id);
        if let Some(item) = metric {
            for (source, count) in &item.source_breakdown {
                *source_breakdown.entry(source.clone()).or_insert(0) += count;
            }
            if eligible_status(&item.status) {
                if let Some(score) = item.mastery_score {
                    scores.push(score);
                }
            }
        }
        cells.push(metric_cell(input, metric));
    }
    let count =
        |status: &str| -> i64 { cells.iter().filter(|cell| cell.status == status).count() as i64 };
    let total_student_count = cells.len() as i64;
    let snapshot_student_count = cells
        .iter()
        .filter(|cell| {
            !matches!(
                cell.status.as_str(),
                "missing_snapshot" | "scope_mismatch" | "stale_snapshot"
            )
        })
        .count() as i64;
    let needs_support_count = count("needs_support");
    let developing_count = count("developing");
    let stable_count = count("stable");
    let eligible_student_count = needs_support_count + developing_count + stable_count;
    let insufficient_evidence_count = count("insufficient_evidence");
    let unassessed_count = count("unassessed");
    let missing_snapshot_count = count("missing_snapshot");
    let scope_mismatch_count = count("scope_mismatch");
    let stale_snapshot_count = count("stale_snapshot");
    let assessed_student_count = eligible_student_count + insufficient_evidence_count;
    let eligible_ratio = if total_student_count == 0 {
        0.0
    } else {
        eligible_student_count as f64 / total_student_count as f64
    };
    let needs_support_ratio = (eligible_student_count > 0)
        .then(|| needs_support_count as f64 / eligible_student_count as f64);
    let sample_sufficient = eligible_student_count >= policy.min_eligible_students
        && eligible_ratio >= policy.min_eligible_ratio;
    let class_status = if eligible_student_count == 0 {
        "no_evidence"
    } else if !sample_sufficient {
        "class_evidence_insufficient"
    } else if needs_support_ratio.unwrap_or(0.0) >= policy.common_support_ratio_at_or_above {
        "common_needs_support"
    } else {
        "observed"
    };
    let confidence_level = if eligible_student_count == 0 {
        "none"
    } else if !sample_sufficient {
        "low"
    } else if eligible_ratio >= 0.8 {
        "high"
    } else if eligible_ratio >= 0.6 {
        "medium"
    } else {
        "low"
    };
    let average_mastery_score = if scores.is_empty() {
        None
    } else {
        Some(scores.iter().sum::<f64>() / scores.len() as f64)
    };
    let last_evidence_at = cells
        .iter()
        .filter_map(|cell| cell.last_evidence_at.as_ref())
        .max()
        .cloned();
    let explanation = if eligible_student_count == 0 {
        "没有学生在该节点达到个人证据门槛，不解释为全班薄弱。".into()
    } else if !sample_sufficient {
        format!(
            "合格样本 {eligible_student_count}/{total_student_count}，未同时达到至少 {} 人且覆盖全班 {:.0}% 的班级门槛。",
            policy.min_eligible_students,
            policy.min_eligible_ratio * 100.0
        )
    } else {
        format!(
            "合格样本 {eligible_student_count}/{total_student_count}；其中需要支持 {needs_support_count}/{eligible_student_count}。"
        )
    };
    ComputedNode {
        target_type: target_type.into(),
        target_public_id: target_public_id.into(),
        target_title: target_title.into(),
        average_mastery_score,
        class_status: class_status.into(),
        confidence_level: confidence_level.into(),
        total_student_count,
        snapshot_student_count,
        assessed_student_count,
        eligible_student_count,
        needs_support_count,
        developing_count,
        stable_count,
        insufficient_evidence_count,
        unassessed_count,
        missing_snapshot_count,
        scope_mismatch_count,
        stale_snapshot_count,
        eligible_ratio,
        needs_support_ratio,
        sample_sufficient,
        last_evidence_at,
        source_breakdown,
        explanation,
        cells,
    }
}

fn source_watermark(
    validated: &ValidatedClassScope,
    policy: &ClassProfilePolicy,
    inputs: &[StudentInput],
) -> CoreResult<String> {
    let rows = inputs
        .iter()
        .map(|input| {
            serde_json::json!({
                "student_id": input.student.id,
                "inclusion_status": input.inclusion_status,
                "latest_snapshot_public_id": input.student_snapshot_public_id,
                "latest_snapshot_payload_sha256": input.snapshot.as_ref().map(|item| &item.payload_sha256),
                "latest_snapshot_source_watermark": input.snapshot.as_ref().map(|item| &item.source_watermark)
            })
        })
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "schema_version": CLASS_PROFILE_SCHEMA_VERSION,
        "rule_version": CLASS_PROFILE_RULE_VERSION,
        "class_id": validated.class.id,
        "range_start": validated.range_start.to_string(),
        "range_end": validated.range_end.to_string(),
        "policy_public_id": policy.public_id,
        "policy_revision": policy.revision,
        "student_inputs": rows
    });
    let bytes =
        serde_json::to_vec(&payload).map_err(|error| CoreError::Invalid(error.to_string()))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn compute(conn: &Connection, scope: &ClassProfileScope<'_>) -> CoreResult<Computation> {
    let validated = validate_scope(conn, scope)?;
    let (policy_id, policy) = active_policy(conn)?;
    let inputs = load_student_inputs(conn, &validated)?;
    let mut targets = BTreeSet::new();
    let mut evidence_cutoff_at = None::<String>;
    for input in &inputs {
        if let Some(snapshot) = &input.snapshot {
            evidence_cutoff_at = Some(
                evidence_cutoff_at
                    .map(|current| current.max(snapshot.evidence_cutoff_at.clone()))
                    .unwrap_or_else(|| snapshot.evidence_cutoff_at.clone()),
            );
            for metric in snapshot
                .knowledge_metrics
                .iter()
                .chain(snapshot.ability_metrics.iter())
            {
                targets.insert((
                    metric.target_type.clone(),
                    metric.target_public_id.clone(),
                    metric.target_title.clone(),
                ));
            }
        }
    }
    let mut nodes = targets
        .into_iter()
        .map(|(target_type, public_id, title)| {
            compute_node(&target_type, &public_id, &title, &inputs, &policy)
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| {
        left.target_type
            .cmp(&right.target_type)
            .then(left.target_title.cmp(&right.target_title))
            .then(left.target_public_id.cmp(&right.target_public_id))
    });
    let snapshot_student_count = inputs
        .iter()
        .filter(|input| input.inclusion_status == "included")
        .count() as i64;
    let eligible_students = inputs
        .iter()
        .filter(|input| {
            input.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot
                    .knowledge_metrics
                    .iter()
                    .chain(snapshot.ability_metrics.iter())
                    .any(|metric| eligible_status(&metric.status))
            })
        })
        .count() as i64;
    let knowledge = nodes
        .iter()
        .filter(|node| node.target_type == "knowledge_node")
        .collect::<Vec<_>>();
    let ability = nodes
        .iter()
        .filter(|node| node.target_type == "ability_dimension")
        .collect::<Vec<_>>();
    let counts = ClassProfilePreviewCounts {
        total_student_count: inputs.len() as i64,
        snapshot_student_count,
        eligible_student_count: eligible_students,
        missing_snapshot_count: inputs
            .iter()
            .filter(|input| input.inclusion_status == "missing_snapshot")
            .count() as i64,
        scope_mismatch_count: inputs
            .iter()
            .filter(|input| input.inclusion_status == "scope_mismatch")
            .count() as i64,
        stale_snapshot_count: inputs
            .iter()
            .filter(|input| input.inclusion_status == "stale_snapshot")
            .count() as i64,
        knowledge_node_total: knowledge.len() as i64,
        knowledge_node_sample_sufficient: knowledge
            .iter()
            .filter(|node| node.sample_sufficient)
            .count() as i64,
        ability_node_total: ability.len() as i64,
        ability_node_sample_sufficient: ability.iter().filter(|node| node.sample_sufficient).count()
            as i64,
    };
    let calculated_at = time::utc_now_rfc3339();
    let watermark = source_watermark(&validated, &policy, &inputs)?;
    Ok(Computation {
        validated,
        policy_id,
        policy,
        inputs,
        nodes,
        counts,
        source_watermark: watermark,
        evidence_cutoff_at: evidence_cutoff_at.unwrap_or_else(|| calculated_at.clone()),
        calculated_at,
    })
}

pub fn preview_class_profile(
    conn: &Connection,
    scope: &ClassProfileScope<'_>,
) -> CoreResult<ClassProfilePreview> {
    let computation = compute(conn, scope)?;
    let can_generate = computation.counts.snapshot_student_count > 0;
    Ok(ClassProfilePreview {
        schema_version: CLASS_PROFILE_SCHEMA_VERSION,
        rule_version: CLASS_PROFILE_RULE_VERSION.into(),
        calculated_at: computation.calculated_at,
        class: computation.validated.class,
        range_start: computation.validated.range_start.to_string(),
        range_end: computation.validated.range_end.to_string(),
        policy: computation.policy,
        counts: computation.counts,
        source_watermark: computation.source_watermark,
        can_generate,
        blocker: (!can_generate)
            .then(|| "所选范围没有任何最新、范围一致且未过期的个人掌握快照。".into()),
        denominator_note: "班级结论同时显示合格样本人数/全班人数；至少 3 人且覆盖全班 50% 才允许标记共同需要支持。".into(),
        scope_note: "只使用每位启用学生最新的个人快照；范围不符、已过期或缺失均单列，不解释为薄弱。".into(),
    })
}

fn payload_hash(computation: &Computation) -> CoreResult<String> {
    let inputs = computation
        .inputs
        .iter()
        .map(|input| {
            serde_json::json!({
                "student_id": input.student.id,
                "snapshot_public_id": input.student_snapshot_public_id,
                "inclusion_status": input.inclusion_status
            })
        })
        .collect::<Vec<_>>();
    let nodes = computation
        .nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "target_type": node.target_type,
                "target_public_id": node.target_public_id,
                "class_status": node.class_status,
                "eligible_student_count": node.eligible_student_count,
                "needs_support_count": node.needs_support_count,
                "sample_sufficient": node.sample_sufficient,
                "cells": node.cells.iter().map(|cell| serde_json::json!({
                    "student_id": cell.student.id,
                    "status": cell.status,
                    "student_metric_public_id": cell.student_metric_public_id
                })).collect::<Vec<_>>()
            })
        })
        .collect::<Vec<_>>();
    let payload = serde_json::json!({
        "schema_version": CLASS_PROFILE_SCHEMA_VERSION,
        "rule_version": CLASS_PROFILE_RULE_VERSION,
        "class_id": computation.validated.class.id,
        "range_start": computation.validated.range_start.to_string(),
        "range_end": computation.validated.range_end.to_string(),
        "policy_public_id": computation.policy.public_id,
        "policy_revision": computation.policy.revision,
        "source_watermark": computation.source_watermark,
        "inputs": inputs,
        "nodes": nodes
    });
    let bytes =
        serde_json::to_vec(&payload).map_err(|error| CoreError::Invalid(error.to_string()))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn insert_snapshot(
    tx: &Transaction<'_>,
    computation: &Computation,
    confirmed_by: &str,
) -> CoreResult<(String, i64, String)> {
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1
         FROM class_profile_snapshots WHERE class_id=?1",
        [computation.validated.class.id],
        |row| row.get(0),
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let payload_sha256 = payload_hash(computation)?;
    let scope_json = serde_json::json!({
        "schema_version": 1,
        "scope_kind": "latest_exact_range_student_snapshots",
        "range_start": computation.validated.range_start.to_string(),
        "range_end": computation.validated.range_end.to_string()
    })
    .to_string();
    let source_config_json = serde_json::json!({
        "schema_version": 1,
        "latest_student_snapshot_only": true,
        "exact_range_required": true,
        "non_stale_required": true,
        "missing_is_not_weak": true,
        "no_student_ranking": true
    })
    .to_string();
    tx.execute(
        "INSERT INTO class_profile_snapshots
          (public_id,class_id,revision,range_start,range_end,scope_kind,scope_json,
           source_config_json,evidence_cutoff_at,policy_id,policy_revision,source_watermark,
           total_student_count,snapshot_student_count,eligible_student_count,
           knowledge_node_total,knowledge_node_sample_sufficient,ability_node_total,
           ability_node_sample_sufficient,state,payload_sha256,generated_by,generated_at,
           confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,?5,'latest_exact_range_student_snapshots',?6,?7,?8,?9,?10,
                 ?11,?12,?13,?14,?15,?16,?17,?18,'teacher_confirmed',?19,?20,?21,?20,?21)",
        params![
            public_id,
            computation.validated.class.id,
            revision,
            computation.validated.range_start.to_string(),
            computation.validated.range_end.to_string(),
            scope_json,
            source_config_json,
            computation.evidence_cutoff_at,
            computation.policy_id,
            computation.policy.revision,
            computation.source_watermark,
            computation.counts.total_student_count,
            computation.counts.snapshot_student_count,
            computation.counts.eligible_student_count,
            computation.counts.knowledge_node_total,
            computation.counts.knowledge_node_sample_sufficient,
            computation.counts.ability_node_total,
            computation.counts.ability_node_sample_sufficient,
            payload_sha256,
            confirmed_by.trim(),
            now,
        ],
    )?;
    let snapshot_id = tx.last_insert_rowid();
    for input in &computation.inputs {
        tx.execute(
            "INSERT INTO class_profile_student_inputs
              (snapshot_id,student_id,student_profile_snapshot_id,inclusion_status,detail)
             VALUES (?1,?2,?3,?4,?5)",
            params![
                snapshot_id,
                input.student.id,
                input.student_snapshot_id,
                input.inclusion_status,
                input.detail
            ],
        )?;
    }
    for node in &computation.nodes {
        let node_public_id = ids::new_public_id();
        tx.execute(
            "INSERT INTO class_profile_node_metrics
              (public_id,snapshot_id,target_type,target_public_id,target_title,
               average_mastery_score,class_status,confidence_level,total_student_count,
               snapshot_student_count,assessed_student_count,eligible_student_count,
               needs_support_count,developing_count,stable_count,
               insufficient_evidence_count,unassessed_count,missing_snapshot_count,
               scope_mismatch_count,stale_snapshot_count,eligible_ratio,
               needs_support_ratio,sample_sufficient,last_evidence_at,
               source_breakdown_json,explanation)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,
                     ?17,?18,?19,?20,?21,?22,?23,?24,?25,?26)",
            params![
                node_public_id,
                snapshot_id,
                node.target_type,
                node.target_public_id,
                node.target_title,
                node.average_mastery_score,
                node.class_status,
                node.confidence_level,
                node.total_student_count,
                node.snapshot_student_count,
                node.assessed_student_count,
                node.eligible_student_count,
                node.needs_support_count,
                node.developing_count,
                node.stable_count,
                node.insufficient_evidence_count,
                node.unassessed_count,
                node.missing_snapshot_count,
                node.scope_mismatch_count,
                node.stale_snapshot_count,
                node.eligible_ratio,
                node.needs_support_ratio,
                i64::from(node.sample_sufficient),
                node.last_evidence_at,
                serde_json::to_string(&node.source_breakdown)
                    .map_err(|error| CoreError::Invalid(error.to_string()))?,
                node.explanation,
            ],
        )?;
        let node_metric_id = tx.last_insert_rowid();
        for cell in &node.cells {
            let profile_metric_id = cell
                .student_metric_public_id
                .as_deref()
                .map(|public_id| {
                    tx.query_row(
                        "SELECT id FROM profile_node_metrics WHERE public_id=?1",
                        [public_id],
                        |row| row.get::<_, i64>(0),
                    )
                })
                .transpose()?;
            tx.execute(
                "INSERT INTO class_profile_student_cells
                  (snapshot_id,node_metric_id,student_id,student_profile_snapshot_id,
                   student_profile_metric_id,status,mastery_score,confidence_level,last_evidence_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![
                    snapshot_id,
                    node_metric_id,
                    cell.student.id,
                    cell.student_snapshot_id,
                    profile_metric_id,
                    cell.status,
                    cell.mastery_score,
                    cell.confidence_level,
                    cell.last_evidence_at
                ],
            )?;
        }
    }
    Ok((public_id, revision, now))
}

pub fn generate_class_profile(
    conn: &mut Connection,
    input: &GenerateClassProfileInput<'_>,
) -> CoreResult<ClassProfileSnapshot> {
    required(input.confirmed_by, "确认人")?;
    required(input.expected_source_watermark, "预览水位")?;
    let tx = conn.transaction()?;
    let computation = compute(&tx, &input.scope)?;
    if computation.counts.snapshot_student_count == 0 {
        return Err(CoreError::Invalid(
            "所选范围没有可用于班级快照的当前个人快照".into(),
        ));
    }
    if computation.source_watermark != input.expected_source_watermark {
        return Err(CoreError::Invalid(
            "班级名单或个人快照在预览后发生变化，请刷新预览再确认".into(),
        ));
    }
    let (public_id, revision, generated_at) =
        insert_snapshot(&tx, &computation, input.confirmed_by)?;
    let event_payload = serde_json::json!({
        "schema_version": CLASS_PROFILE_SCHEMA_VERSION,
        "snapshot_public_id": public_id,
        "class_id": computation.validated.class.id,
        "revision": revision,
        "range_start": computation.validated.range_start.to_string(),
        "range_end": computation.validated.range_end.to_string(),
        "total_student_count": computation.counts.total_student_count,
        "snapshot_student_count": computation.counts.snapshot_student_count,
        "eligible_student_count": computation.counts.eligible_student_count
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("profile:outbox:class-snapshot:{public_id}"),
            event_type: "class_profile_snapshot_created",
            event_version: 1,
            aggregate_type: "class_profile_snapshot",
            aggregate_id: &public_id,
            aggregate_revision: revision,
            payload_json: &event_payload,
            occurred_at: &generated_at,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("profile:audit:class-snapshot:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.confirmed_by.trim()),
            action: "profile.class_snapshot.generated",
            object_type: "class_profile_snapshot",
            object_id: &public_id,
            object_revision: Some(revision),
            note: Some("老师确认范围和样本分母后生成只读班级掌握快照"),
            meta_json: Some(&event_payload),
            occurred_at: &generated_at,
        },
    )?;
    tx.commit()?;
    get_class_profile(conn, &public_id)?
        .ok_or_else(|| CoreError::Db("班级掌握快照写入后无法读取".into()))
}

fn load_inputs(
    conn: &Connection,
    snapshot_id: i64,
) -> CoreResult<Vec<ClassProfileStudentInputView>> {
    let mut stmt = conn.prepare(
        "SELECT s.id,s.class_id,s.student_no,s.name,p.public_id,i.inclusion_status,i.detail
         FROM class_profile_student_inputs i
         JOIN students s ON s.id=i.student_id
         LEFT JOIN profile_snapshots p ON p.id=i.student_profile_snapshot_id
         WHERE i.snapshot_id=?1",
    )?;
    let rows = stmt.query_map([snapshot_id], |row| {
        Ok(ClassProfileStudentInputView {
            student: ProfileStudent {
                id: row.get(0)?,
                class_id: row.get(1)?,
                student_no: row.get(2)?,
                name: row.get(3)?,
            },
            student_snapshot_public_id: row.get(4)?,
            inclusion_status: row.get(5)?,
            detail: row.get(6)?,
        })
    })?;
    let mut inputs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    inputs.sort_by(|left, right| {
        student_no_parts(&left.student.student_no)
            .cmp(&student_no_parts(&right.student.student_no))
            .then(left.student.id.cmp(&right.student.id))
    });
    Ok(inputs)
}

fn load_cells(
    conn: &Connection,
    snapshot_id: i64,
    node_metric_id: i64,
) -> CoreResult<Vec<ClassProfileCell>> {
    let mut stmt = conn.prepare(
        "SELECT s.id,s.class_id,s.student_no,s.name,p.public_id,m.public_id,c.status,
                c.mastery_score,c.confidence_level,c.last_evidence_at
         FROM class_profile_student_cells c
         JOIN students s ON s.id=c.student_id
         LEFT JOIN profile_snapshots p ON p.id=c.student_profile_snapshot_id
         LEFT JOIN profile_node_metrics m ON m.id=c.student_profile_metric_id
         WHERE c.snapshot_id=?1 AND c.node_metric_id=?2",
    )?;
    let rows = stmt.query_map((snapshot_id, node_metric_id), |row| {
        Ok(ClassProfileCell {
            student: ProfileStudent {
                id: row.get(0)?,
                class_id: row.get(1)?,
                student_no: row.get(2)?,
                name: row.get(3)?,
            },
            student_snapshot_public_id: row.get(4)?,
            student_metric_public_id: row.get(5)?,
            status: row.get(6)?,
            mastery_score: row.get(7)?,
            confidence_level: row.get(8)?,
            last_evidence_at: row.get(9)?,
        })
    })?;
    let mut cells = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    cells.sort_by(|left, right| {
        student_no_parts(&left.student.student_no)
            .cmp(&student_no_parts(&right.student.student_no))
            .then(left.student.id.cmp(&right.student.id))
    });
    Ok(cells)
}

fn load_nodes(
    conn: &Connection,
    snapshot_id: i64,
) -> CoreResult<(Vec<ClassProfileNodeMetric>, Vec<ClassProfileNodeMetric>)> {
    let mut stmt = conn.prepare(
        "SELECT id,public_id,target_type,target_public_id,target_title,average_mastery_score,
                class_status,confidence_level,total_student_count,snapshot_student_count,
                assessed_student_count,eligible_student_count,needs_support_count,
                developing_count,stable_count,insufficient_evidence_count,unassessed_count,
                missing_snapshot_count,scope_mismatch_count,stale_snapshot_count,
                eligible_ratio,needs_support_ratio,sample_sufficient,last_evidence_at,
                source_breakdown_json,explanation
         FROM class_profile_node_metrics
         WHERE snapshot_id=?1
         ORDER BY target_type,target_title,target_public_id",
    )?;
    let rows = stmt.query_map([snapshot_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            ClassProfileNodeMetric {
                public_id: row.get(1)?,
                target_type: row.get(2)?,
                target_public_id: row.get(3)?,
                target_title: row.get(4)?,
                average_mastery_score: row.get(5)?,
                class_status: row.get(6)?,
                confidence_level: row.get(7)?,
                total_student_count: row.get(8)?,
                snapshot_student_count: row.get(9)?,
                assessed_student_count: row.get(10)?,
                eligible_student_count: row.get(11)?,
                needs_support_count: row.get(12)?,
                developing_count: row.get(13)?,
                stable_count: row.get(14)?,
                insufficient_evidence_count: row.get(15)?,
                unassessed_count: row.get(16)?,
                missing_snapshot_count: row.get(17)?,
                scope_mismatch_count: row.get(18)?,
                stale_snapshot_count: row.get(19)?,
                eligible_ratio: row.get(20)?,
                needs_support_ratio: row.get(21)?,
                sample_sufficient: row.get::<_, i64>(22)? == 1,
                last_evidence_at: row.get(23)?,
                source_breakdown: serde_json::from_str(&row.get::<_, String>(24)?).map_err(
                    |error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            24,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    },
                )?,
                explanation: row.get(25)?,
                cells: Vec::new(),
            },
        ))
    })?;
    let mut knowledge = Vec::new();
    let mut ability = Vec::new();
    for row in rows {
        let (metric_id, mut metric) = row?;
        metric.cells = load_cells(conn, snapshot_id, metric_id)?;
        if metric.target_type == "knowledge_node" {
            knowledge.push(metric);
        } else {
            ability.push(metric);
        }
    }
    Ok((knowledge, ability))
}

pub fn get_class_profile(
    conn: &Connection,
    public_id: &str,
) -> CoreResult<Option<ClassProfileSnapshot>> {
    let row = conn
        .query_row(
            "SELECT p.id,p.public_id,p.revision,p.class_id,c.name,c.term,c.textbook,
                    p.range_start,p.range_end,p.scope_kind,p.evidence_cutoff_at,
                    policy.public_id,policy.revision,policy.min_eligible_students,
                    policy.min_eligible_ratio,policy.common_support_ratio_at_or_above,
                    p.source_watermark,p.total_student_count,p.snapshot_student_count,
                    p.eligible_student_count,p.knowledge_node_total,
                    p.knowledge_node_sample_sufficient,p.ability_node_total,
                    p.ability_node_sample_sufficient,p.state,p.payload_sha256,
                    p.generated_by,p.generated_at,p.confirmed_by,p.confirmed_at
             FROM class_profile_snapshots p
             JOIN classes c ON c.id=p.class_id
             JOIN class_profile_policy_versions policy ON policy.id=p.policy_id
             WHERE p.public_id=?1",
            [public_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    ClassProfilePolicy {
                        public_id: row.get(11)?,
                        revision: row.get(12)?,
                        min_eligible_students: row.get(13)?,
                        min_eligible_ratio: row.get(14)?,
                        common_support_ratio_at_or_above: row.get(15)?,
                    },
                    row.get::<_, String>(16)?,
                    row.get::<_, i64>(17)?,
                    row.get::<_, i64>(18)?,
                    row.get::<_, i64>(19)?,
                    row.get::<_, i64>(20)?,
                    row.get::<_, i64>(21)?,
                    row.get::<_, i64>(22)?,
                    row.get::<_, i64>(23)?,
                    row.get::<_, String>(24)?,
                    row.get::<_, String>(25)?,
                    row.get::<_, String>(26)?,
                    row.get::<_, String>(27)?,
                    row.get::<_, String>(28)?,
                    row.get::<_, String>(29)?,
                ))
            },
        )
        .optional()?;
    let Some((
        snapshot_id,
        public_id,
        revision,
        class_id,
        class_name,
        term,
        textbook,
        range_start,
        range_end,
        scope_kind,
        evidence_cutoff_at,
        policy,
        source_watermark,
        total_student_count,
        snapshot_student_count,
        eligible_student_count,
        knowledge_node_total,
        knowledge_node_sample_sufficient,
        ability_node_total,
        ability_node_sample_sufficient,
        state,
        payload_sha256,
        generated_by,
        generated_at,
        confirmed_by,
        confirmed_at,
    )) = row
    else {
        return Ok(None);
    };
    let current = compute(
        conn,
        &ClassProfileScope {
            class_id,
            range_start: &range_start,
            range_end: &range_end,
        },
    )?;
    let policy_stale = current.policy.public_id != policy.public_id;
    let source_stale = current.source_watermark != source_watermark;
    let is_stale = policy_stale || source_stale;
    let stale_reason = if policy_stale {
        Some("班级掌握聚合规则已有新版本，建议重新生成。".into())
    } else if source_stale {
        Some("班级名单或最新个人快照已经变化，旧快照保持不变，建议重新生成。".into())
    } else {
        None
    };
    let inputs = load_inputs(conn, snapshot_id)?;
    let (knowledge_metrics, ability_metrics) = load_nodes(conn, snapshot_id)?;
    Ok(Some(ClassProfileSnapshot {
        public_id,
        revision,
        class: ProfileClass {
            id: class_id,
            name: class_name,
            term,
            textbook,
            enabled_student_count: current.validated.class.enabled_student_count,
        },
        range_start,
        range_end,
        scope_kind,
        evidence_cutoff_at,
        policy,
        source_watermark,
        total_student_count,
        snapshot_student_count,
        eligible_student_count,
        knowledge_node_total,
        knowledge_node_sample_sufficient,
        ability_node_total,
        ability_node_sample_sufficient,
        state,
        payload_sha256,
        generated_by,
        generated_at,
        confirmed_by,
        confirmed_at,
        is_stale,
        stale_reason,
        inputs,
        knowledge_metrics,
        ability_metrics,
    }))
}

pub fn latest_class_profile(
    conn: &Connection,
    class_id: i64,
) -> CoreResult<Option<ClassProfileSnapshot>> {
    let public_id = conn
        .query_row(
            "SELECT public_id FROM class_profile_snapshots
             WHERE class_id=?1 ORDER BY revision DESC LIMIT 1",
            [class_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    public_id
        .as_deref()
        .map(|id| get_class_profile(conn, id))
        .transpose()
        .map(Option::flatten)
}

#[cfg(test)]
mod tests {
    use super::*;
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
        knowledge: String,
    }

    fn setup() -> Fixture {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
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
        for index in 1..=4 {
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
             VALUES ('edition-1',?1,'pep','2026','八上历史','8','upper','active',
                     '2026-07-01T00:00:00.000Z')",
            [subject_id],
        )
        .unwrap();
        let edition_id = conn.last_insert_rowid();
        let map_public_id = "map-1".to_string();
        conn.execute(
            "INSERT INTO k1_knowledge_maps
              (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
             VALUES (?1,?2,1,'confirmed','2026-07-01T00:00:00.000Z',
                     '2026-07-01T00:00:00.000Z')",
            (&map_public_id, edition_id),
        )
        .unwrap();
        let map_id = conn.last_insert_rowid();
        let knowledge = "knowledge-1".to_string();
        conn.execute(
            "INSERT INTO k1_knowledge_nodes
              (public_id,stable_id,knowledge_map_id,title,order_index,state,created_at)
             VALUES (?1,'stable-1',?2,'洋务运动失败原因',1,'active',
                     '2026-07-01T00:00:00.000Z')",
            (&knowledge, map_id),
        )
        .unwrap();
        Fixture {
            conn,
            class_id,
            students,
            map_public_id,
            knowledge,
        }
    }

    fn add_evidence(
        fixture: &Fixture,
        student_id: i64,
        key: &str,
        source: &str,
        occurred_at: &str,
        value: f64,
    ) {
        create_or_get(
            &fixture.conn,
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
                knowledge_node_id: Some(&fixture.knowledge),
                ability_dimension_id: None,
                evidence_kind: EvidenceKind::Accuracy,
                value,
                confirmation_level: ConfirmationLevel::TeacherCorrected,
                evidence_quality: 1.0,
                assessment_context: AssessmentContext::ClosedBook,
                occurred_at,
                rule_version: "exam-v1",
                knowledge_map_version: &format!("{}:r1", fixture.map_public_id),
            },
        )
        .unwrap();
    }

    fn generate_personal(
        fixture: &mut Fixture,
        student_id: i64,
        prefix: &str,
        value: f64,
        range_start: &str,
        range_end: &str,
    ) -> StudentProfileSnapshot {
        for (index, date) in [
            "2026-07-05T00:00:00.000Z",
            "2026-07-12T00:00:00.000Z",
            "2026-07-20T00:00:00.000Z",
        ]
        .iter()
        .enumerate()
        {
            add_evidence(
                fixture,
                student_id,
                &format!("{prefix}-{index}"),
                &format!("{prefix}-q{index}"),
                date,
                value,
            );
        }
        profile::generate_student_profile(
            &mut fixture.conn,
            &profile::GenerateStudentProfileInput {
                scope: profile::StudentProfileScope {
                    class_id: fixture.class_id,
                    student_id,
                    range_start,
                    range_end,
                },
                confirmed_by: "teacher-1",
            },
        )
        .unwrap()
    }

    fn scope(fixture: &Fixture) -> ClassProfileScope<'_> {
        ClassProfileScope {
            class_id: fixture.class_id,
            range_start: "2026-07-01",
            range_end: "2026-07-31",
        }
    }

    #[test]
    fn preview_is_read_only_and_separates_missing_scope_and_stale() {
        let mut fixture = setup();
        let first = fixture.students[0];
        let second = fixture.students[1];
        let third = fixture.students[2];
        generate_personal(
            &mut fixture,
            first,
            "included",
            1.0,
            "2026-07-01",
            "2026-07-31",
        );
        generate_personal(
            &mut fixture,
            second,
            "mismatch",
            1.0,
            "2026-07-01",
            "2026-07-20",
        );
        generate_personal(
            &mut fixture,
            third,
            "stale",
            1.0,
            "2026-07-01",
            "2026-07-31",
        );
        add_evidence(
            &fixture,
            third,
            "stale-new",
            "stale-new-q",
            "2026-07-25T00:00:00.000Z",
            0.0,
        );
        let before: i64 = fixture
            .conn
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap();
        let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
        let after: i64 = fixture
            .conn
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap();
        assert_eq!(before, after);
        assert!(preview.can_generate);
        assert_eq!(preview.counts.total_student_count, 4);
        assert_eq!(preview.counts.snapshot_student_count, 1);
        assert_eq!(preview.counts.scope_mismatch_count, 1);
        assert_eq!(preview.counts.stale_snapshot_count, 1);
        assert_eq!(preview.counts.missing_snapshot_count, 1);
    }

    #[test]
    fn generated_snapshot_has_denominators_heatmap_audit_and_immutability() {
        let mut fixture = setup();
        for index in 0..3 {
            let student_id = fixture.students[index];
            generate_personal(
                &mut fixture,
                student_id,
                &format!("weak-{index}"),
                0.0,
                "2026-07-01",
                "2026-07-31",
            );
        }
        let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
        let generated = generate_class_profile(
            &mut fixture.conn,
            &GenerateClassProfileInput {
                scope: ClassProfileScope {
                    class_id: fixture.class_id,
                    range_start: "2026-07-01",
                    range_end: "2026-07-31",
                },
                expected_source_watermark: &preview.source_watermark,
                confirmed_by: "teacher-1",
            },
        )
        .unwrap();
        assert_eq!(generated.total_student_count, 4);
        assert_eq!(generated.snapshot_student_count, 3);
        assert_eq!(generated.eligible_student_count, 3);
        assert_eq!(generated.inputs.len(), 4);
        let node = &generated.knowledge_metrics[0];
        assert!(node.sample_sufficient);
        assert_eq!(node.class_status, "common_needs_support");
        assert_eq!(node.needs_support_count, 3);
        assert_eq!(node.eligible_student_count, 3);
        assert_eq!(node.cells.len(), 4);
        assert_eq!(
            node.cells
                .iter()
                .filter(|cell| cell.status == "missing_snapshot")
                .count(),
            1
        );
        let audit_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM audit_events
                 WHERE action='profile.class_snapshot.generated' AND object_id=?1",
                [&generated.public_id],
                |row| row.get(0),
            )
            .unwrap();
        let outbox_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM outbox_events
                 WHERE event_type='class_profile_snapshot_created' AND aggregate_id=?1",
                [&generated.public_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_count, 1);
        assert_eq!(outbox_count, 1);
        assert!(fixture
            .conn
            .execute(
                "UPDATE class_profile_snapshots SET generated_by='tampered'
                 WHERE public_id=?1",
                [&generated.public_id]
            )
            .is_err());
        assert!(fixture
            .conn
            .execute(
                "DELETE FROM class_profile_student_cells WHERE snapshot_id=
                 (SELECT id FROM class_profile_snapshots WHERE public_id=?1)",
                [&generated.public_id]
            )
            .is_err());
    }

    #[test]
    fn low_sample_never_becomes_common_needs_support() {
        let mut fixture = setup();
        for index in 0..2 {
            let student_id = fixture.students[index];
            generate_personal(
                &mut fixture,
                student_id,
                &format!("low-{index}"),
                0.0,
                "2026-07-01",
                "2026-07-31",
            );
        }
        let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
        let generated = generate_class_profile(
            &mut fixture.conn,
            &GenerateClassProfileInput {
                scope: ClassProfileScope {
                    class_id: fixture.class_id,
                    range_start: "2026-07-01",
                    range_end: "2026-07-31",
                },
                expected_source_watermark: &preview.source_watermark,
                confirmed_by: "teacher-1",
            },
        )
        .unwrap();
        let node = &generated.knowledge_metrics[0];
        assert!(!node.sample_sufficient);
        assert_eq!(node.class_status, "class_evidence_insufficient");
        assert_eq!(node.needs_support_count, 2);
    }

    #[test]
    fn stale_preview_watermark_rejects_without_partial_write() {
        let mut fixture = setup();
        let student_id = fixture.students[0];
        generate_personal(
            &mut fixture,
            student_id,
            "base",
            1.0,
            "2026-07-01",
            "2026-07-31",
        );
        let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
        add_evidence(
            &fixture,
            student_id,
            "after-preview",
            "after-preview-q",
            "2026-07-25T00:00:00.000Z",
            0.0,
        );
        let result = generate_class_profile(
            &mut fixture.conn,
            &GenerateClassProfileInput {
                scope: ClassProfileScope {
                    class_id: fixture.class_id,
                    range_start: "2026-07-01",
                    range_end: "2026-07-31",
                },
                expected_source_watermark: &preview.source_watermark,
                confirmed_by: "teacher-1",
            },
        );
        assert!(result.is_err());
        let count: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM class_profile_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn class_snapshot_becomes_stale_when_personal_input_changes() {
        let mut fixture = setup();
        let student_id = fixture.students[0];
        generate_personal(
            &mut fixture,
            student_id,
            "base",
            1.0,
            "2026-07-01",
            "2026-07-31",
        );
        let preview = preview_class_profile(&fixture.conn, &scope(&fixture)).unwrap();
        let generated = generate_class_profile(
            &mut fixture.conn,
            &GenerateClassProfileInput {
                scope: ClassProfileScope {
                    class_id: fixture.class_id,
                    range_start: "2026-07-01",
                    range_end: "2026-07-31",
                },
                expected_source_watermark: &preview.source_watermark,
                confirmed_by: "teacher-1",
            },
        )
        .unwrap();
        assert!(!generated.is_stale);
        add_evidence(
            &fixture,
            student_id,
            "new-evidence",
            "new-q",
            "2026-07-25T00:00:00.000Z",
            0.0,
        );
        let loaded = get_class_profile(&fixture.conn, &generated.public_id)
            .unwrap()
            .unwrap();
        assert!(loaded.is_stale);
        assert_eq!(loaded.revision, 1);
    }
}
