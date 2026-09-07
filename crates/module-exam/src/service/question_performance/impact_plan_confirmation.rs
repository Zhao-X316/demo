use rusqlite::{params, Connection, OptionalExtension};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use super::{
    load_target, preview_question_version_impact, required, ConfirmQuestionImpactPlanRequest,
    QuestionImpactPlan, TargetIds, TaskScope, QUESTION_IMPACT_RULE_VERSION,
    QUESTION_PERFORMANCE_SCHEMA_VERSION,
};

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
