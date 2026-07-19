//! M6-4 本地个人学习掌握报告。
//!
//! 报告只能由本机老师从当前、未过期的个人掌握快照明确生成。它冻结生成
//! 当时的系统指标和老师补充判断，用于老师内部学习反馈；不建立家长授权、
//! 自动发送或公开分享链，也不带出原始录音、图片、转写、作答正文或逐条证据。

use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::fs::OpenOptions;
use std::io::Write as IoWrite;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use crate::profile::{self, ProfileNodeMetric, StudentProfileSnapshot};
use crate::teacher_assessments::ProfileTeacherAssessment;

pub const STUDENT_PROFILE_REPORT_SCHEMA_VERSION: i64 = 1;
pub const STUDENT_PROFILE_REPORT_RULE_VERSION: &str = "m6-student-learning-summary-local-v1";
pub const LOCAL_TEACHER_ACTOR_ID: &str = "local_teacher";

#[derive(Debug, Clone)]
pub struct CreateStudentProfileReportInput<'a> {
    pub request_key: &'a str,
    pub snapshot_public_id: &'a str,
    pub expected_snapshot_payload_sha256: &'a str,
    pub report_kind: &'a str,
    pub purpose: &'a str,
    pub actor_role: &'a str,
    pub actor_id: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudentProfileReportAccess {
    pub actor_role: String,
    pub allowed: bool,
    pub blockers: Vec<String>,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudentProfileReportSnapshot {
    pub public_id: String,
    pub snapshot_public_id: String,
    pub student_id: i64,
    pub class_id: i64,
    pub report_kind: String,
    pub purpose: String,
    pub actor_role: String,
    pub schema_version: i64,
    pub rule_version: String,
    pub source_snapshot_payload_sha256: String,
    pub teacher_assessment_watermark: String,
    pub payload_sha256: String,
    pub html_sha256: String,
    pub suggested_file_name: String,
    pub generated_by: String,
    pub generated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrittenStudentProfileReport {
    pub snapshot_public_id: String,
    pub file_name: String,
    pub byte_size: i64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReportMetric {
    target_type: String,
    target_title: String,
    system_status: String,
    mastery_percent: Option<i64>,
    confidence_level: String,
    freshness: String,
    evidence_count: i64,
    distinct_date_count: i64,
    distinct_source_count: i64,
    last_evidence_at: Option<String>,
    teacher_assessment: Option<String>,
    teacher_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReportRecitationSummary {
    overall_count: i64,
    fluency_count: i64,
    retention_count: i64,
    latest_overall_percent: Option<i64>,
    latest_fluency_percent: Option<i64>,
    latest_retention_percent: Option<i64>,
    latest_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReportWrongbookSummary {
    fact_count: i64,
    needs_correction_count: i64,
    corrected_once_count: i64,
    rechecked_correct_count: i64,
    repeated_error_count: i64,
    latest_response_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReportTrendSummary {
    comparison_status: String,
    previous_revision: Option<i64>,
    previous_generated_at: Option<String>,
    knowledge_assessed_delta: Option<i64>,
    ability_assessed_delta: Option<i64>,
    needs_support_delta: Option<i64>,
    stable_delta: Option<i64>,
    note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct StudentProfileReportPayload {
    schema_version: i64,
    rule_version: String,
    report_kind: String,
    purpose: String,
    title: String,
    generated_at: String,
    class_name: String,
    student_no: String,
    student_name: String,
    range_start: String,
    range_end: String,
    scope_path: String,
    source_snapshot_public_id: String,
    source_snapshot_revision: i64,
    source_snapshot_generated_at: String,
    evidence_cutoff_at: String,
    evidence_count: i64,
    knowledge_node_total: i64,
    knowledge_node_assessed: i64,
    knowledge_node_eligible: i64,
    ability_node_total: i64,
    ability_node_assessed: i64,
    ability_node_eligible: i64,
    recitation: ReportRecitationSummary,
    wrongbook: ReportWrongbookSummary,
    trend: ReportTrendSummary,
    privacy_note: String,
    interpretation_note: String,
    knowledge_metrics: Vec<ReportMetric>,
    ability_metrics: Vec<ReportMetric>,
}

#[derive(Debug, Clone, Serialize)]
struct TeacherAssessmentWatermarkRow<'a> {
    public_id: &'a str,
    node_metric_public_id: &'a str,
    revision: i64,
    assessment: Option<&'a str>,
    note: Option<&'a str>,
    state: &'a str,
    created_at: &'a str,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

pub fn evaluate_report_access(actor_role: &str, actor_id: &str) -> StudentProfileReportAccess {
    let role = actor_role.trim();
    let actor = actor_id.trim();
    let blockers = match role {
        "local_teacher" if actor == LOCAL_TEACHER_ACTOR_ID => Vec::new(),
        "local_teacher" => vec!["当前本地版本只认本机老师身份，不能代入其他教师账号。".into()],
        "guardian" => {
            vec!["尚无家长与学生授权关系和老师确认的报告发布链，不能直接生成家长报告。".into()]
        }
        "school_admin" => {
            vec!["尚无学校成员、任课班级和管理员授权关系，不能代老师生成个人报告。".into()]
        }
        _ => vec!["未知角色没有个人学习报告生成权限。".into()],
    };
    StudentProfileReportAccess {
        actor_role: role.to_string(),
        allowed: blockers.is_empty(),
        blockers,
        boundary_note: "当前只允许本机老师导出正在查看的个人掌握快照；文件由老师内部保管并决定是否线下提供，不自动发送。".into(),
    }
}

fn validate_input(input: &CreateStudentProfileReportInput<'_>) -> CoreResult<()> {
    required(input.request_key, "请求标识")?;
    required(input.snapshot_public_id, "个人掌握快照")?;
    required(input.expected_snapshot_payload_sha256, "个人掌握快照校验值")?;
    required(input.actor_id, "操作人")?;
    if input.expected_snapshot_payload_sha256.len() != 64 {
        return Err(CoreError::Invalid("个人掌握快照校验值无效".into()));
    }
    if input.report_kind != "student_learning_summary" {
        return Err(CoreError::Invalid("当前只支持个人学习掌握摘要".into()));
    }
    if input.purpose != "teacher_internal_feedback" {
        return Err(CoreError::Invalid(
            "当前只允许用于老师内部学习反馈，不支持自动公开或直接发送".into(),
        ));
    }
    let access = evaluate_report_access(input.actor_role, input.actor_id);
    if !access.allowed {
        return Err(CoreError::Invalid(access.blockers.join("；")));
    }
    Ok(())
}

fn request_hash(input: &CreateStudentProfileReportInput<'_>) -> CoreResult<String> {
    let value = serde_json::json!({
        "schema_version": STUDENT_PROFILE_REPORT_SCHEMA_VERSION,
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

fn teacher_assessment_watermark(assessments: &[ProfileTeacherAssessment]) -> CoreResult<String> {
    let rows = assessments
        .iter()
        .map(|item| TeacherAssessmentWatermarkRow {
            public_id: &item.public_id,
            node_metric_public_id: &item.node_metric_public_id,
            revision: item.revision,
            assessment: item.assessment.as_deref(),
            note: item.note.as_deref(),
            state: &item.state,
            created_at: &item.created_at,
        })
        .collect::<Vec<_>>();
    let bytes = serde_json::to_vec(&rows).map_err(|error| CoreError::Parse(error.to_string()))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn percentage(value: Option<f64>) -> Option<i64> {
    value.map(|number| (number * 100.0).round() as i64)
}

fn report_metrics(
    metrics: &[ProfileNodeMetric],
    assessments: &[ProfileTeacherAssessment],
) -> Vec<ReportMetric> {
    let active = assessments
        .iter()
        .filter(|item| item.state == "active")
        .map(|item| (item.node_metric_public_id.as_str(), item))
        .collect::<HashMap<_, _>>();
    metrics
        .iter()
        .filter_map(|metric| {
            let teacher = active.get(metric.public_id.as_str()).copied();
            if metric.evidence_count == 0 && teacher.is_none() {
                return None;
            }
            Some(ReportMetric {
                target_type: metric.target_type.clone(),
                target_title: metric.target_title.clone(),
                system_status: metric.status.clone(),
                mastery_percent: percentage(metric.mastery_score),
                confidence_level: metric.confidence_level.clone(),
                freshness: metric.freshness.clone(),
                evidence_count: metric.evidence_count,
                distinct_date_count: metric.distinct_date_count,
                distinct_source_count: metric.distinct_source_count,
                last_evidence_at: metric.last_evidence_at.clone(),
                teacher_assessment: teacher.and_then(|item| item.assessment.clone()),
                teacher_note: teacher.and_then(|item| item.note.clone()),
            })
        })
        .collect()
}

fn build_payload(
    snapshot: &StudentProfileSnapshot,
    class_name: String,
    generated_at: String,
) -> CoreResult<StudentProfileReportPayload> {
    if snapshot.is_stale {
        return Err(CoreError::Invalid(
            snapshot
                .stale_reason
                .clone()
                .unwrap_or_else(|| "个人掌握快照已有新输入，请先刷新后再导出。".into()),
        ));
    }
    Ok(StudentProfileReportPayload {
        schema_version: STUDENT_PROFILE_REPORT_SCHEMA_VERSION,
        rule_version: STUDENT_PROFILE_REPORT_RULE_VERSION.into(),
        report_kind: "student_learning_summary".into(),
        purpose: "teacher_internal_feedback".into(),
        title: format!("{}号 {} 个人学习掌握报告", snapshot.student.student_no, snapshot.student.name),
        generated_at,
        class_name,
        student_no: snapshot.student.student_no.clone(),
        student_name: snapshot.student.name.clone(),
        range_start: snapshot.range_start.clone(),
        range_end: snapshot.range_end.clone(),
        scope_path: snapshot.scope_selection.path.clone(),
        source_snapshot_public_id: snapshot.public_id.clone(),
        source_snapshot_revision: snapshot.revision,
        source_snapshot_generated_at: snapshot.generated_at.clone(),
        evidence_cutoff_at: snapshot.evidence_cutoff_at.clone(),
        evidence_count: snapshot.evidence_count,
        knowledge_node_total: snapshot.knowledge_node_total,
        knowledge_node_assessed: snapshot.knowledge_node_assessed,
        knowledge_node_eligible: snapshot.knowledge_node_eligible,
        ability_node_total: snapshot.ability_node_total,
        ability_node_assessed: snapshot.ability_node_assessed,
        ability_node_eligible: snapshot.ability_node_eligible,
        recitation: ReportRecitationSummary {
            overall_count: snapshot.recitation_summary.overall_count,
            fluency_count: snapshot.recitation_summary.fluency_count,
            retention_count: snapshot.recitation_summary.retention_count,
            latest_overall_percent: percentage(snapshot.recitation_summary.latest_overall_value),
            latest_fluency_percent: percentage(snapshot.recitation_summary.latest_fluency_value),
            latest_retention_percent: percentage(snapshot.recitation_summary.latest_retention_value),
            latest_at: snapshot.recitation_summary.latest_at.clone(),
        },
        wrongbook: ReportWrongbookSummary {
            fact_count: snapshot.wrongbook_summary.fact_count,
            needs_correction_count: snapshot.wrongbook_summary.needs_correction_count,
            corrected_once_count: snapshot.wrongbook_summary.corrected_once_count,
            rechecked_correct_count: snapshot.wrongbook_summary.rechecked_correct_count,
            repeated_error_count: snapshot.wrongbook_summary.repeated_error_count,
            latest_response_at: snapshot.wrongbook_summary.latest_response_at.clone(),
        },
        trend: ReportTrendSummary {
            comparison_status: snapshot.trend.comparison_status.clone(),
            previous_revision: snapshot.trend.previous_revision,
            previous_generated_at: snapshot.trend.previous_generated_at.clone(),
            knowledge_assessed_delta: snapshot.trend.knowledge_assessed_delta,
            ability_assessed_delta: snapshot.trend.ability_assessed_delta,
            needs_support_delta: snapshot.trend.needs_support_delta,
            stable_delta: snapshot.trend.stable_delta,
            note: snapshot.trend.note.clone(),
        },
        privacy_note: "不含原始录音、图片、转写、学生作答正文、题干或逐条证据标识；只展示老师确认快照中的汇总和节点结论。".into(),
        interpretation_note: "掌握状态是指定时间、教材范围和证据门槛下的阶段性观察；未评估或证据不足不等于能力差，也不生成学生排名。".into(),
        knowledge_metrics: report_metrics(
            &snapshot.knowledge_metrics,
            &snapshot.teacher_assessments,
        ),
        ability_metrics: report_metrics(
            &snapshot.ability_metrics,
            &snapshot.teacher_assessments,
        ),
    })
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

fn suggested_file_name(payload: &StudentProfileReportPayload) -> String {
    format!(
        "{}_{}号{}_个人学习报告_{}至{}.html",
        safe_name(&payload.class_name),
        safe_name(&payload.student_no),
        safe_name(&payload.student_name),
        payload.range_start,
        payload.range_end
    )
}

fn html_escape(value: &str) -> String {
    value
        .chars()
        .map(|character| match character {
            '&' => "&amp;".into(),
            '<' => "&lt;".into(),
            '>' => "&gt;".into(),
            '"' => "&quot;".into(),
            '\'' => "&#39;".into(),
            _ => character.to_string(),
        })
        .collect()
}

fn status_label(value: &str) -> &str {
    match value {
        "unassessed" => "未评估",
        "insufficient_evidence" => "证据不足",
        "needs_support" => "需要支持",
        "developing" => "发展中",
        "stable" => "相对稳定",
        _ => "未知",
    }
}

fn confidence_label(value: &str) -> &str {
    match value {
        "high" => "高",
        "medium" => "中",
        "low" => "低",
        _ => "暂无",
    }
}

fn freshness_label(value: &str) -> &str {
    match value {
        "fresh" => "近期",
        "aging" => "较早",
        "stale" => "久未更新",
        _ => "暂无",
    }
}

fn assessment_label(value: &str) -> &str {
    match value {
        "not_taught" => "尚未教学",
        "needs_support" => "需要重点支持",
        "developing" => "发展中",
        "stable" => "相对稳定",
        "observe" => "继续观察",
        _ => "未补充",
    }
}

fn optional_percent(value: Option<i64>) -> String {
    value.map_or_else(|| "—".into(), |number| format!("{number}%"))
}

fn optional_text(value: Option<&str>) -> String {
    value.map_or_else(|| "—".into(), html_escape)
}

fn signed_count(value: Option<i64>) -> String {
    value.map_or_else(
        || "暂无可比基线".into(),
        |number| {
            if number > 0 {
                format!("+{number}")
            } else {
                number.to_string()
            }
        },
    )
}

fn metric_table(title: &str, metrics: &[ReportMetric]) -> String {
    let mut output = String::new();
    write!(
        output,
        "<section><h2>{}</h2><table><thead><tr><th>节点</th><th>系统状态</th><th>掌握</th><th>可信度</th><th>证据</th><th>老师补充</th></tr></thead><tbody>",
        html_escape(title)
    )
    .expect("writing to a String cannot fail");
    if metrics.is_empty() {
        output.push_str("<tr><td colspan=\"6\" class=\"empty\">当前范围没有可展示的已评估节点或老师补充判断。</td></tr>");
    } else {
        for metric in metrics {
            let teacher = metric
                .teacher_assessment
                .as_deref()
                .map(assessment_label)
                .unwrap_or("未补充");
            let note = optional_text(metric.teacher_note.as_deref());
            let last_at = optional_text(metric.last_evidence_at.as_deref());
            write!(
                output,
                "<tr><td><b>{}</b><small>{}</small></td><td>{}</td><td>{}</td><td>{} · {}</td><td>{} 条<small>{} 个日期 · {} 类来源 · 最近 {}</small></td><td><b>{}</b><small class=\"teacher-note\">{}</small></td></tr>",
                html_escape(&metric.target_title),
                if metric.target_type == "knowledge_node" { "知识点" } else { "能力维度" },
                status_label(&metric.system_status),
                optional_percent(metric.mastery_percent),
                confidence_label(&metric.confidence_level),
                freshness_label(&metric.freshness),
                metric.evidence_count,
                metric.distinct_date_count,
                metric.distinct_source_count,
                last_at,
                html_escape(teacher),
                note
            )
            .expect("writing to a String cannot fail");
        }
    }
    output.push_str("</tbody></table></section>");
    output
}

fn payload_html(payload: &StudentProfileReportPayload) -> String {
    let recitation_latest = payload
        .recitation
        .latest_at
        .as_deref()
        .map_or_else(|| "暂无".into(), html_escape);
    let wrongbook_latest = payload
        .wrongbook
        .latest_response_at
        .as_deref()
        .map_or_else(|| "暂无".into(), html_escape);
    let trend_baseline = payload.trend.previous_revision.map_or_else(
        || "暂无同范围可比基线".into(),
        |revision| {
            format!(
                "对比第 {} 版（{}）",
                revision,
                optional_text(payload.trend.previous_generated_at.as_deref())
            )
        },
    );
    let mut html = String::from(
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>",
    );
    html.push_str(&html_escape(&payload.title));
    html.push_str(
        "</title><style>
        :root{color-scheme:light;font-family:-apple-system,BlinkMacSystemFont,\"Segoe UI\",\"PingFang SC\",\"Microsoft YaHei\",sans-serif;color:#1d2935;background:#f4f7f5}
        *{box-sizing:border-box}body{margin:0;padding:32px;background:#f4f7f5;line-height:1.5}.page{max-width:1080px;margin:auto;padding:34px;background:#fff;border:1px solid #dbe5df;border-radius:16px;box-shadow:0 8px 30px rgba(31,55,44,.08)}
        h1{margin:0 0 6px;font-size:26px}h2{margin:28px 0 10px;font-size:17px}.muted,small{display:block;color:#66746d;font-size:12px}.badge{display:inline-block;margin-bottom:18px;padding:6px 10px;border-radius:999px;background:#fff0d8;color:#7a5215;font-weight:700;font-size:12px}
        .notice{margin:18px 0;padding:14px 16px;border-left:4px solid #c58d31;background:#fffaf0}.grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:10px;margin:18px 0}.card{padding:14px;border:1px solid #dde5e0;border-radius:10px;background:#f9fbfa}.card b{display:block;margin-top:4px;font-size:20px}.card span{color:#66746d;font-size:12px}
        .facts{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px 22px;padding:14px 0;border-top:1px solid #e2e8e4;border-bottom:1px solid #e2e8e4}.facts div{font-size:13px}.facts b{display:inline-block;min-width:92px}
        table{width:100%;border-collapse:collapse;font-size:12px}th,td{padding:10px 8px;border:1px solid #dde5e0;text-align:left;vertical-align:top}th{background:#f1f6f3}td b{display:block}td small{margin-top:3px}.teacher-note{white-space:pre-wrap}.empty{text-align:center;color:#66746d}
        .summary-line{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:10px}.summary-line div{padding:12px;border:1px solid #dde5e0;border-radius:9px}.summary-line b{display:block;font-size:16px}.foot{margin-top:26px;padding-top:14px;border-top:1px solid #dde5e0;color:#66746d;font-size:11px}
        @media(max-width:760px){body{padding:10px}.page{padding:20px}.grid,.summary-line,.facts{grid-template-columns:1fr 1fr}table{display:block;overflow-x:auto}}
        @media print{body{padding:0;background:#fff}.page{max-width:none;border:0;border-radius:0;box-shadow:none}.badge{border:1px solid #d9b26d}section{break-inside:avoid}}
        </style></head><body><main class=\"page\">",
    );
    write!(
        html,
        "<span class=\"badge\">教师内部学习反馈</span><h1>{}</h1><div class=\"muted\">生成于 {} · 来源个人掌握快照第 {} 版</div>",
        html_escape(&payload.title),
        html_escape(&payload.generated_at),
        payload.source_snapshot_revision
    )
    .expect("writing to a String cannot fail");
    html.push_str("<div class=\"notice\"><b>使用边界</b><br>本文件只供当前老师内部使用。是否线下提供给学生或家长，由老师在核对内容后自行决定；系统尚未建立家长授权、身份核验或自动发送链。</div>");
    write!(
        html,
        "<div class=\"facts\"><div><b>班级</b>{}</div><div><b>学生</b>{}号 {}</div><div><b>时间范围</b>{} 至 {}</div><div><b>教材范围</b>{}</div><div><b>证据截至</b>{}</div><div><b>报告用途</b>教师内部学习反馈</div></div>",
        html_escape(&payload.class_name),
        html_escape(&payload.student_no),
        html_escape(&payload.student_name),
        html_escape(&payload.range_start),
        html_escape(&payload.range_end),
        html_escape(&payload.scope_path),
        html_escape(&payload.evidence_cutoff_at)
    )
    .expect("writing to a String cannot fail");
    write!(
        html,
        "<div class=\"grid\"><div class=\"card\"><span>正式证据</span><b>{}</b></div><div class=\"card\"><span>知识已评估</span><b>{} / {}</b></div><div class=\"card\"><span>知识达门槛</span><b>{}</b></div><div class=\"card\"><span>能力已评估</span><b>{} / {}</b></div></div>",
        payload.evidence_count,
        payload.knowledge_node_assessed,
        payload.knowledge_node_total,
        payload.knowledge_node_eligible,
        payload.ability_node_assessed,
        payload.ability_node_total
    )
    .expect("writing to a String cannot fail");
    html.push_str("<section><h2>背诵汇总</h2><div class=\"summary-line\">");
    write!(
        html,
        "<div><span>内容记录</span><b>{} 条</b></div><div><span>流畅度记录</span><b>{} 条</b></div><div><span>间隔保持记录</span><b>{} 条</b></div><div><span>最近记录</span><b>{}</b></div></div><div class=\"muted\">最近内容 {} · 流畅度 {} · 间隔保持 {}</div></section>",
        payload.recitation.overall_count,
        payload.recitation.fluency_count,
        payload.recitation.retention_count,
        recitation_latest,
        optional_percent(payload.recitation.latest_overall_percent),
        optional_percent(payload.recitation.latest_fluency_percent),
        optional_percent(payload.recitation.latest_retention_percent)
    )
    .expect("writing to a String cannot fail");
    html.push_str("<section><h2>错题恢复汇总</h2><div class=\"summary-line\">");
    write!(
        html,
        "<div><span>错题事实</span><b>{}</b></div><div><span>待订正</span><b>{}</b></div><div><span>再次作答正确</span><b>{}</b></div><div><span>重复出错</span><b>{}</b></div></div><div class=\"muted\">已订正一次 {} · 最近发布 {}</div></section>",
        payload.wrongbook.fact_count,
        payload.wrongbook.needs_correction_count,
        payload.wrongbook.rechecked_correct_count,
        payload.wrongbook.repeated_error_count,
        payload.wrongbook.corrected_once_count,
        wrongbook_latest
    )
    .expect("writing to a String cannot fail");
    write!(
        html,
        "<section><h2>同范围变化观察</h2><div class=\"muted\">{}</div><div class=\"summary-line\"><div><span>知识已评估变化</span><b>{}</b></div><div><span>能力已评估变化</span><b>{}</b></div><div><span>需要支持节点变化</span><b>{}</b></div><div><span>相对稳定节点变化</span><b>{}</b></div></div><div class=\"muted\">{}</div></section>",
        html_escape(&trend_baseline),
        signed_count(payload.trend.knowledge_assessed_delta),
        signed_count(payload.trend.ability_assessed_delta),
        signed_count(payload.trend.needs_support_delta),
        signed_count(payload.trend.stable_delta),
        html_escape(&payload.trend.note)
    )
    .expect("writing to a String cannot fail");
    html.push_str(&metric_table("知识掌握", &payload.knowledge_metrics));
    html.push_str(&metric_table("学科能力", &payload.ability_metrics));
    write!(
        html,
        "<div class=\"notice\"><b>如何理解</b><br>{}<br>{}</div><div class=\"foot\">{}<br>报告规则：{} · 本文件不含逐条证据内容，查看原始证据请回到教师端个人掌握快照。</div></main></body></html>",
        html_escape(&payload.interpretation_note),
        html_escape(&payload.privacy_note),
        html_escape(&payload.privacy_note),
        html_escape(&payload.rule_version)
    )
    .expect("writing to a String cannot fail");
    html
}

fn report_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(StudentProfileReportSnapshot, String, String)> {
    let payload_json: String = row.get(15)?;
    let payload: StudentProfileReportPayload =
        serde_json::from_str(&payload_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                15,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok((
        StudentProfileReportSnapshot {
            public_id: row.get(0)?,
            snapshot_public_id: row.get(1)?,
            student_id: row.get(2)?,
            class_id: row.get(3)?,
            report_kind: row.get(4)?,
            purpose: row.get(5)?,
            actor_role: row.get(6)?,
            schema_version: row.get(7)?,
            rule_version: row.get(8)?,
            source_snapshot_payload_sha256: row.get(9)?,
            teacher_assessment_watermark: row.get(10)?,
            payload_sha256: row.get(11)?,
            html_sha256: row.get(12)?,
            suggested_file_name: suggested_file_name(&payload),
            generated_by: row.get(13)?,
            generated_at: row.get(14)?,
        },
        payload_json,
        row.get(16)?,
    ))
}

fn load_report_with_payload(
    conn: &Connection,
    public_id: &str,
) -> CoreResult<Option<(StudentProfileReportSnapshot, String, String)>> {
    Ok(conn
        .query_row(
            "SELECT report.public_id,source.public_id,report.student_id,report.class_id,
                    report.report_kind,report.purpose,report.actor_role,report.schema_version,
                    report.rule_version,report.source_snapshot_payload_sha256,
                    report.teacher_assessment_watermark,report.payload_sha256,
                    report.html_sha256,report.actor_id,report.generated_at,
                    report.payload_json,report.request_payload_sha256
             FROM student_profile_report_snapshots report
             JOIN profile_snapshots source ON source.id=report.profile_snapshot_id
             WHERE report.public_id=?1",
            [public_id],
            report_from_row,
        )
        .optional()?)
}

pub fn load_report_snapshot(
    conn: &Connection,
    public_id: &str,
) -> CoreResult<StudentProfileReportSnapshot> {
    load_report_with_payload(conn, public_id)?
        .map(|(snapshot, _, _)| snapshot)
        .ok_or_else(|| CoreError::NotFound("个人学习掌握报告快照".into()))
}

pub fn create_report_snapshot(
    conn: &mut Connection,
    input: &CreateStudentProfileReportInput<'_>,
) -> CoreResult<StudentProfileReportSnapshot> {
    validate_input(input)?;
    let request_payload_sha256 = request_hash(input)?;
    let transaction = conn.transaction()?;
    let existing = transaction
        .query_row(
            "SELECT public_id,request_payload_sha256
             FROM student_profile_report_snapshots WHERE request_key=?1",
            [input.request_key.trim()],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    if let Some((public_id, stored_hash)) = existing {
        if stored_hash != request_payload_sha256 {
            return Err(CoreError::Invalid(
                "同一请求标识已用于不同的个人学习报告".into(),
            ));
        }
        let report = load_report_snapshot(&transaction, &public_id)?;
        transaction.commit()?;
        return Ok(report);
    }
    let source = profile::get_student_profile(&transaction, input.snapshot_public_id)?
        .ok_or_else(|| CoreError::NotFound("个人掌握快照".into()))?;
    if source.payload_sha256 != input.expected_snapshot_payload_sha256 {
        return Err(CoreError::Invalid(
            "个人掌握快照已变化，请重新打开后再导出".into(),
        ));
    }
    let class_name = transaction
        .query_row(
            "SELECT name FROM classes WHERE id=?1",
            [source.student.class_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("班级".into()))?;
    let generated_at = time::utc_now_rfc3339();
    let assessment_watermark = teacher_assessment_watermark(&source.teacher_assessments)?;
    let payload = build_payload(&source, class_name, generated_at.clone())?;
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| CoreError::Parse(error.to_string()))?;
    let html = payload_html(&payload);
    let payload_sha256 = hashing::sha256_hex(payload_json.as_bytes());
    let html_sha256 = hashing::sha256_hex(html.as_bytes());
    let public_id = ids::new_public_id();
    let inserted = transaction.execute(
        "INSERT INTO student_profile_report_snapshots
          (public_id,request_key,request_payload_sha256,profile_snapshot_id,student_id,class_id,
           report_kind,purpose,actor_role,actor_id,schema_version,rule_version,
           source_snapshot_payload_sha256,teacher_assessment_watermark,payload_sha256,
           html_sha256,payload_json,generated_at)
         SELECT ?1,?2,?3,id,student_id,class_id,?4,?5,?6,?7,?8,?9,
                payload_sha256,?10,?11,?12,?13,?14
         FROM profile_snapshots WHERE public_id=?15",
        params![
            public_id,
            input.request_key.trim(),
            request_payload_sha256,
            input.report_kind,
            input.purpose,
            input.actor_role,
            input.actor_id.trim(),
            STUDENT_PROFILE_REPORT_SCHEMA_VERSION,
            STUDENT_PROFILE_REPORT_RULE_VERSION,
            assessment_watermark,
            payload_sha256,
            html_sha256,
            payload_json,
            generated_at,
            input.snapshot_public_id,
        ],
    )?;
    if inserted != 1 {
        return Err(CoreError::NotFound("个人掌握快照".into()));
    }
    let event_payload = serde_json::json!({
        "schema_version": STUDENT_PROFILE_REPORT_SCHEMA_VERSION,
        "report_public_id": public_id,
        "source_snapshot_public_id": input.snapshot_public_id,
        "student_id": source.student.id,
        "class_id": source.student.class_id,
        "report_kind": input.report_kind,
        "purpose": input.purpose,
        "actor_role": input.actor_role,
        "raw_evidence_included": false,
        "automatic_delivery_enabled": false
    })
    .to_string();
    outbox::create_event(
        &transaction,
        &NewOutboxEvent {
            idempotency_key: &format!("profile:outbox:student-report:{public_id}"),
            event_type: "student_profile_internal_report_created",
            event_version: 1,
            aggregate_type: "student_profile_report_snapshot",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &generated_at,
        },
    )?;
    audit::append(
        &transaction,
        &NewAuditEvent {
            idempotency_key: &format!("profile:audit:student-report:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.actor_id.trim()),
            action: "profile.student_report.created",
            object_type: "student_profile_report_snapshot",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("本机老师明确生成教师内部个人学习掌握报告"),
            meta_json: Some(&event_payload),
            occurred_at: &generated_at,
        },
    )?;
    transaction.commit()?;
    load_report_snapshot(conn, &public_id)
}

fn write_private_html(path: &Path, bytes: &[u8], report_public_id: &str) -> CoreResult<()> {
    if !path.is_absolute() {
        return Err(CoreError::Invalid("导出位置必须是绝对路径".into()));
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .map_or(true, |value| !value.eq_ignore_ascii_case("html"))
    {
        return Err(CoreError::Invalid(
            "个人学习报告必须使用 .html 扩展名".into(),
        ));
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
    let temporary = parent.join(format!(
        ".{file_name}.{report_public_id}.{}.tmp",
        ids::new_public_id()
    ));
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
            .map_err(|error| CoreError::Io(format!("创建报告临时文件失败：{error}")))?;
        file.write_all(bytes)
            .map_err(|error| CoreError::Io(format!("写入报告文件失败：{error}")))?;
        file.sync_all()
            .map_err(|error| CoreError::Io(format!("刷新报告文件失败：{error}")))?;
        std::fs::hard_link(&temporary, path)
            .map_err(|error| CoreError::Io(format!("保存报告文件失败：{error}")))?;
        std::fs::remove_file(&temporary)
            .map_err(|error| CoreError::Io(format!("清理报告临时文件失败：{error}")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn write_report_snapshot_html(
    conn: &Connection,
    report_public_id: &str,
    output_path: &str,
) -> CoreResult<WrittenStudentProfileReport> {
    let (snapshot, payload_json, _) = load_report_with_payload(conn, report_public_id)?
        .ok_or_else(|| CoreError::NotFound("个人学习掌握报告快照".into()))?;
    if hashing::sha256_hex(payload_json.as_bytes()) != snapshot.payload_sha256 {
        return Err(CoreError::Invalid("个人学习报告快照内容校验失败".into()));
    }
    let payload: StudentProfileReportPayload =
        serde_json::from_str(&payload_json).map_err(|error| CoreError::Parse(error.to_string()))?;
    let html = payload_html(&payload);
    if hashing::sha256_hex(html.as_bytes()) != snapshot.html_sha256 {
        return Err(CoreError::Invalid("个人学习报告 HTML 内容校验失败".into()));
    }
    let path = Path::new(output_path);
    write_private_html(path, html.as_bytes(), report_public_id)?;
    Ok(WrittenStudentProfileReport {
        snapshot_public_id: snapshot.public_id,
        file_name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string(),
        byte_size: html.len() as i64,
        sha256: snapshot.html_sha256,
    })
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
        student_id: i64,
        knowledge_public_id: String,
        map_public_id: String,
        snapshot: StudentProfileSnapshot,
    }

    #[allow(clippy::too_many_arguments)]
    fn add_evidence(
        conn: &Connection,
        student_id: i64,
        map_public_id: &str,
        knowledge_public_id: &str,
        key: &str,
        source_type: &str,
        source_ref_type: &str,
        occurred_at: &str,
        value: f64,
    ) {
        create_or_get(
            conn,
            &NewLearningEvidence {
                idempotency_key: key,
                student_id,
                source_module: EvidenceSourceModule::Grading,
                source_type,
                source_ref_type,
                source_ref_id: key,
                source_revision: 1,
                decision_ref_type: Some("grade_decision"),
                decision_ref_id: Some(key),
                decision_revision: Some(1),
                knowledge_node_id: Some(knowledge_public_id),
                ability_dimension_id: None,
                evidence_kind: EvidenceKind::Accuracy,
                value,
                confirmation_level: ConfirmationLevel::TeacherAccepted,
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
            "INSERT INTO classes(name,term) VALUES ('八年级一班','2026')",
            [],
        )
        .unwrap();
        let class_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO students(student_no,name,class_id,enabled)
             VALUES ('01','张三',?1,1)",
            [class_id],
        )
        .unwrap();
        let student_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO k1_textbook_editions
              (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
             VALUES ('edition-report',?1,'pep','2026','八上历史','8','upper','active',
                     '2026-07-01T00:00:00.000Z')",
            [subject_id],
        )
        .unwrap();
        let edition_id = conn.last_insert_rowid();
        let map_public_id = "map-report".to_string();
        conn.execute(
            "INSERT INTO k1_knowledge_maps
              (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
             VALUES (?1,?2,1,'confirmed','2026-07-01T00:00:00.000Z',
                     '2026-07-01T00:00:00.000Z')",
            (&map_public_id, edition_id),
        )
        .unwrap();
        let map_id = conn.last_insert_rowid();
        let knowledge_public_id = "knowledge-report".to_string();
        conn.execute(
            "INSERT INTO k1_knowledge_nodes
              (public_id,stable_id,knowledge_map_id,title,order_index,state,created_at)
             VALUES (?1,'stable-report',?2,'洋务运动失败原因',1,'active',
                     '2026-07-01T00:00:00.000Z')",
            (&knowledge_public_id, map_id),
        )
        .unwrap();
        for (key, source_type, source_ref_type, occurred_at, value) in [
            (
                "assessment-item-1",
                "objective_question",
                "assessment_item",
                "2026-07-05T00:00:00.000Z",
                0.0,
            ),
            (
                "answer-slot-2",
                "fill_blank_slot",
                "answer_slot",
                "2026-07-12T00:00:00.000Z",
                1.0,
            ),
            (
                "rubric-point-3",
                "question_rubric_point",
                "rubric_point",
                "2026-07-20T00:00:00.000Z",
                1.0,
            ),
        ] {
            add_evidence(
                &conn,
                student_id,
                &map_public_id,
                &knowledge_public_id,
                key,
                source_type,
                source_ref_type,
                occurred_at,
                value,
            );
        }
        let snapshot = profile::generate_student_profile(
            &mut conn,
            &profile::GenerateStudentProfileInput {
                scope: profile::StudentProfileScope {
                    class_id,
                    student_id,
                    range_start: "2026-07-01",
                    range_end: "2026-07-31",
                },
                confirmed_by: LOCAL_TEACHER_ACTOR_ID,
            },
        )
        .unwrap();
        let metric = snapshot.knowledge_metrics.first().unwrap();
        crate::teacher_assessments::save_profile_teacher_assessment(
            &mut conn,
            &crate::teacher_assessments::SaveProfileTeacherAssessmentInput {
                snapshot_public_id: &snapshot.public_id,
                node_metric_public_id: &metric.public_id,
                expected_revision: 0,
                assessment: Some("observe"),
                note: Some("课堂口头回答仍需继续观察。"),
                actor_id: LOCAL_TEACHER_ACTOR_ID,
            },
        )
        .unwrap();
        Fixture {
            conn,
            class_id,
            student_id,
            knowledge_public_id,
            map_public_id,
            snapshot,
        }
    }

    fn create_input<'a>(
        request_key: &'a str,
        snapshot: &'a StudentProfileSnapshot,
    ) -> CreateStudentProfileReportInput<'a> {
        CreateStudentProfileReportInput {
            request_key,
            snapshot_public_id: &snapshot.public_id,
            expected_snapshot_payload_sha256: &snapshot.payload_sha256,
            report_kind: "student_learning_summary",
            purpose: "teacher_internal_feedback",
            actor_role: "local_teacher",
            actor_id: LOCAL_TEACHER_ACTOR_ID,
        }
    }

    #[test]
    fn access_matrix_fails_closed_without_real_relationships() {
        assert!(evaluate_report_access("local_teacher", LOCAL_TEACHER_ACTOR_ID).allowed);
        for role in ["guardian", "school_admin", "unknown"] {
            let access = evaluate_report_access(role, "actor-1");
            assert!(!access.allowed);
            assert!(!access.blockers.is_empty());
        }
        assert!(!evaluate_report_access("local_teacher", "another-teacher").allowed);
    }

    #[test]
    fn report_snapshot_is_immutable_idempotent_and_audited() {
        let mut fixture = setup();
        let first = create_report_snapshot(
            &mut fixture.conn,
            &create_input("report-1", &fixture.snapshot),
        )
        .unwrap();
        let second = create_report_snapshot(
            &mut fixture.conn,
            &create_input("report-1", &fixture.snapshot),
        )
        .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.student_id, fixture.student_id);
        assert_eq!(first.class_id, fixture.class_id);
        assert_eq!(first.teacher_assessment_watermark.len(), 64);
        assert!(first.suggested_file_name.ends_with(".html"));
        let conflicting = CreateStudentProfileReportInput {
            expected_snapshot_payload_sha256:
                "0000000000000000000000000000000000000000000000000000000000000000",
            ..create_input("report-1", &fixture.snapshot)
        };
        assert!(create_report_snapshot(&mut fixture.conn, &conflicting).is_err());
        assert!(fixture
            .conn
            .execute(
                "UPDATE student_profile_report_snapshots SET actor_id='tampered'
                 WHERE public_id=?1",
                [&first.public_id],
            )
            .is_err());
        let audit_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM audit_events
                 WHERE action='profile.student_report.created' AND object_id=?1",
                [&first.public_id],
                |row| row.get(0),
            )
            .unwrap();
        let outbox_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM outbox_events
                 WHERE event_type='student_profile_internal_report_created'
                   AND aggregate_id=?1",
                [&first.public_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_count, 1);
        assert_eq!(outbox_count, 1);
    }

    #[test]
    fn stale_source_snapshot_cannot_create_report() {
        let mut fixture = setup();
        add_evidence(
            &fixture.conn,
            fixture.student_id,
            &fixture.map_public_id,
            &fixture.knowledge_public_id,
            "assessment-item-new",
            "objective_question",
            "assessment_item",
            "2026-07-25T00:00:00.000Z",
            0.0,
        );
        let error =
            create_report_snapshot(&mut fixture.conn, &create_input("stale", &fixture.snapshot))
                .unwrap_err()
                .to_string();
        assert!(error.contains("建议重新生成") || error.contains("已有新输入"));
    }

    #[test]
    fn html_is_private_safe_and_refuses_overwrite() {
        let mut fixture = setup();
        let report = create_report_snapshot(
            &mut fixture.conn,
            &create_input("report-html", &fixture.snapshot),
        )
        .unwrap();
        let path = std::env::temp_dir().join(format!("{}.html", ids::new_public_id()));
        let written =
            write_report_snapshot_html(&fixture.conn, &report.public_id, path.to_str().unwrap())
                .unwrap();
        let html = std::fs::read_to_string(&path).unwrap();
        assert_eq!(written.sha256, hashing::sha256_hex(html.as_bytes()));
        assert!(html.contains("教师内部学习反馈"));
        assert!(html.contains("尚未建立家长授权"));
        assert!(html.contains("课堂口头回答仍需继续观察"));
        assert!(!html.contains("assessment-item-1"));
        assert!(!html.contains("<img"));
        assert!(!html.contains("<audio"));
        assert!(!html.contains("raw_transcript"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        assert!(write_report_snapshot_html(
            &fixture.conn,
            &report.public_id,
            path.to_str().unwrap()
        )
        .is_err());
        std::fs::remove_file(path).unwrap();
    }
}
