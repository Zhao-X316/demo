//! M6-1 个人学习掌握快照。
//!
//! 计算规则刻意保守：
//! - 只读取 active + teacher_accepted/teacher_corrected 且契约明确的正式逐点证据；
//! - 背诵总体、流畅度和保持度只进入内容级历史，不扩散为知识或能力结论；
//! - 只接受仍存在于 confirmed K1 map 的知识/能力节点；
//! - 同一来源同一天折叠为一个独立组，组内取最低表现，订正不覆盖首次错误；
//! - 少于 3 个独立组、2 个日期或 2 个来源时，只显示“证据不足”；
//! - 快照只追加，不修改上游成绩、任务、错题或证据。

mod contracts;
mod metrics;

pub use contracts::{
    GenerateScopedStudentProfileInput, GenerateStudentProfileInput, ProfileEvidenceView,
    ProfileNamedReference, ProfileNodeMetric, ProfileNodeTrendChange, ProfilePolicy,
    ProfilePreviewCounts, ProfileRecitationEvidenceView, ProfileRecitationSummary,
    ProfileScopeOption, ProfileScopeSelectionInput, ProfileScopeSelectionView, ProfileStudent,
    ProfileWrongbookFactView, ProfileWrongbookSummary, StudentProfilePreview, StudentProfileScope,
    StudentProfileSnapshot, StudentProfileTrend,
};
use metrics::compute_metric;

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use chrono::{DateTime, NaiveDate, Utc};
use module_wrongbook::read_model::student_wrongbook_items;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::Serialize;
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

pub const PROFILE_SCHEMA_VERSION: i64 = 3;
pub const PROFILE_RULE_VERSION: &str = "m6-confirmed-evidence-profile-v3";
const MAX_RANGE_DAYS: i64 = 366;

#[derive(Debug, Clone, Serialize)]
struct EvidenceWatermarkRow {
    public_id: String,
    state: String,
    target_type: String,
    target_public_id: String,
    target_title: String,
    value_micros: i64,
    quality_micros: i64,
    occurred_at: String,
    source_ref_type: String,
    source_ref_id: String,
    decision_ref_id: Option<String>,
    decision_revision: Option<i64>,
    knowledge_map_version: String,
}

#[derive(Debug, Clone, Serialize)]
struct RecitationWatermarkRow {
    public_id: String,
    source_type: String,
    source_ref_type: String,
    source_ref_id: String,
    decision_ref_id: Option<String>,
    decision_revision: Option<i64>,
    evidence_kind: String,
    value_micros: i64,
    quality_micros: i64,
    occurred_at: String,
}

#[derive(Debug, Clone)]
struct EvidenceTarget {
    id: i64,
    public_id: String,
    source_module: String,
    source_type: String,
    source_ref_type: String,
    source_ref_id: String,
    decision_ref_id: Option<String>,
    decision_revision: Option<i64>,
    target_type: String,
    target_public_id: String,
    target_title: String,
    evidence_kind: String,
    value: f64,
    evidence_quality: f64,
    assessment_context: String,
    occurred_at: String,
    occurred_date: NaiveDate,
    knowledge_map_version: String,
}

#[derive(Debug, Clone)]
struct RecitationEvidence {
    id: i64,
    public_id: String,
    source_type: String,
    source_ref_type: String,
    source_ref_id: String,
    decision_ref_id: Option<String>,
    decision_revision: Option<i64>,
    evidence_kind: String,
    value: f64,
    evidence_quality: f64,
    assessment_context: String,
    occurred_at: String,
}

#[derive(Debug, Clone)]
struct ScopeNode {
    target_type: String,
    public_id: String,
    title: String,
}

#[derive(Debug, Clone)]
struct ValidatedScope {
    student: ProfileStudent,
    range_start: NaiveDate,
    range_end: NaiveDate,
    selection: ResolvedProfileScope,
}

#[derive(Debug, Clone)]
struct ResolvedProfileScope {
    view: ProfileScopeSelectionView,
    knowledge_node_public_ids: Option<BTreeSet<String>>,
    map_ids: BTreeMap<i64, String>,
}

#[derive(Debug, Clone)]
struct ComputedMetric {
    target_type: String,
    target_public_id: String,
    target_title: String,
    mastery_score: Option<f64>,
    status: String,
    confidence_level: String,
    freshness: String,
    evidence_count: i64,
    independent_group_count: i64,
    distinct_date_count: i64,
    distinct_source_count: i64,
    last_evidence_at: Option<String>,
    source_breakdown: BTreeMap<String, i64>,
    explanation: String,
    evidence: Vec<(EvidenceTarget, String, f64)>,
}

#[derive(Debug, Clone)]
struct Computation {
    validated: ValidatedScope,
    policy_id: i64,
    policy: ProfilePolicy,
    metrics: Vec<ComputedMetric>,
    counts: ProfilePreviewCounts,
    source_watermark: String,
    evidence_ids: HashSet<i64>,
    recitation_evidence: Vec<RecitationEvidence>,
    wrongbook_facts: Vec<ProfileWrongbookFactView>,
    knowledge_map_versions: BTreeSet<String>,
    calculated_at: String,
}

#[derive(Debug, Serialize)]
struct SnapshotHashPayload<'a> {
    schema_version: i64,
    rule_version: &'static str,
    student_id: i64,
    class_id: i64,
    range_start: String,
    range_end: String,
    scope_selector_key: &'a str,
    policy_public_id: &'a str,
    policy_revision: i64,
    source_watermark: &'a str,
    metrics: Vec<MetricHashPayload<'a>>,
}

#[derive(Debug, Serialize)]
struct MetricHashPayload<'a> {
    target_type: &'a str,
    target_public_id: &'a str,
    mastery_micros: Option<i64>,
    status: &'a str,
    confidence_level: &'a str,
    freshness: &'a str,
    evidence_public_ids: Vec<&'a str>,
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

fn auto_scope_selection() -> ResolvedProfileScope {
    ResolvedProfileScope {
        view: ProfileScopeSelectionView {
            selector_kind: "auto_evidence_maps".into(),
            selector_public_id: None,
            selector_key: "auto_evidence_maps".into(),
            title: "自动：按正式证据涉及范围".into(),
            path: "系统按所选日期内的老师确认逐点证据确定教材范围".into(),
            node_type: None,
            knowledge_map_public_id: None,
            knowledge_map_version: None,
            textbook_edition_public_id: None,
            textbook_title: None,
            knowledge_node_count: 0,
        },
        knowledge_node_public_ids: None,
        map_ids: BTreeMap::new(),
    }
}

fn curriculum_path(conn: &Connection, node_id: i64) -> CoreResult<String> {
    let mut titles = Vec::new();
    let mut current = Some(node_id);
    while let Some(id) = current {
        let (parent_id, title) = conn.query_row(
            "SELECT parent_id,title FROM k1_curriculum_nodes WHERE id=?1",
            [id],
            |row| Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, String>(1)?)),
        )?;
        titles.push(title);
        current = parent_id;
    }
    titles.reverse();
    Ok(titles.join(" / "))
}

