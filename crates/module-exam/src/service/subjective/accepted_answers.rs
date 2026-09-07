//! Promote a teacher-confirmed fill answer into a future answer-key version.
use super::{
    fill_answer_values, normalize_fill_value, required, string_values,
    AcceptedAnswerAssessmentHashItem, AcceptedAnswerPromotionResult, AssessmentItemClone,
};
use rusqlite::{Connection, OptionalExtension};
use serde_json::Value;
use suite_core::db::repo::audit;
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

#[derive(Debug)]
struct AcceptedAnswerSourceScope {
    grade_decision_id: i64,
    suggestion_id: i64,
    transcription_revision_id: i64,
    accepted_text: String,
    source_assessment_version_id: i64,
    source_assessment_item_id: i64,
    source_answer_key_version_id: i64,
    assessment_id: i64,
    question_version_id: i64,
}

#[derive(Debug)]
struct AcceptedAnswerBaseItem {
    id: i64,
    assessment_version_id: i64,
    answer_key_version_id: i64,
    answer_json: String,
}

#[derive(Debug)]
struct AcceptedAnswerSlot {
    stable_id: String,
    order_index: i64,
    canonical_answers_json: String,
    normalization_rules_json: Option<String>,
    max_score: f64,
}

fn append_unique_string(value: &mut Value, field: &str, text: &str) -> CoreResult<()> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| CoreError::Invalid("答案版本必须是对象".into()))?;
    let entries = object
        .entry(field)
        .or_insert_with(|| Value::Array(Vec::new()))
        .as_array_mut()
        .ok_or_else(|| CoreError::Invalid(format!("答案版本 {field} 必须是数组")))?;
    if !entries
        .iter()
        .filter_map(Value::as_str)
        .any(|value| normalize_fill_value(value) == normalize_fill_value(text))
    {
        entries.push(Value::String(text.to_owned()));
    }
    Ok(())
}

