//! K1-5 题目实际表现与版本变更影响预览。
//!
//! 表现统计只读取 attempt 当前有效 publication 中老师确认的 grade decision，
//! 不读取 legacy 聚合，也不把单班表现写回预计难度。版本影响确认只冻结计划和
//! 复核清单；不得直接修改 assessment item、成绩、发布、学习证据或图谱快照。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

pub const QUESTION_PERFORMANCE_SCHEMA_VERSION: i64 = 1;
pub const QUESTION_PERFORMANCE_RULE_VERSION: &str = "k1-current-publication-performance-v1";
pub const QUESTION_IMPACT_RULE_VERSION: &str = "k1-version-impact-plan-v1";
pub const QUESTION_IMPACT_REVIEW_RULE_VERSION: &str = "k1-version-impact-review-case-v1";

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
    pub question_stem: String,
    pub question_no: i64,
    pub attempt_public_id: String,
    pub attempt_state: String,
    pub publication_public_id: Option<String>,
    pub source_grade_decision_public_id: Option<String>,
    pub source_grade_decision_revision: Option<i64>,
    pub source_teacher_score: Option<f64>,
    pub source_snapshot_hash: String,
    pub target_answer_key_version_public_id: String,
    pub target_answer_key_revision: i64,
    pub target_rubric_version_public_id: String,
    pub target_rubric_revision: i64,
    pub target_link_set_public_id: String,
    pub target_link_set_revision: i64,
    pub prepared_by: String,
    pub prepared_at: String,
    pub state: String,
    pub next_step_note: String,
    pub changes_assessment_binding: bool,
    pub changes_grade: bool,
    pub changes_publication: bool,
    pub changes_learning_evidence: bool,
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
                assessment_version.public_id,item.public_id,
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
                assessment_item_public_id: row.get(4)?,
                source_answer_key_version_public_id: row.get(5)?,
                source_rubric_version_public_id: row.get(6)?,
                source_link_set_public_id: row.get(7)?,
                answer_changed: row.get(8)?,
                rubric_changed: row.get(9)?,
                link_changed: row.get(10)?,
                unpublished_attempt_count: row.get(11)?,
                published_attempt_count: row.get(12)?,
                active_learning_evidence_count: row.get(13)?,
                profile_snapshot_count: row.get(14)?,
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
                question.public_id,question.stem,item.order_index+1,
                attempt.public_id,attempt.state,publication.public_id,decision.public_id,
                review_case.source_grade_decision_revision,review_case.source_teacher_score,
                review_case.source_snapshot_hash,
                target_answer.public_id,target_answer.revision,
                target_rubric.public_id,target_rubric.revision,
                target_link.public_id,target_link.revision,
                review_case.prepared_by,review_case.prepared_at,review_case.state
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
         WHERE plan.public_id=?1 AND plan.planned_by=?2
         ORDER BY assessment.title,student.student_no,student.id,item.order_index,review_case.id",
    )?;
    let rows = statement.query_map(
        params![plan_public_id.trim(), owner_id.trim()],
        |row| {
            let case_kind: String = row.get(3)?;
            let next_step_note = if case_kind == "unpublished_recalculation" {
                "下一步由老师对照新旧评分证据创建新的评分 revision；当前未发布评分保持不变。"
            } else {
                "下一步由老师核对正式发布证据，再决定是否创建复核评分与新的发布 revision；当前正式成绩保持不变。"
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
                question_stem: row.get(9)?,
                question_no: row.get(10)?,
                attempt_public_id: row.get(11)?,
                attempt_state: row.get(12)?,
                publication_public_id: row.get(13)?,
                source_grade_decision_public_id: row.get(14)?,
                source_grade_decision_revision: row.get(15)?,
                source_teacher_score: row.get(16)?,
                source_snapshot_hash: row.get(17)?,
                target_answer_key_version_public_id: row.get(18)?,
                target_answer_key_revision: row.get(19)?,
                target_rubric_version_public_id: row.get(20)?,
                target_rubric_revision: row.get(21)?,
                target_link_set_public_id: row.get(22)?,
                target_link_set_revision: row.get(23)?,
                prepared_by: row.get(24)?,
                prepared_at: row.get(25)?,
                state: row.get(26)?,
                next_step_note: next_step_note.into(),
                changes_assessment_binding: false,
                changes_grade: false,
                changes_publication: false,
                changes_learning_evidence: false,
            })
        },
    )?;
    Ok(QuestionImpactReviewCaseCatalog {
        schema_version: QUESTION_PERFORMANCE_SCHEMA_VERSION,
        rule_version: QUESTION_IMPACT_REVIEW_RULE_VERSION.into(),
        plan_public_id: plan_public_id.trim().into(),
        cases: rows.collect::<rusqlite::Result<Vec<_>>>()?,
        boundary_note:
            "待处理 case 只冻结旧评分/发布证据与目标版本；建立 case 不会改作业绑定、成绩、发布结果或学习证据。"
                .into(),
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

    fn fixture() -> Fixture {
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
             VALUES (?1,?2,1,'single','鸦片战争爆发于哪一年？',2,
                     'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                     'L3','published',?3)",
            params![&version_public_id, question.id, NOW],
        )
        .unwrap();
        let version = QuestionVersion {
            id: conn.last_insert_rowid(),
            public_id: version_public_id,
            question_id: question.id,
            revision: 1,
            question_type: "single".into(),
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
}