fn resolve_profile_scope(
    conn: &Connection,
    selection: Option<&ProfileScopeSelectionInput<'_>>,
) -> CoreResult<ResolvedProfileScope> {
    let Some(selection) = selection else {
        return Ok(auto_scope_selection());
    };
    match selection.selector_kind.trim() {
        "" | "auto_evidence_maps" => Ok(auto_scope_selection()),
        "knowledge_map" => {
            let public_id = selection
                .selector_public_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| CoreError::Invalid("选择本册范围时必须指定知识图谱".into()))?;
            let row = conn
                .query_row(
                    "SELECT m.id,m.public_id,m.revision,e.public_id,e.title
                     FROM k1_knowledge_maps m
                     JOIN k1_textbook_editions e ON e.id=m.textbook_edition_id
                     WHERE m.public_id=?1 AND m.state='confirmed' AND e.state='active'",
                    [public_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| CoreError::Invalid("所选教材知识图谱不存在或尚未确认".into()))?;
            let mut stmt = conn.prepare(
                "SELECT public_id FROM k1_knowledge_nodes
                 WHERE knowledge_map_id=?1 AND state='active' ORDER BY order_index,id",
            )?;
            let node_ids = stmt
                .query_map([row.0], |node| node.get::<_, String>(0))?
                .collect::<rusqlite::Result<BTreeSet<_>>>()?;
            let map_version = format!("{}:r{}", row.1, row.2);
            Ok(ResolvedProfileScope {
                view: ProfileScopeSelectionView {
                    selector_kind: "knowledge_map".into(),
                    selector_public_id: Some(row.1.clone()),
                    selector_key: format!("knowledge_map:{}", row.1),
                    title: row.4.clone(),
                    path: row.4.clone(),
                    node_type: Some("textbook".into()),
                    knowledge_map_public_id: Some(row.1.clone()),
                    knowledge_map_version: Some(map_version.clone()),
                    textbook_edition_public_id: Some(row.3),
                    textbook_title: Some(row.4),
                    knowledge_node_count: node_ids.len() as i64,
                },
                knowledge_node_public_ids: Some(node_ids),
                map_ids: BTreeMap::from([(row.0, map_version)]),
            })
        }
        "curriculum_node" => {
            let public_id = selection
                .selector_public_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| CoreError::Invalid("选择单元或课时范围时必须指定课程节点".into()))?;
            let row = conn
                .query_row(
                    "SELECT c.id,c.public_id,c.node_type,c.title,m.id,m.public_id,m.revision,
                            e.public_id,e.title
                     FROM k1_curriculum_nodes c
                     JOIN k1_knowledge_maps m ON m.id=c.knowledge_map_id
                     JOIN k1_textbook_editions e ON e.id=m.textbook_edition_id
                     WHERE c.public_id=?1 AND c.state='active'
                       AND m.state='confirmed' AND e.state='active'",
                    [public_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, i64>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, String>(7)?,
                            row.get::<_, String>(8)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| {
                    CoreError::Invalid("所选课程范围不存在或不属于已确认知识图谱".into())
                })?;
            let path = curriculum_path(conn, row.0)?;
            let mut stmt = conn.prepare(
                "WITH RECURSIVE curriculum_scope(id) AS (
                   SELECT ?1
                   UNION ALL
                   SELECT child.id FROM k1_curriculum_nodes child
                   JOIN curriculum_scope parent ON child.parent_id=parent.id
                   WHERE child.state='active'
                 ),
                 knowledge_scope(id,public_id) AS (
                   SELECT k.id,k.public_id FROM k1_knowledge_nodes k
                   WHERE k.knowledge_map_id=?2 AND k.state='active'
                     AND k.curriculum_node_id IN (SELECT id FROM curriculum_scope)
                   UNION
                   SELECT child.id,child.public_id FROM k1_knowledge_nodes child
                   JOIN knowledge_scope parent ON child.parent_id=parent.id
                   WHERE child.state='active'
                 )
                 SELECT public_id FROM knowledge_scope ORDER BY public_id",
            )?;
            let node_ids = stmt
                .query_map((row.0, row.4), |node| node.get::<_, String>(0))?
                .collect::<rusqlite::Result<BTreeSet<_>>>()?;
            let map_version = format!("{}:r{}", row.5, row.6);
            Ok(ResolvedProfileScope {
                view: ProfileScopeSelectionView {
                    selector_kind: "curriculum_node".into(),
                    selector_public_id: Some(row.1.clone()),
                    selector_key: format!("curriculum_node:{}", row.1),
                    title: row.3,
                    path: format!("{} / {}", row.8, path),
                    node_type: Some(row.2),
                    knowledge_map_public_id: Some(row.5.clone()),
                    knowledge_map_version: Some(map_version.clone()),
                    textbook_edition_public_id: Some(row.7),
                    textbook_title: Some(row.8),
                    knowledge_node_count: node_ids.len() as i64,
                },
                knowledge_node_public_ids: Some(node_ids),
                map_ids: BTreeMap::from([(row.4, map_version)]),
            })
        }
        _ => Err(CoreError::Invalid(
            "教材范围类型只支持自动范围、本册或课程节点".into(),
        )),
    }
}