fn promotion_result_by_decision(
    conn: &Connection,
    grade_decision_id: i64,
    outcome: &str,
) -> CoreResult<Option<AcceptedAnswerPromotionResult>> {
    conn.query_row(
        "SELECT promotion.id,promotion.accepted_text,
                promotion.adopted_assessment_version_id,version.revision,
                promotion.adopted_answer_key_version_id
         FROM exam_accepted_answer_promotions_v2 promotion
         JOIN exam_assessment_versions_v2 version
           ON version.id=promotion.adopted_assessment_version_id
         WHERE promotion.grade_decision_id=?1",
        [grade_decision_id],
        |row| {
            Ok(AcceptedAnswerPromotionResult {
                outcome: outcome.to_owned(),
                promotion_id: row.get(0)?,
                accepted_text: row.get(1)?,
                adopted_assessment_version_id: row.get(2)?,
                adopted_assessment_revision: row.get(3)?,
                adopted_answer_key_version_id: row.get(4)?,
                current_grade_unchanged: true,
                current_publication_unchanged: true,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

fn promotion_result_by_identity(
    conn: &Connection,
    assessment_id: i64,
    question_version_id: i64,
    normalized_text: &str,
) -> CoreResult<Option<AcceptedAnswerPromotionResult>> {
    conn.query_row(
        "SELECT promotion.id,promotion.accepted_text,
                promotion.adopted_assessment_version_id,version.revision,
                promotion.adopted_answer_key_version_id
         FROM exam_accepted_answer_promotions_v2 promotion
         JOIN exam_assessment_versions_v2 version
           ON version.id=promotion.adopted_assessment_version_id
         WHERE promotion.assessment_id=?1 AND promotion.question_version_id=?2
           AND promotion.normalized_text=?3",
        (assessment_id, question_version_id, normalized_text),
        |row| {
            Ok(AcceptedAnswerPromotionResult {
                outcome: "already_promoted".into(),
                promotion_id: row.get(0)?,
                accepted_text: row.get(1)?,
                adopted_assessment_version_id: row.get(2)?,
                adopted_assessment_revision: row.get(3)?,
                adopted_answer_key_version_id: row.get(4)?,
                current_grade_unchanged: true,
                current_publication_unchanged: true,
            })
        },
    )
    .optional()
    .map_err(Into::into)
}

/// 将一次老师已判满分的填空写法加入未来答案版本。
///
/// 该操作只允许引用当前 active 的人工修正评分 revision；它会以当前作业的最新确认版本
/// 为基线，追加 K1 答案版本和 assessment version。当前 attempt、评分、发布和学习证据
/// 均不重新绑定，也不重新计算。
pub fn promote_fill_accepted_answer(
    conn: &mut Connection,
    grade_decision_id: i64,
    confirmed_by: &str,
) -> CoreResult<AcceptedAnswerPromotionResult> {
    required(confirmed_by, "答案版本确认人")?;
    if let Some(existing) =
        promotion_result_by_decision(conn, grade_decision_id, "already_promoted")?
    {
        return Ok(existing);
    }

    let source = conn
        .query_row(
            "SELECT decision.id,suggestion.id,transcription.id,
                    COALESCE(transcription.teacher_corrected_text,
                             transcription.normalized_text),
                    source_version.id,source_item.id,source_item.answer_key_version_id,
                    source_version.assessment_id,source_item.question_version_id
             FROM exam_grade_decisions_v2 decision
             JOIN exam_grade_decision_subjective_sources_v2 decision_source
               ON decision_source.grade_decision_id=decision.id
             JOIN exam_subjective_grade_suggestions_v2 suggestion
               ON suggestion.id=decision_source.suggestion_id AND suggestion.state='active'
             JOIN exam_subjective_transcription_revisions_v2 transcription
               ON transcription.id=decision_source.transcription_revision_id
              AND transcription.id=suggestion.transcription_revision_id
              AND transcription.state='active' AND transcription.result_state='recognized'
             JOIN exam_answer_region_revisions_v2 region
               ON region.id=transcription.answer_region_revision_id
              AND region.state='active' AND region.decision='teacher_confirmed'
             JOIN exam_attempts_v2 attempt
               ON attempt.id=decision.attempt_id AND attempt.state<>'voided'
             JOIN exam_assessment_items_v2 source_item
               ON source_item.id=decision.assessment_item_id
              AND source_item.assessment_version_id=attempt.assessment_version_id
              AND source_item.answer_key_version_id=suggestion.answer_key_version_id
              AND source_item.state='active'
             JOIN exam_assessment_versions_v2 source_version
               ON source_version.id=source_item.assessment_version_id
              AND source_version.state='confirmed'
             JOIN k1_question_versions question
               ON question.id=source_item.question_version_id
              AND question.question_type='fill_blank'
             WHERE decision.id=?1 AND decision.state='active'
               AND decision.confirmation_level='teacher_corrected'
               AND ABS(decision.teacher_score-source_item.score) <= 0.000001",
            [grade_decision_id],
            |row| {
                Ok(AcceptedAnswerSourceScope {
                    grade_decision_id: row.get(0)?,
                    suggestion_id: row.get(1)?,
                    transcription_revision_id: row.get(2)?,
                    accepted_text: row.get(3)?,
                    source_assessment_version_id: row.get(4)?,
                    source_assessment_item_id: row.get(5)?,
                    source_answer_key_version_id: row.get(6)?,
                    assessment_id: row.get(7)?,
                    question_version_id: row.get(8)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid(
                "只有老师查看原图后人工判满分的当前填空 revision 才能加入答案库".into(),
            )
        })?;
    let accepted_text = source.accepted_text.trim().to_owned();
    let normalized_text = normalize_fill_value(&accepted_text);
    if normalized_text.is_empty() {
        return Err(CoreError::Invalid("可接受写法不能为空".into()));
    }
    if let Some(existing) = promotion_result_by_identity(
        conn,
        source.assessment_id,
        source.question_version_id,
        &normalized_text,
    )? {
        return Ok(existing);
    }

    let (base_assessment_version_id, base_assessment_revision): (i64, i64) = conn.query_row(
        "SELECT id,revision FROM exam_assessment_versions_v2
             WHERE assessment_id=?1 AND state='confirmed'
             ORDER BY revision DESC,id DESC LIMIT 1",
        [source.assessment_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let base = conn
        .query_row(
            "SELECT item.id,item.assessment_version_id,item.answer_key_version_id,
                    answer.answer_json
             FROM exam_assessment_items_v2 item
             JOIN k1_answer_key_versions answer
               ON answer.id=item.answer_key_version_id AND answer.state='confirmed'
             WHERE item.assessment_version_id=?1 AND item.question_version_id=?2
               AND item.state='active'",
            (base_assessment_version_id, source.question_version_id),
            |row| {
                Ok(AcceptedAnswerBaseItem {
                    id: row.get(0)?,
                    assessment_version_id: row.get(1)?,
                    answer_key_version_id: row.get(2)?,
                    answer_json: row.get(3)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid("最新作业版本已不包含该填空题，请先在题库中人工维护".into())
        })?;
    let mut slot_stmt = conn.prepare(
        "SELECT stable_id,order_index,canonical_answers_json,
                normalization_rules_json,max_score
         FROM k1_answer_slots WHERE answer_key_version_id=?1
         ORDER BY order_index,id",
    )?;
    let mut slots = slot_stmt
        .query_map([base.answer_key_version_id], |row| {
            Ok(AcceptedAnswerSlot {
                stable_id: row.get(0)?,
                order_index: row.get(1)?,
                canonical_answers_json: row.get(2)?,
                normalization_rules_json: row.get(3)?,
                max_score: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(slot_stmt);
    if slots.len() != 1 {
        return Err(CoreError::Invalid(
            "当前只允许把单槽位填空写法加入答案库；多槽位题请在题库中逐槽维护".into(),
        ));
    }

    let mut answer_json: Value = serde_json::from_str(&base.answer_json)
        .map_err(|error| CoreError::Parse(format!("当前填空答案 JSON 损坏：{error}")))?;
    let mut slot_json: Value = serde_json::from_str(&slots[0].canonical_answers_json)
        .map_err(|error| CoreError::Parse(format!("当前填空槽位答案 JSON 损坏：{error}")))?;
    let mut existing_values = fill_answer_values(&answer_json);
    existing_values.extend(string_values(slot_json.get("answers")));
    existing_values.extend(string_values(slot_json.get("accepted_variants")));
    if existing_values
        .iter()
        .any(|value| normalize_fill_value(value) == normalized_text)
    {
        return Ok(AcceptedAnswerPromotionResult {
            outcome: "already_available".into(),
            promotion_id: None,
            accepted_text,
            adopted_assessment_version_id: base_assessment_version_id,
            adopted_assessment_revision: base_assessment_revision,
            adopted_answer_key_version_id: base.answer_key_version_id,
            current_grade_unchanged: true,
            current_publication_unchanged: true,
        });
    }
    append_unique_string(&mut answer_json, "accepted_variants", &accepted_text)?;
    if let Some(answer_slots) = answer_json.get_mut("slots").and_then(Value::as_array_mut) {
        if answer_slots.len() != 1 {
            return Err(CoreError::Invalid(
                "答案 JSON 含多个槽位，请在题库中逐槽维护可接受写法".into(),
            ));
        }
        append_unique_string(&mut answer_slots[0], "accepted_variants", &accepted_text)?;
    }
    append_unique_string(&mut slot_json, "accepted_variants", &accepted_text)?;
    slots[0].canonical_answers_json = slot_json.to_string();

    let now = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    let answer_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_answer_key_versions
         WHERE question_version_id=?1",
        [source.question_version_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO k1_answer_key_versions
         (public_id,question_version_id,revision,answer_json,state,
          supersedes_answer_key_id,created_at,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,'confirmed',?5,?6,?7,?6)",
        rusqlite::params![
            ids::new_public_id(),
            source.question_version_id,
            answer_revision,
            answer_json.to_string(),
            base.answer_key_version_id,
            &now,
            confirmed_by.trim(),
        ],
    )?;
    let adopted_answer_key_version_id = tx.last_insert_rowid();
    for slot in &slots {
        tx.execute(
            "INSERT INTO k1_answer_slots
             (public_id,stable_id,answer_key_version_id,order_index,
              canonical_answers_json,normalization_rules_json,max_score,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            rusqlite::params![
                ids::new_public_id(),
                &slot.stable_id,
                adopted_answer_key_version_id,
                slot.order_index,
                &slot.canonical_answers_json,
                slot.normalization_rules_json.as_deref(),
                slot.max_score,
                &now,
            ],
        )?;
    }

    let adopted_assessment_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_assessment_versions_v2
         WHERE assessment_id=?1",
        [source.assessment_id],
        |row| row.get(0),
    )?;
    let template_version: Option<String> = tx.query_row(
        "SELECT template_version FROM exam_assessment_versions_v2 WHERE id=?1",
        [base.assessment_version_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO exam_assessment_versions_v2
         (public_id,assessment_id,revision,template_version,state,
          supersedes_version_id,created_at)
         VALUES (?1,?2,?3,?4,'draft',?5,?6)",
        rusqlite::params![
            ids::new_public_id(),
            source.assessment_id,
            adopted_assessment_revision,
            template_version.as_deref(),
            base.assessment_version_id,
            &now,
        ],
    )?;
    let adopted_assessment_version_id = tx.last_insert_rowid();
    let mut clone_stmt = tx.prepare(
        "SELECT question_version_id,answer_key_version_id,rubric_version_id,link_set_id,
                order_index,score,option_order_json,presentation_snapshot_json
         FROM exam_assessment_items_v2
         WHERE assessment_version_id=?1 AND state='active'
         ORDER BY order_index,id",
    )?;
    let clone_items = clone_stmt
        .query_map([base.assessment_version_id], |row| {
            Ok(AssessmentItemClone {
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
    drop(clone_stmt);
    let mut hash_items = Vec::with_capacity(clone_items.len());
    let mut adopted_assessment_item_id = None;
    for item in clone_items {
        let answer_key_version_id = if item.question_version_id == source.question_version_id {
            adopted_answer_key_version_id
        } else {
            item.answer_key_version_id
        };
        tx.execute(
            "INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,option_order_json,
              presentation_snapshot_json,state,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11)",
            rusqlite::params![
                ids::new_public_id(),
                adopted_assessment_version_id,
                item.question_version_id,
                answer_key_version_id,
                item.rubric_version_id,
                item.link_set_id,
                item.order_index,
                item.score,
                item.option_order_json.as_deref(),
                &item.presentation_snapshot_json,
                &now,
            ],
        )?;
        let inserted_item_id = tx.last_insert_rowid();
        if item.question_version_id == source.question_version_id {
            adopted_assessment_item_id = Some(inserted_item_id);
        }
        hash_items.push(AcceptedAnswerAssessmentHashItem {
            item_id: inserted_item_id,
            question_version_id: item.question_version_id,
            answer_key_version_id,
            rubric_version_id: item.rubric_version_id,
            link_set_id: item.link_set_id,
            order_index: item.order_index,
            score_millis: (item.score * 1000.0).round() as i64,
        });
    }
    let adopted_assessment_item_id = adopted_assessment_item_id
        .ok_or_else(|| CoreError::Invalid("新作业版本未复制目标填空题，已回滚".into()))?;
    let item_set_hash = hashing::sha256_hex(
        &serde_json::to_vec(&hash_items)
            .map_err(|error| CoreError::Parse(format!("新作业版本 hash 失败：{error}")))?,
    );
    tx.execute(
        "UPDATE exam_assessment_versions_v2
         SET item_set_hash=?1,state='confirmed',confirmed_by=?2,confirmed_at=?3
         WHERE id=?4 AND state='draft'",
        (
            &item_set_hash,
            confirmed_by.trim(),
            &now,
            adopted_assessment_version_id,
        ),
    )?;
    tx.execute(
        "UPDATE exam_assessments_v2 SET updated_at=?1 WHERE id=?2",
        (&now, source.assessment_id),
    )?;

    let promotion_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO exam_accepted_answer_promotions_v2
         (public_id,grade_decision_id,suggestion_id,transcription_revision_id,
          assessment_id,question_version_id,source_assessment_version_id,
          source_assessment_item_id,source_answer_key_version_id,
          base_assessment_version_id,base_assessment_item_id,base_answer_key_version_id,
          adopted_assessment_version_id,adopted_assessment_item_id,
          adopted_answer_key_version_id,answer_slot_stable_id,accepted_text,
          normalized_text,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        rusqlite::params![
            &promotion_public_id,
            source.grade_decision_id,
            source.suggestion_id,
            source.transcription_revision_id,
            source.assessment_id,
            source.question_version_id,
            source.source_assessment_version_id,
            source.source_assessment_item_id,
            source.source_answer_key_version_id,
            base.assessment_version_id,
            base.id,
            base.answer_key_version_id,
            adopted_assessment_version_id,
            adopted_assessment_item_id,
            adopted_answer_key_version_id,
            &slots[0].stable_id,
            &accepted_text,
            &normalized_text,
            confirmed_by.trim(),
            &now,
        ],
    )?;
    let promotion_id = tx.last_insert_rowid();
    let audit_meta = serde_json::json!({
        "schema_version": 1,
        "grade_decision_id": source.grade_decision_id,
        "source_assessment_version_id": source.source_assessment_version_id,
        "base_assessment_version_id": base.assessment_version_id,
        "adopted_assessment_version_id": adopted_assessment_version_id,
        "adopted_answer_key_version_id": adopted_answer_key_version_id,
        "current_grade_unchanged": true,
        "current_publication_unchanged": true,
    })
    .to_string();
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!("exam:accepted-answer-promotion:{promotion_public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "exam.accepted_answer.promoted",
            object_type: "exam_accepted_answer_promotion",
            object_id: &promotion_public_id,
            object_revision: Some(adopted_assessment_revision),
            note: None,
            meta_json: Some(&audit_meta),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    Ok(AcceptedAnswerPromotionResult {
        outcome: "created_new_version".into(),
        promotion_id: Some(promotion_id),
        accepted_text,
        adopted_assessment_version_id,
        adopted_assessment_revision,
        adopted_answer_key_version_id,
        current_grade_unchanged: true,
        current_publication_unchanged: true,
    })
}
