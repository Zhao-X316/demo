use rusqlite::{params, Connection, OptionalExtension};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use super::{
    list_question_impact_review_cases, required, PrepareQuestionImpactReviewCasesRequest,
    PrepareQuestionImpactReviewCasesResult, ReviewTaskSnapshot,
    QUESTION_IMPACT_REVIEW_RULE_VERSION, QUESTION_PERFORMANCE_SCHEMA_VERSION,
};

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
