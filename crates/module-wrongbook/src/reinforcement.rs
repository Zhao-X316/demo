//! M3-3 老师确认的跨日期巩固复测。
//!
//! 预览只读；确认时重新核对当前发布事实、共享排程策略、假期和每日上限，
//! 再原子创建定向作业、core task 与不可变关联。

use chrono::{DateTime, Duration, NaiveDate, Utc};
use module_exam::service::assessment::{
    create_confirmed_single_item_targeted_in_transaction, NewSingleItemTargetedAssessment,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::tasks::{self, NewTask};
use suite_core::domain::{ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ModuleKey, TaskKind};
use suite_core::services::scheduling::{
    self, DueDatePlan, SchedulePolicy, SchedulePolicyUpdate, LEARNING_POLICY_KEY,
};

const STRATEGY: &str = "same_question_recheck";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReinforcementSuggestion {
    pub class_id: i64,
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub question_version_public_id: String,
    pub source_grade_decision_public_id: String,
    pub source_publication_public_id: String,
    pub strategy: String,
    pub priority: String,
    pub reason: String,
    pub policy_public_id: String,
    pub policy_revision: i64,
    pub previewed_as_of_date: String,
    pub corrected_on: String,
    pub earliest_due_date: String,
    pub suggested_due_date: String,
    pub shifted_days: i64,
    pub existing_task_count: i64,
    pub daily_limit_per_student: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReinforcementAssignment {
    pub public_id: String,
    pub class_id: i64,
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub question_version_public_id: String,
    pub source_grade_decision_public_id: String,
    pub source_publication_public_id: String,
    pub strategy: String,
    pub priority: String,
    pub policy_public_id: String,
    pub policy_revision: i64,
    pub due_date: String,
    pub assessment_public_id: String,
    pub assessment_version_public_id: String,
    pub assessment_title: String,
    pub task_id: i64,
    pub status: String,
    pub latest_attempt_public_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ReinforcementScopeInput<'a> {
    pub class_id: i64,
    pub student_id: i64,
    pub question_version_public_id: &'a str,
    pub source_grade_decision_public_id: &'a str,
    pub source_publication_public_id: &'a str,
}

#[derive(Debug, Clone)]
pub struct ConfirmReinforcementInput<'a> {
    pub scope: ReinforcementScopeInput<'a>,
    pub expected_policy_public_id: &'a str,
    pub expected_due_date: &'a str,
    pub previewed_as_of_date: &'a str,
    pub created_by: &'a str,
}

struct SourceScope {
    student_no: String,
    student_name: String,
    stem: String,
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    score: f64,
    option_order_json: Option<String>,
    presentation_snapshot_json: String,
    grade_decision_id: i64,
    publication_id: i64,
    corrected_at: String,
    error_response_count: i64,
}

fn business_date(value: &str) -> CoreResult<NaiveDate> {
    let parsed = DateTime::parse_from_rfc3339(value)
        .map_err(|_| CoreError::Parse("已发布时间不是有效 RFC3339".into()))?;
    Ok(time::shanghai_business_date_at(parsed.with_timezone(&Utc)))
}

fn resolve_source(
    conn: &Connection,
    input: &ReinforcementScopeInput<'_>,
) -> CoreResult<SourceScope> {
    let source = conn
        .query_row(
            "SELECT student.student_no,student.name,question.stem,question.id,
                    source_item.answer_key_version_id,source_item.rubric_version_id,
                    source_item.link_set_id,source_item.score,source_item.option_order_json,
                    source_item.presentation_snapshot_json,source_decision.id,
                    source_publication.id
             FROM exam_grade_decisions_v2 source_decision
             JOIN exam_attempts_v2 source_attempt
               ON source_attempt.id=source_decision.attempt_id
             JOIN students student
               ON student.id=source_attempt.student_id
              AND student.id=?2
              AND student.class_id=?1
              AND student.enabled=1
             JOIN exam_grade_publications_v2 source_publication
               ON source_publication.id=source_attempt.active_publication_id
              AND source_publication.public_id=?5
              AND source_publication.state='published'
             JOIN exam_grade_publication_items_v2 source_publication_item
               ON source_publication_item.publication_id=source_publication.id
              AND source_publication_item.attempt_id=source_attempt.id
             JOIN exam_grade_publication_decisions_v2 source_publication_decision
               ON source_publication_decision.publication_item_id=source_publication_item.id
              AND source_publication_decision.grade_decision_id=source_decision.id
             JOIN exam_assessment_items_v2 source_item
               ON source_item.id=source_decision.assessment_item_id
             JOIN k1_question_versions question
               ON question.id=source_item.question_version_id
              AND question.public_id=?3
             JOIN exam_assessment_versions_v2 source_version
               ON source_version.id=source_attempt.assessment_version_id
             JOIN exam_assessments_v2 source_assessment
               ON source_assessment.id=source_version.assessment_id
              AND source_assessment.class_id=?1
             WHERE source_decision.public_id=?4
               AND source_decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
               AND source_decision.teacher_score < source_item.score - 0.000001",
            (
                input.class_id,
                input.student_id,
                input.question_version_public_id,
                input.source_grade_decision_public_id,
                input.source_publication_public_id,
            ),
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, f64>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("该评分不是可巩固的已发布错误事实".into()))?;

    let latest = conn
        .query_row(
            "SELECT decision.teacher_score,item.score,attempt.attempt_kind,
                    publication.published_at
             FROM exam_grade_decisions_v2 decision
             JOIN exam_attempts_v2 attempt
               ON attempt.id=decision.attempt_id
              AND attempt.student_id=?1
             JOIN exam_grade_publications_v2 publication
               ON publication.id=attempt.active_publication_id
              AND publication.state='published'
             JOIN exam_grade_publication_items_v2 publication_item
               ON publication_item.publication_id=publication.id
              AND publication_item.attempt_id=attempt.id
             JOIN exam_grade_publication_decisions_v2 publication_decision
               ON publication_decision.publication_item_id=publication_item.id
              AND publication_decision.grade_decision_id=decision.id
             JOIN exam_assessment_items_v2 item
               ON item.id=decision.assessment_item_id
             JOIN k1_question_versions question
               ON question.id=item.question_version_id
              AND question.public_id=?2
             JOIN exam_assessment_versions_v2 version
               ON version.id=attempt.assessment_version_id
             JOIN exam_assessments_v2 assessment
               ON assessment.id=version.assessment_id
              AND assessment.class_id=?3
             WHERE decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
             ORDER BY publication.published_at DESC,decision.id DESC
             LIMIT 1",
            (
                input.student_id,
                input.question_version_public_id,
                input.class_id,
            ),
            |row| {
                Ok((
                    row.get::<_, f64>(0)?,
                    row.get::<_, f64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("未找到当前最新已发布作答".into()))?;
    if latest.2 != "correction" || latest.0 < latest.1 - 0.000001 {
        return Err(CoreError::Invalid(
            "只有完成一次满分订正后才能安排跨日期巩固".into(),
        ));
    }
    let error_response_count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM exam_grade_decisions_v2 decision
         JOIN exam_attempts_v2 attempt
           ON attempt.id=decision.attempt_id AND attempt.student_id=?1
         JOIN exam_grade_publications_v2 publication
           ON publication.id=attempt.active_publication_id AND publication.state='published'
         JOIN exam_grade_publication_items_v2 publication_item
           ON publication_item.publication_id=publication.id
          AND publication_item.attempt_id=attempt.id
         JOIN exam_grade_publication_decisions_v2 publication_decision
           ON publication_decision.publication_item_id=publication_item.id
          AND publication_decision.grade_decision_id=decision.id
         JOIN exam_assessment_items_v2 item
           ON item.id=decision.assessment_item_id
         JOIN k1_question_versions question
           ON question.id=item.question_version_id AND question.public_id=?2
         JOIN exam_assessment_versions_v2 version
           ON version.id=attempt.assessment_version_id
         JOIN exam_assessments_v2 assessment
           ON assessment.id=version.assessment_id AND assessment.class_id=?3
         WHERE decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
           AND decision.teacher_score < item.score - 0.000001",
        (
            input.student_id,
            input.question_version_public_id,
            input.class_id,
        ),
        |row| row.get(0),
    )?;

    Ok(SourceScope {
        student_no: source.0,
        student_name: source.1,
        stem: source.2,
        question_version_id: source.3,
        answer_key_version_id: source.4,
        rubric_version_id: source.5,
        link_set_id: source.6,
        score: source.7,
        option_order_json: source.8,
        presentation_snapshot_json: source.9,
        grade_decision_id: source.10,
        publication_id: source.11,
        corrected_at: latest.3,
        error_response_count,
    })
}

fn compact_title(student_no: &str, student_name: &str, stem: &str) -> String {
    let compact_stem = stem.chars().take(22).collect::<String>();
    let suffix = if stem.chars().count() > 22 { "…" } else { "" };
    format!("{student_no}号 {student_name} · {compact_stem}{suffix} · 巩固复测")
}

fn build_suggestion(
    conn: &Connection,
    input: &ReinforcementScopeInput<'_>,
    as_of_date: NaiveDate,
) -> CoreResult<(
    SourceScope,
    SchedulePolicy,
    DueDatePlan,
    ReinforcementSuggestion,
)> {
    if load_by_source_decision(conn, input.source_grade_decision_public_id)?.is_some() {
        return Err(CoreError::Invalid("该错误已经安排巩固复测".into()));
    }
    let source = resolve_source(conn, input)?;
    let policy = scheduling::load_active_policy(conn, LEARNING_POLICY_KEY)?;
    let corrected_on = business_date(&source.corrected_at)?;
    let earliest_due_date =
        (corrected_on + Duration::days(policy.default_delay_days)).max(as_of_date);
    let due = scheduling::plan_due_date(conn, input.student_id, earliest_due_date, &policy)?;
    let priority = if source.error_response_count > 1 {
        "high"
    } else {
        "normal"
    };
    let reason = if priority == "high" {
        format!(
            "这道题已出现 {} 次非满分；建议跨日期复测，但不会自动增加作业。",
            source.error_response_count
        )
    } else {
        "已完成一次订正；建议跨日期再次作答，验证是否保持。".to_string()
    };
    let suggestion = ReinforcementSuggestion {
        class_id: input.class_id,
        student_id: input.student_id,
        student_no: source.student_no.clone(),
        student_name: source.student_name.clone(),
        question_version_public_id: input.question_version_public_id.to_string(),
        source_grade_decision_public_id: input.source_grade_decision_public_id.to_string(),
        source_publication_public_id: input.source_publication_public_id.to_string(),
        strategy: STRATEGY.into(),
        priority: priority.into(),
        reason,
        policy_public_id: policy.public_id.clone(),
        policy_revision: policy.revision,
        previewed_as_of_date: as_of_date.format("%Y-%m-%d").to_string(),
        corrected_on: corrected_on.format("%Y-%m-%d").to_string(),
        earliest_due_date: due.earliest_due_date.clone(),
        suggested_due_date: due.suggested_due_date.clone(),
        shifted_days: due.shifted_days,
        existing_task_count: due.existing_task_count,
        daily_limit_per_student: due.daily_limit_per_student,
    };
    Ok((source, policy, due, suggestion))
}

pub fn preview_reinforcement_at(
    conn: &Connection,
    input: &ReinforcementScopeInput<'_>,
    as_of_date: NaiveDate,
) -> CoreResult<ReinforcementSuggestion> {
    Ok(build_suggestion(conn, input, as_of_date)?.3)
}

pub fn preview_reinforcement(
    conn: &Connection,
    input: &ReinforcementScopeInput<'_>,
) -> CoreResult<ReinforcementSuggestion> {
    preview_reinforcement_at(conn, input, time::shanghai_business_date_at(Utc::now()))
}

fn load_by_source_decision(
    conn: &Connection,
    source_grade_decision_public_id: &str,
) -> CoreResult<Option<ReinforcementAssignment>> {
    let row = conn
        .query_row(
            "SELECT assignment.public_id,assignment.class_id,assignment.student_id,
                    student.student_no,student.name,question.public_id,
                    source_decision.public_id,source_publication.public_id,
                    assignment.strategy,assignment.priority,policy.public_id,policy.revision,
                    assignment.due_date,assessment.public_id,version.public_id,
                    assessment.title,assignment.task_id,assignment.created_by,
                    assignment.created_at,version.id
             FROM wb_reinforcement_assignments assignment
             JOIN students student ON student.id=assignment.student_id
             JOIN k1_question_versions question ON question.id=assignment.question_version_id
             JOIN exam_grade_decisions_v2 source_decision
               ON source_decision.id=assignment.source_grade_decision_id
             JOIN exam_grade_publications_v2 source_publication
               ON source_publication.id=assignment.source_publication_id
             JOIN schedule_policy_versions policy
               ON policy.id=assignment.schedule_policy_version_id
             JOIN exam_assessments_v2 assessment ON assessment.id=assignment.assessment_id
             JOIN exam_assessment_versions_v2 version
               ON version.id=assignment.assessment_version_id
             WHERE source_decision.public_id=?1",
            [source_grade_decision_public_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, String>(15)?,
                    row.get::<_, i64>(16)?,
                    row.get::<_, String>(17)?,
                    row.get::<_, String>(18)?,
                    row.get::<_, i64>(19)?,
                ))
            },
        )
        .optional()?;
    let Some(row) = row else {
        return Ok(None);
    };
    let attempt = conn
        .query_row(
            "SELECT public_id,state FROM exam_attempts_v2
             WHERE assessment_version_id=?1 AND student_id=?2 AND state<>'voided'
             ORDER BY attempt_no DESC,id DESC LIMIT 1",
            (row.19, row.2),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let (status, latest_attempt_public_id) = match attempt {
        None => ("scheduled".to_string(), None),
        Some((public_id, state)) => {
            let status = match state.as_str() {
                "ready_to_publish" => "ready_to_publish",
                "published" => "published",
                _ => "in_progress",
            };
            (status.to_string(), Some(public_id))
        }
    };
    Ok(Some(ReinforcementAssignment {
        public_id: row.0,
        class_id: row.1,
        student_id: row.2,
        student_no: row.3,
        student_name: row.4,
        question_version_public_id: row.5,
        source_grade_decision_public_id: row.6,
        source_publication_public_id: row.7,
        strategy: row.8,
        priority: row.9,
        policy_public_id: row.10,
        policy_revision: row.11,
        due_date: row.12,
        assessment_public_id: row.13,
        assessment_version_public_id: row.14,
        assessment_title: row.15,
        task_id: row.16,
        status,
        latest_attempt_public_id,
        created_by: row.17,
        created_at: row.18,
    }))
}

pub fn load_for_source_decision(
    conn: &Connection,
    source_grade_decision_public_id: &str,
) -> CoreResult<Option<ReinforcementAssignment>> {
    load_by_source_decision(conn, source_grade_decision_public_id)
}

pub fn confirm_reinforcement_at(
    conn: &mut Connection,
    input: &ConfirmReinforcementInput<'_>,
    as_of_date: NaiveDate,
) -> CoreResult<ReinforcementAssignment> {
    if input.created_by.trim().is_empty() {
        return Err(CoreError::Invalid("巩固任务创建人不能为空".into()));
    }
    if let Some(existing) =
        load_by_source_decision(conn, input.scope.source_grade_decision_public_id)?
    {
        if existing.class_id == input.scope.class_id
            && existing.student_id == input.scope.student_id
            && existing.question_version_public_id == input.scope.question_version_public_id
            && existing.source_publication_public_id == input.scope.source_publication_public_id
        {
            return Ok(existing);
        }
        return Err(CoreError::Invalid(
            "既有巩固任务范围与当前请求不一致".into(),
        ));
    }
    if input.previewed_as_of_date != as_of_date.format("%Y-%m-%d").to_string() {
        return Err(CoreError::Invalid(
            "巩固建议已跨日，请刷新后重新确认".into(),
        ));
    }

    let transaction = conn.transaction()?;
    let (source, policy, due, suggestion) =
        build_suggestion(&transaction, &input.scope, as_of_date)?;
    if input.expected_policy_public_id != policy.public_id
        || input.expected_due_date != due.suggested_due_date
    {
        return Err(CoreError::Invalid(
            "排程规则或当天任务量已变化，请刷新建议后确认".into(),
        ));
    }
    let title = compact_title(&source.student_no, &source.student_name, &source.stem);
    let assessment = create_confirmed_single_item_targeted_in_transaction(
        &transaction,
        &NewSingleItemTargetedAssessment {
            title: &title,
            class_id: input.scope.class_id,
            target_student_id: input.scope.student_id,
            assessment_context: "homework",
            evidence_policy: "include_low_weight",
            template_version: "m3-reinforcement-recheck-v1",
            due_date: Some(&due.suggested_due_date),
            schedule_policy_version_id: Some(policy.id),
            created_by: input.created_by,
            question_version_id: source.question_version_id,
            answer_key_version_id: source.answer_key_version_id,
            rubric_version_id: source.rubric_version_id,
            link_set_id: source.link_set_id,
            score: source.score,
            option_order_json: source.option_order_json.as_deref(),
            presentation_snapshot_json: &source.presentation_snapshot_json,
        },
    )?;
    let task_id = tasks::insert(
        &transaction,
        &NewTask {
            module: ModuleKey::Wrongbook,
            student_id: input.scope.student_id,
            subject_id: None,
            ref_type: "assessment_version",
            ref_id: assessment.assessment_version_id,
            kind: TaskKind::Review,
            due_date: &due.suggested_due_date,
            source_task_id: None,
            card_id: None,
        },
    )?;
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    transaction.execute(
        "INSERT INTO wb_reinforcement_assignments
         (public_id,class_id,student_id,question_version_id,source_grade_decision_id,
          source_publication_id,strategy,priority,schedule_policy_version_id,due_date,
          assessment_id,assessment_version_id,task_id,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        (
            &public_id,
            input.scope.class_id,
            input.scope.student_id,
            source.question_version_id,
            source.grade_decision_id,
            source.publication_id,
            STRATEGY,
            suggestion.priority,
            policy.id,
            &due.suggested_due_date,
            assessment.assessment_id,
            assessment.assessment_version_id,
            task_id,
            input.created_by.trim(),
            &created_at,
        ),
    )?;
    let result =
        load_by_source_decision(&transaction, input.scope.source_grade_decision_public_id)?
            .ok_or_else(|| CoreError::NotFound("刚创建的巩固任务".into()))?;
    transaction.commit()?;
    Ok(result)
}

pub fn confirm_reinforcement(
    conn: &mut Connection,
    input: &ConfirmReinforcementInput<'_>,
) -> CoreResult<ReinforcementAssignment> {
    confirm_reinforcement_at(conn, input, time::shanghai_business_date_at(Utc::now()))
}

pub fn load_schedule_policy(conn: &Connection) -> CoreResult<SchedulePolicy> {
    scheduling::load_active_policy(conn, LEARNING_POLICY_KEY)
}

pub fn update_schedule_policy(
    conn: &mut Connection,
    input: &SchedulePolicyUpdate,
    created_by: &str,
) -> CoreResult<SchedulePolicy> {
    scheduling::replace_active_policy(conn, LEARNING_POLICY_KEY, input, created_by)
}
