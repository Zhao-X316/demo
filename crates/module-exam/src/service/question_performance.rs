//! K1-5 题目实际表现与版本变更影响预览。
//!
//! 表现统计只读取 attempt 当前有效 publication 中老师确认的 grade decision，
//! 不读取 legacy 聚合，也不把单班表现写回预计难度。版本影响确认只冻结计划和
//! 复核清单；不得直接修改 assessment item、成绩、发布、学习证据或图谱快照。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use super::assessment::{self, GradeDecision, NewGradeDecision, Publication};

mod default_upgrade;
pub use default_upgrade::upgrade_assessment_default_from_impact;
mod impact_plan_confirmation;
pub use impact_plan_confirmation::confirm_question_impact_plan;
mod review_case_preparation;
pub use review_case_preparation::prepare_question_impact_review_cases;
mod read_model;
use read_model::load_target;
pub use read_model::{
    list_question_impact_review_cases, list_question_performance, preview_question_version_impact,
};

pub const QUESTION_PERFORMANCE_SCHEMA_VERSION: i64 = 1;
pub const QUESTION_PERFORMANCE_RULE_VERSION: &str = "k1-current-publication-performance-v1";
pub const QUESTION_IMPACT_RULE_VERSION: &str = "k1-version-impact-plan-v1";
pub const QUESTION_IMPACT_REVIEW_RULE_VERSION: &str = "k1-version-impact-review-case-v1";
pub const QUESTION_IMPACT_RESOLUTION_RULE_VERSION: &str = "k1-version-impact-teacher-resolution-v1";
pub const ASSESSMENT_DEFAULT_UPGRADE_RULE_VERSION: &str = "m2-assessment-future-default-upgrade-v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceContextBreakdown {
    pub assessment_context: String,
    pub published_response_count: i64,
    pub average_score_rate: Option<f64>,
    pub full_credit_rate: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionPerformanceItem {
    pub question_version_public_id: String,
    pub revision: i64,
    pub owner_scope: String,
    pub question_type: String,
    pub stem: String,
    pub max_score: f64,
    pub quality_level: String,
    pub state: String,
    pub assessment_usage_count: i64,
    pub published_response_count: i64,
    pub full_credit_count: i64,
    pub partial_credit_count: i64,
    pub zero_score_count: i64,
    pub average_score_rate: Option<f64>,
    pub full_credit_rate: Option<f64>,
    pub first_attempt_count: i64,
    pub correction_attempt_count: i64,
    pub latest_published_at: Option<String>,
    pub context_breakdown: Vec<PerformanceContextBreakdown>,
    pub has_version_update_impact: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionPerformanceCatalog {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub items: Vec<QuestionPerformanceItem>,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImpactTargetVersion {
    pub answer_key_version_public_id: String,
    pub answer_key_revision: i64,
    pub rubric_version_public_id: String,
    pub rubric_revision: i64,
    pub link_set_public_id: String,
    pub link_set_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionImpactRow {
    pub assessment_public_id: String,
    pub assessment_title: String,
    pub class_name: String,
    pub assessment_version_public_id: String,
    pub assessment_version_revision: i64,
    pub assessment_item_public_id: String,
    pub source_answer_key_version_public_id: String,
    pub source_rubric_version_public_id: String,
    pub source_link_set_public_id: String,
    pub answer_changed: bool,
    pub rubric_changed: bool,
    pub link_changed: bool,
    pub unpublished_attempt_count: i64,
    pub published_attempt_count: i64,
    pub active_learning_evidence_count: i64,
    pub profile_snapshot_count: i64,
    pub is_current_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionVersionImpactPreview {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub preview_hash: String,
    pub question_version_public_id: String,
    pub question_type: String,
    pub stem: String,
    pub target: ImpactTargetVersion,
    pub affected_assessment_count: i64,
    pub affected_assessment_version_count: i64,
    pub affected_item_count: i64,
    pub unpublished_attempt_count: i64,
    pub published_attempt_count: i64,
    pub active_learning_evidence_count: i64,
    pub profile_snapshot_count: i64,
    pub rows: Vec<QuestionImpactRow>,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmQuestionImpactPlanRequest {
    pub request_key: String,
    pub question_version_public_id: String,
    pub expected_preview_hash: String,
    pub action: String,
    pub planned_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionImpactPlan {
    pub public_id: String,
    pub question_version_public_id: String,
    pub expected_preview_hash: String,
    pub action: String,
    pub task_count: i64,
    pub planned_by: String,
    pub planned_at: String,
    pub changes_assessment_binding: bool,
    pub changes_grade: bool,
    pub changes_publication: bool,
    pub changes_learning_evidence: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpgradeAssessmentDefaultRequest {
    pub request_key: String,
    pub plan_public_id: String,
    pub source_assessment_version_public_id: String,
    pub expected_current_default_version_public_id: String,
    pub upgraded_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessmentDefaultUpgrade {
    pub selection_public_id: String,
    pub plan_public_id: String,
    pub assessment_public_id: String,
    pub assessment_title: String,
    pub source_assessment_version_public_id: String,
    pub source_revision: i64,
    pub default_assessment_version_public_id: String,
    pub default_revision: i64,
    pub upgraded_item_count: i64,
    pub selected_by: String,
    pub selected_at: String,
    pub default_for_future_intake: bool,
    pub changes_historical_attempts: bool,
    pub changes_grade: bool,
    pub changes_publication: bool,
    pub changes_learning_evidence: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionImpactTargetComponent {
    pub source_type: String,
    pub source_public_id: String,
    pub stable_id: String,
    pub order_index: i64,
    pub label: String,
    pub max_score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionImpactReviewCase {
    pub public_id: String,
    pub impact_task_public_id: String,
    pub plan_public_id: String,
    pub case_kind: String,
    pub assessment_title: String,
    pub class_name: String,
    pub student_no: String,
    pub student_name: String,
    pub question_version_public_id: String,
    pub question_type: String,
    pub question_stem: String,
    pub question_no: i64,
    pub max_score: f64,
    pub attempt_public_id: String,
    pub attempt_state: String,
    pub publication_public_id: Option<String>,
    pub source_grade_decision_public_id: Option<String>,
    pub source_grade_decision_revision: Option<i64>,
    pub source_teacher_score: Option<f64>,
    pub source_point_results_json: Option<String>,
    pub source_snapshot_hash: String,
    pub student_response_state: Option<String>,
    pub student_response_text: Option<String>,
    pub crop_path: Option<String>,
    pub target_answer_json: String,
    pub target_components: Vec<QuestionImpactTargetComponent>,
    pub target_answer_key_version_public_id: String,
    pub target_answer_key_revision: i64,
    pub target_rubric_version_public_id: String,
    pub target_rubric_revision: i64,
    pub target_link_set_public_id: String,
    pub target_link_set_revision: i64,
    pub prepared_by: String,
    pub prepared_at: String,
    pub state: String,
    pub resolution_public_id: Option<String>,
    pub resolved_grade_decision_public_id: Option<String>,
    pub resolved_teacher_score: Option<f64>,
    pub resolved_at: Option<String>,
    pub active_publication_public_id: Option<String>,
    pub next_step_note: String,
    pub changes_assessment_binding: bool,
    pub changes_grade: bool,
    pub changes_publication: bool,
    pub changes_learning_evidence: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveQuestionImpactComponentInput {
    pub source_public_id: String,
    pub teacher_score: f64,
    pub evidence_text: Option<String>,
    pub teacher_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveQuestionImpactReviewCaseRequest {
    pub request_key: String,
    pub case_public_id: String,
    pub expected_source_snapshot_hash: String,
    pub teacher_score: Option<f64>,
    pub components: Vec<ResolveQuestionImpactComponentInput>,
    pub teacher_note: String,
    pub resolved_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveQuestionImpactReviewCaseResult {
    pub case_public_id: String,
    pub resolution_public_id: String,
    pub grade_decision: GradeDecision,
    pub old_publication_unchanged: bool,
    pub learning_evidence_unchanged: bool,
    pub requires_explicit_publication: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishQuestionImpactReviewCaseRequest {
    pub case_public_id: String,
    pub expected_grade_decision_public_id: String,
    pub published_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishQuestionImpactReviewCaseResult {
    pub case_public_id: String,
    pub publication: Publication,
    pub prior_publication_superseded: bool,
    pub learning_evidence_switched: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionImpactReviewCaseCatalog {
    pub schema_version: i64,
    pub rule_version: String,
    pub plan_public_id: String,
    pub cases: Vec<QuestionImpactReviewCase>,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareQuestionImpactReviewCasesRequest {
    pub plan_public_id: String,
    pub expected_task_count: i64,
    pub prepared_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareQuestionImpactReviewCasesResult {
    pub plan_public_id: String,
    pub task_count: i64,
    pub created_count: i64,
    pub existing_count: i64,
    pub cases: Vec<QuestionImpactReviewCase>,
    pub changes_assessment_binding: bool,
    pub changes_grade: bool,
    pub changes_publication: bool,
    pub changes_learning_evidence: bool,
}

#[derive(Debug, Clone)]
struct TargetIds {
    question_version_id: i64,
    target_answer_key_version_id: i64,
    target_rubric_version_id: i64,
    target_link_set_id: i64,
}

#[derive(Debug, Clone)]
struct TaskScope {
    attempt_id: i64,
    assessment_item_id: i64,
    publication_id: Option<i64>,
    source_answer_key_version_id: i64,
    source_rubric_version_id: i64,
    source_link_set_id: i64,
}

#[derive(Debug, Clone)]
struct ReviewTaskSnapshot {
    task_id: i64,
    task_public_id: String,
    task_kind: String,
    attempt_id: i64,
    assessment_item_id: i64,
    publication_id: Option<i64>,
    source_grade_decision_id: Option<i64>,
    source_grade_decision_public_id: Option<String>,
    source_grade_decision_revision: Option<i64>,
    source_teacher_score: Option<f64>,
    source_point_results_json: Option<String>,
    target_answer_key_version_id: i64,
    target_rubric_version_id: i64,
    target_link_set_id: i64,
}

#[derive(Debug, Clone)]
struct ReviewResolutionScope {
    case_id: i64,
    case_kind: String,
    attempt_id: i64,
    assessment_item_id: i64,
    publication_id: Option<i64>,
    question_type: String,
    max_score: f64,
    source_grade_decision_id: Option<i64>,
    source_grade_decision_revision: Option<i64>,
    source_snapshot_hash: String,
    target_answer_key_version_id: i64,
    target_answer_key_version_public_id: String,
    target_rubric_version_id: i64,
    target_rubric_version_public_id: String,
    target_link_set_id: i64,
    target_link_set_public_id: String,
    student_response_text: Option<String>,
    attempt_state: String,
    current_grade_decision_id: Option<i64>,
    current_grade_decision_revision: Option<i64>,
    max_grade_decision_revision: i64,
    active_publication_id: Option<i64>,
}

#[derive(Debug, Clone)]
struct PublishReviewScope {
    attempt_id: i64,
    grade_decision_id: i64,
    grade_decision_public_id: String,
    attempt_state: String,
    active_publication_id: Option<i64>,
    current_grade_decision_public_id: String,
    original_publication_id: Option<i64>,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn canonical_rate(value: Option<f64>) -> Option<f64> {
    value.map(|value| (value.clamp(0.0, 1.0) * 10_000.0).round() / 10_000.0)
}

fn accessible_question_clause() -> &'static str {
    "(question.owner_scope='personal' AND question.owner_id=?1)
      OR (question.owner_scope='official' AND question.sharing_allowed=1)"
}

fn target_component_label(source_type: &str, raw_label: &str) -> String {
    if source_type != "answer_slot" {
        return raw_label.trim().to_owned();
    }
    let parsed: Option<Value> = serde_json::from_str(raw_label).ok();
    let answers = parsed.as_ref().and_then(|value| {
        value
            .get("canonical_answers")
            .or_else(|| value.get("answers"))
            .and_then(Value::as_array)
    });
    let labels = answers
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if labels.is_empty() {
        raw_label.trim().to_owned()
    } else {
        labels.join(" / ")
    }
}

fn load_target_components(
    conn: &Connection,
    question_type: &str,
    answer_key_version_public_id: &str,
    rubric_version_public_id: &str,
) -> CoreResult<Vec<QuestionImpactTargetComponent>> {
    let (source_type, sql, version_public_id) = match question_type {
        "single" | "multiple" | "true_false" => return Ok(Vec::new()),
        "fill_blank" => (
            "answer_slot",
            "SELECT slot.public_id,slot.stable_id,slot.order_index,
                    slot.canonical_answers_json,slot.max_score
             FROM k1_answer_key_versions version
             JOIN k1_answer_slots slot ON slot.answer_key_version_id=version.id
             WHERE version.public_id=?1
             ORDER BY slot.order_index,slot.id",
            answer_key_version_public_id,
        ),
        "short_answer" => (
            "rubric_point",
            "SELECT point.public_id,point.stable_id,point.order_index,
                    point.canonical_text,point.max_score
             FROM k1_rubric_versions version
             JOIN k1_rubric_points point ON point.rubric_version_id=version.id
             WHERE version.public_id=?1
             ORDER BY point.order_index,point.id",
            rubric_version_public_id,
        ),
        _ => return Err(CoreError::Invalid("版本影响 case 的题型不受支持".into())),
    };
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([version_public_id], |row| {
        let raw_label: String = row.get(3)?;
        Ok(QuestionImpactTargetComponent {
            source_type: source_type.into(),
            source_public_id: row.get(0)?,
            stable_id: row.get(1)?,
            order_index: row.get(2)?,
            label: target_component_label(source_type, &raw_label),
            max_score: row.get(4)?,
        })
    })?;
    let components = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if components.is_empty() {
        return Err(CoreError::Invalid(
            "目标答案或评分规则缺少可确认的逐项结构".into(),
        ));
    }
    Ok(components)
}

fn grade_decision_by_id(conn: &Connection, id: i64) -> CoreResult<GradeDecision> {
    conn.query_row(
        "SELECT id,public_id,attempt_id,assessment_item_id,revision,
                machine_grade_ai_run_id,teacher_score,point_results_json,teacher_note,
                confirmation_level,state,decided_by,decided_at
         FROM exam_grade_decisions_v2 WHERE id=?1",
        [id],
        |row| {
            Ok(GradeDecision {
                id: row.get(0)?,
                public_id: row.get(1)?,
                attempt_id: row.get(2)?,
                assessment_item_id: row.get(3)?,
                revision: row.get(4)?,
                machine_grade_ai_run_id: row.get(5)?,
                teacher_score: row.get(6)?,
                point_results_json: row.get(7)?,
                teacher_note: row.get(8)?,
                confirmation_level: row.get(9)?,
                state: row.get(10)?,
                decided_by: row.get(11)?,
                decided_at: row.get(12)?,
            })
        },
    )
    .map_err(CoreError::from)
}

fn publication_by_id(
    conn: &Connection,
    publication_id: i64,
    attempt_id: i64,
) -> CoreResult<Publication> {
    conn.query_row(
        "SELECT publication.id,publication.public_id,publication.assessment_version_id,
                publication.revision,publication.state,publication_item.attempt_id,
                publication_item.total_score,publication_item.grade_decision_set_hash,
                publication.published_by,publication.published_at
         FROM exam_grade_publications_v2 publication
         JOIN exam_grade_publication_items_v2 publication_item
           ON publication_item.publication_id=publication.id
          AND publication_item.attempt_id=?2
         WHERE publication.id=?1",
        (publication_id, attempt_id),
        |row| {
            Ok(Publication {
                id: row.get(0)?,
                public_id: row.get(1)?,
                assessment_version_id: row.get(2)?,
                revision: row.get(3)?,
                state: row.get(4)?,
                attempt_id: row.get(5)?,
                total_score: row.get(6)?,
                grade_decision_set_hash: row.get(7)?,
                published_by: row.get(8)?,
                published_at: row.get(9)?,
            })
        },
    )
    .map_err(CoreError::from)
}

fn review_resolution_scope(
    conn: &Connection,
    owner_id: &str,
    case_public_id: &str,
) -> CoreResult<ReviewResolutionScope> {
    conn.query_row(
        "SELECT review_case.id,review_case.case_kind,attempt.id,item.id,
                review_case.publication_id,question.question_type,item.score,
                review_case.source_grade_decision_id,
                review_case.source_grade_decision_revision,
                review_case.source_snapshot_hash,
                target_answer.id,target_answer.public_id,
                target_rubric.id,target_rubric.public_id,
                target_link.id,target_link.public_id,
                COALESCE(
                  (SELECT COALESCE(transcription.teacher_corrected_text,
                                   transcription.normalized_text,
                                   transcription.raw_ocr_text)
                   FROM exam_subjective_transcription_revisions_v2 transcription
                   WHERE transcription.attempt_id=attempt.id
                     AND transcription.assessment_item_id=item.id
                     AND transcription.state='active'
                   ORDER BY transcription.revision DESC,transcription.id DESC LIMIT 1),
                  (SELECT observation.observed_answer_json
                   FROM exam_objective_observation_revisions_v2 observation
                   WHERE observation.attempt_id=attempt.id
                     AND observation.assessment_item_id=item.id
                     AND observation.state='active'
                   ORDER BY observation.revision DESC,observation.id DESC LIMIT 1)
                ),
                attempt.state,current_decision.id,current_decision.revision,
                COALESCE((
                  SELECT MAX(history.revision)
                  FROM exam_grade_decisions_v2 history
                  WHERE history.attempt_id=attempt.id
                    AND history.assessment_item_id=item.id
                ),0),
                attempt.active_publication_id
         FROM exam_question_version_review_cases_v2 review_case
         JOIN exam_question_version_impact_tasks_v2 task
           ON task.id=review_case.impact_task_id
         JOIN exam_question_version_impact_plans_v2 plan
           ON plan.id=task.impact_plan_id
         JOIN exam_attempts_v2 attempt ON attempt.id=review_case.attempt_id
         JOIN exam_assessment_items_v2 item ON item.id=review_case.assessment_item_id
         JOIN k1_question_versions question ON question.id=item.question_version_id
         JOIN k1_answer_key_versions target_answer
           ON target_answer.id=review_case.target_answer_key_version_id
         JOIN k1_rubric_versions target_rubric
           ON target_rubric.id=review_case.target_rubric_version_id
         JOIN k1_link_sets target_link
           ON target_link.id=review_case.target_link_set_id
         LEFT JOIN exam_grade_decisions_v2 current_decision
           ON current_decision.attempt_id=attempt.id
          AND current_decision.assessment_item_id=item.id
          AND current_decision.state='active'
         WHERE review_case.public_id=?1
           AND review_case.state='open'
           AND plan.planned_by=?2",
        params![case_public_id.trim(), owner_id.trim()],
        |row| {
            Ok(ReviewResolutionScope {
                case_id: row.get(0)?,
                case_kind: row.get(1)?,
                attempt_id: row.get(2)?,
                assessment_item_id: row.get(3)?,
                publication_id: row.get(4)?,
                question_type: row.get(5)?,
                max_score: row.get(6)?,
                source_grade_decision_id: row.get(7)?,
                source_grade_decision_revision: row.get(8)?,
                source_snapshot_hash: row.get(9)?,
                target_answer_key_version_id: row.get(10)?,
                target_answer_key_version_public_id: row.get(11)?,
                target_rubric_version_id: row.get(12)?,
                target_rubric_version_public_id: row.get(13)?,
                target_link_set_id: row.get(14)?,
                target_link_set_public_id: row.get(15)?,
                student_response_text: row.get(16)?,
                attempt_state: row.get(17)?,
                current_grade_decision_id: row.get(18)?,
                current_grade_decision_revision: row.get(19)?,
                max_grade_decision_revision: row.get(20)?,
                active_publication_id: row.get(21)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound("未找到当前老师可处理的版本影响 case".into()))
}

fn resolution_result(
    conn: &Connection,
    case_public_id: &str,
    resolution_public_id: String,
    grade_decision_id: i64,
) -> CoreResult<ResolveQuestionImpactReviewCaseResult> {
    Ok(ResolveQuestionImpactReviewCaseResult {
        case_public_id: case_public_id.trim().into(),
        resolution_public_id,
        grade_decision: grade_decision_by_id(conn, grade_decision_id)?,
        old_publication_unchanged: true,
        learning_evidence_unchanged: true,
        requires_explicit_publication: true,
    })
}

pub fn resolve_question_impact_review_case(
    conn: &mut Connection,
    owner_id: &str,
    request: &ResolveQuestionImpactReviewCaseRequest,
) -> CoreResult<ResolveQuestionImpactReviewCaseResult> {
    required(owner_id, "题库老师")?;
    required(&request.request_key, "请求标识")?;
    required(&request.case_public_id, "待处理 case")?;
    required(&request.expected_source_snapshot_hash, "来源快照校验值")?;
    required(&request.teacher_note, "老师处理说明")?;
    required(&request.resolved_by, "处理老师")?;
    if request.resolved_by.trim() != owner_id.trim() {
        return Err(CoreError::Invalid("只能以当前老师身份确认新评分".into()));
    }
    let scope = review_resolution_scope(conn, owner_id, &request.case_public_id)?;
    if scope.source_snapshot_hash != request.expected_source_snapshot_hash.trim() {
        return Err(CoreError::Invalid(
            "case 来源快照已不匹配，请刷新后重新核对".into(),
        ));
    }
    let target_components = load_target_components(
        conn,
        &scope.question_type,
        &scope.target_answer_key_version_public_id,
        &scope.target_rubric_version_public_id,
    )?;
    let teacher_note = request.teacher_note.trim();
    let (teacher_score, component_results) = if matches!(
        scope.question_type.as_str(),
        "single" | "multiple" | "true_false"
    ) {
        if !request.components.is_empty() {
            return Err(CoreError::Invalid("客观题不能提交逐项评分".into()));
        }
        let score = request
            .teacher_score
            .ok_or_else(|| CoreError::Invalid("客观题必须填写老师确认分数".into()))?;
        if !score.is_finite() || score < 0.0 || score > scope.max_score + 0.000_001 {
            return Err(CoreError::Invalid("老师确认分数超出题目分值".into()));
        }
        (score, Vec::<Value>::new())
    } else {
        if request.teacher_score.is_some() {
            return Err(CoreError::Invalid(
                "填空和简答题总分由逐项分数自动汇总".into(),
            ));
        }
        let target_total = target_components
            .iter()
            .map(|component| component.max_score)
            .sum::<f64>();
        if (target_total - scope.max_score).abs() > 0.000_001 {
            return Err(CoreError::Invalid(
                "目标答案或评分点总分与作业题目分值不一致".into(),
            ));
        }
        let mut inputs = BTreeMap::new();
        for input in &request.components {
            required(&input.source_public_id, "评分项")?;
            if inputs
                .insert(input.source_public_id.trim().to_owned(), input)
                .is_some()
            {
                return Err(CoreError::Invalid("同一评分项不能重复提交".into()));
            }
        }
        if inputs.len() != target_components.len() {
            return Err(CoreError::Invalid(
                "必须逐项确认目标答案槽位或评分点".into(),
            ));
        }
        let response_text = scope
            .student_response_text
            .as_deref()
            .unwrap_or_default()
            .trim();
        let mut total_score = 0.0;
        let mut results = Vec::with_capacity(target_components.len());
        for component in &target_components {
            let input = inputs
                .get(&component.source_public_id)
                .ok_or_else(|| CoreError::Invalid(format!("缺少评分项：{}", component.label)))?;
            if !input.teacher_score.is_finite()
                || input.teacher_score < 0.0
                || input.teacher_score > component.max_score + 0.000_001
            {
                return Err(CoreError::Invalid(format!(
                    "评分项“{}”的得分超出范围",
                    component.label
                )));
            }
            let evidence_text = input
                .evidence_text
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if input.teacher_score > 0.000_001 {
                let evidence = evidence_text.ok_or_else(|| {
                    CoreError::Invalid(format!(
                        "评分项“{}”给分时必须引用学生原答案",
                        component.label
                    ))
                })?;
                if response_text.is_empty() || !response_text.contains(evidence) {
                    return Err(CoreError::Invalid(format!(
                        "评分项“{}”的证据必须来自当前学生答案",
                        component.label
                    )));
                }
            }
            let result_status = if input.teacher_score <= 0.000_001 {
                "incorrect"
            } else if (input.teacher_score - component.max_score).abs() <= 0.000_001 {
                "correct"
            } else {
                "partial"
            };
            total_score += input.teacher_score;
            results.push(serde_json::json!({
                "source_type": component.source_type,
                "source_public_id": component.source_public_id,
                "stable_id": component.stable_id,
                "order_index": component.order_index,
                "label": component.label,
                "max_score": component.max_score,
                "teacher_score": input.teacher_score,
                "result_status": result_status,
                "evidence_text": evidence_text,
                "teacher_note": input.teacher_note.as_deref().map(str::trim)
                    .filter(|value| !value.is_empty())
            }));
        }
        (total_score, results)
    };
    let component_results_json = serde_json::json!({
        "schema_version": 1,
        "question_type": scope.question_type,
        "components": component_results
    })
    .to_string();
    let component_results_value: Value = serde_json::from_str(&component_results_json)
        .map_err(|error| CoreError::Parse(format!("逐项评分序列化失败：{error}")))?;
    let point_results_json = serde_json::json!({
        "schema_version": 3,
        "source": "question_version_review_teacher_resolution",
        "review_case_public_id": request.case_public_id.trim(),
        "expected_source_snapshot_hash": request.expected_source_snapshot_hash.trim(),
        "target_answer_key_version_id": scope.target_answer_key_version_id,
        "target_answer_key_version_public_id": scope.target_answer_key_version_public_id,
        "target_rubric_version_id": scope.target_rubric_version_id,
        "target_rubric_version_public_id": scope.target_rubric_version_public_id,
        "target_link_set_id": scope.target_link_set_id,
        "target_link_set_public_id": scope.target_link_set_public_id,
        "component_results": component_results_value.clone()
    })
    .to_string();
    let request_hash_input = serde_json::json!({
        "schema_version": 1,
        "rule_version": QUESTION_IMPACT_RESOLUTION_RULE_VERSION,
        "case_public_id": request.case_public_id.trim(),
        "expected_source_snapshot_hash": request.expected_source_snapshot_hash.trim(),
        "teacher_score_millis": (teacher_score * 1000.0).round() as i64,
        "component_results": component_results_value,
        "teacher_note": teacher_note,
        "resolved_by": request.resolved_by.trim()
    });
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&request_hash_input)
            .map_err(|error| CoreError::Parse(format!("处理请求校验失败：{error}")))?,
    );
    let request_key_conflict: Option<i64> = conn
        .query_row(
            "SELECT review_case_id
             FROM exam_question_version_review_resolutions_v2 WHERE request_key=?1",
            [request.request_key.trim()],
            |row| row.get(0),
        )
        .optional()?;
    if request_key_conflict.is_some_and(|case_id| case_id != scope.case_id) {
        return Err(CoreError::Invalid("请求标识已用于其他待处理 case".into()));
    }
    let existing: Option<(String, String, i64)> = conn
        .query_row(
            "SELECT public_id,request_hash,grade_decision_id
             FROM exam_question_version_review_resolutions_v2
             WHERE review_case_id=?1",
            [scope.case_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    if let Some((resolution_public_id, existing_hash, grade_decision_id)) = existing {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid(
                "该 case 已按不同内容确认，不能覆盖原处理记录".into(),
            ));
        }
        return resolution_result(
            conn,
            &request.case_public_id,
            resolution_public_id,
            grade_decision_id,
        );
    }
    if scope.attempt_state == "voided" {
        return Err(CoreError::Invalid("作答已作废，不能继续处理".into()));
    }
    if scope.current_grade_decision_id != scope.source_grade_decision_id
        || scope.current_grade_decision_revision != scope.source_grade_decision_revision
        || scope.max_grade_decision_revision != scope.source_grade_decision_revision.unwrap_or(0)
    {
        return Err(CoreError::Invalid(
            "当前评分已在 case 建立后变化，请重新生成影响计划".into(),
        ));
    }
    match scope.case_kind.as_str() {
        "unpublished_recalculation" if scope.active_publication_id.is_none() => {}
        "published_review"
            if scope.publication_id.is_some()
                && scope.active_publication_id == scope.publication_id => {}
        _ => {
            return Err(CoreError::Invalid(
                "当前发布状态已在 case 建立后变化，请重新生成影响计划".into(),
            ));
        }
    }
    let tx = conn.transaction()?;
    let evidence_count_before: i64 = tx.query_row(
        "SELECT COUNT(*)
         FROM learning_evidence evidence
         JOIN exam_grade_decisions_v2 decision
           ON evidence.decision_ref_type='grade_decision'
          AND evidence.decision_ref_id=decision.public_id
          AND evidence.decision_revision=decision.revision
         WHERE decision.attempt_id=?1 AND evidence.state='active'",
        [scope.attempt_id],
        |row| row.get(0),
    )?;
    let decision = assessment::decide_grade_in_transaction(
        &tx,
        &NewGradeDecision {
            attempt_id: scope.attempt_id,
            assessment_item_id: scope.assessment_item_id,
            machine_grade_ai_run_id: None,
            teacher_score,
            point_results_json: &point_results_json,
            teacher_note: Some(teacher_note),
            confirmation_level: "teacher_corrected",
            decided_by: request.resolved_by.trim(),
        },
    )?;
    let resolution_public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    tx.execute(
        "INSERT INTO exam_question_version_review_resolutions_v2
         (public_id,request_key,request_hash,review_case_id,grade_decision_id,
          expected_source_snapshot_hash,component_results_json,teacher_note,
          resolved_by,resolved_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            &resolution_public_id,
            request.request_key.trim(),
            &request_hash,
            scope.case_id,
            decision.id,
            request.expected_source_snapshot_hash.trim(),
            &component_results_json,
            teacher_note,
            request.resolved_by.trim(),
            &now
        ],
    )?;
    let (active_publication_after, evidence_count_after): (Option<i64>, i64) = tx.query_row(
        "SELECT attempt.active_publication_id,
                (SELECT COUNT(*)
                 FROM learning_evidence evidence
                 JOIN exam_grade_decisions_v2 source_decision
                   ON evidence.decision_ref_type='grade_decision'
                  AND evidence.decision_ref_id=source_decision.public_id
                  AND evidence.decision_revision=source_decision.revision
                 WHERE source_decision.attempt_id=attempt.id
                   AND evidence.state='active')
         FROM exam_attempts_v2 attempt WHERE attempt.id=?1",
        [scope.attempt_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if active_publication_after != scope.active_publication_id
        || evidence_count_after != evidence_count_before
    {
        return Err(CoreError::Invalid(
            "保存新评分时检测到正式发布或学习证据发生意外变化".into(),
        ));
    }
    let payload = serde_json::json!({
        "schema_version": QUESTION_PERFORMANCE_SCHEMA_VERSION,
        "rule_version": QUESTION_IMPACT_RESOLUTION_RULE_VERSION,
        "review_case_public_id": request.case_public_id.trim(),
        "grade_decision_public_id": decision.public_id,
        "teacher_score": teacher_score,
        "target_answer_key_version_public_id": scope.target_answer_key_version_public_id,
        "target_rubric_version_public_id": scope.target_rubric_version_public_id,
        "target_link_set_public_id": scope.target_link_set_public_id,
        "changes_assessment_binding": false,
        "changes_publication": false,
        "changes_learning_evidence": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "k1.question_version_impact.grade_confirmed",
            event_version: 1,
            aggregate_type: "exam_question_version_review_case",
            aggregate_id: request.case_public_id.trim(),
            aggregate_revision: decision.revision,
            payload_json: &payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("{}:audit", request.request_key.trim()),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(request.resolved_by.trim()),
            action: "k1.question_version_impact.grade_confirmed",
            object_type: "exam_question_version_review_case",
            object_id: request.case_public_id.trim(),
            object_revision: Some(decision.revision),
            note: Some("新评分已确认；旧正式发布和学习证据保持不变，等待老师明确再发布"),
            meta_json: Some(&payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    resolution_result(
        conn,
        &request.case_public_id,
        resolution_public_id,
        decision.id,
    )
}

pub fn publish_question_impact_review_case(
    conn: &Connection,
    owner_id: &str,
    request: &PublishQuestionImpactReviewCaseRequest,
) -> CoreResult<PublishQuestionImpactReviewCaseResult> {
    required(owner_id, "题库老师")?;
    required(&request.case_public_id, "待处理 case")?;
    required(&request.expected_grade_decision_public_id, "待发布评分")?;
    required(&request.published_by, "发布老师")?;
    if request.published_by.trim() != owner_id.trim() {
        return Err(CoreError::Invalid("只能以当前老师身份发布成绩".into()));
    }
    let scope: Option<PublishReviewScope> = conn
        .query_row(
            "SELECT attempt.id,resolution.grade_decision_id,decision.public_id,
                    attempt.state,attempt.active_publication_id,
                    current_decision.public_id,review_case.publication_id
             FROM exam_question_version_review_cases_v2 review_case
             JOIN exam_question_version_impact_tasks_v2 task
               ON task.id=review_case.impact_task_id
             JOIN exam_question_version_impact_plans_v2 plan
               ON plan.id=task.impact_plan_id
             JOIN exam_question_version_review_resolutions_v2 resolution
               ON resolution.review_case_id=review_case.id
             JOIN exam_grade_decisions_v2 decision
               ON decision.id=resolution.grade_decision_id
             JOIN exam_attempts_v2 attempt ON attempt.id=review_case.attempt_id
             LEFT JOIN exam_grade_decisions_v2 current_decision
               ON current_decision.attempt_id=attempt.id
              AND current_decision.assessment_item_id=review_case.assessment_item_id
              AND current_decision.state='active'
             WHERE review_case.public_id=?1 AND plan.planned_by=?2",
            params![request.case_public_id.trim(), owner_id.trim()],
            |row| {
                Ok(PublishReviewScope {
                    attempt_id: row.get(0)?,
                    grade_decision_id: row.get(1)?,
                    grade_decision_public_id: row.get(2)?,
                    attempt_state: row.get(3)?,
                    active_publication_id: row.get(4)?,
                    current_grade_decision_public_id: row.get(5)?,
                    original_publication_id: row.get(6)?,
                })
            },
        )
        .optional()?;
    let Some(scope) = scope else {
        return Err(CoreError::NotFound(
            "未找到已确认新评分的版本影响 case".into(),
        ));
    };
    let PublishReviewScope {
        attempt_id,
        grade_decision_id,
        grade_decision_public_id,
        attempt_state,
        active_publication_id,
        current_grade_decision_public_id,
        original_publication_id,
    } = scope;
    if grade_decision_public_id != request.expected_grade_decision_public_id.trim()
        || current_grade_decision_public_id != grade_decision_public_id
    {
        return Err(CoreError::Invalid(
            "当前有效评分与待发布评分不一致，请刷新后核对".into(),
        ));
    }
    if let Some(publication_id) = active_publication_id {
        let already_published: bool = conn.query_row(
            "SELECT EXISTS(
               SELECT 1
               FROM exam_grade_publication_items_v2 publication_item
               JOIN exam_grade_publication_decisions_v2 publication_decision
                 ON publication_decision.publication_item_id=publication_item.id
                AND publication_decision.attempt_id=publication_item.attempt_id
               WHERE publication_item.publication_id=?1
                 AND publication_item.attempt_id=?2
                 AND publication_decision.grade_decision_id=?3
             )",
            (publication_id, attempt_id, grade_decision_id),
            |row| row.get(0),
        )?;
        if already_published {
            let active_resolution_evidence: i64 = conn.query_row(
                "SELECT COUNT(*) FROM learning_evidence
                 WHERE decision_ref_type='grade_decision'
                   AND decision_ref_id=?1 AND state='active'",
                [&grade_decision_public_id],
                |row| row.get(0),
            )?;
            let reverted_original_evidence: i64 = if let Some(original_id) =
                original_publication_id.filter(|old_id| *old_id != publication_id)
            {
                conn.query_row(
                    "SELECT COUNT(*)
                     FROM learning_evidence evidence
                     JOIN exam_grade_decisions_v2 decision
                       ON evidence.decision_ref_type='grade_decision'
                      AND evidence.decision_ref_id=decision.public_id
                      AND evidence.decision_revision=decision.revision
                     JOIN exam_grade_publication_decisions_v2 publication_decision
                       ON publication_decision.grade_decision_id=decision.id
                     JOIN exam_grade_publication_items_v2 publication_item
                       ON publication_item.id=publication_decision.publication_item_id
                      AND publication_item.attempt_id=publication_decision.attempt_id
                     WHERE publication_item.publication_id=?1
                       AND publication_item.attempt_id=?2
                       AND evidence.state='reverted'",
                    (original_id, attempt_id),
                    |row| row.get(0),
                )?
            } else {
                0
            };
            return Ok(PublishQuestionImpactReviewCaseResult {
                case_public_id: request.case_public_id.trim().into(),
                publication: publication_by_id(conn, publication_id, attempt_id)?,
                prior_publication_superseded: original_publication_id
                    .is_some_and(|old_id| old_id != publication_id),
                learning_evidence_switched: active_resolution_evidence > 0
                    || reverted_original_evidence > 0,
            });
        }
    }
    if attempt_state != "ready_to_publish" {
        return Err(CoreError::Invalid(
            "整份作业仍有题目待终审，暂不能再次发布".into(),
        ));
    }
    let old_active_evidence_count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM learning_evidence evidence
         JOIN exam_grade_decisions_v2 decision
           ON evidence.decision_ref_type='grade_decision'
          AND evidence.decision_ref_id=decision.public_id
          AND evidence.decision_revision=decision.revision
         JOIN exam_grade_publication_decisions_v2 publication_decision
           ON publication_decision.grade_decision_id=decision.id
         JOIN exam_grade_publication_items_v2 publication_item
           ON publication_item.id=publication_decision.publication_item_id
          AND publication_item.attempt_id=publication_decision.attempt_id
         WHERE publication_item.publication_id=?1
           AND publication_item.attempt_id=?2
           AND evidence.state='active'",
        (active_publication_id, attempt_id),
        |row| row.get(0),
    )?;
    let publication = assessment::publish_attempt(conn, attempt_id, request.published_by.trim())?;
    let new_active_evidence_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM learning_evidence
         WHERE decision_ref_type='grade_decision'
           AND decision_ref_id=?1 AND state='active'",
        [&grade_decision_public_id],
        |row| row.get(0),
    )?;
    let old_active_evidence_after: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM learning_evidence evidence
         JOIN exam_grade_decisions_v2 decision
           ON evidence.decision_ref_type='grade_decision'
          AND evidence.decision_ref_id=decision.public_id
          AND evidence.decision_revision=decision.revision
         JOIN exam_grade_publication_decisions_v2 publication_decision
           ON publication_decision.grade_decision_id=decision.id
         JOIN exam_grade_publication_items_v2 publication_item
           ON publication_item.id=publication_decision.publication_item_id
          AND publication_item.attempt_id=publication_decision.attempt_id
         WHERE publication_item.publication_id=?1
           AND publication_item.attempt_id=?2
           AND evidence.state='active'",
        (active_publication_id, attempt_id),
        |row| row.get(0),
    )?;
    Ok(PublishQuestionImpactReviewCaseResult {
        case_public_id: request.case_public_id.trim().into(),
        publication,
        prior_publication_superseded: active_publication_id.is_some(),
        learning_evidence_switched: new_active_evidence_count > 0
            || old_active_evidence_count > old_active_evidence_after,
    })
}

#[cfg(test)]
#[path = "question_performance_tests.rs"]
mod tests;
