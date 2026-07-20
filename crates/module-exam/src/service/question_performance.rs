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

pub fn list_question_performance(
    conn: &Connection,
    owner_id: &str,
    limit: i64,
) -> CoreResult<QuestionPerformanceCatalog> {
    required(owner_id, "题库老师")?;
    if !(1..=500).contains(&limit) {
        return Err(CoreError::Invalid(
            "题目表现列表数量必须在 1 到 500 之间".into(),
        ));
    }
    let sql = format!(
        "WITH published_response AS (
           SELECT item.question_version_id,assessment.id AS assessment_id,
                  assessment.assessment_context,attempt.id AS attempt_id,
                  attempt.attempt_kind,decision.teacher_score,item.score,
                  publication.published_at
           FROM exam_attempts_v2 attempt
           JOIN exam_grade_publications_v2 publication
             ON publication.id=attempt.active_publication_id
            AND publication.state='published'
           JOIN exam_grade_publication_items_v2 publication_item
             ON publication_item.publication_id=publication.id
            AND publication_item.attempt_id=attempt.id
           JOIN exam_grade_publication_decisions_v2 publication_decision
             ON publication_decision.publication_item_id=publication_item.id
            AND publication_decision.attempt_id=attempt.id
           JOIN exam_grade_decisions_v2 decision
             ON decision.id=publication_decision.grade_decision_id
            AND decision.attempt_id=attempt.id
            AND decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
           JOIN exam_assessment_items_v2 item
             ON item.id=decision.assessment_item_id
            AND item.assessment_version_id=attempt.assessment_version_id
            AND item.state='active'
           JOIN exam_assessment_versions_v2 assessment_version
             ON assessment_version.id=attempt.assessment_version_id
           JOIN exam_assessments_v2 assessment
             ON assessment.id=assessment_version.assessment_id
         )
         SELECT version.id,version.public_id,version.revision,question.owner_scope,
                version.question_type,version.stem,version.max_score,
                version.quality_level,version.state,
                (SELECT COUNT(DISTINCT assessment_version.assessment_id)
                 FROM exam_assessment_items_v2 item
                 JOIN exam_assessment_versions_v2 assessment_version
                   ON assessment_version.id=item.assessment_version_id
                 WHERE item.question_version_id=version.id),
                COUNT(response.attempt_id),
                COALESCE(SUM(CASE WHEN response.teacher_score>=response.score-0.000001
                                  THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN response.teacher_score>0.000001
                                    AND response.teacher_score<response.score-0.000001
                                  THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN response.teacher_score<=0.000001
                                  THEN 1 ELSE 0 END),0),
                AVG(CASE WHEN response.score>0
                         THEN response.teacher_score/response.score END),
                AVG(CASE WHEN response.attempt_id IS NULL THEN NULL
                         WHEN response.teacher_score>=response.score-0.000001 THEN 1.0
                         ELSE 0.0 END),
                COALESCE(SUM(CASE WHEN response.attempt_kind='first' THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN response.attempt_kind='correction' THEN 1 ELSE 0 END),0),
                MAX(response.published_at),
                EXISTS(
                  SELECT 1 FROM exam_assessment_items_v2 item
                  WHERE item.question_version_id=version.id
                    AND (
                      item.answer_key_version_id<>(
                        SELECT target.id FROM k1_answer_key_versions target
                        WHERE target.question_version_id=version.id AND target.state='confirmed'
                        ORDER BY target.revision DESC,target.id DESC LIMIT 1
                      )
                      OR item.rubric_version_id<>(
                        SELECT target.id FROM k1_rubric_versions target
                        WHERE target.question_version_id=version.id AND target.state='confirmed'
                        ORDER BY target.revision DESC,target.id DESC LIMIT 1
                      )
                      OR item.link_set_id<>(
                        SELECT target.id FROM k1_link_sets target
                        WHERE target.question_version_id=version.id AND target.state='confirmed'
                        ORDER BY target.revision DESC,target.id DESC LIMIT 1
                      )
                    )
                )
         FROM k1_question_versions version
         JOIN k1_questions question ON question.id=version.question_id
         LEFT JOIN published_response response ON response.question_version_id=version.id
         WHERE ({})
           AND (
             response.attempt_id IS NOT NULL
             OR EXISTS (
               SELECT 1 FROM exam_assessment_items_v2 used_item
               WHERE used_item.question_version_id=version.id
             )
           )
         GROUP BY version.id
         ORDER BY COUNT(response.attempt_id) DESC,version.created_at DESC,version.id DESC
         LIMIT ?2",
        accessible_question_clause()
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params![owner_id.trim(), limit], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            QuestionPerformanceItem {
                question_version_public_id: row.get(1)?,
                revision: row.get(2)?,
                owner_scope: row.get(3)?,
                question_type: row.get(4)?,
                stem: row.get(5)?,
                max_score: row.get(6)?,
                quality_level: row.get(7)?,
                state: row.get(8)?,
                assessment_usage_count: row.get(9)?,
                published_response_count: row.get(10)?,
                full_credit_count: row.get(11)?,
                partial_credit_count: row.get(12)?,
                zero_score_count: row.get(13)?,
                average_score_rate: canonical_rate(row.get(14)?),
                full_credit_rate: canonical_rate(row.get(15)?),
                first_attempt_count: row.get(16)?,
                correction_attempt_count: row.get(17)?,
                latest_published_at: row.get(18)?,
                context_breakdown: Vec::new(),
                has_version_update_impact: row.get(19)?,
            },
        ))
    })?;
    let mut items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut context_statement = conn.prepare(
        "SELECT assessment.assessment_context,COUNT(*),
                AVG(decision.teacher_score/item.score),
                AVG(CASE WHEN decision.teacher_score>=item.score-0.000001
                         THEN 1.0 ELSE 0.0 END)
         FROM exam_attempts_v2 attempt
         JOIN exam_grade_publications_v2 publication
           ON publication.id=attempt.active_publication_id AND publication.state='published'
         JOIN exam_grade_publication_items_v2 publication_item
           ON publication_item.publication_id=publication.id
          AND publication_item.attempt_id=attempt.id
         JOIN exam_grade_publication_decisions_v2 publication_decision
           ON publication_decision.publication_item_id=publication_item.id
          AND publication_decision.attempt_id=attempt.id
         JOIN exam_grade_decisions_v2 decision
           ON decision.id=publication_decision.grade_decision_id
          AND decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
         JOIN exam_assessment_items_v2 item
           ON item.id=decision.assessment_item_id
          AND item.assessment_version_id=attempt.assessment_version_id
          AND item.question_version_id=?1
          AND item.state='active'
         JOIN exam_assessment_versions_v2 assessment_version
           ON assessment_version.id=attempt.assessment_version_id
         JOIN exam_assessments_v2 assessment ON assessment.id=assessment_version.assessment_id
         GROUP BY assessment.assessment_context
         ORDER BY COUNT(*) DESC,assessment.assessment_context",
    )?;
    for (question_version_id, item) in &mut items {
        let context_rows = context_statement.query_map([*question_version_id], |row| {
            Ok(PerformanceContextBreakdown {
                assessment_context: row.get(0)?,
                published_response_count: row.get(1)?,
                average_score_rate: canonical_rate(row.get(2)?),
                full_credit_rate: canonical_rate(row.get(3)?),
            })
        })?;
        item.context_breakdown = context_rows.collect::<rusqlite::Result<Vec<_>>>()?;
    }
    Ok(QuestionPerformanceCatalog {
        schema_version: QUESTION_PERFORMANCE_SCHEMA_VERSION,
        rule_version: QUESTION_PERFORMANCE_RULE_VERSION.into(),
        calculated_at: time::utc_now_rfc3339(),
        items: items.into_iter().map(|(_, item)| item).collect(),
        boundary_note:
            "只统计每份答卷当前有效发布快照中的老师确认得分；满分率不是永久难度，班级和作业场景不会被混写回题目。"
                .into(),
    })
}

