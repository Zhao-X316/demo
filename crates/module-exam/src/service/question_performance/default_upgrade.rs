use super::{
    required, AssessmentDefaultUpgrade, UpgradeAssessmentDefaultRequest,
    ASSESSMENT_DEFAULT_UPGRADE_RULE_VERSION, QUESTION_PERFORMANCE_SCHEMA_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

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