pub fn list_profile_scope_options(conn: &Connection) -> CoreResult<Vec<ProfileScopeOption>> {
    let automatic = auto_scope_selection().view;
    let mut options = vec![ProfileScopeOption {
        selector_kind: automatic.selector_kind,
        selector_public_id: automatic.selector_public_id,
        selector_key: automatic.selector_key,
        label: automatic.title,
        detail: automatic.path,
        node_type: automatic.node_type,
        knowledge_map_public_id: automatic.knowledge_map_public_id,
        textbook_edition_public_id: automatic.textbook_edition_public_id,
        knowledge_node_count: automatic.knowledge_node_count,
    }];
    let mut stmt = conn.prepare(
        "SELECT m.public_id FROM k1_knowledge_maps m
         JOIN k1_textbook_editions e ON e.id=m.textbook_edition_id
         WHERE m.state='confirmed' AND e.state='active'
         ORDER BY e.title,m.revision DESC",
    )?;
    let maps = stmt
        .query_map([], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for map_public_id in maps {
        let map_input = ProfileScopeSelectionInput {
            selector_kind: "knowledge_map",
            selector_public_id: Some(&map_public_id),
        };
        let map = resolve_profile_scope(conn, Some(&map_input))?.view;
        options.push(ProfileScopeOption {
            selector_kind: map.selector_kind,
            selector_public_id: map.selector_public_id,
            selector_key: map.selector_key,
            label: map.title,
            detail: "本册全部知识节点".into(),
            node_type: map.node_type,
            knowledge_map_public_id: map.knowledge_map_public_id.clone(),
            textbook_edition_public_id: map.textbook_edition_public_id,
            knowledge_node_count: map.knowledge_node_count,
        });
        let mut curriculum = conn.prepare(
            "SELECT c.public_id FROM k1_curriculum_nodes c
             JOIN k1_knowledge_maps m ON m.id=c.knowledge_map_id
             WHERE m.public_id=?1 AND c.state='active'
             ORDER BY c.order_index,c.id",
        )?;
        let curriculum_ids = curriculum
            .query_map([&map_public_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for curriculum_public_id in curriculum_ids {
            let node_input = ProfileScopeSelectionInput {
                selector_kind: "curriculum_node",
                selector_public_id: Some(&curriculum_public_id),
            };
            let node = resolve_profile_scope(conn, Some(&node_input))?.view;
            options.push(ProfileScopeOption {
                selector_kind: node.selector_kind,
                selector_public_id: node.selector_public_id,
                selector_key: node.selector_key,
                label: node.title,
                detail: node.path,
                node_type: node.node_type,
                knowledge_map_public_id: node.knowledge_map_public_id,
                textbook_edition_public_id: node.textbook_edition_public_id,
                knowledge_node_count: node.knowledge_node_count,
            });
        }
    }
    Ok(options)
}

pub fn resolve_profile_scope_selection(
    conn: &Connection,
    selection: &ProfileScopeSelectionInput<'_>,
) -> CoreResult<ProfileScopeSelectionView> {
    Ok(resolve_profile_scope(conn, Some(selection))?.view)
}

fn validate_scope(
    conn: &Connection,
    scope: &StudentProfileScope<'_>,
    selection: Option<&ProfileScopeSelectionInput<'_>>,
) -> CoreResult<ValidatedScope> {
    let range_start = parse_date(scope.range_start, "开始日期")?;
    let range_end = parse_date(scope.range_end, "结束日期")?;
    if range_start > range_end {
        return Err(CoreError::Invalid("开始日期不能晚于结束日期".into()));
    }
    if (range_end - range_start).num_days() > MAX_RANGE_DAYS {
        return Err(CoreError::Invalid("单次图谱范围不能超过 366 天".into()));
    }
    let student = conn
        .query_row(
            "SELECT id,class_id,student_no,name
             FROM students
             WHERE id=?1 AND class_id=?2 AND enabled=1",
            (scope.student_id, scope.class_id),
            |row| {
                Ok(ProfileStudent {
                    id: row.get(0)?,
                    class_id: row.get(1)?,
                    student_no: row.get(2)?,
                    name: row.get(3)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("学生不属于当前班级或已停用".into()))?;
    Ok(ValidatedScope {
        student,
        range_start,
        range_end,
        selection: resolve_profile_scope(conn, selection)?,
    })
}

fn active_policy(conn: &Connection) -> CoreResult<(i64, ProfilePolicy)> {
    conn.query_row(
        "SELECT id,public_id,revision,min_independent_groups,min_distinct_dates,
                min_distinct_sources,needs_support_below,stable_at_or_above,freshness_days
         FROM profile_policy_versions
         WHERE policy_key='default' AND state='active'
         ORDER BY revision DESC LIMIT 1",
        [],
        |row| {
            Ok((
                row.get(0)?,
                ProfilePolicy {
                    public_id: row.get(1)?,
                    revision: row.get(2)?,
                    min_independent_groups: row.get(3)?,
                    min_distinct_dates: row.get(4)?,
                    min_distinct_sources: row.get(5)?,
                    needs_support_below: row.get(6)?,
                    stable_at_or_above: row.get(7)?,
                    freshness_days: row.get(8)?,
                },
            ))
        },
    )
    .map_err(Into::into)
}

fn shanghai_date(value: &str) -> CoreResult<NaiveDate> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| CoreError::Invalid("学习证据时间不是 RFC3339".into()))?;
    Ok(time::shanghai_business_date_at(parsed.with_timezone(&Utc)))
}

fn map_public_id(version: &str) -> &str {
    version.split(":r").next().unwrap_or(version)
}

fn supports_formal_target(
    source_module: &str,
    source_type: &str,
    source_ref_type: &str,
    evidence_kind: &str,
    target_type: &str,
) -> bool {
    match source_module {
        "grading" | "correction" => {
            evidence_kind == "accuracy"
                && matches!(
                    (source_type, source_ref_type),
                    ("objective_question", "assessment_item")
                        | ("fill_blank_slot", "answer_slot")
                        | ("question_rubric_point", "rubric_point")
                        | ("dictation_rubric_point", "rubric_point")
                )
                && matches!(target_type, "knowledge_node" | "ability_dimension")
        }
        "recitation" => {
            source_type == "recitation_rubric_point"
                && source_ref_type == "recitation_point_review"
                && matches!(evidence_kind, "coverage" | "accuracy" | "contradiction")
                && target_type == "knowledge_node"
        }
        _ => false,
    }
}

fn supports_recitation_history(
    source_type: &str,
    source_ref_type: &str,
    evidence_kind: &str,
) -> bool {
    matches!(
        (source_type, source_ref_type, evidence_kind),
        ("recitation_overall", "recitation_submission", "accuracy")
            | ("recitation_fluency", "recitation_submission", "fluency")
            | (
                "recitation_retention",
                "recitation_retention_window",
                "retention"
            )
    )
}

type LoadedEvidenceTargets = (
    Vec<EvidenceTarget>,
    BTreeMap<i64, String>,
    BTreeSet<String>,
    i64,
    i64,
);

fn load_evidence_targets(
    conn: &Connection,
    validated: &ValidatedScope,
) -> CoreResult<LoadedEvidenceTargets> {
    let mut stmt = conn.prepare(
        "SELECT id,public_id,source_module,source_type,source_ref_type,source_ref_id,
                decision_ref_type,decision_ref_id,decision_revision,knowledge_node_id,
                ability_dimension_id,evidence_kind,value,confirmation_level,evidence_quality,
                assessment_context,occurred_at,knowledge_map_version
         FROM learning_evidence
         WHERE student_id=?1 AND state='active'
           AND confirmation_level IN ('teacher_accepted','teacher_corrected')
         ORDER BY occurred_at,id",
    )?;
    let rows = stmt.query_map([validated.student.id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, Option<String>>(6)?,
            row.get::<_, Option<String>>(7)?,
            row.get::<_, Option<i64>>(8)?,
            row.get::<_, Option<String>>(9)?,
            row.get::<_, Option<String>>(10)?,
            row.get::<_, String>(11)?,
            row.get::<_, f64>(12)?,
            row.get::<_, String>(13)?,
            row.get::<_, f64>(14)?,
            row.get::<_, String>(15)?,
            row.get::<_, String>(16)?,
            row.get::<_, String>(17)?,
        ))
    })?;
    let mut targets = Vec::new();
    let mut map_ids = validated.selection.map_ids.clone();
    let mut map_versions = validated
        .selection
        .map_ids
        .values()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut unsupported_evidence_ids = HashSet::new();
    let mut out_of_scope_evidence_ids = HashSet::new();
    for row in rows {
        let (
            id,
            public_id,
            source_module,
            source_type,
            source_ref_type,
            source_ref_id,
            _decision_ref_type,
            decision_ref_id,
            decision_revision,
            knowledge_node_id,
            ability_dimension_id,
            evidence_kind,
            value,
            _confirmation_level,
            evidence_quality,
            assessment_context,
            occurred_at,
            knowledge_map_version,
        ) = row?;
        let occurred_date = shanghai_date(&occurred_at)?;
        if occurred_date < validated.range_start || occurred_date > validated.range_end {
            continue;
        }
        let mut knowledge_supported = knowledge_node_id.is_some()
            && supports_formal_target(
                &source_module,
                &source_type,
                &source_ref_type,
                &evidence_kind,
                "knowledge_node",
            );
        let mut ability_supported = ability_dimension_id.is_some()
            && supports_formal_target(
                &source_module,
                &source_type,
                &source_ref_type,
                &evidence_kind,
                "ability_dimension",
            );
        if (knowledge_node_id.is_some() && !knowledge_supported)
            || (ability_dimension_id.is_some() && !ability_supported)
        {
            unsupported_evidence_ids.insert(id);
        }
        if let Some(selected_nodes) = &validated.selection.knowledge_node_public_ids {
            let selected = knowledge_node_id
                .as_ref()
                .is_some_and(|node| selected_nodes.contains(node));
            if (knowledge_supported || ability_supported) && !selected {
                out_of_scope_evidence_ids.insert(id);
            }
            knowledge_supported &= selected;
            ability_supported &= selected;
        }
        if !knowledge_supported && !ability_supported {
            continue;
        }
        let map_id = conn
            .query_row(
                "SELECT id FROM k1_knowledge_maps
                 WHERE public_id=?1 AND state='confirmed'",
                [map_public_id(&knowledge_map_version)],
                |map_row| map_row.get::<_, i64>(0),
            )
            .optional()?;
        let Some(map_id) = map_id else {
            continue;
        };
        if !validated.selection.map_ids.is_empty()
            && !validated.selection.map_ids.contains_key(&map_id)
        {
            out_of_scope_evidence_ids.insert(id);
            continue;
        }
        map_ids.insert(map_id, knowledge_map_version.clone());
        map_versions.insert(knowledge_map_version.clone());
        if knowledge_supported {
            let node = knowledge_node_id.expect("supported knowledge target must exist");
            let title = conn
                .query_row(
                    "SELECT title FROM k1_knowledge_nodes
                     WHERE public_id=?1 AND knowledge_map_id=?2 AND state='active'",
                    (&node, map_id),
                    |target_row| target_row.get::<_, String>(0),
                )
                .optional()?;
            if let Some(title) = title {
                targets.push(EvidenceTarget {
                    id,
                    public_id: public_id.clone(),
                    source_module: source_module.clone(),
                    source_type: source_type.clone(),
                    source_ref_type: source_ref_type.clone(),
                    source_ref_id: source_ref_id.clone(),
                    decision_ref_id: decision_ref_id.clone(),
                    decision_revision,
                    target_type: "knowledge_node".into(),
                    target_public_id: node,
                    target_title: title,
                    evidence_kind: evidence_kind.clone(),
                    value,
                    evidence_quality,
                    assessment_context: assessment_context.clone(),
                    occurred_at: occurred_at.clone(),
                    occurred_date,
                    knowledge_map_version: knowledge_map_version.clone(),
                });
            }
        }
        if ability_supported {
            let dimension = ability_dimension_id.expect("supported ability target must exist");
            let title = conn
                .query_row(
                    "SELECT title FROM k1_ability_dimensions
                     WHERE public_id=?1 AND state='active'",
                    [&dimension],
                    |target_row| target_row.get::<_, String>(0),
                )
                .optional()?;
            if let Some(title) = title {
                targets.push(EvidenceTarget {
                    id,
                    public_id,
                    source_module,
                    source_type,
                    source_ref_type,
                    source_ref_id,
                    decision_ref_id,
                    decision_revision,
                    target_type: "ability_dimension".into(),
                    target_public_id: dimension,
                    target_title: title,
                    evidence_kind,
                    value,
                    evidence_quality,
                    assessment_context,
                    occurred_at,
                    occurred_date,
                    knowledge_map_version,
                });
            }
        }
    }
    Ok((
        targets,
        map_ids,
        map_versions,
        unsupported_evidence_ids.len() as i64,
        out_of_scope_evidence_ids.len() as i64,
    ))
}

type LoadedRecitationHistory = (Vec<RecitationEvidence>, i64);

fn load_recitation_history(
    conn: &Connection,
    validated: &ValidatedScope,
    selected_decisions: Option<&BTreeSet<(String, i64)>>,
) -> CoreResult<LoadedRecitationHistory> {
    let mut stmt = conn.prepare(
        "SELECT id,public_id,source_type,source_ref_type,source_ref_id,
                decision_ref_id,decision_revision,evidence_kind,value,evidence_quality,
                assessment_context,occurred_at
         FROM learning_evidence
         WHERE student_id=?1 AND source_module='recitation' AND state='active'
           AND confirmation_level='teacher_overall'
         ORDER BY occurred_at,id",
    )?;
    let rows = stmt.query_map([validated.student.id], |row| {
        Ok(RecitationEvidence {
            id: row.get(0)?,
            public_id: row.get(1)?,
            source_type: row.get(2)?,
            source_ref_type: row.get(3)?,
            source_ref_id: row.get(4)?,
            decision_ref_id: row.get(5)?,
            decision_revision: row.get(6)?,
            evidence_kind: row.get(7)?,
            value: row.get(8)?,
            evidence_quality: row.get(9)?,
            assessment_context: row.get(10)?,
            occurred_at: row.get(11)?,
        })
    })?;
    let mut history = Vec::new();
    let mut unsupported = 0_i64;
    for row in rows {
        let evidence = row?;
        let occurred_date = shanghai_date(&evidence.occurred_at)?;
        if occurred_date < validated.range_start || occurred_date > validated.range_end {
            continue;
        }
        if supports_recitation_history(
            &evidence.source_type,
            &evidence.source_ref_type,
            &evidence.evidence_kind,
        ) {
            if let Some(decisions) = selected_decisions {
                let selected = evidence
                    .decision_ref_id
                    .as_ref()
                    .zip(evidence.decision_revision)
                    .is_some_and(|(decision, revision)| {
                        decisions.contains(&(decision.clone(), revision))
                    });
                if !selected {
                    continue;
                }
            }
            history.push(evidence);
        } else {
            unsupported += 1;
        }
    }
    Ok((history, unsupported))
}

fn recitation_summary(evidence: &[RecitationEvidence]) -> ProfileRecitationSummary {
    let latest_value = |kind: &str| {
        evidence
            .iter()
            .rev()
            .find(|item| item.evidence_kind == kind)
            .map(|item| item.value)
    };
    ProfileRecitationSummary {
        overall_count: evidence
            .iter()
            .filter(|item| item.source_type == "recitation_overall")
            .count() as i64,
        fluency_count: evidence
            .iter()
            .filter(|item| item.source_type == "recitation_fluency")
            .count() as i64,
        retention_count: evidence
            .iter()
            .filter(|item| item.source_type == "recitation_retention")
            .count() as i64,
        latest_overall_value: latest_value("accuracy"),
        latest_fluency_value: latest_value("fluency"),
        latest_retention_value: latest_value("retention"),
        latest_at: evidence.last().map(|item| item.occurred_at.clone()),
        evidence: evidence
            .iter()
            .map(|item| ProfileRecitationEvidenceView {
                public_id: item.public_id.clone(),
                source_type: item.source_type.clone(),
                source_ref_type: item.source_ref_type.clone(),
                source_ref_id: item.source_ref_id.clone(),
                decision_ref_id: item.decision_ref_id.clone(),
                decision_revision: item.decision_revision,
                evidence_kind: item.evidence_kind.clone(),
                value: item.value,
                evidence_quality: item.evidence_quality,
                assessment_context: item.assessment_context.clone(),
                occurred_at: item.occurred_at.clone(),
            })
            .collect(),
    }
}

fn load_wrongbook_facts(
    conn: &Connection,
    validated: &ValidatedScope,
) -> CoreResult<Vec<ProfileWrongbookFactView>> {
    let mut facts =
        student_wrongbook_items(conn, validated.student.class_id, validated.student.id)?
            .into_iter()
            .filter_map(|item| match shanghai_date(&item.latest_response_at) {
                Ok(date) if date >= validated.range_start && date <= validated.range_end => {
                    if let Some(selected_nodes) = &validated.selection.knowledge_node_public_ids {
                        if !item
                            .knowledge_nodes
                            .iter()
                            .any(|node| selected_nodes.contains(&node.public_id))
                        {
                            return None;
                        }
                    }
                    Some(Ok(ProfileWrongbookFactView {
                        question_version_public_id: item.question_version_id,
                        question_type: item.question_type,
                        stem: item.stem,
                        status: item.status,
                        first_error_at: item.first_error_at,
                        last_error_at: item.last_error_at,
                        latest_response_at: item.latest_response_at,
                        published_response_count: item.published_response_count,
                        error_response_count: item.error_response_count,
                        repeated_error: item.repeated_error,
                        correction_status: item
                            .correction_assignment
                            .map(|assignment| assignment.status),
                        reinforcement_status: item
                            .reinforcement_assignment
                            .map(|assignment| assignment.status),
                        knowledge_nodes: item
                            .knowledge_nodes
                            .into_iter()
                            .map(|node| ProfileNamedReference {
                                public_id: node.public_id,
                                title: node.title,
                            })
                            .collect(),
                        ability_dimensions: item
                            .ability_dimensions
                            .into_iter()
                            .map(|node| ProfileNamedReference {
                                public_id: node.public_id,
                                title: node.title,
                            })
                            .collect(),
                    }))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<CoreResult<Vec<_>>>()?;
    facts.sort_by(|left, right| {
        right.latest_response_at.cmp(&left.latest_response_at).then(
            left.question_version_public_id
                .cmp(&right.question_version_public_id),
        )
    });
    Ok(facts)
}

fn wrongbook_summary(facts: &[ProfileWrongbookFactView]) -> ProfileWrongbookSummary {
    ProfileWrongbookSummary {
        fact_count: facts.len() as i64,
        needs_correction_count: facts
            .iter()
            .filter(|item| item.status == "needs_correction")
            .count() as i64,
        corrected_once_count: facts
            .iter()
            .filter(|item| item.status == "corrected_once")
            .count() as i64,
        rechecked_correct_count: facts
            .iter()
            .filter(|item| item.status == "rechecked_correct")
            .count() as i64,
        repeated_error_count: facts.iter().filter(|item| item.repeated_error).count() as i64,
        latest_response_at: facts
            .iter()
            .map(|item| item.latest_response_at.as_str())
            .max()
            .map(str::to_owned),
        note: "仅展示所选范围内仍属于 M3 当前事实的错题恢复状态；订正一次和再次答对不会直接替代节点掌握结论。".into(),
        facts: facts.to_vec(),
    }
}

fn load_scope_nodes(
    conn: &Connection,
    map_ids: &BTreeMap<i64, String>,
    selected_knowledge_nodes: Option<&BTreeSet<String>>,
) -> CoreResult<Vec<ScopeNode>> {
    let mut nodes = Vec::new();
    let mut subject_ids = BTreeSet::new();
    for map_id in map_ids.keys() {
        let subject_id: i64 = conn.query_row(
            "SELECT e.subject_id
             FROM k1_knowledge_maps m
             JOIN k1_textbook_editions e ON e.id=m.textbook_edition_id
             WHERE m.id=?1",
            [map_id],
            |row| row.get(0),
        )?;
        subject_ids.insert(subject_id);
        let mut stmt = conn.prepare(
            "SELECT public_id,title FROM k1_knowledge_nodes
             WHERE knowledge_map_id=?1 AND state='active'
             ORDER BY order_index,id",
        )?;
        let rows = stmt.query_map([map_id], |row| {
            Ok(ScopeNode {
                target_type: "knowledge_node".into(),
                public_id: row.get(0)?,
                title: row.get(1)?,
            })
        })?;
        for row in rows {
            let node = row?;
            if selected_knowledge_nodes
                .map(|selected| selected.contains(&node.public_id))
                .unwrap_or(true)
            {
                nodes.push(node);
            }
        }
    }
    for subject_id in subject_ids {
        let mut stmt = conn.prepare(
            "SELECT public_id,title FROM k1_ability_dimensions
             WHERE subject_id=?1 AND state='active'
             ORDER BY code,revision,id",
        )?;
        let rows = stmt.query_map([subject_id], |row| {
            Ok(ScopeNode {
                target_type: "ability_dimension".into(),
                public_id: row.get(0)?,
                title: row.get(1)?,
            })
        })?;
        for row in rows {
            nodes.push(row?);
        }
    }
    nodes.sort_by(|left, right| {
        left.target_type
            .cmp(&right.target_type)
            .then(left.title.cmp(&right.title))
            .then(left.public_id.cmp(&right.public_id))
    });
    nodes.dedup_by(|left, right| {
        left.target_type == right.target_type && left.public_id == right.public_id
    });
    Ok(nodes)
}

fn excluded_count(
    conn: &Connection,
    validated: &ValidatedScope,
    predicate: &str,
) -> CoreResult<i64> {
    let sql = format!(
        "SELECT COUNT(*) FROM learning_evidence
         WHERE student_id=?1 AND state='active'
           AND date(datetime(occurred_at,'+8 hours')) BETWEEN ?2 AND ?3
           AND ({predicate})"
    );
    Ok(conn.query_row(
        &sql,
        (
            validated.student.id,
            validated.range_start.format("%Y-%m-%d").to_string(),
            validated.range_end.format("%Y-%m-%d").to_string(),
        ),
        |row| row.get(0),
    )?)
}

fn compute_with_selection(
    conn: &Connection,
    scope: &StudentProfileScope<'_>,
    selection: Option<&ProfileScopeSelectionInput<'_>>,
) -> CoreResult<Computation> {
    let mut validated = validate_scope(conn, scope, selection)?;
    let (policy_id, policy) = active_policy(conn)?;
    let (targets, map_ids, map_versions, unsupported_formal_excluded, out_of_scope_excluded) =
        load_evidence_targets(conn, &validated)?;
    let selected_recitation_decisions =
        validated
            .selection
            .knowledge_node_public_ids
            .as_ref()
            .map(|_| {
                targets
                    .iter()
                    .filter(|item| item.source_module == "recitation")
                    .filter_map(|item| {
                        item.decision_ref_id
                            .as_ref()
                            .zip(item.decision_revision)
                            .map(|(decision, revision)| (decision.clone(), revision))
                    })
                    .collect::<BTreeSet<_>>()
            });
    let (recitation_evidence, unsupported_history_excluded) =
        load_recitation_history(conn, &validated, selected_recitation_decisions.as_ref())?;
    let wrongbook_facts = load_wrongbook_facts(conn, &validated)?;
    let scope_nodes = load_scope_nodes(
        conn,
        &map_ids,
        validated.selection.knowledge_node_public_ids.as_ref(),
    )?;
    if validated.selection.view.selector_kind == "auto_evidence_maps" {
        validated.selection.view.knowledge_node_count = scope_nodes
            .iter()
            .filter(|node| node.target_type == "knowledge_node")
            .count() as i64;
        if map_versions.len() == 1 {
            validated.selection.view.knowledge_map_version = map_versions.iter().next().cloned();
            validated.selection.view.knowledge_map_public_id = map_versions
                .iter()
                .next()
                .map(|version| map_public_id(version).to_owned());
        }
    }
    let mut by_target: HashMap<(String, String), Vec<EvidenceTarget>> = HashMap::new();
    for target in targets {
        by_target
            .entry((target.target_type.clone(), target.target_public_id.clone()))
            .or_default()
            .push(target);
    }
    let metrics = scope_nodes
        .iter()
        .map(|node| {
            compute_metric(
                node,
                by_target
                    .remove(&(node.target_type.clone(), node.public_id.clone()))
                    .unwrap_or_default(),
                &policy,
                validated.range_end,
            )
        })
        .collect::<Vec<_>>();
    let evidence_ids = metrics
        .iter()
        .flat_map(|metric| metric.evidence.iter().map(|(item, _, _)| item.id))
        .collect::<HashSet<_>>();
    let knowledge_metrics = metrics
        .iter()
        .filter(|metric| metric.target_type == "knowledge_node")
        .collect::<Vec<_>>();
    let ability_metrics = metrics
        .iter()
        .filter(|metric| metric.target_type == "ability_dimension")
        .collect::<Vec<_>>();
    let counts = ProfilePreviewCounts {
        mapped_formal_evidence: evidence_ids.len() as i64,
        knowledge_node_total: knowledge_metrics.len() as i64,
        knowledge_node_assessed: knowledge_metrics
            .iter()
            .filter(|metric| metric.evidence_count > 0)
            .count() as i64,
        knowledge_node_eligible: knowledge_metrics
            .iter()
            .filter(|metric| {
                matches!(
                    metric.status.as_str(),
                    "needs_support" | "developing" | "stable"
                )
            })
            .count() as i64,
        ability_node_total: ability_metrics.len() as i64,
        ability_node_assessed: ability_metrics
            .iter()
            .filter(|metric| metric.evidence_count > 0)
            .count() as i64,
        ability_node_eligible: ability_metrics
            .iter()
            .filter(|metric| {
                matches!(
                    metric.status.as_str(),
                    "needs_support" | "developing" | "stable"
                )
            })
            .count() as i64,
        machine_only_excluded: excluded_count(
            conn,
            &validated,
            "confirmation_level='machine_only'",
        )?,
        teacher_overall_excluded: recitation_evidence.len() as i64,
        unmapped_formal_excluded: excluded_count(
            conn,
            &validated,
            "confirmation_level IN ('teacher_accepted','teacher_corrected')
             AND knowledge_node_id IS NULL AND ability_dimension_id IS NULL",
        )?,
        unsupported_contract_excluded: unsupported_formal_excluded + unsupported_history_excluded,
        out_of_scope_excluded,
        referenced_knowledge_map_count: map_ids.len() as i64,
    };
    let mut watermark_rows = Vec::new();
    for metric in &metrics {
        for (item, _, _) in &metric.evidence {
            watermark_rows.push(EvidenceWatermarkRow {
                public_id: item.public_id.clone(),
                state: "active".into(),
                target_type: item.target_type.clone(),
                target_public_id: item.target_public_id.clone(),
                target_title: item.target_title.clone(),
                value_micros: (item.value * 1_000_000.0).round() as i64,
                quality_micros: (item.evidence_quality * 1_000_000.0).round() as i64,
                occurred_at: item.occurred_at.clone(),
                source_ref_type: item.source_ref_type.clone(),
                source_ref_id: item.source_ref_id.clone(),
                decision_ref_id: item.decision_ref_id.clone(),
                decision_revision: item.decision_revision,
                knowledge_map_version: item.knowledge_map_version.clone(),
            });
        }
    }
    watermark_rows.sort_by(|left, right| {
        left.public_id
            .cmp(&right.public_id)
            .then(left.target_type.cmp(&right.target_type))
            .then(left.target_public_id.cmp(&right.target_public_id))
    });
    let recitation_watermark_rows = recitation_evidence
        .iter()
        .map(|item| RecitationWatermarkRow {
            public_id: item.public_id.clone(),
            source_type: item.source_type.clone(),
            source_ref_type: item.source_ref_type.clone(),
            source_ref_id: item.source_ref_id.clone(),
            decision_ref_id: item.decision_ref_id.clone(),
            decision_revision: item.decision_revision,
            evidence_kind: item.evidence_kind.clone(),
            value_micros: (item.value * 1_000_000.0).round() as i64,
            quality_micros: (item.evidence_quality * 1_000_000.0).round() as i64,
            occurred_at: item.occurred_at.clone(),
        })
        .collect::<Vec<_>>();
    let scope_identity = scope_nodes
        .iter()
        .map(|node| {
            serde_json::json!({
                "target_type": node.target_type,
                "public_id": node.public_id,
                "title": node.title
            })
        })
        .collect::<Vec<_>>();
    let watermark_json = serde_json::json!({
        "schema_version": PROFILE_SCHEMA_VERSION,
        "rule_version": PROFILE_RULE_VERSION,
        "evidence": watermark_rows,
        "recitation_history": recitation_watermark_rows,
        "wrongbook_current_facts": &wrongbook_facts,
        "scope_nodes": scope_identity,
        "scope_selection": &validated.selection.view,
        "knowledge_map_versions": map_versions,
        "policy_public_id": policy.public_id,
        "policy_revision": policy.revision
    });
    let source_watermark = hashing::sha256_hex(
        serde_json::to_vec(&watermark_json)
            .map_err(|error| CoreError::Invalid(error.to_string()))?
            .as_slice(),
    );
    Ok(Computation {
        validated,
        policy_id,
        policy,
        metrics,
        counts,
        source_watermark,
        evidence_ids,
        recitation_evidence,
        wrongbook_facts,
        knowledge_map_versions: map_versions,
        calculated_at: time::utc_now_rfc3339(),
    })
}

#[cfg(test)]
fn compute(conn: &Connection, scope: &StudentProfileScope<'_>) -> CoreResult<Computation> {
    compute_with_selection(conn, scope, None)
}

pub fn preview_student_profile(
    conn: &Connection,
    scope: &StudentProfileScope<'_>,
) -> CoreResult<StudentProfilePreview> {
    preview_student_profile_with_selection(conn, scope, None)
}

fn preview_student_profile_with_selection(
    conn: &Connection,
    scope: &StudentProfileScope<'_>,
    selection: Option<&ProfileScopeSelectionInput<'_>>,
) -> CoreResult<StudentProfilePreview> {
    let computation = compute_with_selection(conn, scope, selection)?;
    let can_generate = computation.counts.mapped_formal_evidence > 0;
    Ok(StudentProfilePreview {
        schema_version: PROFILE_SCHEMA_VERSION,
        rule_version: PROFILE_RULE_VERSION.into(),
        calculated_at: computation.calculated_at,
        student: computation.validated.student,
        range_start: computation.validated.range_start.to_string(),
        range_end: computation.validated.range_end.to_string(),
        scope_selection: computation.validated.selection.view,
        policy: computation.policy,
        counts: computation.counts,
        recitation_summary: recitation_summary(&computation.recitation_evidence),
        wrongbook_summary: wrongbook_summary(&computation.wrongbook_facts),
        source_watermark: computation.source_watermark,
        can_generate,
        blocker: (!can_generate)
            .then(|| "所选范围还没有老师确认、已发布且知识/能力链接明确的逐点证据。".into()),
        scope_note: "教材范围已冻结到本次预览；未覆盖节点保留为“未评估”，范围外证据不参与计算。".into(),
        evidence_note: "M1 总体、流畅度和保持度只显示为背诵内容历史；M3 订正按低权重正式证据进入原知识/能力节点，当前错题恢复状态另行展示，不直接等同掌握。".into(),
    })
}

pub fn preview_scoped_student_profile(
    conn: &Connection,
    scope: &StudentProfileScope<'_>,
    selection: &ProfileScopeSelectionInput<'_>,
) -> CoreResult<StudentProfilePreview> {
    preview_student_profile_with_selection(conn, scope, Some(selection))
}

fn payload_hash(computation: &Computation) -> CoreResult<String> {
    let metrics = computation
        .metrics
        .iter()
        .map(|metric| {
            let mut evidence_public_ids = metric
                .evidence
                .iter()
                .map(|(item, _, _)| item.public_id.as_str())
                .collect::<Vec<_>>();
            evidence_public_ids.sort_unstable();
            MetricHashPayload {
                target_type: &metric.target_type,
                target_public_id: &metric.target_public_id,
                mastery_micros: metric
                    .mastery_score
                    .map(|value| (value * 1_000_000.0).round() as i64),
                status: &metric.status,
                confidence_level: &metric.confidence_level,
                freshness: &metric.freshness,
                evidence_public_ids,
            }
        })
        .collect();
    let payload = SnapshotHashPayload {
        schema_version: PROFILE_SCHEMA_VERSION,
        rule_version: PROFILE_RULE_VERSION,
        student_id: computation.validated.student.id,
        class_id: computation.validated.student.class_id,
        range_start: computation.validated.range_start.to_string(),
        range_end: computation.validated.range_end.to_string(),
        scope_selector_key: &computation.validated.selection.view.selector_key,
        policy_public_id: &computation.policy.public_id,
        policy_revision: computation.policy.revision,
        source_watermark: &computation.source_watermark,
        metrics,
    };
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
        "SELECT COALESCE(MAX(revision),0)+1 FROM profile_snapshots WHERE student_id=?1",
        [computation.validated.student.id],
        |row| row.get(0),
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let payload_sha256 = payload_hash(computation)?;
    let scope_json = serde_json::json!({
        "schema_version": PROFILE_SCHEMA_VERSION,
        "scope_kind": "confirmed_evidence_maps",
        "selection": &computation.validated.selection.view,
        "knowledge_map_versions": computation.knowledge_map_versions,
        "range_start": computation.validated.range_start.to_string(),
        "range_end": computation.validated.range_end.to_string()
    })
    .to_string();
    let source_config_json = serde_json::json!({
        "schema_version": PROFILE_SCHEMA_VERSION,
        "rule_version": PROFILE_RULE_VERSION,
        "formal_confirmation_levels": ["teacher_accepted","teacher_corrected"],
        "formal_adapters": {
            "grading": ["objective_question","fill_blank_slot","question_rubric_point","dictation_rubric_point"],
            "correction": ["objective_question","fill_blank_slot","question_rubric_point","dictation_rubric_point"],
            "recitation": ["recitation_rubric_point:knowledge_only"]
        },
        "recitation_history": ["recitation_overall","recitation_fluency","recitation_retention"],
        "wrongbook_history": "m3_current_facts_latest_response_in_scope",
        "scope_selector_key": computation.validated.selection.view.selector_key,
        "active_only": true,
        "same_source_same_day": "collapse_minimum",
        "context_weights": {
            "closed_book": 1.0,
            "in_class": 0.9,
            "homework": 0.8,
            "open_book": 0.6,
            "correction": 0.6
        }
    })
    .to_string();
    tx.execute(
        "INSERT INTO profile_snapshots
          (public_id,student_id,class_id,revision,range_start,range_end,scope_kind,
           scope_json,source_config_json,evidence_cutoff_at,policy_id,policy_revision,
           source_watermark,evidence_count,knowledge_node_total,knowledge_node_assessed,
           knowledge_node_eligible,ability_node_total,ability_node_assessed,
           ability_node_eligible,state,payload_sha256,generated_by,generated_at,
           confirmed_by,confirmed_at)
         VALUES
          (?1,?2,?3,?4,?5,?6,'confirmed_evidence_maps',?7,?8,?9,?10,?11,?12,
           ?13,?14,?15,?16,?17,?18,?19,'teacher_confirmed',?20,?21,?22,?21,?22)",
        params![
            public_id,
            computation.validated.student.id,
            computation.validated.student.class_id,
            revision,
            computation.validated.range_start.to_string(),
            computation.validated.range_end.to_string(),
            scope_json,
            source_config_json,
            computation.calculated_at,
            computation.policy_id,
            computation.policy.revision,
            computation.source_watermark,
            computation.evidence_ids.len() as i64,
            computation.counts.knowledge_node_total,
            computation.counts.knowledge_node_assessed,
            computation.counts.knowledge_node_eligible,
            computation.counts.ability_node_total,
            computation.counts.ability_node_assessed,
            computation.counts.ability_node_eligible,
            payload_sha256,
            confirmed_by.trim(),
            now,
        ],
    )?;
    let snapshot_id = tx.last_insert_rowid();
    for metric in &computation.metrics {
        let metric_public_id = ids::new_public_id();
        tx.execute(
            "INSERT INTO profile_node_metrics
              (public_id,snapshot_id,target_type,target_public_id,target_title,mastery_score,
               status,confidence_level,freshness,evidence_count,independent_group_count,
               distinct_date_count,distinct_source_count,last_evidence_at,
               source_breakdown_json,explanation)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![
                metric_public_id,
                snapshot_id,
                metric.target_type,
                metric.target_public_id,
                metric.target_title,
                metric.mastery_score,
                metric.status,
                metric.confidence_level,
                metric.freshness,
                metric.evidence_count,
                metric.independent_group_count,
                metric.distinct_date_count,
                metric.distinct_source_count,
                metric.last_evidence_at,
                serde_json::to_string(&metric.source_breakdown)
                    .map_err(|error| CoreError::Invalid(error.to_string()))?,
                metric.explanation,
            ],
        )?;
        let metric_id = tx.last_insert_rowid();
        for (item, group_key, weight) in &metric.evidence {
            tx.execute(
                "INSERT INTO profile_evidence_links
                  (snapshot_id,node_metric_id,learning_evidence_id,
                   independence_group_key,effective_weight)
                 VALUES (?1,?2,?3,?4,?5)",
                params![snapshot_id, metric_id, item.id, group_key, weight],
            )?;
        }
    }
    for item in &computation.recitation_evidence {
        tx.execute(
            "INSERT INTO profile_recitation_evidence_links
              (snapshot_id,learning_evidence_id)
             VALUES (?1,?2)",
            params![snapshot_id, item.id],
        )?;
    }
    for fact in &computation.wrongbook_facts {
        tx.execute(
            "INSERT INTO profile_wrongbook_fact_links
              (snapshot_id,question_version_public_id,question_type,stem,status,
               first_error_at,last_error_at,latest_response_at,published_response_count,
               error_response_count,repeated_error,correction_status,reinforcement_status,
               knowledge_nodes_json,ability_dimensions_json)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                snapshot_id,
                fact.question_version_public_id,
                fact.question_type,
                fact.stem,
                fact.status,
                fact.first_error_at,
                fact.last_error_at,
                fact.latest_response_at,
                fact.published_response_count,
                fact.error_response_count,
                i64::from(fact.repeated_error),
                fact.correction_status,
                fact.reinforcement_status,
                serde_json::to_string(&fact.knowledge_nodes)
                    .map_err(|error| CoreError::Invalid(error.to_string()))?,
                serde_json::to_string(&fact.ability_dimensions)
                    .map_err(|error| CoreError::Invalid(error.to_string()))?
            ],
        )?;
    }
    Ok((public_id, revision, now))
}

pub fn generate_student_profile(
    conn: &mut Connection,
    input: &GenerateStudentProfileInput<'_>,
) -> CoreResult<StudentProfileSnapshot> {
    generate_student_profile_with_selection(conn, &input.scope, None, input.confirmed_by)
}

fn generate_student_profile_with_selection(
    conn: &mut Connection,
    scope: &StudentProfileScope<'_>,
    selection: Option<&ProfileScopeSelectionInput<'_>>,
    confirmed_by: &str,
) -> CoreResult<StudentProfileSnapshot> {
    required(confirmed_by, "确认人")?;
    let tx = conn.transaction()?;
    let computation = compute_with_selection(&tx, scope, selection)?;
    if computation.counts.mapped_formal_evidence == 0 {
        return Err(CoreError::Invalid(
            "所选范围没有可生成正式快照的逐点证据".into(),
        ));
    }
    let (public_id, revision, generated_at) = insert_snapshot(&tx, &computation, confirmed_by)?;
    let event_payload = serde_json::json!({
        "schema_version": PROFILE_SCHEMA_VERSION,
        "snapshot_public_id": public_id,
        "student_id": computation.validated.student.id,
        "class_id": computation.validated.student.class_id,
        "revision": revision,
        "range_start": computation.validated.range_start.to_string(),
        "range_end": computation.validated.range_end.to_string(),
        "scope_selector_key": computation.validated.selection.view.selector_key,
        "evidence_count": computation.evidence_ids.len()
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("profile:outbox:snapshot:{public_id}"),
            event_type: "student_profile_snapshot_created",
            event_version: 1,
            aggregate_type: "profile_snapshot",
            aggregate_id: &public_id,
            aggregate_revision: revision,
            payload_json: &event_payload,
            occurred_at: &generated_at,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("profile:audit:snapshot:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "profile.snapshot.generated",
            object_type: "profile_snapshot",
            object_id: &public_id,
            object_revision: Some(revision),
            note: Some("老师确认范围后生成只读学生学习掌握快照"),
            meta_json: Some(&event_payload),
            occurred_at: &generated_at,
        },
    )?;
    tx.commit()?;
    get_student_profile(conn, &public_id)?
        .ok_or_else(|| CoreError::Db("学习掌握快照写入后无法读取".into()))
}

pub fn generate_scoped_student_profile(
    conn: &mut Connection,
    input: &GenerateScopedStudentProfileInput<'_>,
) -> CoreResult<StudentProfileSnapshot> {
    generate_student_profile_with_selection(
        conn,
        &input.scope,
        Some(&input.selection),
        input.confirmed_by,
    )
}

fn load_metric_evidence(
    conn: &Connection,
    snapshot_id: i64,
    metric_id: i64,
) -> CoreResult<Vec<ProfileEvidenceView>> {
    let mut stmt = conn.prepare(
        "SELECT e.public_id,e.source_module,e.source_type,e.source_ref_type,e.source_ref_id,
                e.decision_ref_type,e.decision_ref_id,e.decision_revision,e.evidence_kind,
                e.value,e.evidence_quality,e.assessment_context,e.confirmation_level,
                e.occurred_at,l.independence_group_key,l.effective_weight
         FROM profile_evidence_links l
         JOIN learning_evidence e ON e.id=l.learning_evidence_id
         WHERE l.snapshot_id=?1 AND l.node_metric_id=?2
         ORDER BY e.occurred_at,e.id",
    )?;
    let rows = stmt.query_map((snapshot_id, metric_id), |row| {
        Ok(ProfileEvidenceView {
            public_id: row.get(0)?,
            source_module: row.get(1)?,
            source_type: row.get(2)?,
            source_ref_type: row.get(3)?,
            source_ref_id: row.get(4)?,
            decision_ref_type: row.get(5)?,
            decision_ref_id: row.get(6)?,
            decision_revision: row.get(7)?,
            evidence_kind: row.get(8)?,
            value: row.get(9)?,
            evidence_quality: row.get(10)?,
            assessment_context: row.get(11)?,
            confirmation_level: row.get(12)?,
            occurred_at: row.get(13)?,
            independence_group_key: row.get(14)?,
            effective_weight: row.get(15)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_recitation_summary(
    conn: &Connection,
    snapshot_id: i64,
) -> CoreResult<ProfileRecitationSummary> {
    let mut stmt = conn.prepare(
        "SELECT e.id,e.public_id,e.source_type,e.source_ref_type,e.source_ref_id,
                e.decision_ref_id,e.decision_revision,e.evidence_kind,e.value,
                e.evidence_quality,e.assessment_context,e.occurred_at
         FROM profile_recitation_evidence_links l
         JOIN learning_evidence e ON e.id=l.learning_evidence_id
         WHERE l.snapshot_id=?1
         ORDER BY e.occurred_at,e.id",
    )?;
    let rows = stmt.query_map([snapshot_id], |row| {
        Ok(RecitationEvidence {
            id: row.get(0)?,
            public_id: row.get(1)?,
            source_type: row.get(2)?,
            source_ref_type: row.get(3)?,
            source_ref_id: row.get(4)?,
            decision_ref_id: row.get(5)?,
            decision_revision: row.get(6)?,
            evidence_kind: row.get(7)?,
            value: row.get(8)?,
            evidence_quality: row.get(9)?,
            assessment_context: row.get(10)?,
            occurred_at: row.get(11)?,
        })
    })?;
    let evidence = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(recitation_summary(&evidence))
}

fn load_wrongbook_summary(
    conn: &Connection,
    snapshot_id: i64,
) -> CoreResult<ProfileWrongbookSummary> {
    let mut statement = conn.prepare(
        "SELECT question_version_public_id,question_type,stem,status,first_error_at,
                last_error_at,latest_response_at,published_response_count,
                error_response_count,repeated_error,correction_status,reinforcement_status,
                knowledge_nodes_json,ability_dimensions_json
         FROM profile_wrongbook_fact_links
         WHERE snapshot_id=?1
         ORDER BY latest_response_at DESC,question_version_public_id",
    )?;
    let rows = statement.query_map([snapshot_id], |row| {
        let knowledge_json = row.get::<_, String>(12)?;
        let ability_json = row.get::<_, String>(13)?;
        Ok(ProfileWrongbookFactView {
            question_version_public_id: row.get(0)?,
            question_type: row.get(1)?,
            stem: row.get(2)?,
            status: row.get(3)?,
            first_error_at: row.get(4)?,
            last_error_at: row.get(5)?,
            latest_response_at: row.get(6)?,
            published_response_count: row.get(7)?,
            error_response_count: row.get(8)?,
            repeated_error: row.get::<_, i64>(9)? == 1,
            correction_status: row.get(10)?,
            reinforcement_status: row.get(11)?,
            knowledge_nodes: serde_json::from_str(&knowledge_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    12,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
            ability_dimensions: serde_json::from_str(&ability_json).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    13,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
        })
    })?;
    let facts = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(wrongbook_summary(&facts))
}

fn load_metrics(
    conn: &Connection,
    snapshot_id: i64,
) -> CoreResult<(Vec<ProfileNodeMetric>, Vec<ProfileNodeMetric>)> {
    let mut stmt = conn.prepare(
        "SELECT id,public_id,target_type,target_public_id,target_title,mastery_score,
                status,confidence_level,freshness,evidence_count,independent_group_count,
                distinct_date_count,distinct_source_count,last_evidence_at,
                source_breakdown_json,explanation
         FROM profile_node_metrics
         WHERE snapshot_id=?1
         ORDER BY target_type,target_title,target_public_id",
    )?;
    let rows = stmt.query_map([snapshot_id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            ProfileNodeMetric {
                public_id: row.get(1)?,
                target_type: row.get(2)?,
                target_public_id: row.get(3)?,
                target_title: row.get(4)?,
                mastery_score: row.get(5)?,
                status: row.get(6)?,
                confidence_level: row.get(7)?,
                freshness: row.get(8)?,
                evidence_count: row.get(9)?,
                independent_group_count: row.get(10)?,
                distinct_date_count: row.get(11)?,
                distinct_source_count: row.get(12)?,
                last_evidence_at: row.get(13)?,
                source_breakdown: serde_json::from_str(&row.get::<_, String>(14)?).map_err(
                    |error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            14,
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    },
                )?,
                explanation: row.get(15)?,
                evidence: Vec::new(),
            },
        ))
    })?;
    let mut knowledge = Vec::new();
    let mut ability = Vec::new();
    for row in rows {
        let (metric_id, mut metric) = row?;
        metric.evidence = load_metric_evidence(conn, snapshot_id, metric_id)?;
        if metric.target_type == "knowledge_node" {
            knowledge.push(metric);
        } else {
            ability.push(metric);
        }
    }
    Ok((knowledge, ability))
}

fn assessed_count(metrics: &[ProfileNodeMetric]) -> i64 {
    metrics
        .iter()
        .filter(|metric| metric.evidence_count > 0)
        .count() as i64
}

fn node_status_count(
    knowledge: &[ProfileNodeMetric],
    ability: &[ProfileNodeMetric],
    status: &str,
) -> i64 {
    knowledge
        .iter()
        .chain(ability)
        .filter(|metric| metric.status == status)
        .count() as i64
}

struct StudentTrendInput<'a> {
    snapshot_id: i64,
    student_id: i64,
    revision: i64,
    range_start: &'a str,
    range_end: &'a str,
    policy_public_id: &'a str,
    source_config_json: &'a str,
    knowledge_metrics: &'a [ProfileNodeMetric],
    ability_metrics: &'a [ProfileNodeMetric],
}

fn load_student_trend(
    conn: &Connection,
    input: &StudentTrendInput<'_>,
) -> CoreResult<StudentProfileTrend> {
    let knowledge_current = assessed_count(input.knowledge_metrics);
    let ability_current = assessed_count(input.ability_metrics);
    let needs_support_current = node_status_count(
        input.knowledge_metrics,
        input.ability_metrics,
        "needs_support",
    );
    let stable_current =
        node_status_count(input.knowledge_metrics, input.ability_metrics, "stable");
    let previous = conn
        .query_row(
            "SELECT snapshot.id,snapshot.public_id,snapshot.revision,snapshot.generated_at
             FROM profile_snapshots snapshot
             JOIN profile_policy_versions policy ON policy.id=snapshot.policy_id
             WHERE snapshot.student_id=?1 AND snapshot.revision<?2
               AND snapshot.range_start=?3 AND snapshot.range_end=?4
               AND policy.public_id=?5 AND snapshot.source_config_json=?6
             ORDER BY snapshot.revision DESC LIMIT 1",
            params![
                input.student_id,
                input.revision,
                input.range_start,
                input.range_end,
                input.policy_public_id,
                input.source_config_json
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((previous_id, previous_public_id, previous_revision, previous_generated_at)) =
        previous
    else {
        return Ok(StudentProfileTrend {
            comparison_status: "no_comparable_baseline".into(),
            comparison_kind: None,
            previous_snapshot_public_id: None,
            previous_revision: None,
            previous_generated_at: None,
            knowledge_assessed_before: None,
            knowledge_assessed_current: knowledge_current,
            knowledge_assessed_delta: None,
            ability_assessed_before: None,
            ability_assessed_current: ability_current,
            ability_assessed_delta: None,
            needs_support_before: None,
            needs_support_current,
            needs_support_delta: None,
            stable_before: None,
            stable_current,
            stable_delta: None,
            changed_nodes: Vec::new(),
            note: "暂无同学生、同日期范围、同策略且同证据适配契约的上一版快照；不跨口径拼接趋势。"
                .into(),
        });
    };
    if previous_id == input.snapshot_id {
        return Err(CoreError::Db("个人趋势基线不能指向当前快照".into()));
    }
    let (previous_knowledge, previous_ability) = load_metrics(conn, previous_id)?;
    let previous_knowledge_assessed = assessed_count(&previous_knowledge);
    let previous_ability_assessed = assessed_count(&previous_ability);
    let previous_needs_support =
        node_status_count(&previous_knowledge, &previous_ability, "needs_support");
    let previous_stable = node_status_count(&previous_knowledge, &previous_ability, "stable");
    let mut previous_by_target = previous_knowledge
        .iter()
        .chain(&previous_ability)
        .map(|metric| {
            (
                (metric.target_type.clone(), metric.target_public_id.clone()),
                metric,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut changed_nodes = Vec::new();
    for current in input.knowledge_metrics.iter().chain(input.ability_metrics) {
        let Some(previous) = previous_by_target.remove(&(
            current.target_type.clone(),
            current.target_public_id.clone(),
        )) else {
            continue;
        };
        let delta = previous
            .mastery_score
            .zip(current.mastery_score)
            .map(|(before, now)| now - before);
        if previous.status != current.status || delta.is_some_and(|value| value.abs() > 0.000_001) {
            changed_nodes.push(ProfileNodeTrendChange {
                target_type: current.target_type.clone(),
                target_public_id: current.target_public_id.clone(),
                target_title: current.target_title.clone(),
                previous_status: previous.status.clone(),
                current_status: current.status.clone(),
                previous_mastery_score: previous.mastery_score,
                current_mastery_score: current.mastery_score,
                mastery_score_delta: delta,
            });
        }
    }
    changed_nodes.sort_by(|left, right| {
        left.target_type
            .cmp(&right.target_type)
            .then(left.target_title.cmp(&right.target_title))
            .then(left.target_public_id.cmp(&right.target_public_id))
    });
    Ok(StudentProfileTrend {
        comparison_status: "comparable".into(),
        comparison_kind: Some("same_scope_refresh".into()),
        previous_snapshot_public_id: Some(previous_public_id),
        previous_revision: Some(previous_revision),
        previous_generated_at: Some(previous_generated_at),
        knowledge_assessed_before: Some(previous_knowledge_assessed),
        knowledge_assessed_current: knowledge_current,
        knowledge_assessed_delta: Some(knowledge_current - previous_knowledge_assessed),
        ability_assessed_before: Some(previous_ability_assessed),
        ability_assessed_current: ability_current,
        ability_assessed_delta: Some(ability_current - previous_ability_assessed),
        needs_support_before: Some(previous_needs_support),
        needs_support_current,
        needs_support_delta: Some(needs_support_current - previous_needs_support),
        stable_before: Some(previous_stable),
        stable_current,
        stable_delta: Some(stable_current - previous_stable),
        changed_nodes,
        note: "只比较同一日期范围、同一策略和同一证据适配契约的两次快照刷新；这是观察性变化，不自动宣称教学导致进步或退步。".into(),
    })
}

pub fn get_student_profile(
    conn: &Connection,
    public_id: &str,
) -> CoreResult<Option<StudentProfileSnapshot>> {
    let row = conn
        .query_row(
            "SELECT p.id,p.public_id,p.revision,p.student_id,p.class_id,s.student_no,s.name,
                    p.range_start,p.range_end,p.scope_kind,p.evidence_cutoff_at,
                    policy.id,policy.public_id,policy.revision,policy.min_independent_groups,
                    policy.min_distinct_dates,policy.min_distinct_sources,
                    policy.needs_support_below,policy.stable_at_or_above,policy.freshness_days,
                    p.source_watermark,p.evidence_count,p.knowledge_node_total,
                    p.knowledge_node_assessed,p.knowledge_node_eligible,p.ability_node_total,
                    p.ability_node_assessed,p.ability_node_eligible,p.state,p.payload_sha256,
                    p.generated_by,p.generated_at,p.confirmed_by,p.confirmed_at,
                    p.source_config_json,p.scope_json
             FROM profile_snapshots p
             JOIN students s ON s.id=p.student_id
             JOIN profile_policy_versions policy ON policy.id=p.policy_id
             WHERE p.public_id=?1",
            [public_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    ProfileStudent {
                        id: row.get(3)?,
                        class_id: row.get(4)?,
                        student_no: row.get(5)?,
                        name: row.get(6)?,
                    },
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    ProfilePolicy {
                        public_id: row.get(12)?,
                        revision: row.get(13)?,
                        min_independent_groups: row.get(14)?,
                        min_distinct_dates: row.get(15)?,
                        min_distinct_sources: row.get(16)?,
                        needs_support_below: row.get(17)?,
                        stable_at_or_above: row.get(18)?,
                        freshness_days: row.get(19)?,
                    },
                    row.get::<_, String>(20)?,
                    row.get::<_, i64>(21)?,
                    row.get::<_, i64>(22)?,
                    row.get::<_, i64>(23)?,
                    row.get::<_, i64>(24)?,
                    row.get::<_, i64>(25)?,
                    row.get::<_, i64>(26)?,
                    row.get::<_, i64>(27)?,
                    row.get::<_, String>(28)?,
                    row.get::<_, String>(29)?,
                    row.get::<_, String>(30)?,
                    row.get::<_, String>(31)?,
                    row.get::<_, String>(32)?,
                    row.get::<_, String>(33)?,
                    row.get::<_, String>(34)?,
                    row.get::<_, String>(35)?,
                ))
            },
        )
        .optional()?;
    let Some((
        snapshot_id,
        public_id,
        revision,
        student,
        range_start,
        range_end,
        scope_kind,
        evidence_cutoff_at,
        policy,
        source_watermark,
        evidence_count,
        knowledge_node_total,
        knowledge_node_assessed,
        knowledge_node_eligible,
        ability_node_total,
        ability_node_assessed,
        ability_node_eligible,
        state,
        payload_sha256,
        generated_by,
        generated_at,
        confirmed_by,
        confirmed_at,
        source_config_json,
        scope_json,
    )) = row
    else {
        return Ok(None);
    };
    let scope = StudentProfileScope {
        class_id: student.class_id,
        student_id: student.id,
        range_start: &range_start,
        range_end: &range_end,
    };
    let scope_selection = serde_json::from_str::<serde_json::Value>(&scope_json)
        .ok()
        .and_then(|value| value.get("selection").cloned())
        .and_then(|value| serde_json::from_value::<ProfileScopeSelectionView>(value).ok())
        .unwrap_or_else(|| auto_scope_selection().view);
    let selector_input = ProfileScopeSelectionInput {
        selector_kind: &scope_selection.selector_kind,
        selector_public_id: scope_selection.selector_public_id.as_deref(),
    };
    let current = compute_with_selection(conn, &scope, Some(&selector_input))?;
    let policy_stale = current.policy.public_id != policy.public_id;
    let source_stale = current.source_watermark != source_watermark;
    let is_stale = policy_stale || source_stale;
    let stale_reason = if policy_stale {
        Some("掌握计算规则已有新版本，建议重新生成。".into())
    } else if source_stale {
        Some("上游证据或知识范围已经变化，旧快照保持不变，建议重新生成。".into())
    } else {
        None
    };
    let recitation_summary = load_recitation_summary(conn, snapshot_id)?;
    let wrongbook_summary = load_wrongbook_summary(conn, snapshot_id)?;
    let (knowledge_metrics, ability_metrics) = load_metrics(conn, snapshot_id)?;
    let trend = load_student_trend(
        conn,
        &StudentTrendInput {
            snapshot_id,
            student_id: student.id,
            revision,
            range_start: &range_start,
            range_end: &range_end,
            policy_public_id: &policy.public_id,
            source_config_json: &source_config_json,
            knowledge_metrics: &knowledge_metrics,
            ability_metrics: &ability_metrics,
        },
    )?;
    let teacher_assessments =
        crate::teacher_assessments::list_profile_teacher_assessments(conn, &public_id)?;
    Ok(Some(StudentProfileSnapshot {
        public_id,
        revision,
        student,
        range_start,
        range_end,
        scope_kind,
        scope_selection,
        evidence_cutoff_at,
        policy,
        source_watermark,
        evidence_count,
        knowledge_node_total,
        knowledge_node_assessed,
        knowledge_node_eligible,
        ability_node_total,
        ability_node_assessed,
        ability_node_eligible,
        state,
        payload_sha256,
        generated_by,
        generated_at,
        confirmed_by,
        confirmed_at,
        is_stale,
        stale_reason,
        recitation_summary,
        wrongbook_summary,
        trend,
        teacher_assessments,
        knowledge_metrics,
        ability_metrics,
    }))
}

pub fn latest_student_profile(
    conn: &Connection,
    class_id: i64,
    student_id: i64,
) -> CoreResult<Option<StudentProfileSnapshot>> {
    let public_id = conn
        .query_row(
            "SELECT public_id FROM profile_snapshots
             WHERE class_id=?1 AND student_id=?2
             ORDER BY revision DESC LIMIT 1",
            (class_id, student_id),
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    public_id
        .as_deref()
        .map(|id| get_student_profile(conn, id))
        .transpose()
        .map(Option::flatten)
}

#[cfg(test)]
#[path = "profile_tests.rs"]
mod tests;
