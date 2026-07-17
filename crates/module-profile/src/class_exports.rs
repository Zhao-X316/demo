//! M6.1-5 本地安全子集：班级掌握脱敏聚合导出。
//!
//! 当前桌面应用没有学校成员、教师任课或家长关系事实，因此仅允许本机
//! `local_teacher` 为内部教学生成不含学生行、姓名、学号或原始证据的班级聚合。
//! 其他角色始终 fail-closed，直到真实授权关系和合规边界另行落地。

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use crate::class_profile::{self, ClassProfileNodeMetric, ClassProfileSnapshot};

pub const CLASS_PROFILE_EXPORT_SCHEMA_VERSION: i64 = 1;
pub const CLASS_PROFILE_EXPORT_RULE_VERSION: &str = "m6.1-deidentified-class-summary-local-v1";
pub const CLASS_PROFILE_EXPORT_MIN_GROUP_SIZE: i64 = 3;
pub const LOCAL_TEACHER_ACTOR_ID: &str = "local_teacher";

#[derive(Debug, Clone)]
pub struct CreateClassProfileExportInput<'a> {
    pub request_key: &'a str,
    pub snapshot_public_id: &'a str,
    pub expected_snapshot_payload_sha256: &'a str,
    pub report_kind: &'a str,
    pub purpose: &'a str,
    pub actor_role: &'a str,
    pub actor_id: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassProfileExportAccess {
    pub actor_role: String,
    pub allowed: bool,
    pub blockers: Vec<String>,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ExportNode {
    target_type: String,
    target_title: String,
    class_status: String,
    confidence_level: String,
    average_mastery_percent: Option<i64>,
    eligible_student_count: i64,
    total_student_count: i64,
    needs_support_count: i64,
    developing_count: i64,
    stable_count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct ClassProfileExportPayload {
    schema_version: i64,
    rule_version: String,
    report_kind: String,
    purpose: String,
    title: String,
    generated_at: String,
    class_name: String,
    range_start: String,
    range_end: String,
    source_snapshot_public_id: String,
    source_snapshot_revision: i64,
    source_snapshot_generated_at: String,
    min_group_size: i64,
    total_student_count: i64,
    snapshot_student_count: i64,
    eligible_student_count: i64,
    knowledge_node_total: i64,
    ability_node_total: i64,
    included_node_count: i64,
    suppressed_node_count: i64,
    stable_student_count: i64,
    developing_student_count: i64,
    needs_support_student_count: i64,
    evidence_insufficient_student_count: i64,
    data_unavailable_student_count: i64,
    privacy_note: String,
    no_ranking_note: String,
    nodes: Vec<ExportNode>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassProfileExportSnapshot {
    pub public_id: String,
    pub snapshot_public_id: String,
    pub class_id: i64,
    pub report_kind: String,
    pub purpose: String,
    pub actor_role: String,
    pub min_group_size: i64,
    pub schema_version: i64,
    pub rule_version: String,
    pub source_snapshot_payload_sha256: String,
    pub payload_sha256: String,
    pub csv_sha256: String,
    pub suggested_file_name: String,
    pub generated_by: String,
    pub generated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrittenClassProfileExport {
    pub snapshot_public_id: String,
    pub file_name: String,
    pub byte_size: i64,
    pub sha256: String,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

pub fn evaluate_export_access(actor_role: &str, actor_id: &str) -> ClassProfileExportAccess {
    let role = actor_role.trim();
    let actor = actor_id.trim();
    let blockers = match role {
        "local_teacher" if actor == LOCAL_TEACHER_ACTOR_ID => Vec::new(),
        "local_teacher" => vec!["当前本地版本只认本机老师身份，不能代入其他教师账号。".into()],
        "school_admin" => {
            vec!["尚无学校成员、教师任课和管理员授权关系，不能生成学校或跨班聚合。".into()]
        }
        "guardian" => {
            vec!["尚无家长与学生的授权关系，也没有老师确认的家长报告发布链，不能导出。".into()]
        }
        _ => vec!["未知角色没有班级掌握查看或导出权限。".into()],
    };
    ClassProfileExportAccess {
        actor_role: role.to_string(),
        allowed: blockers.is_empty(),
        blockers,
        boundary_note: "当前只支持本机老师导出自己正在查看的班级脱敏聚合；不含学生姓名、学号、逐人状态、原始录音、试卷、答案或证据。".into(),
    }
}

fn validate_input(input: &CreateClassProfileExportInput<'_>) -> CoreResult<()> {
    required(input.request_key, "请求标识")?;
    required(input.snapshot_public_id, "班级掌握快照")?;
    required(input.expected_snapshot_payload_sha256, "班级掌握快照校验值")?;
    required(input.actor_id, "操作人")?;
    if input.expected_snapshot_payload_sha256.len() != 64 {
        return Err(CoreError::Invalid("班级掌握快照校验值无效".into()));
    }
    if input.report_kind != "deidentified_class_summary" {
        return Err(CoreError::Invalid("当前只支持脱敏班级掌握摘要".into()));
    }
    if input.purpose != "internal_teaching" {
        return Err(CoreError::Invalid(
            "当前只允许用于本机老师内部教学，不支持公开或学校级传播".into(),
        ));
    }
    let access = evaluate_export_access(input.actor_role, input.actor_id);
    if !access.allowed {
        return Err(CoreError::Invalid(access.blockers.join("；")));
    }
    Ok(())
}

fn request_hash(input: &CreateClassProfileExportInput<'_>) -> CoreResult<String> {
    let value = serde_json::json!({
        "schema_version": CLASS_PROFILE_EXPORT_SCHEMA_VERSION,
        "snapshot_public_id": input.snapshot_public_id,
        "expected_snapshot_payload_sha256": input.expected_snapshot_payload_sha256,
        "report_kind": input.report_kind,
        "purpose": input.purpose,
        "actor_role": input.actor_role,
        "actor_id": input.actor_id
    });
    let bytes = serde_json::to_vec(&value).map_err(|error| CoreError::Parse(error.to_string()))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn safe_name(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() || "<>:\"/\\|?*".contains(character) {
                '_'
            } else {
                character
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

fn suggested_file_name(payload: &ClassProfileExportPayload) -> String {
    format!(
        "{}_班级掌握脱敏摘要_{}_至{}.csv",
        safe_name(&payload.class_name),
        payload.range_start,
        payload.range_end
    )
}

fn csv_escape(value: &str) -> String {
    if value
        .chars()
        .any(|character| matches!(character, ',' | '"' | '\r' | '\n'))
    {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn push_csv_row(output: &mut String, values: &[String]) {
    output.push_str(
        &values
            .iter()
            .map(|value| csv_escape(value))
            .collect::<Vec<_>>()
            .join(","),
    );
    output.push_str("\r\n");
}

fn target_type_label(value: &str) -> &str {
    match value {
        "knowledge_node" => "知识点",
        "ability_dimension" => "能力维度",
        _ => "节点",
    }
}

fn class_status_label(value: &str) -> &str {
    match value {
        "common_needs_support" => "共同需要支持",
        "observed" => "已形成班级观察",
        "class_evidence_insufficient" => "班级证据不足",
        "no_evidence" => "未形成证据",
        _ => "未知",
    }
}

fn confidence_label(value: &str) -> &str {
    match value {
        "high" => "高",
        "medium" => "中",
        "low" => "低",
        _ => "无",
    }
}

fn payload_csv(payload: &ClassProfileExportPayload) -> String {
    let mut output = String::from("\u{feff}");
    push_csv_row(&mut output, &["报告信息".into(), String::new()]);
    for (label, value) in [
        ("报告标题", payload.title.clone()),
        ("用途", "仅供当前老师内部教学".into()),
        ("班级", payload.class_name.clone()),
        (
            "时间范围",
            format!("{} 至 {}", payload.range_start, payload.range_end),
        ),
        ("生成时间", payload.generated_at.clone()),
        (
            "来源快照",
            format!(
                "v{} · {}",
                payload.source_snapshot_revision, payload.source_snapshot_generated_at
            ),
        ),
        (
            "隐私门槛",
            format!("每个展示节点至少 {} 名合格学生", payload.min_group_size),
        ),
        ("隐私说明", payload.privacy_note.clone()),
        ("排名说明", payload.no_ranking_note.clone()),
    ] {
        push_csv_row(&mut output, &[label.into(), value]);
    }
    push_csv_row(&mut output, &[]);
    push_csv_row(&mut output, &["汇总".into(), "人数或数量".into()]);
    for (label, value) in [
        ("班级启用学生", payload.total_student_count),
        ("有范围一致个人快照", payload.snapshot_student_count),
        (
            "至少一个节点达到个人证据门槛",
            payload.eligible_student_count,
        ),
        ("临时状态：较稳定", payload.stable_student_count),
        ("临时状态：发展中", payload.developing_student_count),
        ("临时状态：需要支持", payload.needs_support_student_count),
        (
            "临时状态：证据不足",
            payload.evidence_insufficient_student_count,
        ),
        ("临时状态：数据不足", payload.data_unavailable_student_count),
        ("展示节点", payload.included_node_count),
        ("因小样本隐藏节点", payload.suppressed_node_count),
    ] {
        push_csv_row(&mut output, &[label.into(), value.to_string()]);
    }
    push_csv_row(&mut output, &[]);
    push_csv_row(
        &mut output,
        &[
            "节点类型".into(),
            "节点".into(),
            "班级状态".into(),
            "平均掌握".into(),
            "可信度".into(),
            "合格样本".into(),
            "全班人数".into(),
            "需要支持".into(),
            "发展中".into(),
            "较稳定".into(),
        ],
    );
    for node in &payload.nodes {
        push_csv_row(
            &mut output,
            &[
                target_type_label(&node.target_type).into(),
                node.target_title.clone(),
                class_status_label(&node.class_status).into(),
                node.average_mastery_percent
                    .map(|value| format!("{value}%"))
                    .unwrap_or_else(|| "—".into()),
                confidence_label(&node.confidence_level).into(),
                node.eligible_student_count.to_string(),
                node.total_student_count.to_string(),
                node.needs_support_count.to_string(),
                node.developing_count.to_string(),
                node.stable_count.to_string(),
            ],
        );
    }
    output
}

fn export_nodes(snapshot: &ClassProfileSnapshot) -> (Vec<ExportNode>, i64) {
    let all_nodes: Vec<&ClassProfileNodeMetric> = snapshot
        .knowledge_metrics
        .iter()
        .chain(snapshot.ability_metrics.iter())
        .collect();
    let nodes = all_nodes
        .iter()
        .filter(|node| {
            node.sample_sufficient
                && node.eligible_student_count >= CLASS_PROFILE_EXPORT_MIN_GROUP_SIZE
        })
        .map(|node| ExportNode {
            target_type: node.target_type.clone(),
            target_title: node.target_title.clone(),
            class_status: node.class_status.clone(),
            confidence_level: node.confidence_level.clone(),
            average_mastery_percent: node
                .average_mastery_score
                .map(|value| (value * 100.0).round() as i64),
            eligible_student_count: node.eligible_student_count,
            total_student_count: node.total_student_count,
            needs_support_count: node.needs_support_count,
            developing_count: node.developing_count,
            stable_count: node.stable_count,
        })
        .collect::<Vec<_>>();
    let suppressed = all_nodes.len() as i64 - nodes.len() as i64;
    (nodes, suppressed)
}

fn build_payload(
    snapshot: &ClassProfileSnapshot,
    generated_at: String,
) -> CoreResult<ClassProfileExportPayload> {
    if snapshot.is_stale {
        return Err(CoreError::Invalid(
            snapshot
                .stale_reason
                .clone()
                .unwrap_or_else(|| "班级掌握快照已有新输入，请先刷新后再导出。".into()),
        ));
    }
    if snapshot.total_student_count < CLASS_PROFILE_EXPORT_MIN_GROUP_SIZE {
        return Err(CoreError::Invalid(format!(
            "班级少于 {} 人，不能生成脱敏聚合摘要",
            CLASS_PROFILE_EXPORT_MIN_GROUP_SIZE
        )));
    }
    let (nodes, suppressed_node_count) = export_nodes(snapshot);
    Ok(ClassProfileExportPayload {
        schema_version: CLASS_PROFILE_EXPORT_SCHEMA_VERSION,
        rule_version: CLASS_PROFILE_EXPORT_RULE_VERSION.into(),
        report_kind: "deidentified_class_summary".into(),
        purpose: "internal_teaching".into(),
        title: format!("{} 班级掌握脱敏摘要", snapshot.class.name),
        generated_at,
        class_name: snapshot.class.name.clone(),
        range_start: snapshot.range_start.clone(),
        range_end: snapshot.range_end.clone(),
        source_snapshot_public_id: snapshot.public_id.clone(),
        source_snapshot_revision: snapshot.revision,
        source_snapshot_generated_at: snapshot.generated_at.clone(),
        min_group_size: CLASS_PROFILE_EXPORT_MIN_GROUP_SIZE,
        total_student_count: snapshot.total_student_count,
        snapshot_student_count: snapshot.snapshot_student_count,
        eligible_student_count: snapshot.eligible_student_count,
        knowledge_node_total: snapshot.knowledge_node_total,
        ability_node_total: snapshot.ability_node_total,
        included_node_count: nodes.len() as i64,
        suppressed_node_count,
        stable_student_count: snapshot.student_status_counts.stable_count,
        developing_student_count: snapshot.student_status_counts.developing_count,
        needs_support_student_count: snapshot.student_status_counts.needs_support_count,
        evidence_insufficient_student_count: snapshot
            .student_status_counts
            .evidence_insufficient_count,
        data_unavailable_student_count: snapshot
            .student_status_counts
            .data_unavailable_count,
        privacy_note: format!(
            "不含学生姓名、学号、逐人状态、原始录音、试卷、答案或证据；合格样本少于 {} 人的节点整行隐藏。",
            CLASS_PROFILE_EXPORT_MIN_GROUP_SIZE
        ),
        no_ranking_note: "不含学生排名、班级排名或能力总分；掌握是阶段性证据汇总。".into(),
        nodes,
    })
}

fn snapshot_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(ClassProfileExportSnapshot, String, String)> {
    let payload_json: String = row.get(14)?;
    let payload: ClassProfileExportPayload =
        serde_json::from_str(&payload_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                14,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok((
        ClassProfileExportSnapshot {
            public_id: row.get(0)?,
            snapshot_public_id: row.get(1)?,
            class_id: row.get(2)?,
            report_kind: row.get(3)?,
            purpose: row.get(4)?,
            actor_role: row.get(5)?,
            min_group_size: row.get(6)?,
            schema_version: row.get(7)?,
            rule_version: row.get(8)?,
            source_snapshot_payload_sha256: row.get(9)?,
            payload_sha256: row.get(10)?,
            csv_sha256: row.get(11)?,
            suggested_file_name: suggested_file_name(&payload),
            generated_by: row.get(12)?,
            generated_at: row.get(13)?,
        },
        payload_json,
        row.get(15)?,
    ))
}

fn load_snapshot_with_payload(
    conn: &Connection,
    public_id: &str,
) -> CoreResult<Option<(ClassProfileExportSnapshot, String, String)>> {
    Ok(conn
        .query_row(
            "SELECT export.public_id,source.public_id,export.class_id,export.report_kind,
                    export.purpose,export.actor_role,export.min_group_size,
                    export.schema_version,export.rule_version,
                    export.source_snapshot_payload_sha256,export.payload_sha256,
                    export.csv_sha256,export.actor_id,export.generated_at,
                    export.payload_json,export.request_payload_sha256
             FROM class_profile_export_snapshots export
             JOIN class_profile_snapshots source
               ON source.id=export.class_profile_snapshot_id
             WHERE export.public_id=?1",
            [public_id],
            snapshot_from_row,
        )
        .optional()?)
}

pub fn load_export_snapshot(
    conn: &Connection,
    public_id: &str,
) -> CoreResult<ClassProfileExportSnapshot> {
    load_snapshot_with_payload(conn, public_id)?
        .map(|(snapshot, _, _)| snapshot)
        .ok_or_else(|| CoreError::NotFound("班级掌握脱敏导出快照".into()))
}

pub fn create_export_snapshot(
    conn: &mut Connection,
    input: &CreateClassProfileExportInput<'_>,
) -> CoreResult<ClassProfileExportSnapshot> {
    validate_input(input)?;
    let request_payload_sha256 = request_hash(input)?;
    let existing = conn
        .query_row(
            "SELECT public_id,request_payload_sha256
             FROM class_profile_export_snapshots WHERE request_key=?1",
            [input.request_key.trim()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    if let Some((public_id, stored_hash)) = existing {
        if stored_hash != request_payload_sha256 {
            return Err(CoreError::Invalid(
                "同一请求标识已用于不同的班级导出".into(),
            ));
        }
        return load_export_snapshot(conn, &public_id);
    }
    let source = class_profile::get_class_profile(conn, input.snapshot_public_id)?
        .ok_or_else(|| CoreError::NotFound("班级掌握快照".into()))?;
    if source.payload_sha256 != input.expected_snapshot_payload_sha256 {
        return Err(CoreError::Invalid(
            "班级掌握快照已变化，请重新打开后再导出".into(),
        ));
    }
    let generated_at = time::utc_now_rfc3339();
    let payload = build_payload(&source, generated_at.clone())?;
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| CoreError::Parse(error.to_string()))?;
    let csv = payload_csv(&payload);
    let payload_sha256 = hashing::sha256_hex(payload_json.as_bytes());
    let csv_sha256 = hashing::sha256_hex(csv.as_bytes());
    let public_id = ids::new_public_id();
    let transaction = conn.transaction()?;
    transaction.execute(
        "INSERT INTO class_profile_export_snapshots
          (public_id,request_key,request_payload_sha256,class_profile_snapshot_id,class_id,
           report_kind,purpose,actor_role,actor_id,min_group_size,schema_version,rule_version,
           source_snapshot_payload_sha256,payload_sha256,csv_sha256,payload_json,generated_at)
         SELECT ?1,?2,?3,id,class_id,?4,?5,?6,?7,?8,?9,?10,
                payload_sha256,?11,?12,?13,?14
         FROM class_profile_snapshots WHERE public_id=?15",
        params![
            public_id,
            input.request_key.trim(),
            request_payload_sha256,
            input.report_kind,
            input.purpose,
            input.actor_role,
            input.actor_id.trim(),
            CLASS_PROFILE_EXPORT_MIN_GROUP_SIZE,
            CLASS_PROFILE_EXPORT_SCHEMA_VERSION,
            CLASS_PROFILE_EXPORT_RULE_VERSION,
            payload_sha256,
            csv_sha256,
            payload_json,
            generated_at,
            input.snapshot_public_id,
        ],
    )?;
    let event_payload = serde_json::json!({
        "schema_version": CLASS_PROFILE_EXPORT_SCHEMA_VERSION,
        "export_public_id": public_id,
        "source_snapshot_public_id": input.snapshot_public_id,
        "report_kind": input.report_kind,
        "purpose": input.purpose,
        "actor_role": input.actor_role,
        "min_group_size": CLASS_PROFILE_EXPORT_MIN_GROUP_SIZE,
        "student_rows_included": false,
        "raw_evidence_included": false
    })
    .to_string();
    outbox::create_event(
        &transaction,
        &NewOutboxEvent {
            idempotency_key: &format!("profile:outbox:class-export:{public_id}"),
            event_type: "class_profile_deidentified_export_created",
            event_version: 1,
            aggregate_type: "class_profile_export_snapshot",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &generated_at,
        },
    )?;
    audit::append(
        &transaction,
        &NewAuditEvent {
            idempotency_key: &format!("profile:audit:class-export:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.actor_id.trim()),
            action: "profile.class_export.created",
            object_type: "class_profile_export_snapshot",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("本机老师明确生成不含学生行和原始证据的班级掌握脱敏摘要"),
            meta_json: Some(&event_payload),
            occurred_at: &generated_at,
        },
    )?;
    transaction.commit()?;
    load_export_snapshot(conn, &public_id)
}

fn write_private_file(path: &Path, bytes: &[u8], snapshot_public_id: &str) -> CoreResult<()> {
    if !path.is_absolute() {
        return Err(CoreError::Invalid("导出位置必须是绝对路径".into()));
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map_or(true, |value| !value.eq_ignore_ascii_case("csv"))
    {
        return Err(CoreError::Invalid("导出文件必须使用 .csv 扩展名".into()));
    }
    if path.exists() {
        return Err(CoreError::Invalid(
            "所选文件已存在，请在保存窗口中换一个名称".into(),
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| CoreError::Invalid("导出目录不存在".into()))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CoreError::Invalid("导出文件名无效".into()))?;
    let temporary = parent.join(format!(".{file_name}.{snapshot_public_id}.tmp"));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> CoreResult<()> {
        let mut file = options
            .open(&temporary)
            .map_err(|error| CoreError::Io(format!("创建导出临时文件失败：{error}")))?;
        file.write_all(bytes)
            .map_err(|error| CoreError::Io(format!("写入导出文件失败：{error}")))?;
        file.sync_all()
            .map_err(|error| CoreError::Io(format!("刷新导出文件失败：{error}")))?;
        std::fs::hard_link(&temporary, path)
            .map_err(|error| CoreError::Io(format!("保存导出文件失败：{error}")))?;
        std::fs::remove_file(&temporary)
            .map_err(|error| CoreError::Io(format!("清理导出临时文件失败：{error}")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn write_export_snapshot_csv(
    conn: &Connection,
    export_public_id: &str,
    output_path: &str,
) -> CoreResult<WrittenClassProfileExport> {
    let (snapshot, payload_json, _) = load_snapshot_with_payload(conn, export_public_id)?
        .ok_or_else(|| CoreError::NotFound("班级掌握脱敏导出快照".into()))?;
    if hashing::sha256_hex(payload_json.as_bytes()) != snapshot.payload_sha256 {
        return Err(CoreError::Invalid("班级导出快照内容校验失败".into()));
    }
    let payload: ClassProfileExportPayload =
        serde_json::from_str(&payload_json).map_err(|error| CoreError::Parse(error.to_string()))?;
    let csv = payload_csv(&payload);
    if hashing::sha256_hex(csv.as_bytes()) != snapshot.csv_sha256 {
        return Err(CoreError::Invalid("班级导出 CSV 内容校验失败".into()));
    }
    let path = Path::new(output_path);
    write_private_file(path, csv.as_bytes(), export_public_id)?;
    Ok(WrittenClassProfileExport {
        snapshot_public_id: snapshot.public_id,
        file_name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string(),
        byte_size: csv.len() as i64,
        sha256: snapshot.csv_sha256,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_matrix_fails_closed_without_real_membership_relations() {
        assert!(evaluate_export_access("local_teacher", LOCAL_TEACHER_ACTOR_ID).allowed);
        for role in ["school_admin", "guardian", "unknown"] {
            let access = evaluate_export_access(role, "actor-1");
            assert!(!access.allowed);
            assert!(!access.blockers.is_empty());
        }
        assert!(!evaluate_export_access("local_teacher", "another-teacher").allowed);
    }
}