fn load_target(
    conn: &Connection,
    owner_id: &str,
    question_version_public_id: &str,
) -> CoreResult<(TargetIds, String, String, ImpactTargetVersion)> {
    required(question_version_public_id, "题目版本")?;
    let sql = format!(
        "SELECT version.id,version.question_type,version.stem
         FROM k1_question_versions version
         JOIN k1_questions question ON question.id=version.question_id
         WHERE version.public_id=?2 AND ({})",
        accessible_question_clause()
    );
    let (question_version_id, question_type, stem): (i64, String, String) = conn
        .query_row(
            &sql,
            params![owner_id.trim(), question_version_public_id.trim()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("可访问题目版本".into()))?;
    let (answer_id, answer_public_id, answer_revision): (i64, String, i64) = conn
        .query_row(
            "SELECT id,public_id,revision FROM k1_answer_key_versions
             WHERE question_version_id=?1 AND state='confirmed'
             ORDER BY revision DESC,id DESC LIMIT 1",
            [question_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("题目没有已确认答案版本".into()))?;
    let (rubric_id, rubric_public_id, rubric_revision): (i64, String, i64) = conn
        .query_row(
            "SELECT id,public_id,revision FROM k1_rubric_versions
             WHERE question_version_id=?1 AND state='confirmed'
             ORDER BY revision DESC,id DESC LIMIT 1",
            [question_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("题目没有已确认评分规则版本".into()))?;
    let (link_id, link_public_id, link_revision): (i64, String, i64) = conn
        .query_row(
            "SELECT id,public_id,revision FROM k1_link_sets
             WHERE question_version_id=?1 AND state='confirmed'
             ORDER BY revision DESC,id DESC LIMIT 1",
            [question_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("题目没有已确认知识能力链接版本".into()))?;
    Ok((
        TargetIds {
            question_version_id,
            target_answer_key_version_id: answer_id,
            target_rubric_version_id: rubric_id,
            target_link_set_id: link_id,
        },
        question_type,
        stem,
        ImpactTargetVersion {
            answer_key_version_public_id: answer_public_id,
            answer_key_revision: answer_revision,
            rubric_version_public_id: rubric_public_id,
            rubric_revision,
            link_set_public_id: link_public_id,
            link_set_revision: link_revision,
        },
    ))
}

pub fn preview_question_version_impact(
    conn: &Connection,
    owner_id: &str,
    question_version_public_id: &str,
) -> CoreResult<QuestionVersionImpactPreview> {
    required(owner_id, "题库老师")?;
    let (target_ids, question_type, stem, target) =
        load_target(conn, owner_id, question_version_public_id)?;
    let mut statement = conn.prepare(
        "SELECT assessment.public_id,assessment.title,class.name,
                assessment_version.public_id,assessment_version.revision,item.public_id,
                answer.public_id,rubric.public_id,links.public_id,
                item.answer_key_version_id<>?2,
                item.rubric_version_id<>?3,
                item.link_set_id<>?4,
                (SELECT COUNT(*) FROM exam_attempts_v2 attempt
                 WHERE attempt.assessment_version_id=item.assessment_version_id
                   AND attempt.active_publication_id IS NULL AND attempt.state<>'voided'),
                (SELECT COUNT(*) FROM exam_attempts_v2 attempt
                 JOIN exam_grade_publications_v2 publication
                   ON publication.id=attempt.active_publication_id
                  AND publication.state='published'
                 WHERE attempt.assessment_version_id=item.assessment_version_id),
                (SELECT COUNT(DISTINCT evidence.id)
                 FROM exam_grade_decisions_v2 decision
                 JOIN exam_attempts_v2 attempt ON attempt.id=decision.attempt_id
                 JOIN learning_evidence evidence
                   ON evidence.decision_ref_type='grade_decision'
                  AND evidence.decision_ref_id=decision.public_id
                  AND evidence.state='active'
                 WHERE decision.assessment_item_id=item.id
                   AND attempt.assessment_version_id=item.assessment_version_id),
                (SELECT COUNT(DISTINCT profile_link.snapshot_id)
                 FROM exam_grade_decisions_v2 decision
                 JOIN learning_evidence evidence
                   ON evidence.decision_ref_type='grade_decision'
                  AND evidence.decision_ref_id=decision.public_id
                  AND evidence.state='active'
                 JOIN profile_evidence_links profile_link
                   ON profile_link.learning_evidence_id=evidence.id
                 WHERE decision.assessment_item_id=item.id)
                ,
                assessment_version.id=COALESCE(
                  (SELECT default_selection.selected_assessment_version_id
                   FROM exam_assessment_default_version_selections_v2 default_selection
                   WHERE default_selection.assessment_id=assessment.id
                   ORDER BY default_selection.revision DESC,default_selection.id DESC
                   LIMIT 1),
                  (SELECT latest_version.id
                   FROM exam_assessment_versions_v2 latest_version
                   WHERE latest_version.assessment_id=assessment.id
                     AND latest_version.state='confirmed'
                   ORDER BY latest_version.revision DESC,latest_version.id DESC
                   LIMIT 1)
                )
         FROM exam_assessment_items_v2 item
         JOIN exam_assessment_versions_v2 assessment_version
           ON assessment_version.id=item.assessment_version_id
         JOIN exam_assessments_v2 assessment ON assessment.id=assessment_version.assessment_id
         JOIN classes class ON class.id=assessment.class_id
         JOIN k1_answer_key_versions answer ON answer.id=item.answer_key_version_id
         JOIN k1_rubric_versions rubric ON rubric.id=item.rubric_version_id
         JOIN k1_link_sets links ON links.id=item.link_set_id
         WHERE item.question_version_id=?1
           AND item.state='active'
           AND (item.answer_key_version_id<>?2
                OR item.rubric_version_id<>?3
                OR item.link_set_id<>?4)
         ORDER BY assessment.created_at,assessment_version.revision,item.order_index,item.id",
    )?;
    let rows = statement.query_map(
        params![
            target_ids.question_version_id,
            target_ids.target_answer_key_version_id,
            target_ids.target_rubric_version_id,
            target_ids.target_link_set_id
        ],
        |row| {
            Ok(QuestionImpactRow {
                assessment_public_id: row.get(0)?,
                assessment_title: row.get(1)?,
                class_name: row.get(2)?,
                assessment_version_public_id: row.get(3)?,
                assessment_version_revision: row.get(4)?,
                assessment_item_public_id: row.get(5)?,
                source_answer_key_version_public_id: row.get(6)?,
                source_rubric_version_public_id: row.get(7)?,
                source_link_set_public_id: row.get(8)?,
                answer_changed: row.get(9)?,
                rubric_changed: row.get(10)?,
                link_changed: row.get(11)?,
                unpublished_attempt_count: row.get(12)?,
                published_attempt_count: row.get(13)?,
                active_learning_evidence_count: row.get(14)?,
                profile_snapshot_count: row.get(15)?,
                is_current_default: row.get(16)?,
            })
        },
    )?;
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let affected_assessment_count = rows
        .iter()
        .map(|row| row.assessment_public_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len() as i64;
    let affected_assessment_version_count = rows
        .iter()
        .map(|row| row.assessment_version_public_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len() as i64;
    let unpublished_attempt_count = rows.iter().map(|row| row.unpublished_attempt_count).sum();
    let published_attempt_count = rows.iter().map(|row| row.published_attempt_count).sum();
    let active_learning_evidence_count = rows
        .iter()
        .map(|row| row.active_learning_evidence_count)
        .sum();
    let profile_snapshot_count = rows.iter().map(|row| row.profile_snapshot_count).sum();
    let hash_payload = serde_json::json!({
        "schema_version": QUESTION_PERFORMANCE_SCHEMA_VERSION,
        "rule_version": QUESTION_IMPACT_RULE_VERSION,
        "question_version_public_id": question_version_public_id.trim(),
        "target": &target,
        "rows": &rows
    });
    let preview_hash = hashing::sha256_hex(
        &serde_json::to_vec(&hash_payload)
            .map_err(|error| CoreError::Parse(format!("影响预览 hash 序列化失败：{error}")))?,
    );
    Ok(QuestionVersionImpactPreview {
        schema_version: QUESTION_PERFORMANCE_SCHEMA_VERSION,
        rule_version: QUESTION_IMPACT_RULE_VERSION.into(),
        calculated_at: time::utc_now_rfc3339(),
        preview_hash,
        question_version_public_id: question_version_public_id.trim().into(),
        question_type,
        stem,
        target,
        affected_assessment_count,
        affected_assessment_version_count,
        affected_item_count: rows.len() as i64,
        unpublished_attempt_count,
        published_attempt_count,
        active_learning_evidence_count,
        profile_snapshot_count,
        rows,
        boundary_note:
            "这里只冻结影响范围和老师选择；不会直接换绑作业、重算成绩、重新发布或改写既有学习图谱。"
                .into(),
    })
}

fn validate_action(action: &str) -> CoreResult<()> {
    if matches!(
        action,
        "future_only" | "recalculate_unpublished" | "review_published"
    ) {
        Ok(())
    } else {
        Err(CoreError::Invalid("版本影响处理方式非法".into()))
    }
}

fn plan_by_id(conn: &Connection, id: i64) -> CoreResult<QuestionImpactPlan> {
    conn.query_row(
        "SELECT plan.public_id,question.public_id,plan.expected_preview_hash,
                plan.action,
                (SELECT COUNT(*) FROM exam_question_version_impact_tasks_v2 task
                 WHERE task.impact_plan_id=plan.id),
                plan.planned_by,plan.planned_at
         FROM exam_question_version_impact_plans_v2 plan
         JOIN k1_question_versions question ON question.id=plan.question_version_id
         WHERE plan.id=?1",
        [id],
        |row| {
            Ok(QuestionImpactPlan {
                public_id: row.get(0)?,
                question_version_public_id: row.get(1)?,
                expected_preview_hash: row.get(2)?,
                action: row.get(3)?,
                task_count: row.get(4)?,
                planned_by: row.get(5)?,
                planned_at: row.get(6)?,
                changes_assessment_binding: false,
                changes_grade: false,
                changes_publication: false,
                changes_learning_evidence: false,
            })
        },
    )
    .map_err(Into::into)
}

fn task_scopes(conn: &Connection, target: &TargetIds, action: &str) -> CoreResult<Vec<TaskScope>> {
    if action == "future_only" {
        return Ok(Vec::new());
    }
    let (publication_predicate, task_kind) = if action == "recalculate_unpublished" {
        (
            "attempt.active_publication_id IS NULL AND attempt.state<>'voided'",
            "recalculate_unpublished",
        )
    } else {
        (
            "attempt.active_publication_id IS NOT NULL
             AND EXISTS (
               SELECT 1 FROM exam_grade_publications_v2 publication
               WHERE publication.id=attempt.active_publication_id
                 AND publication.state='published'
             )",
            "review_published",
        )
    };
    let sql = format!(
        "SELECT attempt.id,item.id,
                CASE WHEN ?5='review_published' THEN attempt.active_publication_id ELSE NULL END,
                item.answer_key_version_id,item.rubric_version_id,item.link_set_id
         FROM exam_assessment_items_v2 item
         JOIN exam_attempts_v2 attempt
           ON attempt.assessment_version_id=item.assessment_version_id
         WHERE item.question_version_id=?1
           AND item.state='active'
           AND (item.answer_key_version_id<>?2
                OR item.rubric_version_id<>?3
                OR item.link_set_id<>?4)
           AND ({})
         ORDER BY attempt.id,item.id",
        publication_predicate
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(
        params![
            target.question_version_id,
            target.target_answer_key_version_id,
            target.target_rubric_version_id,
            target.target_link_set_id,
            task_kind
        ],
        |row| {
            Ok(TaskScope {
                attempt_id: row.get(0)?,
                assessment_item_id: row.get(1)?,
                publication_id: row.get(2)?,
                source_answer_key_version_id: row.get(3)?,
                source_rubric_version_id: row.get(4)?,
                source_link_set_id: row.get(5)?,
            })
        },
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn confirm_question_impact_plan(
    conn: &mut Connection,
    owner_id: &str,
    request: &ConfirmQuestionImpactPlanRequest,
) -> CoreResult<QuestionImpactPlan> {
    required(owner_id, "题库老师")?;
    required(&request.request_key, "请求键")?;
    required(&request.question_version_public_id, "题目版本")?;
    required(&request.expected_preview_hash, "影响预览 hash")?;
    required(&request.planned_by, "确认老师")?;
    validate_action(request.action.trim())?;
    if request.planned_by.trim() != owner_id.trim() {
        return Err(CoreError::Invalid("只能以当前老师身份确认影响计划".into()));
    }
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": QUESTION_PERFORMANCE_SCHEMA_VERSION,
            "question_version_public_id": request.question_version_public_id.trim(),
            "expected_preview_hash": request.expected_preview_hash.trim(),
            "action": request.action.trim(),
            "planned_by": request.planned_by.trim()
        }))
        .map_err(|error| CoreError::Parse(format!("影响计划请求序列化失败：{error}")))?,
    );
    if let Some((id, existing_hash)) = conn
        .query_row(
            "SELECT id,request_hash FROM exam_question_version_impact_plans_v2
             WHERE request_key=?1",
            [request.request_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid("同一请求键对应不同影响计划".into()));
        }
        return plan_by_id(conn, id);
    }
    let preview =
        preview_question_version_impact(conn, owner_id, &request.question_version_public_id)?;
    if preview.preview_hash != request.expected_preview_hash.trim() {
        return Err(CoreError::Invalid(
            "题目版本或历史使用范围已变化，请刷新影响预览".into(),
        ));
    }
    let (target, _, _, _) = load_target(conn, owner_id, &request.question_version_public_id)?;
    let scopes = task_scopes(conn, &target, request.action.trim())?;
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let impact_json = serde_json::to_string(&preview)
        .map_err(|error| CoreError::Parse(format!("影响预览冻结失败：{error}")))?;
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO exam_question_version_impact_plans_v2
         (public_id,request_key,request_hash,question_version_id,
          target_answer_key_version_id,target_rubric_version_id,target_link_set_id,
          expected_preview_hash,action,impact_json,planned_by,planned_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            &public_id,
            request.request_key.trim(),
            &request_hash,
            target.question_version_id,
            target.target_answer_key_version_id,
            target.target_rubric_version_id,
            target.target_link_set_id,
            request.expected_preview_hash.trim(),
            request.action.trim(),
            &impact_json,
            request.planned_by.trim(),
            &now
        ],
    )?;
    let plan_id = tx.last_insert_rowid();
    for scope in &scopes {
        tx.execute(
            "INSERT INTO exam_question_version_impact_tasks_v2
             (public_id,impact_plan_id,attempt_id,assessment_item_id,publication_id,task_kind,
              source_answer_key_version_id,source_rubric_version_id,source_link_set_id,
              target_answer_key_version_id,target_rubric_version_id,target_link_set_id,
              state,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'open',?13)",
            params![
                ids::new_public_id(),
                plan_id,
                scope.attempt_id,
                scope.assessment_item_id,
                scope.publication_id,
                request.action.trim(),
                scope.source_answer_key_version_id,
                scope.source_rubric_version_id,
                scope.source_link_set_id,
                target.target_answer_key_version_id,
                target.target_rubric_version_id,
                target.target_link_set_id,
                &now
            ],
        )?;
    }
    let payload = serde_json::json!({
        "schema_version": QUESTION_PERFORMANCE_SCHEMA_VERSION,
        "rule_version": QUESTION_IMPACT_RULE_VERSION,
        "question_version_public_id": request.question_version_public_id.trim(),
        "preview_hash": request.expected_preview_hash.trim(),
        "action": request.action.trim(),
        "task_count": scopes.len(),
        "changes_assessment_binding": false,
        "changes_grade": false,
        "changes_publication": false,
        "changes_learning_evidence": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "k1.question_version_impact.planned",
            event_version: 1,
            aggregate_type: "exam_question_version_impact_plan",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("{}:audit", request.request_key.trim()),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(request.planned_by.trim()),
            action: "k1.question_version_impact.planned",
            object_type: "exam_question_version_impact_plan",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("只冻结版本影响范围和复核清单；未改作业绑定、成绩、发布或图谱"),
            meta_json: Some(&payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    plan_by_id(conn, plan_id)
}

pub fn list_question_impact_review_cases(
    conn: &Connection,
    owner_id: &str,
    plan_public_id: &str,
) -> CoreResult<QuestionImpactReviewCaseCatalog> {
    required(owner_id, "题库老师")?;
    required(plan_public_id, "影响计划")?;
    let exists = conn
        .query_row(
            "SELECT 1 FROM exam_question_version_impact_plans_v2
             WHERE public_id=?1 AND planned_by=?2",
            params![plan_public_id.trim(), owner_id.trim()],
            |_| Ok(()),
        )
        .optional()?;
    if exists.is_none() {
        return Err(CoreError::NotFound("未找到当前老师的版本影响计划".into()));
    }
    let mut statement = conn.prepare(
        "SELECT review_case.public_id,task.public_id,plan.public_id,review_case.case_kind,
                assessment.title,class.name,student.student_no,student.name,
                question.public_id,question.question_type,question.stem,item.order_index+1,item.score,
                attempt.public_id,attempt.state,publication.public_id,decision.public_id,
                review_case.source_grade_decision_revision,review_case.source_teacher_score,
                review_case.source_point_results_json,review_case.source_snapshot_hash,
                COALESCE(
                  (SELECT transcription.result_state
                   FROM exam_subjective_transcription_revisions_v2 transcription
                   WHERE transcription.attempt_id=attempt.id
                     AND transcription.assessment_item_id=item.id
                     AND transcription.state='active'
                   ORDER BY transcription.revision DESC,transcription.id DESC LIMIT 1),
                  (SELECT observation.result_state
                   FROM exam_objective_observation_revisions_v2 observation
                   WHERE observation.attempt_id=attempt.id
                     AND observation.assessment_item_id=item.id
                     AND observation.state='active'
                   ORDER BY observation.revision DESC,observation.id DESC LIMIT 1)
                ),
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
                COALESCE(
                  (SELECT artifact.archived_path
                   FROM exam_subjective_transcription_revisions_v2 transcription
                   JOIN exam_answer_region_revisions_v2 region
                     ON region.id=transcription.answer_region_revision_id
                   JOIN artifacts artifact ON artifact.id=region.crop_artifact_id
                   WHERE transcription.attempt_id=attempt.id
                     AND transcription.assessment_item_id=item.id
                     AND transcription.state='active'
                     AND artifact.archive_status='ready'
                   ORDER BY transcription.revision DESC,transcription.id DESC LIMIT 1),
                  (SELECT artifact.archived_path
                   FROM exam_objective_observation_revisions_v2 observation
                   JOIN exam_answer_region_revisions_v2 region
                     ON region.id=observation.answer_region_revision_id
                   JOIN artifacts artifact ON artifact.id=region.crop_artifact_id
                   WHERE observation.attempt_id=attempt.id
                     AND observation.assessment_item_id=item.id
                     AND observation.state='active'
                     AND artifact.archive_status='ready'
                   ORDER BY observation.revision DESC,observation.id DESC LIMIT 1)
                ),
                target_answer.public_id,target_answer.revision,target_answer.answer_json,
                target_rubric.public_id,target_rubric.revision,
                target_link.public_id,target_link.revision,
                review_case.prepared_by,review_case.prepared_at,review_case.state,
                resolution.public_id,resolved_decision.public_id,resolved_decision.teacher_score,
                resolution.resolved_at,active_publication.public_id,
                CASE WHEN resolution.id IS NOT NULL AND EXISTS (
                  SELECT 1
                  FROM exam_grade_publication_items_v2 resolved_publication_item
                  JOIN exam_grade_publication_decisions_v2 resolved_publication_decision
                    ON resolved_publication_decision.publication_item_id=resolved_publication_item.id
                   AND resolved_publication_decision.attempt_id=resolved_publication_item.attempt_id
                  WHERE resolved_publication_item.publication_id=attempt.active_publication_id
                    AND resolved_publication_item.attempt_id=attempt.id
                    AND resolved_publication_decision.grade_decision_id=resolution.grade_decision_id
                ) THEN 1 ELSE 0 END
         FROM exam_question_version_review_cases_v2 review_case
         JOIN exam_question_version_impact_tasks_v2 task
           ON task.id=review_case.impact_task_id
         JOIN exam_question_version_impact_plans_v2 plan
           ON plan.id=task.impact_plan_id
         JOIN exam_attempts_v2 attempt ON attempt.id=review_case.attempt_id
         JOIN students student ON student.id=attempt.student_id
         JOIN exam_assessment_items_v2 item ON item.id=review_case.assessment_item_id
         JOIN exam_assessment_versions_v2 assessment_version
           ON assessment_version.id=item.assessment_version_id
         JOIN exam_assessments_v2 assessment
           ON assessment.id=assessment_version.assessment_id
         JOIN classes class ON class.id=assessment.class_id
         JOIN k1_question_versions question ON question.id=item.question_version_id
         JOIN k1_answer_key_versions target_answer
           ON target_answer.id=review_case.target_answer_key_version_id
         JOIN k1_rubric_versions target_rubric
           ON target_rubric.id=review_case.target_rubric_version_id
         JOIN k1_link_sets target_link ON target_link.id=review_case.target_link_set_id
         LEFT JOIN exam_grade_publications_v2 publication
           ON publication.id=review_case.publication_id
         LEFT JOIN exam_grade_decisions_v2 decision
           ON decision.id=review_case.source_grade_decision_id
         LEFT JOIN exam_question_version_review_resolutions_v2 resolution
           ON resolution.review_case_id=review_case.id
         LEFT JOIN exam_grade_decisions_v2 resolved_decision
           ON resolved_decision.id=resolution.grade_decision_id
         LEFT JOIN exam_grade_publications_v2 active_publication
           ON active_publication.id=attempt.active_publication_id
         WHERE plan.public_id=?1 AND plan.planned_by=?2
         ORDER BY assessment.title,student.student_no,student.id,item.order_index,review_case.id",
    )?;
    let rows = statement.query_map(
        params![plan_public_id.trim(), owner_id.trim()],
        |row| {
            let case_kind: String = row.get(3)?;
            let resolved_is_published = row.get::<_, i64>(39)? != 0;
            let resolution_public_id: Option<String> = row.get(34)?;
            let state = if resolved_is_published {
                "republished"
            } else if resolution_public_id.is_some() {
                "grade_confirmed"
            } else {
                "open"
            };
            let next_step_note = match (state, case_kind.as_str()) {
                ("republished", _) => "新评分已由老师再次明确发布；旧发布快照保持审计。",
                ("grade_confirmed", _) => {
                    "新评分 revision 已确认但尚未正式生效；请明确发布整份成绩。"
                }
                (_, "unpublished_recalculation") => {
                    "请对照学生作答与新答案逐条确认；保存后仍须显式发布整份成绩。"
                }
                _ => {
                    "请核对正式发布时的旧评分与学生作答；保存新评分后旧正式成绩仍保持不变，直到明确生成新的发布 revision。"
                }
            };
            Ok(QuestionImpactReviewCase {
                public_id: row.get(0)?,
                impact_task_public_id: row.get(1)?,
                plan_public_id: row.get(2)?,
                case_kind,
                assessment_title: row.get(4)?,
                class_name: row.get(5)?,
                student_no: row.get(6)?,
                student_name: row.get(7)?,
                question_version_public_id: row.get(8)?,
                question_type: row.get(9)?,
                question_stem: row.get(10)?,
                question_no: row.get(11)?,
                max_score: row.get(12)?,
                attempt_public_id: row.get(13)?,
                attempt_state: row.get(14)?,
                publication_public_id: row.get(15)?,
                source_grade_decision_public_id: row.get(16)?,
                source_grade_decision_revision: row.get(17)?,
                source_teacher_score: row.get(18)?,
                source_point_results_json: row.get(19)?,
                source_snapshot_hash: row.get(20)?,
                student_response_state: row.get(21)?,
                student_response_text: row.get(22)?,
                crop_path: row.get(23)?,
                target_answer_key_version_public_id: row.get(24)?,
                target_answer_key_revision: row.get(25)?,
                target_answer_json: row.get(26)?,
                target_components: Vec::new(),
                target_rubric_version_public_id: row.get(27)?,
                target_rubric_revision: row.get(28)?,
                target_link_set_public_id: row.get(29)?,
                target_link_set_revision: row.get(30)?,
                prepared_by: row.get(31)?,
                prepared_at: row.get(32)?,
                state: state.into(),
                resolution_public_id,
                resolved_grade_decision_public_id: row.get(35)?,
                resolved_teacher_score: row.get(36)?,
                resolved_at: row.get(37)?,
                active_publication_public_id: row.get(38)?,
                next_step_note: next_step_note.into(),
                changes_assessment_binding: false,
                changes_grade: state != "open",
                changes_publication: state == "republished",
                changes_learning_evidence: state == "republished",
            })
        },
    )?;
    let mut cases = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for review_case in &mut cases {
        review_case.target_components = load_target_components(
            conn,
            &review_case.question_type,
            &review_case.target_answer_key_version_public_id,
            &review_case.target_rubric_version_public_id,
        )?;
    }
    Ok(QuestionImpactReviewCaseCatalog {
        schema_version: QUESTION_PERFORMANCE_SCHEMA_VERSION,
        rule_version: QUESTION_IMPACT_REVIEW_RULE_VERSION.into(),
        plan_public_id: plan_public_id.trim().into(),
        cases,
        boundary_note:
            "建立 case 不改任何成绩。老师保存新评分后仍不会自动发布；只有再次明确发布，正式成绩和学习证据才在同一事务中切换。"
                .into(),
    })
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

pub fn prepare_question_impact_review_cases(
    conn: &mut Connection,
    owner_id: &str,
    request: &PrepareQuestionImpactReviewCasesRequest,
) -> CoreResult<PrepareQuestionImpactReviewCasesResult> {
    required(owner_id, "题库老师")?;
    required(&request.plan_public_id, "影响计划")?;
    required(&request.prepared_by, "准备老师")?;
    if request.prepared_by.trim() != owner_id.trim() {
        return Err(CoreError::Invalid(
            "只能以当前老师身份建立待处理 case".into(),
        ));
    }
    if request.expected_task_count < 0 {
        return Err(CoreError::Invalid("预期待办数不能为负数".into()));
    }
    let (plan_id, action, task_count): (i64, String, i64) = conn
        .query_row(
            "SELECT plan.id,plan.action,
                    (SELECT COUNT(*) FROM exam_question_version_impact_tasks_v2 task
                     WHERE task.impact_plan_id=plan.id)
             FROM exam_question_version_impact_plans_v2 plan
             WHERE plan.public_id=?1 AND plan.planned_by=?2",
            params![request.plan_public_id.trim(), owner_id.trim()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("未找到当前老师的版本影响计划".into()))?;
    if task_count != request.expected_task_count {
        return Err(CoreError::Invalid(
            "影响计划待办数量已变化，请刷新后重试".into(),
        ));
    }
    if action == "future_only" || task_count == 0 {
        return Err(CoreError::Invalid(
            "“只用于以后新作业”没有历史待办，无需建立 case".into(),
        ));
    }

    let now = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    let tasks = {
        let mut statement = tx.prepare(
            "SELECT task.id,task.public_id,task.task_kind,task.attempt_id,
                    task.assessment_item_id,task.publication_id,
                    task.target_answer_key_version_id,task.target_rubric_version_id,
                    task.target_link_set_id
             FROM exam_question_version_impact_tasks_v2 task
             WHERE task.impact_plan_id=?1 AND task.state='open'
             ORDER BY task.id",
        )?;
        let rows = statement.query_map([plan_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, i64>(8)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    if tasks.len() as i64 != task_count {
        return Err(CoreError::Invalid(
            "影响计划待办状态已变化，请刷新后重试".into(),
        ));
    }

    let mut created_count = 0_i64;
    let mut existing_count = 0_i64;
    for (
        task_id,
        task_public_id,
        task_kind,
        attempt_id,
        assessment_item_id,
        publication_id,
        target_answer_key_version_id,
        target_rubric_version_id,
        target_link_set_id,
    ) in tasks
    {
        let existing = tx
            .query_row(
                "SELECT 1 FROM exam_question_version_review_cases_v2
                 WHERE impact_task_id=?1",
                [task_id],
                |_| Ok(()),
            )
            .optional()?;
        if existing.is_some() {
            existing_count += 1;
            continue;
        }

        let source = if task_kind == "recalculate_unpublished" {
            tx.query_row(
                "SELECT id,public_id,revision,teacher_score,point_results_json
                 FROM exam_grade_decisions_v2
                 WHERE attempt_id=?1 AND assessment_item_id=?2 AND state='active'",
                params![attempt_id, assessment_item_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, f64>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()?
        } else if task_kind == "review_published" {
            let publication_id = publication_id
                .ok_or_else(|| CoreError::Invalid("已发布复核待办缺少正式发布版本".into()))?;
            Some(
                tx.query_row(
                    "SELECT decision.id,decision.public_id,decision.revision,
                            decision.teacher_score,decision.point_results_json
                     FROM exam_grade_publication_items_v2 publication_item
                     JOIN exam_grade_publication_decisions_v2 publication_decision
                       ON publication_decision.publication_item_id=publication_item.id
                      AND publication_decision.attempt_id=publication_item.attempt_id
                     JOIN exam_grade_decisions_v2 decision
                       ON decision.id=publication_decision.grade_decision_id
                     WHERE publication_item.publication_id=?1
                       AND publication_item.attempt_id=?2
                       AND decision.assessment_item_id=?3",
                    params![publication_id, attempt_id, assessment_item_id],
                    |row| {
                        Ok((
                            row.get::<_, i64>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, f64>(3)?,
                            row.get::<_, String>(4)?,
                        ))
                    },
                )
                .optional()?
                .ok_or_else(|| CoreError::Invalid("正式发布版本未引用该题的老师评分".into()))?,
            )
        } else {
            return Err(CoreError::Invalid("影响计划待办类型非法".into()));
        };
        let snapshot = ReviewTaskSnapshot {
            task_id,
            task_public_id,
            task_kind: task_kind.clone(),
            attempt_id,
            assessment_item_id,
            publication_id,
            source_grade_decision_id: source.as_ref().map(|value| value.0),
            source_grade_decision_public_id: source.as_ref().map(|value| value.1.clone()),
            source_grade_decision_revision: source.as_ref().map(|value| value.2),
            source_teacher_score: source.as_ref().map(|value| value.3),
            source_point_results_json: source.as_ref().map(|value| value.4.clone()),
            target_answer_key_version_id,
            target_rubric_version_id,
            target_link_set_id,
        };
        let snapshot_hash = hashing::sha256_hex(
            &serde_json::to_vec(&serde_json::json!({
                "schema_version": QUESTION_PERFORMANCE_SCHEMA_VERSION,
                "rule_version": QUESTION_IMPACT_REVIEW_RULE_VERSION,
                "task_public_id": snapshot.task_public_id,
                "task_kind": snapshot.task_kind,
                "attempt_id": snapshot.attempt_id,
                "assessment_item_id": snapshot.assessment_item_id,
                "publication_id": snapshot.publication_id,
                "source_grade_decision_public_id": snapshot.source_grade_decision_public_id,
                "source_grade_decision_revision": snapshot.source_grade_decision_revision,
                "source_teacher_score": snapshot.source_teacher_score,
                "source_point_results_json": snapshot.source_point_results_json,
                "target_answer_key_version_id": snapshot.target_answer_key_version_id,
                "target_rubric_version_id": snapshot.target_rubric_version_id,
                "target_link_set_id": snapshot.target_link_set_id
            }))
            .map_err(|error| CoreError::Parse(format!("待处理快照序列化失败：{error}")))?,
        );
        let case_kind = if task_kind == "recalculate_unpublished" {
            "unpublished_recalculation"
        } else {
            "published_review"
        };
        tx.execute(
            "INSERT INTO exam_question_version_review_cases_v2
             (public_id,impact_task_id,case_kind,attempt_id,assessment_item_id,publication_id,
              source_grade_decision_id,source_grade_decision_revision,source_teacher_score,
              source_point_results_json,source_snapshot_hash,
              target_answer_key_version_id,target_rubric_version_id,target_link_set_id,
              prepared_by,prepared_at,state)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,'open')",
            params![
                ids::new_public_id(),
                snapshot.task_id,
                case_kind,
                snapshot.attempt_id,
                snapshot.assessment_item_id,
                snapshot.publication_id,
                snapshot.source_grade_decision_id,
                snapshot.source_grade_decision_revision,
                snapshot.source_teacher_score,
                snapshot.source_point_results_json,
                snapshot_hash,
                snapshot.target_answer_key_version_id,
                snapshot.target_rubric_version_id,
                snapshot.target_link_set_id,
                request.prepared_by.trim(),
                &now
            ],
        )?;
        created_count += 1;
    }

    if created_count > 0 {
        let payload = serde_json::json!({
            "schema_version": QUESTION_PERFORMANCE_SCHEMA_VERSION,
            "rule_version": QUESTION_IMPACT_REVIEW_RULE_VERSION,
            "plan_public_id": request.plan_public_id.trim(),
            "task_count": task_count,
            "changes_assessment_binding": false,
            "changes_grade": false,
            "changes_publication": false,
            "changes_learning_evidence": false
        })
        .to_string();
        outbox::create_event(
            &tx,
            &NewOutboxEvent {
                idempotency_key: &format!("{}:review-cases:outbox", request.plan_public_id.trim()),
                event_type: "k1.question_version_impact.review_cases_prepared",
                event_version: 1,
                aggregate_type: "exam_question_version_impact_plan",
                aggregate_id: request.plan_public_id.trim(),
                aggregate_revision: 1,
                payload_json: &payload,
                occurred_at: &now,
            },
        )?;
        audit::append(
            &tx,
            &NewAuditEvent {
                idempotency_key: &format!("{}:review-cases:audit", request.plan_public_id.trim()),
                actor_type: AuditActorType::Teacher,
                actor_id: Some(request.prepared_by.trim()),
                action: "k1.question_version_impact.review_cases_prepared",
                object_type: "exam_question_version_impact_plan",
                object_id: request.plan_public_id.trim(),
                object_revision: Some(1),
                note: Some("只冻结待重评/已发布复核证据；未改作业绑定、成绩、发布或图谱"),
                meta_json: Some(&payload),
                occurred_at: &now,
            },
        )?;
    }
    tx.commit()?;
    let catalog = list_question_impact_review_cases(conn, owner_id, &request.plan_public_id)?;
    Ok(PrepareQuestionImpactReviewCasesResult {
        plan_public_id: request.plan_public_id.trim().into(),
        task_count,
        created_count,
        existing_count,
        cases: catalog.cases,
        changes_assessment_binding: false,
        changes_grade: false,
        changes_publication: false,
        changes_learning_evidence: false,
    })
}

#[derive(Serialize)]
struct DefaultUpgradeItemHashInput {
    item_id: i64,
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    order_index: i64,
    score_millis: i64,
}

struct DefaultUpgradeSourceItem {
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    order_index: i64,
    score: f64,
    option_order_json: Option<String>,
    presentation_snapshot_json: String,
}

struct AssessmentDefaultUpgradeScope {
    plan_id: i64,
    plan_question_version_id: i64,
    target_answer_key_version_id: i64,
    target_rubric_version_id: i64,
    target_link_set_id: i64,
    impact_json: String,
    planned_by: String,
    assessment_id: i64,
    assessment_public_id: String,
    assessment_created_by: String,
    source_version_id: i64,
    source_revision: i64,
    source_state: String,
    template_version: Option<String>,
}

fn current_assessment_default(conn: &Connection, assessment_id: i64) -> CoreResult<(i64, String)> {
    conn.query_row(
        "SELECT version.id,version.public_id
         FROM exam_assessment_versions_v2 version
         WHERE version.id=COALESCE(
           (SELECT selection.selected_assessment_version_id
            FROM exam_assessment_default_version_selections_v2 selection
            WHERE selection.assessment_id=?1
            ORDER BY selection.revision DESC,selection.id DESC
            LIMIT 1),
           (SELECT latest.id
            FROM exam_assessment_versions_v2 latest
            WHERE latest.assessment_id=?1 AND latest.state='confirmed'
            ORDER BY latest.revision DESC,latest.id DESC
            LIMIT 1)
         )",
        [assessment_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("作业没有可用于未来上传的已确认版本".into()))
}

fn assessment_default_upgrade_by_id(
    conn: &Connection,
    selection_id: i64,
) -> CoreResult<AssessmentDefaultUpgrade> {
    conn.query_row(
        "SELECT selection.public_id,plan.public_id,assessment.public_id,assessment.title,
                previous.public_id,previous.revision,selected.public_id,selected.revision,
                (SELECT COUNT(*) FROM exam_assessment_items_v2 item
                 WHERE item.assessment_version_id=selected.id
                   AND item.question_version_id=plan.question_version_id
                   AND item.answer_key_version_id=plan.target_answer_key_version_id
                   AND item.rubric_version_id=plan.target_rubric_version_id
                   AND item.link_set_id=plan.target_link_set_id
                   AND item.state='active'),
                selection.selected_by,selection.selected_at
         FROM exam_assessment_default_version_selections_v2 selection
         JOIN exam_question_version_impact_plans_v2 plan
           ON plan.id=selection.source_impact_plan_id
         JOIN exam_assessments_v2 assessment ON assessment.id=selection.assessment_id
         JOIN exam_assessment_versions_v2 previous
           ON previous.id=selection.previous_assessment_version_id
         JOIN exam_assessment_versions_v2 selected
           ON selected.id=selection.selected_assessment_version_id
         WHERE selection.id=?1",
        [selection_id],
        |row| {
            Ok(AssessmentDefaultUpgrade {
                selection_public_id: row.get(0)?,
                plan_public_id: row.get(1)?,
                assessment_public_id: row.get(2)?,
                assessment_title: row.get(3)?,
                source_assessment_version_public_id: row.get(4)?,
                source_revision: row.get(5)?,
                default_assessment_version_public_id: row.get(6)?,
                default_revision: row.get(7)?,
                upgraded_item_count: row.get(8)?,
                selected_by: row.get(9)?,
                selected_at: row.get(10)?,
                default_for_future_intake: true,
                changes_historical_attempts: false,
                changes_grade: false,
                changes_publication: false,
                changes_learning_evidence: false,
            })
        },
    )
    .map_err(Into::into)
}

pub fn upgrade_assessment_default_from_impact(
    conn: &mut Connection,
    owner_id: &str,
    request: &UpgradeAssessmentDefaultRequest,
) -> CoreResult<AssessmentDefaultUpgrade> {
    required(owner_id, "题库老师")?;
    required(&request.request_key, "请求键")?;
    required(&request.plan_public_id, "影响计划")?;
    required(&request.source_assessment_version_public_id, "来源作业版本")?;
    required(
        &request.expected_current_default_version_public_id,
        "预期当前默认版本",
    )?;
    required(&request.upgraded_by, "升级老师")?;
    if request.upgraded_by.trim() != owner_id.trim() {
        return Err(CoreError::Invalid(
            "只能以当前老师身份升级未来作业版本".into(),
        ));
    }
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": QUESTION_PERFORMANCE_SCHEMA_VERSION,
            "rule_version": ASSESSMENT_DEFAULT_UPGRADE_RULE_VERSION,
            "plan_public_id": request.plan_public_id.trim(),
            "source_assessment_version_public_id":
                request.source_assessment_version_public_id.trim(),
            "expected_current_default_version_public_id":
                request.expected_current_default_version_public_id.trim(),
            "upgraded_by": request.upgraded_by.trim()
        }))
        .map_err(|error| CoreError::Parse(format!("未来默认版本请求序列化失败：{error}")))?,
    );
    if let Some((selection_id, existing_hash)) = conn
        .query_row(
            "SELECT id,request_hash
             FROM exam_assessment_default_version_selections_v2
             WHERE request_key=?1",
            [request.request_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid(
                "同一请求键对应不同的未来默认版本升级".into(),
            ));
        }
        return assessment_default_upgrade_by_id(conn, selection_id);
    }

    let scope = conn
        .query_row(
            "SELECT plan.id,plan.question_version_id,plan.target_answer_key_version_id,
                    plan.target_rubric_version_id,plan.target_link_set_id,plan.impact_json,
                    plan.planned_by,assessment.id,assessment.public_id,
                    assessment.created_by,source.id,source.revision,source.state,
                    source.template_version
             FROM exam_question_version_impact_plans_v2 plan
             JOIN exam_assessment_versions_v2 source
               ON source.public_id=?2
             JOIN exam_assessments_v2 assessment ON assessment.id=source.assessment_id
             WHERE plan.public_id=?1",
            params![
                request.plan_public_id.trim(),
                request.source_assessment_version_public_id.trim()
            ],
            |row| {
                Ok(AssessmentDefaultUpgradeScope {
                    plan_id: row.get(0)?,
                    plan_question_version_id: row.get(1)?,
                    target_answer_key_version_id: row.get(2)?,
                    target_rubric_version_id: row.get(3)?,
                    target_link_set_id: row.get(4)?,
                    impact_json: row.get(5)?,
                    planned_by: row.get(6)?,
                    assessment_id: row.get(7)?,
                    assessment_public_id: row.get(8)?,
                    assessment_created_by: row.get(9)?,
                    source_version_id: row.get(10)?,
                    source_revision: row.get(11)?,
                    source_state: row.get(12)?,
                    template_version: row.get(13)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("未找到影响计划或来源作业版本".into()))?;
    let AssessmentDefaultUpgradeScope {
        plan_id,
        plan_question_version_id,
        target_answer_key_version_id,
        target_rubric_version_id,
        target_link_set_id,
        impact_json,
        planned_by,
        assessment_id,
        assessment_public_id,
        assessment_created_by,
        source_version_id,
        source_revision,
        source_state,
        template_version,
    } = scope;
    if planned_by != owner_id.trim() || assessment_created_by != owner_id.trim() {
        return Err(CoreError::Invalid("影响计划或作业不属于当前老师".into()));
    }
    if source_state != "confirmed" {
        return Err(CoreError::Invalid(
            "只能从已确认作业版本派生未来版本".into(),
        ));
    }
    let frozen_impact: Value = serde_json::from_str(&impact_json)
        .map_err(|error| CoreError::Parse(format!("影响计划快照无法读取：{error}")))?;
    let source_was_reviewed = frozen_impact
        .get("rows")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|row| {
            row.get("assessmentVersionPublicId").and_then(Value::as_str)
                == Some(request.source_assessment_version_public_id.trim())
        });
    if !source_was_reviewed {
        return Err(CoreError::Invalid(
            "来源作业版本不在已冻结的影响范围内".into(),
        ));
    }
    let (current_default_id, current_default_public_id) =
        current_assessment_default(conn, assessment_id)?;
    if current_default_public_id != request.expected_current_default_version_public_id.trim()
        || current_default_id != source_version_id
    {
        return Err(CoreError::Invalid(
            "未来默认作业版本已变化，请刷新影响预览后重试".into(),
        ));
    }
    let affected_item_count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM exam_assessment_items_v2 item
         WHERE item.assessment_version_id=?1
           AND item.question_version_id=?2
           AND item.state='active'
           AND (item.answer_key_version_id<>?3
                OR item.rubric_version_id<>?4
                OR item.link_set_id<>?5)",
        params![
            source_version_id,
            plan_question_version_id,
            target_answer_key_version_id,
            target_rubric_version_id,
            target_link_set_id
        ],
        |row| row.get(0),
    )?;
    if affected_item_count != 1 {
        return Err(CoreError::Invalid(
            "当前默认作业版本不再包含唯一的待升级题目".into(),
        ));
    }

    let mut statement = conn.prepare(
        "SELECT question_version_id,answer_key_version_id,rubric_version_id,link_set_id,
                order_index,score,option_order_json,presentation_snapshot_json
         FROM exam_assessment_items_v2
         WHERE assessment_version_id=?1 AND state='active'
         ORDER BY order_index,id",
    )?;
    let source_items = statement
        .query_map([source_version_id], |row| {
            Ok(DefaultUpgradeSourceItem {
                question_version_id: row.get(0)?,
                answer_key_version_id: row.get(1)?,
                rubric_version_id: row.get(2)?,
                link_set_id: row.get(3)?,
                order_index: row.get(4)?,
                score: row.get(5)?,
                option_order_json: row.get(6)?,
                presentation_snapshot_json: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    if source_items.is_empty() {
        return Err(CoreError::Invalid("来源作业版本没有有效题目".into()));
    }

    let now = time::utc_now_rfc3339();
    let selection_public_id = ids::new_public_id();
    let selected_version_public_id = ids::new_public_id();
    let tx = conn.transaction()?;
    let selected_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1
         FROM exam_assessment_versions_v2 WHERE assessment_id=?1",
        [assessment_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO exam_assessment_versions_v2
         (public_id,assessment_id,revision,template_version,state,
          supersedes_version_id,created_at)
         VALUES (?1,?2,?3,?4,'draft',?5,?6)",
        params![
            &selected_version_public_id,
            assessment_id,
            selected_revision,
            template_version.as_deref(),
            source_version_id,
            &now
        ],
    )?;
    let selected_version_id = tx.last_insert_rowid();
    let mut hash_items = Vec::with_capacity(source_items.len());
    let mut upgraded_item_count = 0_i64;
    for item in source_items {
        let (answer_key_version_id, rubric_version_id, link_set_id) =
            if item.question_version_id == plan_question_version_id {
                upgraded_item_count += 1;
                (
                    target_answer_key_version_id,
                    target_rubric_version_id,
                    target_link_set_id,
                )
            } else {
                (
                    item.answer_key_version_id,
                    item.rubric_version_id,
                    item.link_set_id,
                )
            };
        tx.execute(
            "INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,option_order_json,
              presentation_snapshot_json,state,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11)",
            params![
                ids::new_public_id(),
                selected_version_id,
                item.question_version_id,
                answer_key_version_id,
                rubric_version_id,
                link_set_id,
                item.order_index,
                item.score,
                item.option_order_json.as_deref(),
                &item.presentation_snapshot_json,
                &now
            ],
        )?;
        hash_items.push(DefaultUpgradeItemHashInput {
            item_id: tx.last_insert_rowid(),
            question_version_id: item.question_version_id,
            answer_key_version_id,
            rubric_version_id,
            link_set_id,
            order_index: item.order_index,
            score_millis: (item.score * 1000.0).round() as i64,
        });
    }
    if upgraded_item_count != 1 {
        return Err(CoreError::Invalid("升级题目数量发生变化，已回滚".into()));
    }
    let item_set_hash = hashing::sha256_hex(
        &serde_json::to_vec(&hash_items)
            .map_err(|error| CoreError::Parse(format!("未来作业版本 hash 失败：{error}")))?,
    );
    tx.execute(
        "UPDATE exam_assessment_versions_v2
         SET item_set_hash=?1,state='confirmed',confirmed_by=?2,confirmed_at=?3
         WHERE id=?4 AND state='draft'",
        params![
            &item_set_hash,
            request.upgraded_by.trim(),
            &now,
            selected_version_id
        ],
    )?;
    let selection_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1
         FROM exam_assessment_default_version_selections_v2
         WHERE assessment_id=?1",
        [assessment_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO exam_assessment_default_version_selections_v2
         (public_id,request_key,request_hash,assessment_id,revision,
          previous_assessment_version_id,selected_assessment_version_id,
          source_impact_plan_id,selected_by,selected_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![
            &selection_public_id,
            request.request_key.trim(),
            &request_hash,
            assessment_id,
            selection_revision,
            source_version_id,
            selected_version_id,
            plan_id,
            request.upgraded_by.trim(),
            &now
        ],
    )?;
    let selection_id = tx.last_insert_rowid();
    tx.execute(
        "UPDATE exam_assessments_v2 SET updated_at=?1 WHERE id=?2",
        params![&now, assessment_id],
    )?;
    let payload = serde_json::json!({
        "schema_version": QUESTION_PERFORMANCE_SCHEMA_VERSION,
        "rule_version": ASSESSMENT_DEFAULT_UPGRADE_RULE_VERSION,
        "assessment_public_id": assessment_public_id,
        "source_assessment_version_public_id":
            request.source_assessment_version_public_id.trim(),
        "source_revision": source_revision,
        "selected_assessment_version_public_id": selected_version_public_id,
        "selected_revision": selected_revision,
        "upgraded_item_count": upgraded_item_count,
        "default_for_future_intake": true,
        "changes_historical_attempts": false,
        "changes_grade": false,
        "changes_publication": false,
        "changes_learning_evidence": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "exam.assessment.future_default_upgraded",
            event_version: 1,
            aggregate_type: "exam_assessment_default_version_selection",
            aggregate_id: &selection_public_id,
            aggregate_revision: selection_revision,
            payload_json: &payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("{}:audit", request.request_key.trim()),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(request.upgraded_by.trim()),
            action: "exam.assessment.future_default_upgraded",
            object_type: "exam_assessment_default_version_selection",
            object_id: &selection_public_id,
            object_revision: Some(selection_revision),
            note: Some("仅切换未来上传默认版本；历史作答、成绩、发布和学习证据均未改动"),
            meta_json: Some(&payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    assessment_default_upgrade_by_id(conn, selection_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use module_knowledge::db::content::{
        create_answer_key_version, create_link_set, create_question, create_rubric_version,
        NewAnswerKeyVersion, NewQuestion, NewRubricPoint, NewRubricVersion, QuestionVersion,
    };
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    const NOW: &str = "2026-07-19T10:00:00Z";

    struct Fixture {
        conn: Connection,
        question_version_public_id: String,
        question_version_id: i64,
        old_answer_id: i64,
        old_rubric_id: i64,
        old_link_id: i64,
    }

    fn fixture_with_question_type(question_type: &str) -> Fixture {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        conn.execute_batch(
            "CREATE TABLE profile_evidence_links(
               snapshot_id INTEGER NOT NULL,
               node_metric_id INTEGER NOT NULL,
               learning_evidence_id INTEGER NOT NULL
             );
             INSERT INTO subjects(name) VALUES ('历史');
             INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
             INSERT INTO students(name,student_no,class_id,enabled)
             VALUES ('学生甲','01',1,1),('学生乙','02',1,1);
             INSERT INTO k1_textbook_editions
             (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
             VALUES ('edition-history-8a',1,'pep','2026','中国历史八年级上册','八年级',
                     'upper','active','2026-07-01T00:00:00Z');
             INSERT INTO k1_knowledge_maps
             (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
             VALUES ('knowledge-map-history-8a-v1',1,1,'confirmed',
                     '2026-07-01T00:00:00Z','2026-07-01T00:00:00Z');",
        )
        .unwrap();
        let question = create_question(
            &conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: "local_teacher",
                question_family_id: None,
                rights_status: "cleared",
                sharing_allowed: false,
            },
        )
        .unwrap();
        let version_public_id = "question-version-opium-war-year".to_string();
        conn.execute(
            "INSERT INTO k1_question_versions
             (public_id,question_id,revision,question_type,stem,max_score,content_hash,
              quality_level,state,created_at)
             VALUES (?1,?2,1,?3,'鸦片战争爆发于哪一年？',2,
                     'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                     'L3','published',?4)",
            params![&version_public_id, question.id, question_type, NOW],
        )
        .unwrap();
        let version = QuestionVersion {
            id: conn.last_insert_rowid(),
            public_id: version_public_id,
            question_id: question.id,
            revision: 1,
            question_type: question_type.into(),
            stem: "鸦片战争爆发于哪一年？".into(),
            material_text: None,
            max_score: 2.0,
            content_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "L3".into(),
            state: "published".into(),
            created_at: NOW.into(),
        };
        let old_answer = create_answer_key_version(
            &conn,
            &NewAnswerKeyVersion {
                question_version_id: version.id,
                revision: 1,
                answer_json: "{\"schema_version\":1,\"correct_labels\":[\"A\"]}",
                state: "confirmed",
                supersedes_answer_key_id: None,
                confirmed_by: Some("local_teacher"),
                slots: &[],
            },
        )
        .unwrap();
        let old_rubric = create_rubric_version(
            &conn,
            &NewRubricVersion {
                question_version_id: version.id,
                revision: 1,
                max_score: 2.0,
                state: "confirmed",
                supersedes_rubric_id: None,
                confirmed_by: Some("local_teacher"),
                points: &[NewRubricPoint {
                    stable_id: None,
                    order_index: 0,
                    canonical_text: "选择正确年份",
                    allowed_paraphrases_json: None,
                    required_concepts_json: None,
                    max_score: 2.0,
                }],
            },
        )
        .unwrap();
        let old_link = create_link_set(
            &conn,
            version.id,
            1,
            1,
            "confirmed",
            Some("local_teacher"),
            None,
        )
        .unwrap();
        let _new_answer = create_answer_key_version(
            &conn,
            &NewAnswerKeyVersion {
                question_version_id: version.id,
                revision: 2,
                answer_json: "{\"schema_version\":1,\"correct_labels\":[\"B\"]}",
                state: "confirmed",
                supersedes_answer_key_id: Some(old_answer.id),
                confirmed_by: Some("local_teacher"),
                slots: &[],
            },
        )
        .unwrap();
        let _new_rubric = create_rubric_version(
            &conn,
            &NewRubricVersion {
                question_version_id: version.id,
                revision: 2,
                max_score: 2.0,
                state: "confirmed",
                supersedes_rubric_id: Some(old_rubric.id),
                confirmed_by: Some("local_teacher"),
                points: &[NewRubricPoint {
                    stable_id: None,
                    order_index: 0,
                    canonical_text: "选择修订后的正确年份",
                    allowed_paraphrases_json: None,
                    required_concepts_json: None,
                    max_score: 2.0,
                }],
            },
        )
        .unwrap();
        let _new_link = create_link_set(
            &conn,
            version.id,
            1,
            2,
            "confirmed",
            Some("local_teacher"),
            Some(old_link.id),
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exam_assessments_v2
             (public_id,title,class_id,assessment_context,evidence_policy,state,
              created_by,created_at,updated_at)
             VALUES ('assessment-1','第一单元检测',1,'quiz','include','active',
                     'local_teacher',?1,?1)",
            [NOW],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exam_assessment_versions_v2
             (public_id,assessment_id,revision,item_set_hash,state,created_at,confirmed_by,confirmed_at)
             VALUES ('assessment-version-1',1,1,
             'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
             'confirmed',?1,'local_teacher',?1)",
            [NOW],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,state,created_at)
             VALUES ('assessment-item-1',1,?1,?2,?3,?4,0,2,
                     '{\"schema_version\":1}','active',?5)",
            params![version.id, old_answer.id, old_rubric.id, old_link.id, NOW],
        )
        .unwrap();
        for (student_id, state, publication) in [
            (1_i64, "ready_to_publish", false),
            (2_i64, "published", true),
        ] {
            conn.execute(
                "INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES (?1,1,?2,1,'image','first',?3,?4,?4)",
                params![format!("attempt-{student_id}"), student_id, state, NOW],
            )
            .unwrap();
            let attempt_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO exam_grade_decisions_v2
                 (public_id,attempt_id,assessment_item_id,revision,teacher_score,
                  point_results_json,confirmation_level,state,decided_by,decided_at,created_at)
                 VALUES (?1,?2,1,1,2,'{\"schema_version\":1}',
                         'teacher_accepted','active','local_teacher',?3,?3)",
                params![format!("decision-{student_id}"), attempt_id, NOW],
            )
            .unwrap();
            if publication {
                let decision_id = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO exam_grade_publications_v2
                     (public_id,assessment_version_id,revision,state,published_by,published_at,created_at)
                     VALUES ('publication-1',1,1,'published','local_teacher',?1,?1)",
                    [NOW],
                )
                .unwrap();
                let publication_id = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO exam_grade_publication_items_v2
                     (publication_id,attempt_id,grade_decision_set_hash,total_score,created_at)
                     VALUES (?1,?2,
                     'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',2,?3)",
                    params![publication_id, attempt_id, NOW],
                )
                .unwrap();
                let publication_item_id = conn.last_insert_rowid();
                conn.execute(
                    "INSERT INTO exam_grade_publication_decisions_v2
                     (publication_item_id,attempt_id,grade_decision_id,created_at)
                     VALUES (?1,?2,?3,?4)",
                    params![publication_item_id, attempt_id, decision_id, NOW],
                )
                .unwrap();
                conn.execute(
                    "UPDATE exam_attempts_v2 SET active_publication_id=?1 WHERE id=?2",
                    params![publication_id, attempt_id],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO learning_evidence
                     (public_id,idempotency_key,student_id,source_module,source_type,
                      source_ref_type,source_ref_id,source_revision,decision_ref_type,
                      decision_ref_id,decision_revision,evidence_kind,value,confirmation_level,
                      evidence_quality,assessment_context,occurred_at,rule_version,
                      knowledge_map_version,state)
                     VALUES ('00000000-0000-7000-8000-000000000001','evidence-1',2,'grading',
                      'objective_question','assessment_item','assessment-item-1',1,
                      'grade_decision','decision-2',1,'accuracy',1,'teacher_accepted',1,
                      'closed_book',?1,'objective-grading-v1','map-v1','active')",
                    [NOW],
                )
                .unwrap();
                conn.execute(
                    "INSERT INTO profile_evidence_links(snapshot_id,node_metric_id,learning_evidence_id)
                     VALUES (1,1,1)",
                    [],
                )
                .unwrap();
            }
        }
        Fixture {
            conn,
            question_version_public_id: version.public_id,
            question_version_id: version.id,
            old_answer_id: old_answer.id,
            old_rubric_id: old_rubric.id,
            old_link_id: old_link.id,
        }
    }

    fn fixture() -> Fixture {
        fixture_with_question_type("single")
    }

    #[test]
    fn performance_uses_only_current_published_teacher_decisions() {
        let fixture = fixture();
        fixture
            .conn
            .execute_batch(
                "INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('unused-question','personal','local_teacher','cleared',0,
                         '2026-07-19T10:00:00Z');
                 INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('unused-version',2,1,'true_false','未使用候选题',1,
                         'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                         'C0','candidate','2026-07-19T10:00:00Z');",
            )
            .unwrap();
        let catalog = list_question_performance(&fixture.conn, "local_teacher", 50).unwrap();
        assert_eq!(catalog.items.len(), 1, "未被作业使用的候选题不进入表现页");
        let item = catalog
            .items
            .iter()
            .find(|item| item.question_version_public_id == fixture.question_version_public_id)
            .unwrap();
        assert_eq!(item.assessment_usage_count, 1);
        assert_eq!(item.published_response_count, 1);
        assert_eq!(item.full_credit_count, 1);
        assert_eq!(item.average_score_rate, Some(1.0));
        assert_eq!(item.context_breakdown[0].assessment_context, "quiz");
        assert!(item.has_version_update_impact);
    }

    #[test]
    fn impact_preview_and_plans_never_change_grades_or_evidence() {
        let mut fixture = fixture();
        let preview = preview_question_version_impact(
            &fixture.conn,
            "local_teacher",
            &fixture.question_version_public_id,
        )
        .unwrap();
        assert_eq!(preview.affected_assessment_count, 1);
        assert_eq!(preview.unpublished_attempt_count, 1);
        assert_eq!(preview.published_attempt_count, 1);
        assert_eq!(preview.active_learning_evidence_count, 1);
        assert_eq!(preview.profile_snapshot_count, 1);
        let before: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                 (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                 (SELECT COUNT(*) FROM exam_grade_publications_v2),
                 (SELECT COUNT(*) FROM learning_evidence)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let request = ConfirmQuestionImpactPlanRequest {
            request_key: "impact-unpublished".into(),
            question_version_public_id: fixture.question_version_public_id.clone(),
            expected_preview_hash: preview.preview_hash.clone(),
            action: "recalculate_unpublished".into(),
            planned_by: "local_teacher".into(),
        };
        let plan =
            confirm_question_impact_plan(&mut fixture.conn, "local_teacher", &request).unwrap();
        assert_eq!(plan.task_count, 1);
        assert!(!plan.changes_grade);
        let repeated =
            confirm_question_impact_plan(&mut fixture.conn, "local_teacher", &request).unwrap();
        assert_eq!(plan.public_id, repeated.public_id);
        let prepare = PrepareQuestionImpactReviewCasesRequest {
            plan_public_id: plan.public_id.clone(),
            expected_task_count: 1,
            prepared_by: "local_teacher".into(),
        };
        let prepared =
            prepare_question_impact_review_cases(&mut fixture.conn, "local_teacher", &prepare)
                .unwrap();
        assert_eq!(prepared.created_count, 1);
        assert_eq!(prepared.existing_count, 0);
        assert_eq!(prepared.cases.len(), 1);
        assert_eq!(prepared.cases[0].case_kind, "unpublished_recalculation");
        assert_eq!(
            prepared.cases[0].source_grade_decision_public_id.as_deref(),
            Some("decision-1")
        );
        assert_eq!(prepared.cases[0].source_teacher_score, Some(2.0));
        assert!(!prepared.changes_grade);
        let retried =
            prepare_question_impact_review_cases(&mut fixture.conn, "local_teacher", &prepare)
                .unwrap();
        assert_eq!(retried.created_count, 0);
        assert_eq!(retried.existing_count, 1);
        assert_eq!(retried.cases[0].public_id, prepared.cases[0].public_id);
        assert!(fixture
            .conn
            .execute(
                "UPDATE exam_question_version_review_cases_v2 SET state='open'",
                [],
            )
            .is_err());
        assert!(fixture
            .conn
            .execute("DELETE FROM exam_question_version_review_cases_v2", [])
            .is_err());
        let after: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                 (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                 (SELECT COUNT(*) FROM exam_grade_publications_v2),
                 (SELECT COUNT(*) FROM learning_evidence)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(before, after);
        let source_ids: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT source_answer_key_version_id,source_rubric_version_id,source_link_set_id
                 FROM exam_question_version_impact_tasks_v2",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            source_ids,
            (
                fixture.old_answer_id,
                fixture.old_rubric_id,
                fixture.old_link_id
            )
        );
    }

    #[test]
    fn published_review_is_separate_and_preview_drift_is_rejected() {
        let mut fixture = fixture();
        let preview = preview_question_version_impact(
            &fixture.conn,
            "local_teacher",
            &fixture.question_version_public_id,
        )
        .unwrap();
        let new_answer = create_answer_key_version(
            &fixture.conn,
            &NewAnswerKeyVersion {
                question_version_id: fixture.question_version_id,
                revision: 3,
                answer_json: "{\"schema_version\":1,\"correct_labels\":[\"C\"]}",
                state: "confirmed",
                supersedes_answer_key_id: None,
                confirmed_by: Some("local_teacher"),
                slots: &[],
            },
        )
        .unwrap();
        assert!(new_answer.id > fixture.old_answer_id);
        let stale = ConfirmQuestionImpactPlanRequest {
            request_key: "impact-stale".into(),
            question_version_public_id: fixture.question_version_public_id.clone(),
            expected_preview_hash: preview.preview_hash,
            action: "review_published".into(),
            planned_by: "local_teacher".into(),
        };
        assert!(confirm_question_impact_plan(&mut fixture.conn, "local_teacher", &stale).is_err());
        let refreshed = preview_question_version_impact(
            &fixture.conn,
            "local_teacher",
            &fixture.question_version_public_id,
        )
        .unwrap();
        let request = ConfirmQuestionImpactPlanRequest {
            request_key: "impact-published".into(),
            question_version_public_id: fixture.question_version_public_id.clone(),
            expected_preview_hash: refreshed.preview_hash,
            action: "review_published".into(),
            planned_by: "local_teacher".into(),
        };
        let plan =
            confirm_question_impact_plan(&mut fixture.conn, "local_teacher", &request).unwrap();
        assert_eq!(plan.task_count, 1);
        let kind: (String, Option<i64>) = fixture
            .conn
            .query_row(
                "SELECT task_kind,publication_id
                 FROM exam_question_version_impact_tasks_v2",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(kind.0, "review_published");
        assert!(kind.1.is_some());
        let prepared = prepare_question_impact_review_cases(
            &mut fixture.conn,
            "local_teacher",
            &PrepareQuestionImpactReviewCasesRequest {
                plan_public_id: plan.public_id,
                expected_task_count: 1,
                prepared_by: "local_teacher".into(),
            },
        )
        .unwrap();
        assert_eq!(prepared.cases[0].case_kind, "published_review");
        assert_eq!(
            prepared.cases[0].publication_public_id.as_deref(),
            Some("publication-1")
        );
        assert_eq!(
            prepared.cases[0].source_grade_decision_public_id.as_deref(),
            Some("decision-2")
        );
        assert!(prepared.cases[0]
            .next_step_note
            .contains("新的发布 revision"));
    }

    fn prepared_case(
        fixture: &mut Fixture,
        action: &str,
        request_key: &str,
    ) -> QuestionImpactReviewCase {
        let preview = preview_question_version_impact(
            &fixture.conn,
            "local_teacher",
            &fixture.question_version_public_id,
        )
        .unwrap();
        let plan = confirm_question_impact_plan(
            &mut fixture.conn,
            "local_teacher",
            &ConfirmQuestionImpactPlanRequest {
                request_key: request_key.into(),
                question_version_public_id: fixture.question_version_public_id.clone(),
                expected_preview_hash: preview.preview_hash,
                action: action.into(),
                planned_by: "local_teacher".into(),
            },
        )
        .unwrap();
        prepare_question_impact_review_cases(
            &mut fixture.conn,
            "local_teacher",
            &PrepareQuestionImpactReviewCasesRequest {
                plan_public_id: plan.public_id,
                expected_task_count: 1,
                prepared_by: "local_teacher".into(),
            },
        )
        .unwrap()
        .cases
        .remove(0)
    }

    #[test]
    fn unpublished_case_confirms_new_grade_then_requires_explicit_publication() {
        let mut fixture = fixture();
        let review_case = prepared_case(
            &mut fixture,
            "recalculate_unpublished",
            "impact-unpublished-resolution-plan",
        );
        let before: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_grade_publications_v2),
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
                   (SELECT COUNT(*) FROM exam_question_version_review_resolutions_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        let request = ResolveQuestionImpactReviewCaseRequest {
            request_key: "impact-unpublished-resolution".into(),
            case_public_id: review_case.public_id.clone(),
            expected_source_snapshot_hash: review_case.source_snapshot_hash.clone(),
            teacher_score: Some(0.0),
            components: vec![],
            teacher_note: "按修订后的标准答案确认不得分".into(),
            resolved_by: "local_teacher".into(),
        };
        let resolved =
            resolve_question_impact_review_case(&mut fixture.conn, "local_teacher", &request)
                .unwrap();
        assert_eq!(resolved.grade_decision.revision, 2);
        assert_eq!(resolved.grade_decision.teacher_score, 0.0);
        assert!(resolved.old_publication_unchanged);
        assert!(resolved.learning_evidence_unchanged);
        assert!(resolved.requires_explicit_publication);
        let after_resolution: (i64, i64, i64, Option<i64>, String) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_grade_publications_v2),
                   (SELECT COUNT(*) FROM learning_evidence WHERE state='active'),
                   (SELECT COUNT(*) FROM exam_question_version_review_resolutions_v2),
                   attempt.active_publication_id,attempt.state
                 FROM exam_attempts_v2 attempt WHERE attempt.public_id='attempt-1'",
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
        assert_eq!(
            (before.0, before.1, 1),
            (after_resolution.0, after_resolution.1, after_resolution.2)
        );
        assert_eq!(after_resolution.3, None);
        assert_eq!(after_resolution.4, "ready_to_publish");
        let repeated =
            resolve_question_impact_review_case(&mut fixture.conn, "local_teacher", &request)
                .unwrap();
        assert_eq!(
            repeated.grade_decision.public_id,
            resolved.grade_decision.public_id
        );
        let catalog = list_question_impact_review_cases(
            &fixture.conn,
            "local_teacher",
            &review_case.plan_public_id,
        )
        .unwrap();
        assert_eq!(catalog.cases[0].state, "grade_confirmed");
        assert!(!catalog.cases[0].changes_publication);
        let published = publish_question_impact_review_case(
            &fixture.conn,
            "local_teacher",
            &PublishQuestionImpactReviewCaseRequest {
                case_public_id: review_case.public_id.clone(),
                expected_grade_decision_public_id: resolved.grade_decision.public_id.clone(),
                published_by: "local_teacher".into(),
            },
        )
        .unwrap();
        assert_eq!(published.publication.total_score, 0.0);
        assert!(!published.prior_publication_superseded);
        let final_catalog = list_question_impact_review_cases(
            &fixture.conn,
            "local_teacher",
            &review_case.plan_public_id,
        )
        .unwrap();
        assert_eq!(final_catalog.cases[0].state, "republished");
        assert!(final_catalog.cases[0].changes_publication);
    }

    #[test]
    fn published_case_keeps_old_evidence_until_republication_switches_it() {
        let mut fixture = fixture();
        let review_case = prepared_case(
            &mut fixture,
            "review_published",
            "impact-published-resolution-plan",
        );
        let request = ResolveQuestionImpactReviewCaseRequest {
            request_key: "impact-published-resolution".into(),
            case_public_id: review_case.public_id.clone(),
            expected_source_snapshot_hash: review_case.source_snapshot_hash.clone(),
            teacher_score: Some(0.0),
            components: vec![],
            teacher_note: "复核正式成绩后按新答案改为不得分".into(),
            resolved_by: "local_teacher".into(),
        };
        let resolved =
            resolve_question_impact_review_case(&mut fixture.conn, "local_teacher", &request)
                .unwrap();
        let after_resolution: (String, String, i64, String) = fixture
            .conn
            .query_row(
                "SELECT publication.public_id,publication.state,
                        (SELECT COUNT(*) FROM learning_evidence
                         WHERE decision_ref_id='decision-2' AND state='active'),
                        attempt.state
                 FROM exam_attempts_v2 attempt
                 JOIN exam_grade_publications_v2 publication
                   ON publication.id=attempt.active_publication_id
                 WHERE attempt.public_id='attempt-2'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(after_resolution.0, "publication-1");
        assert_eq!(after_resolution.1, "published");
        assert_eq!(after_resolution.2, 1);
        assert_eq!(after_resolution.3, "ready_to_publish");
        let published = publish_question_impact_review_case(
            &fixture.conn,
            "local_teacher",
            &PublishQuestionImpactReviewCaseRequest {
                case_public_id: review_case.public_id.clone(),
                expected_grade_decision_public_id: resolved.grade_decision.public_id.clone(),
                published_by: "local_teacher".into(),
            },
        )
        .unwrap();
        assert!(published.prior_publication_superseded);
        assert!(published.learning_evidence_switched);
        let after_publication: (String, String, i64, String) = fixture
            .conn
            .query_row(
                "SELECT old_publication.state,new_publication.state,
                        (SELECT COUNT(*) FROM learning_evidence
                         WHERE decision_ref_id='decision-2' AND state='reverted'),
                        attempt.state
                 FROM exam_attempts_v2 attempt
                 JOIN exam_grade_publications_v2 new_publication
                   ON new_publication.id=attempt.active_publication_id
                 JOIN exam_grade_publications_v2 old_publication
                   ON old_publication.public_id='publication-1'
                 WHERE attempt.public_id='attempt-2'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(after_publication.0, "superseded");
        assert_eq!(after_publication.1, "published");
        assert_eq!(after_publication.2, 1);
        assert_eq!(after_publication.3, "published");
        let repeated = publish_question_impact_review_case(
            &fixture.conn,
            "local_teacher",
            &PublishQuestionImpactReviewCaseRequest {
                case_public_id: review_case.public_id,
                expected_grade_decision_public_id: resolved.grade_decision.public_id,
                published_by: "local_teacher".into(),
            },
        )
        .unwrap();
        assert_eq!(
            repeated.publication.public_id,
            published.publication.public_id
        );
    }

    #[test]
    fn subjective_case_requires_complete_components_and_student_evidence_for_credit() {
        let mut fixture = fixture_with_question_type("short_answer");
        let review_case = prepared_case(
            &mut fixture,
            "recalculate_unpublished",
            "impact-subjective-resolution-plan",
        );
        assert_eq!(review_case.target_components.len(), 1);
        assert_eq!(review_case.target_components[0].source_type, "rubric_point");
        let component = review_case.target_components[0].clone();
        let unsupported_credit = resolve_question_impact_review_case(
            &mut fixture.conn,
            "local_teacher",
            &ResolveQuestionImpactReviewCaseRequest {
                request_key: "impact-subjective-without-evidence".into(),
                case_public_id: review_case.public_id.clone(),
                expected_source_snapshot_hash: review_case.source_snapshot_hash.clone(),
                teacher_score: None,
                components: vec![ResolveQuestionImpactComponentInput {
                    source_public_id: component.source_public_id.clone(),
                    teacher_score: component.max_score,
                    evidence_text: None,
                    teacher_note: None,
                }],
                teacher_note: "尝试无证据给分".into(),
                resolved_by: "local_teacher".into(),
            },
        );
        assert!(unsupported_credit.is_err());
        let resolution_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_question_version_review_resolutions_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(resolution_count, 0);
        let resolved = resolve_question_impact_review_case(
            &mut fixture.conn,
            "local_teacher",
            &ResolveQuestionImpactReviewCaseRequest {
                request_key: "impact-subjective-zero".into(),
                case_public_id: review_case.public_id,
                expected_source_snapshot_hash: review_case.source_snapshot_hash,
                teacher_score: None,
                components: vec![ResolveQuestionImpactComponentInput {
                    source_public_id: component.source_public_id,
                    teacher_score: 0.0,
                    evidence_text: None,
                    teacher_note: Some("未找到该评分点".into()),
                }],
                teacher_note: "逐项核对后确认不得分".into(),
                resolved_by: "local_teacher".into(),
            },
        )
        .unwrap();
        assert_eq!(resolved.grade_decision.teacher_score, 0.0);
        let point_results: Value =
            serde_json::from_str(&resolved.grade_decision.point_results_json).unwrap();
        assert_eq!(
            point_results["component_results"]["components"][0]["result_status"],
            "incorrect"
        );
    }

    #[test]
    fn review_case_preparation_rejects_attempt_scope_drift() {
        let mut fixture = fixture();
        let preview = preview_question_version_impact(
            &fixture.conn,
            "local_teacher",
            &fixture.question_version_public_id,
        )
        .unwrap();
        let plan = confirm_question_impact_plan(
            &mut fixture.conn,
            "local_teacher",
            &ConfirmQuestionImpactPlanRequest {
                request_key: "impact-drift-before-case".into(),
                question_version_public_id: fixture.question_version_public_id.clone(),
                expected_preview_hash: preview.preview_hash,
                action: "recalculate_unpublished".into(),
                planned_by: "local_teacher".into(),
            },
        )
        .unwrap();
        fixture
            .conn
            .execute(
                "UPDATE exam_attempts_v2 SET state='voided' WHERE public_id='attempt-1'",
                [],
            )
            .unwrap();
        let result = prepare_question_impact_review_cases(
            &mut fixture.conn,
            "local_teacher",
            &PrepareQuestionImpactReviewCasesRequest {
                plan_public_id: plan.public_id,
                expected_task_count: 1,
                prepared_by: "local_teacher".into(),
            },
        );
        assert!(result.is_err());
        let case_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_question_version_review_cases_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(case_count, 0);
    }

    #[test]
    fn future_default_upgrade_clones_version_without_rebinding_history() {
        let mut fixture = fixture();
        let preview = preview_question_version_impact(
            &fixture.conn,
            "local_teacher",
            &fixture.question_version_public_id,
        )
        .unwrap();
        assert!(preview.rows[0].is_current_default);
        assert_eq!(preview.rows[0].assessment_version_revision, 1);
        let plan = confirm_question_impact_plan(
            &mut fixture.conn,
            "local_teacher",
            &ConfirmQuestionImpactPlanRequest {
                request_key: "future-default-plan".into(),
                question_version_public_id: fixture.question_version_public_id.clone(),
                expected_preview_hash: preview.preview_hash,
                action: "future_only".into(),
                planned_by: "local_teacher".into(),
            },
        )
        .unwrap();
        let before: (i64, i64, i64, i64, i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_attempts_v2),
                   (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                   (SELECT COUNT(*) FROM exam_grade_publications_v2),
                   (SELECT COUNT(*) FROM learning_evidence),
                   (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                   (SELECT COUNT(*) FROM exam_assessment_items_v2),
                   (SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2)",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        let request = UpgradeAssessmentDefaultRequest {
            request_key: "future-default-upgrade".into(),
            plan_public_id: plan.public_id,
            source_assessment_version_public_id: "assessment-version-1".into(),
            expected_current_default_version_public_id: "assessment-version-1".into(),
            upgraded_by: "local_teacher".into(),
        };
        let upgraded =
            upgrade_assessment_default_from_impact(&mut fixture.conn, "local_teacher", &request)
                .unwrap();
        assert_eq!(upgraded.source_revision, 1);
        assert_eq!(upgraded.default_revision, 2);
        assert_eq!(upgraded.upgraded_item_count, 1);
        assert!(upgraded.default_for_future_intake);
        assert!(!upgraded.changes_historical_attempts);
        assert!(!upgraded.changes_grade);
        assert!(!upgraded.changes_publication);
        assert!(!upgraded.changes_learning_evidence);
        let after: (i64, i64, i64, i64, i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_attempts_v2),
                   (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                   (SELECT COUNT(*) FROM exam_grade_publications_v2),
                   (SELECT COUNT(*) FROM learning_evidence),
                   (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                   (SELECT COUNT(*) FROM exam_assessment_items_v2),
                   (SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2)",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                        row.get(6)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(before.0, after.0);
        assert_eq!(before.1, after.1);
        assert_eq!(before.2, after.2);
        assert_eq!(before.3, after.3);
        assert_eq!(after.4, before.4 + 1);
        assert_eq!(after.5, before.5 + 1);
        assert_eq!(after.6, before.6 + 1);
        let history_versions: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_attempts_v2
                 WHERE assessment_version_id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(history_versions, 2);
        let default_binding: (String, i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT version.public_id,item.answer_key_version_id,
                        item.rubric_version_id,item.link_set_id
                 FROM exam_assessment_current_defaults_v2 current
                 JOIN exam_assessment_versions_v2 version
                   ON version.id=current.assessment_version_id
                 JOIN exam_assessment_items_v2 item
                   ON item.assessment_version_id=version.id
                 WHERE current.assessment_id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(
            default_binding.0,
            upgraded.default_assessment_version_public_id
        );
        let targets: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT target_answer_key_version_id,target_rubric_version_id,target_link_set_id
                 FROM exam_question_version_impact_plans_v2
                 WHERE public_id=?1",
                [&request.plan_public_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (default_binding.1, default_binding.2, default_binding.3),
            targets
        );
        let repeated =
            upgrade_assessment_default_from_impact(&mut fixture.conn, "local_teacher", &request)
                .unwrap();
        assert_eq!(repeated.selection_public_id, upgraded.selection_public_id);
        assert_eq!(repeated.default_revision, 2);
        assert!(fixture
            .conn
            .execute(
                "UPDATE exam_assessment_default_version_selections_v2
                 SET selected_by='other' WHERE public_id=?1",
                [&upgraded.selection_public_id],
            )
            .is_err());
        assert!(fixture
            .conn
            .execute(
                "DELETE FROM exam_assessment_default_version_selections_v2
                 WHERE public_id=?1",
                [&upgraded.selection_public_id],
            )
            .is_err());
    }

    #[test]
    fn future_default_upgrade_rejects_stale_or_unreviewed_source() {
        let mut fixture = fixture();
        let preview = preview_question_version_impact(
            &fixture.conn,
            "local_teacher",
            &fixture.question_version_public_id,
        )
        .unwrap();
        let plan = confirm_question_impact_plan(
            &mut fixture.conn,
            "local_teacher",
            &ConfirmQuestionImpactPlanRequest {
                request_key: "future-default-stale-plan".into(),
                question_version_public_id: fixture.question_version_public_id.clone(),
                expected_preview_hash: preview.preview_hash,
                action: "future_only".into(),
                planned_by: "local_teacher".into(),
            },
        )
        .unwrap();
        let first = UpgradeAssessmentDefaultRequest {
            request_key: "future-default-first".into(),
            plan_public_id: plan.public_id.clone(),
            source_assessment_version_public_id: "assessment-version-1".into(),
            expected_current_default_version_public_id: "assessment-version-1".into(),
            upgraded_by: "local_teacher".into(),
        };
        upgrade_assessment_default_from_impact(&mut fixture.conn, "local_teacher", &first).unwrap();
        let stale = UpgradeAssessmentDefaultRequest {
            request_key: "future-default-stale".into(),
            plan_public_id: plan.public_id,
            source_assessment_version_public_id: "assessment-version-1".into(),
            expected_current_default_version_public_id: "assessment-version-1".into(),
            upgraded_by: "local_teacher".into(),
        };
        assert!(
            upgrade_assessment_default_from_impact(&mut fixture.conn, "local_teacher", &stale)
                .is_err()
        );
        let counts: (i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                   (SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, (2, 1));
    }
}
