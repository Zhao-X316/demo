//! M3-4 错题统计与可追溯导出。
//!
//! 统计只消费 M3 当前错题事实和老师已确认错因；不会计算掌握分或学生排名。
//! 导出先冻结不可变快照，再按快照生成 Excel 可直接打开的 UTF-8 CSV。

use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

use chrono::{DateTime, FixedOffset, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};

use crate::read_model::{class_wrongbook_dashboard, WrongbookClass, WrongbookQuestion};

pub const WRONGBOOK_STATISTICS_SCHEMA_VERSION: i64 = 1;
pub const WRONGBOOK_STATISTICS_RULE_VERSION: &str = "m3-current-facts-statistics-v1";
pub const WRONGBOOK_REPORT_SCHEMA_VERSION: i64 = 1;
pub const WRONGBOOK_REPORT_RULE_VERSION: &str = "m3-wrongbook-report-v1";
const MAX_RANGE_DAYS: i64 = 366;
const ACTIVITY_FILTER_RULE: &str =
    "仅纳入最近一次有效发布作答的上海业务日期落在所选范围内的当前错题事实。";
const EVIDENCE_COUNT_RULE: &str =
    "证据数为入选事实关联的全部当前有效已发布老师评分次数，不等同于独立掌握证据数。";

#[derive(Debug, Clone)]
pub struct WrongbookStatisticsScope<'a> {
    pub class_id: i64,
    pub student_id: Option<i64>,
    pub range_start: &'a str,
    pub range_end: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudentReference {
    pub id: i64,
    pub student_no: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrongbookStatisticsMeta {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub range_start: String,
    pub range_end: String,
    pub exam_watermark: Option<String>,
    pub activity_filter_rule: String,
    pub evidence_count_rule: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrongbookStatisticsSummary {
    pub student_count: i64,
    pub fact_count: i64,
    pub evidence_count: i64,
    pub needs_correction_count: i64,
    pub corrected_once_count: i64,
    pub rechecked_correct_count: i64,
    pub repeated_error_count: i64,
    pub confirmed_cause_review_count: i64,
    pub confirmed_cause_item_count: i64,
    pub unlinked_fact_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudentWrongbookFacts {
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub fact_count: i64,
    pub evidence_count: i64,
    pub needs_correction_count: i64,
    pub corrected_once_count: i64,
    pub rechecked_correct_count: i64,
    pub repeated_error_count: i64,
    pub confirmed_cause_review_count: i64,
    pub latest_response_at: Option<String>,
    pub latest_verification_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CauseCount {
    pub cause_code: String,
    pub cause_label: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmedCauseDistribution {
    pub public_id: String,
    pub title: String,
    pub confirmed_review_count: i64,
    pub causes: Vec<CauseCount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrongbookStatistics {
    pub meta: WrongbookStatisticsMeta,
    pub class: WrongbookClass,
    pub selected_student: Option<StudentReference>,
    pub summary: WrongbookStatisticsSummary,
    pub students: Vec<StudentWrongbookFacts>,
    pub question_causes: Vec<ConfirmedCauseDistribution>,
    pub knowledge_causes: Vec<ConfirmedCauseDistribution>,
}

#[derive(Debug)]
struct ValidatedScope {
    range_start: NaiveDate,
    range_end: NaiveDate,
    selected_student: Option<StudentReference>,
}

#[derive(Debug)]
struct StatisticsView {
    statistics: WrongbookStatistics,
    items: Vec<WrongbookQuestion>,
}

#[derive(Default)]
struct StudentFactsBuilder {
    student_id: i64,
    student_no: String,
    student_name: String,
    fact_count: i64,
    evidence_count: i64,
    needs_correction_count: i64,
    corrected_once_count: i64,
    rechecked_correct_count: i64,
    repeated_error_count: i64,
    confirmed_cause_review_count: i64,
    latest_response_at: Option<String>,
    latest_verification_at: Option<String>,
}

#[derive(Default)]
struct CauseDistributionBuilder {
    public_id: String,
    title: String,
    confirmed_review_count: i64,
    causes: BTreeMap<String, (String, i64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReportKind {
    ClassSummary,
    StudentParent,
}

impl ReportKind {
    fn parse(value: &str) -> CoreResult<Self> {
        match value {
            "class_summary" => Ok(Self::ClassSummary),
            "student_parent" => Ok(Self::StudentParent),
            _ => Err(CoreError::Invalid("不支持的报告类型".into())),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::ClassSummary => "class_summary",
            Self::StudentParent => "student_parent",
        }
    }
}

#[derive(Debug, Clone)]
pub struct CreateWrongbookReportInput<'a> {
    pub report_kind: &'a str,
    pub class_id: i64,
    pub student_id: Option<i64>,
    pub range_start: &'a str,
    pub range_end: &'a str,
    pub generated_by: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct WrongbookReportRow {
    student_no: String,
    student_name: String,
    question_version_public_id: String,
    question_type: String,
    stem: String,
    status: String,
    latest_score: f64,
    latest_max_score: f64,
    first_error_at: String,
    last_error_at: String,
    latest_response_at: String,
    published_response_count: i64,
    error_response_count: i64,
    repeated_error: bool,
    confirmed_cause_labels: Vec<String>,
    teacher_note: Option<String>,
    knowledge_titles: Vec<String>,
    ability_titles: Vec<String>,
    correction_status: Option<String>,
    reinforcement_due_date: Option<String>,
    advice: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct WrongbookReportPayload {
    schema_version: i64,
    rule_version: String,
    report_kind: String,
    title: String,
    generated_at: String,
    class: WrongbookClass,
    selected_student: Option<StudentReference>,
    range_start: String,
    range_end: String,
    source_exam_watermark: Option<String>,
    activity_filter_rule: String,
    evidence_count_rule: String,
    privacy_note: String,
    no_ranking_note: String,
    summary: WrongbookStatisticsSummary,
    rows: Vec<WrongbookReportRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrongbookReportSnapshot {
    pub public_id: String,
    pub report_kind: String,
    pub class_id: i64,
    pub student_id: Option<i64>,
    pub range_start: String,
    pub range_end: String,
    pub schema_version: i64,
    pub rule_version: String,
    pub source_exam_watermark: Option<String>,
    pub evidence_count: i64,
    pub fact_count: i64,
    pub confirmed_cause_review_count: i64,
    pub payload_sha256: String,
    pub csv_sha256: String,
    pub suggested_file_name: String,
    pub generated_by: String,
    pub generated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrittenWrongbookReport {
    pub snapshot_public_id: String,
    pub file_name: String,
    pub byte_size: i64,
    pub sha256: String,
}

fn parse_date(value: &str, field: &str) -> CoreResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid(format!("{field}必须是 YYYY-MM-DD 日期")))
}

fn shanghai_date(value: &str) -> CoreResult<NaiveDate> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| CoreError::Parse("已发布证据时间格式无效".into()))?;
    let offset = FixedOffset::east_opt(8 * 60 * 60).expect("valid UTC+8 offset");
    Ok(parsed.with_timezone(&offset).date_naive())
}

fn load_student_reference(
    conn: &Connection,
    class_id: i64,
    student_id: i64,
) -> CoreResult<StudentReference> {
    conn.query_row(
        "SELECT id,student_no,name FROM students
         WHERE id=?1 AND class_id=?2 AND enabled=1",
        (student_id, class_id),
        |row| {
            Ok(StudentReference {
                id: row.get(0)?,
                student_no: row.get(1)?,
                name: row.get(2)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("学生不属于当前班级或已停用".into()))
}

fn validate_scope(
    conn: &Connection,
    scope: &WrongbookStatisticsScope<'_>,
) -> CoreResult<ValidatedScope> {
    let range_start = parse_date(scope.range_start, "开始日期")?;
    let range_end = parse_date(scope.range_end, "结束日期")?;
    if range_start > range_end {
        return Err(CoreError::Invalid("开始日期不能晚于结束日期".into()));
    }
    if (range_end - range_start).num_days() > MAX_RANGE_DAYS {
        return Err(CoreError::Invalid("一次统计范围不能超过 366 天".into()));
    }
    let selected_student = scope
        .student_id
        .map(|student_id| load_student_reference(conn, scope.class_id, student_id))
        .transpose()?;
    Ok(ValidatedScope {
        range_start,
        range_end,
        selected_student,
    })
}

fn natural_student_no(left: &str, right: &str) -> std::cmp::Ordering {
    match (left.parse::<u64>(), right.parse::<u64>()) {
        (Ok(left_number), Ok(right_number)) => {
            left_number.cmp(&right_number).then_with(|| left.cmp(right))
        }
        _ => left.cmp(right),
    }
}

fn cause_label(item: &WrongbookQuestion, code: &str) -> String {
    item.cause_options
        .iter()
        .find(|option| option.code == code)
        .map(|option| option.label.clone())
        .unwrap_or_else(|| code.to_string())
}

fn latest(current: &mut Option<String>, candidate: &str) {
    if current
        .as_deref()
        .map_or(true, |existing| candidate > existing)
    {
        *current = Some(candidate.to_string());
    }
}

fn add_cause(builder: &mut CauseDistributionBuilder, code: &str, label: String) {
    let entry = builder.causes.entry(code.to_string()).or_insert((label, 0));
    entry.1 += 1;
}

fn finish_distributions(
    builders: BTreeMap<String, CauseDistributionBuilder>,
) -> Vec<ConfirmedCauseDistribution> {
    let mut values = builders
        .into_values()
        .map(|builder| ConfirmedCauseDistribution {
            public_id: builder.public_id,
            title: builder.title,
            confirmed_review_count: builder.confirmed_review_count,
            causes: builder
                .causes
                .into_iter()
                .map(|(cause_code, (cause_label, count))| CauseCount {
                    cause_code,
                    cause_label,
                    count,
                })
                .collect(),
        })
        .collect::<Vec<_>>();
    values.sort_by(|left, right| {
        right
            .confirmed_review_count
            .cmp(&left.confirmed_review_count)
            .then_with(|| left.title.cmp(&right.title))
            .then_with(|| left.public_id.cmp(&right.public_id))
    });
    values
}

fn build_statistics_view(
    conn: &Connection,
    scope: &WrongbookStatisticsScope<'_>,
) -> CoreResult<StatisticsView> {
    let validated = validate_scope(conn, scope)?;
    let dashboard = class_wrongbook_dashboard(conn, scope.class_id)?;
    let mut items = dashboard
        .items
        .iter()
        .filter(|item| scope.student_id.map_or(true, |id| item.student_id == id))
        .filter_map(|item| {
            let date = shanghai_date(&item.latest_response_at);
            match date {
                Ok(date) if date >= validated.range_start && date <= validated.range_end => {
                    Some(Ok(item.clone()))
                }
                Ok(_) => None,
                Err(error) => Some(Err(error)),
            }
        })
        .collect::<CoreResult<Vec<_>>>()?;

    items.sort_by(|left, right| {
        natural_student_no(&left.student_no, &right.student_no)
            .then_with(|| left.student_id.cmp(&right.student_id))
            .then_with(|| right.latest_response_at.cmp(&left.latest_response_at))
            .then_with(|| left.question_version_id.cmp(&right.question_version_id))
    });

    let mut students = BTreeMap::<i64, StudentFactsBuilder>::new();
    let mut questions = BTreeMap::<String, CauseDistributionBuilder>::new();
    let mut knowledge = BTreeMap::<String, CauseDistributionBuilder>::new();
    let mut confirmed_cause_review_count = 0_i64;
    let mut confirmed_cause_item_count = 0_i64;
    for item in &items {
        let student = students
            .entry(item.student_id)
            .or_insert_with(|| StudentFactsBuilder {
                student_id: item.student_id,
                student_no: item.student_no.clone(),
                student_name: item.student_name.clone(),
                ..StudentFactsBuilder::default()
            });
        student.fact_count += 1;
        student.evidence_count += item.published_response_count;
        student.repeated_error_count += i64::from(item.repeated_error);
        match item.status.as_str() {
            "needs_correction" => student.needs_correction_count += 1,
            "corrected_once" => student.corrected_once_count += 1,
            "rechecked_correct" => student.rechecked_correct_count += 1,
            _ => {}
        }
        latest(&mut student.latest_response_at, &item.latest_response_at);
        if item.status != "needs_correction" {
            latest(
                &mut student.latest_verification_at,
                &item.latest_response_at,
            );
        }
        let Some(review) = &item.cause_review else {
            continue;
        };
        confirmed_cause_review_count += 1;
        confirmed_cause_item_count += review.cause_codes.len() as i64;
        student.confirmed_cause_review_count += 1;

        let question = questions
            .entry(item.question_version_id.clone())
            .or_insert_with(|| CauseDistributionBuilder {
                public_id: item.question_version_id.clone(),
                title: item.stem.clone(),
                ..CauseDistributionBuilder::default()
            });
        question.confirmed_review_count += 1;
        for code in &review.cause_codes {
            add_cause(question, code, cause_label(item, code));
        }
        for node in &item.knowledge_nodes {
            let distribution = knowledge.entry(node.public_id.clone()).or_insert_with(|| {
                CauseDistributionBuilder {
                    public_id: node.public_id.clone(),
                    title: node.title.clone(),
                    ..CauseDistributionBuilder::default()
                }
            });
            distribution.confirmed_review_count += 1;
            for code in &review.cause_codes {
                add_cause(distribution, code, cause_label(item, code));
            }
        }
    }

    let mut student_facts = students
        .into_values()
        .map(|student| StudentWrongbookFacts {
            student_id: student.student_id,
            student_no: student.student_no,
            student_name: student.student_name,
            fact_count: student.fact_count,
            evidence_count: student.evidence_count,
            needs_correction_count: student.needs_correction_count,
            corrected_once_count: student.corrected_once_count,
            rechecked_correct_count: student.rechecked_correct_count,
            repeated_error_count: student.repeated_error_count,
            confirmed_cause_review_count: student.confirmed_cause_review_count,
            latest_response_at: student.latest_response_at,
            latest_verification_at: student.latest_verification_at,
        })
        .collect::<Vec<_>>();
    student_facts.sort_by(|left, right| {
        natural_student_no(&left.student_no, &right.student_no)
            .then_with(|| left.student_id.cmp(&right.student_id))
    });

    let summary = WrongbookStatisticsSummary {
        student_count: student_facts.len() as i64,
        fact_count: items.len() as i64,
        evidence_count: items.iter().map(|item| item.published_response_count).sum(),
        needs_correction_count: items
            .iter()
            .filter(|item| item.status == "needs_correction")
            .count() as i64,
        corrected_once_count: items
            .iter()
            .filter(|item| item.status == "corrected_once")
            .count() as i64,
        rechecked_correct_count: items
            .iter()
            .filter(|item| item.status == "rechecked_correct")
            .count() as i64,
        repeated_error_count: items.iter().filter(|item| item.repeated_error).count() as i64,
        confirmed_cause_review_count,
        confirmed_cause_item_count,
        unlinked_fact_count: items
            .iter()
            .filter(|item| item.knowledge_nodes.is_empty())
            .count() as i64,
    };
    Ok(StatisticsView {
        statistics: WrongbookStatistics {
            meta: WrongbookStatisticsMeta {
                schema_version: WRONGBOOK_STATISTICS_SCHEMA_VERSION,
                rule_version: WRONGBOOK_STATISTICS_RULE_VERSION.into(),
                calculated_at: time::utc_now_rfc3339(),
                range_start: validated.range_start.to_string(),
                range_end: validated.range_end.to_string(),
                exam_watermark: dashboard.meta.exam_watermark,
                activity_filter_rule: ACTIVITY_FILTER_RULE.into(),
                evidence_count_rule: EVIDENCE_COUNT_RULE.into(),
            },
            class: dashboard.class,
            selected_student: validated.selected_student,
            summary,
            students: student_facts,
            question_causes: finish_distributions(questions),
            knowledge_causes: finish_distributions(knowledge),
        },
        items,
    })
}

pub fn wrongbook_statistics(
    conn: &Connection,
    scope: &WrongbookStatisticsScope<'_>,
) -> CoreResult<WrongbookStatistics> {
    Ok(build_statistics_view(conn, scope)?.statistics)
}

fn advice(item: &WrongbookQuestion) -> String {
    match item.status.as_str() {
        "needs_correction" if item.repeated_error => {
            "重复出错；建议先完成订正，再由老师安排跨日期巩固。".into()
        }
        "needs_correction" => "建议完成订正并核对老师确认的错因。".into(),
        "corrected_once" => item
            .reinforcement_assignment
            .as_ref()
            .map(|assignment| {
                format!(
                    "已安排 {} 跨日期巩固；完成后继续观察。",
                    assignment.due_date
                )
            })
            .unwrap_or_else(|| "已完成一次订正；可由老师安排跨日期再次作答。".into()),
        "rechecked_correct" => "已有再次作答正确记录；继续观察，不直接解释为稳定掌握。".into(),
        _ => "请结合原题、老师评分和后续作答继续观察。".into(),
    }
}

fn report_rows(items: &[WrongbookQuestion]) -> Vec<WrongbookReportRow> {
    items
        .iter()
        .map(|item| {
            let confirmed_cause_labels = item
                .cause_review
                .as_ref()
                .map(|review| {
                    review
                        .cause_codes
                        .iter()
                        .map(|code| cause_label(item, code))
                        .collect()
                })
                .unwrap_or_default();
            WrongbookReportRow {
                student_no: item.student_no.clone(),
                student_name: item.student_name.clone(),
                question_version_public_id: item.question_version_id.clone(),
                question_type: item.question_type.clone(),
                stem: item.stem.clone(),
                status: item.status.clone(),
                latest_score: item.latest_score,
                latest_max_score: item.latest_max_score,
                first_error_at: item.first_error_at.clone(),
                last_error_at: item.last_error_at.clone(),
                latest_response_at: item.latest_response_at.clone(),
                published_response_count: item.published_response_count,
                error_response_count: item.error_response_count,
                repeated_error: item.repeated_error,
                confirmed_cause_labels,
                teacher_note: item
                    .cause_review
                    .as_ref()
                    .and_then(|review| review.teacher_note.clone()),
                knowledge_titles: item
                    .knowledge_nodes
                    .iter()
                    .map(|node| node.title.clone())
                    .collect(),
                ability_titles: item
                    .ability_dimensions
                    .iter()
                    .map(|node| node.title.clone())
                    .collect(),
                correction_status: item
                    .correction_assignment
                    .as_ref()
                    .map(|assignment| assignment.status.clone()),
                reinforcement_due_date: item
                    .reinforcement_assignment
                    .as_ref()
                    .map(|assignment| assignment.due_date.clone()),
                advice: advice(item),
            }
        })
        .collect()
}

fn report_title(
    kind: ReportKind,
    class: &WrongbookClass,
    student: Option<&StudentReference>,
) -> String {
    match kind {
        ReportKind::ClassSummary => format!("{}错题事实汇总", class.name),
        ReportKind::StudentParent => {
            let student = student.expect("student report has a student");
            format!(
                "{}{}号{}学习事实报告",
                class.name, student.student_no, student.name
            )
        }
    }
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

fn suggested_file_name(payload: &WrongbookReportPayload) -> String {
    format!(
        "{}_{}_至{}.csv",
        safe_name(&payload.title),
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

fn score_text(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn status_label(status: &str) -> &str {
    match status {
        "needs_correction" => "待订正",
        "corrected_once" => "已订正一次",
        "rechecked_correct" => "再次作答正确",
        _ => "未知",
    }
}

fn question_type_label(question_type: &str) -> &str {
    match question_type {
        "single" => "单选",
        "multiple" => "多选",
        "true_false" => "判断",
        "fill_blank" => "填空",
        "short_answer" => "简答",
        _ => question_type,
    }
}

fn report_csv(payload: &WrongbookReportPayload) -> String {
    let mut output = String::from("\u{feff}");
    push_csv_row(&mut output, &["报告信息".into(), String::new()]);
    for (label, value) in [
        ("报告标题", payload.title.clone()),
        ("报告类型", payload.report_kind.clone()),
        ("班级", payload.class.name.clone()),
        (
            "学生范围",
            payload
                .selected_student
                .as_ref()
                .map(|student| format!("{}号 {}", student.student_no, student.name))
                .unwrap_or_else(|| "全班（按学号排列，不含排名）".into()),
        ),
        (
            "时间范围",
            format!("{} 至 {}", payload.range_start, payload.range_end),
        ),
        ("生成时间", payload.generated_at.clone()),
        ("统计口径版本", payload.rule_version.clone()),
        (
            "来源发布水位",
            payload
                .source_exam_watermark
                .clone()
                .unwrap_or_else(|| "暂无".into()),
        ),
        ("当前事实数", payload.summary.fact_count.to_string()),
        ("关联证据数", payload.summary.evidence_count.to_string()),
        (
            "老师确认错因数",
            payload.summary.confirmed_cause_review_count.to_string(),
        ),
        ("活动筛选口径", payload.activity_filter_rule.clone()),
        ("证据数量口径", payload.evidence_count_rule.clone()),
        ("隐私说明", payload.privacy_note.clone()),
        ("排序说明", payload.no_ranking_note.clone()),
    ] {
        push_csv_row(&mut output, &[label.into(), value]);
    }
    output.push_str("\r\n");
    push_csv_row(&mut output, &["学习事实".into()]);
    push_csv_row(
        &mut output,
        &[
            "学号".into(),
            "姓名".into(),
            "题型".into(),
            "题目".into(),
            "当前状态".into(),
            "最近得分".into(),
            "满分".into(),
            "首次错误".into(),
            "最近错误".into(),
            "最近验证".into(),
            "已发布作答次数".into(),
            "非满分次数".into(),
            "是否重复出错".into(),
            "老师确认错因".into(),
            "老师备注".into(),
            "知识点".into(),
            "能力维度".into(),
            "订正状态".into(),
            "巩固到期日".into(),
            "建议".into(),
            "题目版本".into(),
        ],
    );
    for row in &payload.rows {
        push_csv_row(
            &mut output,
            &[
                row.student_no.clone(),
                row.student_name.clone(),
                question_type_label(&row.question_type).into(),
                row.stem.clone(),
                status_label(&row.status).into(),
                score_text(row.latest_score),
                score_text(row.latest_max_score),
                row.first_error_at.clone(),
                row.last_error_at.clone(),
                row.latest_response_at.clone(),
                row.published_response_count.to_string(),
                row.error_response_count.to_string(),
                if row.repeated_error { "是" } else { "否" }.into(),
                row.confirmed_cause_labels.join("；"),
                row.teacher_note.clone().unwrap_or_default(),
                row.knowledge_titles.join("；"),
                row.ability_titles.join("；"),
                row.correction_status.clone().unwrap_or_default(),
                row.reinforcement_due_date.clone().unwrap_or_default(),
                row.advice.clone(),
                row.question_version_public_id.clone(),
            ],
        );
    }
    output
}

fn payload_for(kind: ReportKind, view: StatisticsView) -> CoreResult<WrongbookReportPayload> {
    if kind == ReportKind::StudentParent && view.statistics.selected_student.is_none() {
        return Err(CoreError::Invalid("家长沟通报告必须选择一名学生".into()));
    }
    if kind == ReportKind::ClassSummary && view.statistics.selected_student.is_some() {
        return Err(CoreError::Invalid("班级汇总不能同时限定单名学生".into()));
    }
    let title = report_title(
        kind,
        &view.statistics.class,
        view.statistics.selected_student.as_ref(),
    );
    Ok(WrongbookReportPayload {
        schema_version: WRONGBOOK_REPORT_SCHEMA_VERSION,
        rule_version: WRONGBOOK_REPORT_RULE_VERSION.into(),
        report_kind: kind.as_str().into(),
        title,
        generated_at: view.statistics.meta.calculated_at.clone(),
        class: view.statistics.class,
        selected_student: view.statistics.selected_student,
        range_start: view.statistics.meta.range_start,
        range_end: view.statistics.meta.range_end,
        source_exam_watermark: view.statistics.meta.exam_watermark,
        activity_filter_rule: view.statistics.meta.activity_filter_rule,
        evidence_count_rule: view.statistics.meta.evidence_count_rule,
        privacy_note: match kind {
            ReportKind::ClassSummary => {
                "仅供当前老师内部教学使用；包含本班学生姓名，不应公开传播。".into()
            }
            ReportKind::StudentParent => "只包含所选学生的学习事实，不含同班其他学生信息。".into(),
        },
        no_ranking_note: "按学号自然顺序输出；不含名次、班级排名或掌握总分。".into(),
        summary: view.statistics.summary,
        rows: report_rows(&view.items),
    })
}

fn snapshot_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<(WrongbookReportSnapshot, String)> {
    let payload_json: String = row.get(14)?;
    let payload: WrongbookReportPayload = serde_json::from_str(&payload_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(14, rusqlite::types::Type::Text, Box::new(error))
    })?;
    Ok((
        WrongbookReportSnapshot {
            public_id: row.get(0)?,
            report_kind: row.get(1)?,
            class_id: row.get(2)?,
            student_id: row.get(3)?,
            range_start: row.get(4)?,
            range_end: row.get(5)?,
            schema_version: row.get(6)?,
            rule_version: row.get(7)?,
            source_exam_watermark: row.get(8)?,
            evidence_count: row.get(9)?,
            fact_count: row.get(10)?,
            confirmed_cause_review_count: row.get(11)?,
            payload_sha256: row.get(12)?,
            csv_sha256: row.get(13)?,
            suggested_file_name: suggested_file_name(&payload),
            generated_by: row.get(15)?,
            generated_at: row.get(16)?,
        },
        payload_json,
    ))
}

fn load_snapshot_with_payload(
    conn: &Connection,
    public_id: &str,
) -> CoreResult<Option<(WrongbookReportSnapshot, String)>> {
    Ok(conn
        .query_row(
            "SELECT public_id,report_kind,class_id,student_id,range_start,range_end,
                    schema_version,rule_version,source_exam_watermark,evidence_count,
                    fact_count,confirmed_cause_review_count,payload_sha256,csv_sha256,
                    payload_json,generated_by,generated_at
             FROM wb_report_snapshots WHERE public_id=?1",
            [public_id],
            snapshot_from_row,
        )
        .optional()?)
}

pub fn load_report_snapshot(
    conn: &Connection,
    public_id: &str,
) -> CoreResult<WrongbookReportSnapshot> {
    load_snapshot_with_payload(conn, public_id)?
        .map(|(snapshot, _)| snapshot)
        .ok_or_else(|| CoreError::NotFound("错题报告快照".into()))
}

pub fn create_report_snapshot(
    conn: &mut Connection,
    input: &CreateWrongbookReportInput<'_>,
) -> CoreResult<WrongbookReportSnapshot> {
    if input.generated_by.trim().is_empty() {
        return Err(CoreError::Invalid("报告生成人不能为空".into()));
    }
    let kind = ReportKind::parse(input.report_kind)?;
    let expected_student = match kind {
        ReportKind::ClassSummary => None,
        ReportKind::StudentParent => Some(
            input
                .student_id
                .ok_or_else(|| CoreError::Invalid("家长沟通报告必须选择一名学生".into()))?,
        ),
    };
    if kind == ReportKind::ClassSummary && input.student_id.is_some() {
        return Err(CoreError::Invalid("班级汇总不能同时限定单名学生".into()));
    }
    let view = build_statistics_view(
        conn,
        &WrongbookStatisticsScope {
            class_id: input.class_id,
            student_id: expected_student,
            range_start: input.range_start,
            range_end: input.range_end,
        },
    )?;
    let payload = payload_for(kind, view)?;
    let payload_json =
        serde_json::to_string(&payload).map_err(|error| CoreError::Parse(error.to_string()))?;
    let csv = report_csv(&payload);
    let payload_sha256 = hashing::sha256_hex(payload_json.as_bytes());
    let csv_sha256 = hashing::sha256_hex(csv.as_bytes());
    let public_id = ids::new_public_id();
    let transaction = conn.transaction()?;
    transaction.execute(
        "INSERT INTO wb_report_snapshots
         (public_id,report_kind,class_id,student_id,range_start,range_end,
          schema_version,rule_version,source_exam_watermark,evidence_count,fact_count,
          confirmed_cause_review_count,payload_sha256,csv_sha256,payload_json,
          generated_by,generated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
        params![
            public_id,
            payload.report_kind,
            payload.class.id,
            payload.selected_student.as_ref().map(|student| student.id),
            payload.range_start,
            payload.range_end,
            payload.schema_version,
            payload.rule_version,
            payload.source_exam_watermark,
            payload.summary.evidence_count,
            payload.summary.fact_count,
            payload.summary.confirmed_cause_review_count,
            payload_sha256,
            csv_sha256,
            payload_json,
            input.generated_by.trim(),
            payload.generated_at,
        ],
    )?;
    transaction.commit()?;
    load_report_snapshot(conn, &public_id)
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

pub fn write_report_snapshot_csv(
    conn: &Connection,
    snapshot_public_id: &str,
    output_path: &str,
) -> CoreResult<WrittenWrongbookReport> {
    let (snapshot, payload_json) = load_snapshot_with_payload(conn, snapshot_public_id)?
        .ok_or_else(|| CoreError::NotFound("错题报告快照".into()))?;
    if hashing::sha256_hex(payload_json.as_bytes()) != snapshot.payload_sha256 {
        return Err(CoreError::Invalid("报告快照内容校验失败".into()));
    }
    let payload: WrongbookReportPayload =
        serde_json::from_str(&payload_json).map_err(|error| CoreError::Parse(error.to_string()))?;
    let csv = report_csv(&payload);
    if hashing::sha256_hex(csv.as_bytes()) != snapshot.csv_sha256 {
        return Err(CoreError::Invalid("报告 CSV 内容校验失败".into()));
    }
    let path = Path::new(output_path);
    write_private_file(path, csv.as_bytes(), snapshot_public_id)?;
    Ok(WrittenWrongbookReport {
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
    fn csv_escapes_cells_and_contains_no_ranking_column() {
        let payload = WrongbookReportPayload {
            schema_version: 1,
            rule_version: WRONGBOOK_REPORT_RULE_VERSION.into(),
            report_kind: "student_parent".into(),
            title: "学生报告".into(),
            generated_at: "2026-07-17T00:00:00.000Z".into(),
            class: WrongbookClass {
                id: 1,
                name: "八年级一班".into(),
                term: None,
                textbook: None,
                enabled_student_count: 1,
            },
            selected_student: Some(StudentReference {
                id: 1,
                student_no: "01".into(),
                name: "小林".into(),
            }),
            range_start: "2026-07-01".into(),
            range_end: "2026-07-17".into(),
            source_exam_watermark: None,
            activity_filter_rule: ACTIVITY_FILTER_RULE.into(),
            evidence_count_rule: EVIDENCE_COUNT_RULE.into(),
            privacy_note: "只含一名学生".into(),
            no_ranking_note: "不含排名".into(),
            summary: WrongbookStatisticsSummary {
                student_count: 1,
                fact_count: 1,
                evidence_count: 2,
                needs_correction_count: 1,
                corrected_once_count: 0,
                rechecked_correct_count: 0,
                repeated_error_count: 1,
                confirmed_cause_review_count: 1,
                confirmed_cause_item_count: 1,
                unlinked_fact_count: 0,
            },
            rows: vec![WrongbookReportRow {
                student_no: "01".into(),
                student_name: "小林".into(),
                question_version_public_id: "qv-1".into(),
                question_type: "single".into(),
                stem: "原因是“制度,局限”吗？".into(),
                status: "needs_correction".into(),
                latest_score: 0.0,
                latest_max_score: 2.0,
                first_error_at: "2026-07-01T00:00:00Z".into(),
                last_error_at: "2026-07-02T00:00:00Z".into(),
                latest_response_at: "2026-07-02T00:00:00Z".into(),
                published_response_count: 2,
                error_response_count: 2,
                repeated_error: true,
                confirmed_cause_labels: vec!["概念混淆".into()],
                teacher_note: None,
                knowledge_titles: vec!["洋务运动".into()],
                ability_titles: vec![],
                correction_status: None,
                reinforcement_due_date: None,
                advice: "建议订正".into(),
            }],
        };
        let csv = report_csv(&payload);
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("\"原因是“制度,局限”吗？\""));
        assert!(csv.contains("不含排名"));
        assert!(!csv.contains("排名,"));
    }
}
