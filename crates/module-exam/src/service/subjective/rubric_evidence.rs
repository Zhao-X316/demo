//! Promote teacher-confirmed evidence into a future rubric version.
use super::{
    json_string_array, required, AcceptedAnswerAssessmentHashItem, AssessmentItemClone,
    RubricEvidencePromotionResult,
};
use rusqlite::{Connection, OptionalExtension};
use suite_core::db::repo::audit;
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

#[derive(Debug)]
struct RubricEvidenceSourceScope {
    grade_decision_id: i64,
    component_id: i64,
    suggestion_id: i64,
    transcription_revision_id: i64,
    evidence_text: String,
    rubric_point_stable_id: String,
    source_assessment_version_id: i64,
    source_assessment_item_id: i64,
    source_rubric_version_id: i64,
    assessment_id: i64,
    question_version_id: i64,
}

#[derive(Debug)]
struct RubricEvidenceBaseItem {
    id: i64,
    assessment_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
}

#[derive(Debug)]
struct RubricPointClone {
    id: i64,
    public_id: String,
    stable_id: String,
    order_index: i64,
    canonical_text: String,
    allowed_paraphrases_json: Option<String>,
    required_concepts_json: Option<String>,
    max_score: f64,
}

fn normalize_rubric_evidence(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn rubric_evidence_promotion_result(
    conn: &Connection,
    predicate: &str,
    values: &[&dyn rusqlite::ToSql],
    outcome: &str,
) -> CoreResult<Option<RubricEvidencePromotionResult>> {
    let sql = format!(
        "SELECT promotion.id,promotion.rubric_point_stable_id,promotion.evidence_text,
                promotion.adopted_assessment_version_id,version.revision,
                promotion.adopted_rubric_version_id,promotion.adopted_link_set_id,
                (SELECT COUNT(*) FROM k1_knowledge_links
                 WHERE link_set_id=promotion.adopted_link_set_id),
                (SELECT COUNT(*) FROM k1_ability_links
                 WHERE link_set_id=promotion.adopted_link_set_id)
         FROM exam_rubric_evidence_promotions_v2 promotion
         JOIN exam_assessment_versions_v2 version
           ON version.id=promotion.adopted_assessment_version_id
         WHERE {predicate}"
    );
    conn.query_row(&sql, values, |row| {
        Ok(RubricEvidencePromotionResult {
            outcome: outcome.into(),
            promotion_id: row.get(0)?,
            rubric_point_stable_id: row.get(1)?,
            evidence_text: row.get(2)?,
            adopted_assessment_version_id: row.get(3)?,
            adopted_assessment_revision: row.get(4)?,
            adopted_rubric_version_id: row.get(5)?,
            adopted_link_set_id: row.get(6)?,
            carried_knowledge_link_count: row.get(7)?,
            carried_ability_link_count: row.get(8)?,
            current_grade_unchanged: true,
            current_publication_unchanged: true,
        })
    })
    .optional()
    .map_err(Into::into)
}

/// 将老师逐点评分时引用的一条学生原文，加入未来简答题评分点的允许改述。
///
/// 只有当前 active、老师修正且已逐评分点给分的 revision 可以触发。服务会从最新确认
/// 作业版本继续追加 rubric、link set 和 assessment version；当前学生成绩、历史发布和
/// learning evidence 均不改写。
pub fn promote_short_answer_rubric_evidence(
    conn: &mut Connection,
    grade_decision_id: i64,
    source_public_id: &str,
    confirmed_by: &str,
) -> CoreResult<RubricEvidencePromotionResult> {
    required(source_public_id, "评分点")?;
    required(confirmed_by, "评分规则确认人")?;

    if let Some(existing) = rubric_evidence_promotion_result(
        conn,
        "promotion.grade_decision_id=?1
         AND EXISTS (
           SELECT 1 FROM exam_grade_decision_subjective_components_v2 component
           WHERE component.id=promotion.component_id
             AND component.source_public_id=?2
         )",
        &[&grade_decision_id, &source_public_id.trim()],
        "already_promoted",
    )? {
        return Ok(existing);
    }

    let source = conn
        .query_row(
            "SELECT decision.id,component.id,suggestion.id,transcription.id,
                    component.evidence_text,component.stable_id,
                    source_version.id,source_item.id,source_item.rubric_version_id,
                    source_version.assessment_id,source_item.question_version_id
             FROM exam_grade_decisions_v2 decision
             JOIN exam_grade_decision_subjective_sources_v2 decision_source
               ON decision_source.grade_decision_id=decision.id
             JOIN exam_grade_decision_subjective_components_v2 component
               ON component.grade_decision_id=decision.id
              AND component.source_type='rubric_point'
              AND component.source_public_id=?2
              AND component.teacher_score > 0.000001
              AND component.evidence_text IS NOT NULL
             JOIN exam_subjective_grade_suggestions_v2 suggestion
               ON suggestion.id=decision_source.suggestion_id AND suggestion.state='active'
             JOIN exam_subjective_transcription_revisions_v2 transcription
               ON transcription.id=decision_source.transcription_revision_id
              AND transcription.id=suggestion.transcription_revision_id
              AND transcription.state='active' AND transcription.result_state='recognized'
             JOIN exam_attempts_v2 attempt
               ON attempt.id=decision.attempt_id AND attempt.state<>'voided'
             JOIN exam_assessment_items_v2 source_item
               ON source_item.id=decision.assessment_item_id
              AND source_item.assessment_version_id=attempt.assessment_version_id
              AND source_item.rubric_version_id=suggestion.rubric_version_id
              AND source_item.state='active'
             JOIN exam_assessment_versions_v2 source_version
               ON source_version.id=source_item.assessment_version_id
              AND source_version.state='confirmed'
             JOIN k1_question_versions question
               ON question.id=source_item.question_version_id
              AND question.question_type='short_answer'
             WHERE decision.id=?1 AND decision.state='active'
               AND decision.confirmation_level='teacher_corrected'",
            (grade_decision_id, source_public_id.trim()),
            |row| {
                Ok(RubricEvidenceSourceScope {
                    grade_decision_id: row.get(0)?,
                    component_id: row.get(1)?,
                    suggestion_id: row.get(2)?,
                    transcription_revision_id: row.get(3)?,
                    evidence_text: row.get(4)?,
                    rubric_point_stable_id: row.get(5)?,
                    source_assessment_version_id: row.get(6)?,
                    source_assessment_item_id: row.get(7)?,
                    source_rubric_version_id: row.get(8)?,
                    assessment_id: row.get(9)?,
                    question_version_id: row.get(10)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid(
                "只有老师查看原图、逐评分点给分并引用学生原文的当前简答题 revision 才能更新评分规则"
                    .into(),
            )
        })?;
    let evidence_text = source.evidence_text.trim().to_owned();
    let normalized_text = normalize_rubric_evidence(&evidence_text);
    if normalized_text.is_empty() {
        return Err(CoreError::Invalid("评分点证据不能为空".into()));
    }
    if let Some(existing) = rubric_evidence_promotion_result(
        conn,
        "promotion.assessment_id=?1 AND promotion.question_version_id=?2
         AND promotion.rubric_point_stable_id=?3 AND promotion.normalized_text=?4",
        &[
            &source.assessment_id,
            &source.question_version_id,
            &source.rubric_point_stable_id,
            &normalized_text,
        ],
        "already_promoted",
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
            "SELECT item.id,item.assessment_version_id,item.rubric_version_id,item.link_set_id
             FROM exam_assessment_items_v2 item
             JOIN k1_rubric_versions rubric
               ON rubric.id=item.rubric_version_id AND rubric.state='confirmed'
             JOIN k1_link_sets links
               ON links.id=item.link_set_id AND links.state='confirmed'
             WHERE item.assessment_version_id=?1 AND item.question_version_id=?2
               AND item.state='active'",
            (base_assessment_version_id, source.question_version_id),
            |row| {
                Ok(RubricEvidenceBaseItem {
                    id: row.get(0)?,
                    assessment_version_id: row.get(1)?,
                    rubric_version_id: row.get(2)?,
                    link_set_id: row.get(3)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| {
            CoreError::Invalid("最新作业版本已不包含该简答题，请先在题库中人工维护".into())
        })?;

    let mut point_stmt = conn.prepare(
        "SELECT id,public_id,stable_id,order_index,canonical_text,
                allowed_paraphrases_json,required_concepts_json,max_score
         FROM k1_rubric_points WHERE rubric_version_id=?1
         ORDER BY order_index,id",
    )?;
    let mut points = point_stmt
        .query_map([base.rubric_version_id], |row| {
            Ok(RubricPointClone {
                id: row.get(0)?,
                public_id: row.get(1)?,
                stable_id: row.get(2)?,
                order_index: row.get(3)?,
                canonical_text: row.get(4)?,
                allowed_paraphrases_json: row.get(5)?,
                required_concepts_json: row.get(6)?,
                max_score: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(point_stmt);
    let target_point = points
        .iter_mut()
        .find(|point| point.stable_id == source.rubric_point_stable_id)
        .ok_or_else(|| {
            CoreError::Invalid("最新评分规则已不存在该评分点，请老师在题库中人工维护".into())
        })?;
    let mut allowed = json_string_array(target_point.allowed_paraphrases_json.clone(), "允许改述")?;
    let already_available = normalize_rubric_evidence(&target_point.canonical_text)
        == normalized_text
        || allowed
            .iter()
            .any(|value| normalize_rubric_evidence(value) == normalized_text);
    if already_available {
        return Ok(RubricEvidencePromotionResult {
            outcome: "already_available".into(),
            promotion_id: None,
            rubric_point_stable_id: source.rubric_point_stable_id,
            evidence_text,
            adopted_assessment_version_id: base.assessment_version_id,
            adopted_assessment_revision: base_assessment_revision,
            adopted_rubric_version_id: base.rubric_version_id,
            adopted_link_set_id: base.link_set_id,
            carried_knowledge_link_count: 0,
            carried_ability_link_count: 0,
            current_grade_unchanged: true,
            current_publication_unchanged: true,
        });
    }
    allowed.push(evidence_text.clone());
    target_point.allowed_paraphrases_json = Some(
        serde_json::to_string(&allowed)
            .map_err(|error| CoreError::Parse(format!("评分点允许改述序列化失败：{error}")))?,
    );

    let now = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    let rubric_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_rubric_versions
         WHERE question_version_id=?1",
        [source.question_version_id],
        |row| row.get(0),
    )?;
    let rubric_max_score: f64 = points.iter().map(|point| point.max_score).sum();
    tx.execute(
        "INSERT INTO k1_rubric_versions
         (public_id,question_version_id,revision,max_score,state,supersedes_rubric_id,
          created_at,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,'confirmed',?5,?6,?7,?6)",
        rusqlite::params![
            ids::new_public_id(),
            source.question_version_id,
            rubric_revision,
            rubric_max_score,
            base.rubric_version_id,
            &now,
            confirmed_by.trim(),
        ],
    )?;
    let adopted_rubric_version_id = tx.last_insert_rowid();
    let mut point_id_map = std::collections::BTreeMap::new();
    let mut point_public_id_map = std::collections::BTreeMap::new();
    for point in &points {
        let public_id = ids::new_public_id();
        tx.execute(
            "INSERT INTO k1_rubric_points
             (public_id,stable_id,rubric_version_id,order_index,canonical_text,
              allowed_paraphrases_json,required_concepts_json,max_score,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            rusqlite::params![
                &public_id,
                &point.stable_id,
                adopted_rubric_version_id,
                point.order_index,
                &point.canonical_text,
                point.allowed_paraphrases_json.as_deref(),
                point.required_concepts_json.as_deref(),
                point.max_score,
                &now,
            ],
        )?;
        point_id_map.insert(point.id, tx.last_insert_rowid());
        point_public_id_map.insert(point.public_id.clone(), public_id);
    }
    let mut rule_stmt = tx.prepare(
        "SELECT rubric_point_id,rule_type,rule_json
         FROM k1_contradiction_rules
         WHERE rubric_point_id IN (
           SELECT id FROM k1_rubric_points WHERE rubric_version_id=?1
         ) ORDER BY id",
    )?;
    let rules = rule_stmt
        .query_map([base.rubric_version_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(rule_stmt);
    for (old_point_id, rule_type, rule_json) in rules {
        let new_point_id = point_id_map
            .get(&old_point_id)
            .copied()
            .ok_or_else(|| CoreError::Invalid("评分点矛盾规则无法对应新版本".into()))?;
        tx.execute(
            "INSERT INTO k1_contradiction_rules
             (public_id,rubric_point_id,rule_type,rule_json,created_at)
             VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![
                ids::new_public_id(),
                new_point_id,
                &rule_type,
                &rule_json,
                &now,
            ],
        )?;
    }

    let (knowledge_map_id, base_link_state): (i64, String) = tx.query_row(
        "SELECT knowledge_map_id,state FROM k1_link_sets WHERE id=?1",
        [base.link_set_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if base_link_state != "confirmed" {
        return Err(CoreError::Invalid(
            "当前知识链接集尚未确认，不能沿用".into(),
        ));
    }
    let link_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_link_sets
         WHERE question_version_id=?1",
        [source.question_version_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO k1_link_sets
         (public_id,question_version_id,knowledge_map_id,revision,state,
          supersedes_link_set_id,created_at,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,'confirmed',?5,?6,?7,?6)",
        rusqlite::params![
            ids::new_public_id(),
            source.question_version_id,
            knowledge_map_id,
            link_revision,
            base.link_set_id,
            &now,
            confirmed_by.trim(),
        ],
    )?;
    let adopted_link_set_id = tx.last_insert_rowid();
    let mut carried_knowledge_link_count = 0;
    let mut knowledge_stmt = tx.prepare(
        "SELECT source_type,source_public_id,knowledge_node_id,relation_type,
                confirmation_level
         FROM k1_knowledge_links WHERE link_set_id=?1 ORDER BY id",
    )?;
    let knowledge_rows = knowledge_stmt
        .query_map([base.link_set_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(knowledge_stmt);
    for (source_type, old_source_public_id, node_id, relation_type, level) in knowledge_rows {
        let source_public_id = if source_type == "rubric_point" {
            point_public_id_map
                .get(&old_source_public_id)
                .cloned()
                .ok_or_else(|| CoreError::Invalid("知识链接无法对应新评分点".into()))?
        } else {
            old_source_public_id
        };
        let teacher_confirmed = level == "teacher_confirmed";
        tx.execute(
            "INSERT INTO k1_knowledge_links
             (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
              relation_type,confirmation_level,verified_by,verified_at,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            rusqlite::params![
                ids::new_public_id(),
                adopted_link_set_id,
                &source_type,
                &source_public_id,
                node_id,
                &relation_type,
                &level,
                teacher_confirmed.then_some(confirmed_by.trim()),
                teacher_confirmed.then_some(now.as_str()),
                &now,
            ],
        )?;
        carried_knowledge_link_count += 1;
    }
    let mut carried_ability_link_count = 0;
    let mut ability_stmt = tx.prepare(
        "SELECT source_type,source_public_id,ability_dimension_id,evidence_strength,
                response_mode,confirmation_level
         FROM k1_ability_links WHERE link_set_id=?1 ORDER BY id",
    )?;
    let ability_rows = ability_stmt
        .query_map([base.link_set_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(ability_stmt);
    for (source_type, old_source_public_id, dimension_id, strength, mode, level) in ability_rows {
        let source_public_id = if source_type == "rubric_point" {
            point_public_id_map
                .get(&old_source_public_id)
                .cloned()
                .ok_or_else(|| CoreError::Invalid("能力链接无法对应新评分点".into()))?
        } else {
            old_source_public_id
        };
        let teacher_confirmed = level == "teacher_confirmed";
        tx.execute(
            "INSERT INTO k1_ability_links
             (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
              evidence_strength,response_mode,confirmation_level,verified_by,verified_at,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            rusqlite::params![
                ids::new_public_id(),
                adopted_link_set_id,
                &source_type,
                &source_public_id,
                dimension_id,
                strength,
                &mode,
                &level,
                teacher_confirmed.then_some(confirmed_by.trim()),
                teacher_confirmed.then_some(now.as_str()),
                &now,
            ],
        )?;
        carried_ability_link_count += 1;
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
        let is_target = item.question_version_id == source.question_version_id;
        let rubric_version_id = if is_target {
            adopted_rubric_version_id
        } else {
            item.rubric_version_id
        };
        let link_set_id = if is_target {
            adopted_link_set_id
        } else {
            item.link_set_id
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
                item.answer_key_version_id,
                rubric_version_id,
                link_set_id,
                item.order_index,
                item.score,
                item.option_order_json.as_deref(),
                &item.presentation_snapshot_json,
                &now,
            ],
        )?;
        let inserted_item_id = tx.last_insert_rowid();
        if is_target {
            adopted_assessment_item_id = Some(inserted_item_id);
        }
        hash_items.push(AcceptedAnswerAssessmentHashItem {
            item_id: inserted_item_id,
            question_version_id: item.question_version_id,
            answer_key_version_id: item.answer_key_version_id,
            rubric_version_id,
            link_set_id,
            order_index: item.order_index,
            score_millis: (item.score * 1000.0).round() as i64,
        });
    }
    let adopted_assessment_item_id = adopted_assessment_item_id
        .ok_or_else(|| CoreError::Invalid("新作业版本未复制目标简答题，已回滚".into()))?;
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
        "INSERT INTO exam_rubric_evidence_promotions_v2
         (public_id,grade_decision_id,component_id,suggestion_id,
          transcription_revision_id,assessment_id,question_version_id,
          source_assessment_version_id,source_assessment_item_id,source_rubric_version_id,
          base_assessment_version_id,base_assessment_item_id,base_rubric_version_id,
          base_link_set_id,adopted_assessment_version_id,adopted_assessment_item_id,
          adopted_rubric_version_id,adopted_link_set_id,rubric_point_stable_id,
          evidence_text,normalized_text,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,
                 ?17,?18,?19,?20,?21,?22,?23)",
        rusqlite::params![
            &promotion_public_id,
            source.grade_decision_id,
            source.component_id,
            source.suggestion_id,
            source.transcription_revision_id,
            source.assessment_id,
            source.question_version_id,
            source.source_assessment_version_id,
            source.source_assessment_item_id,
            source.source_rubric_version_id,
            base.assessment_version_id,
            base.id,
            base.rubric_version_id,
            base.link_set_id,
            adopted_assessment_version_id,
            adopted_assessment_item_id,
            adopted_rubric_version_id,
            adopted_link_set_id,
            &source.rubric_point_stable_id,
            &evidence_text,
            &normalized_text,
            confirmed_by.trim(),
            &now,
        ],
    )?;
    let promotion_id = tx.last_insert_rowid();
    let audit_meta = serde_json::json!({
        "schema_version": 1,
        "grade_decision_id": source.grade_decision_id,
        "rubric_point_stable_id": source.rubric_point_stable_id,
        "source_assessment_version_id": source.source_assessment_version_id,
        "base_assessment_version_id": base.assessment_version_id,
        "adopted_assessment_version_id": adopted_assessment_version_id,
        "adopted_rubric_version_id": adopted_rubric_version_id,
        "adopted_link_set_id": adopted_link_set_id,
        "current_grade_unchanged": true,
        "current_publication_unchanged": true,
    })
    .to_string();
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!("exam:rubric-evidence-promotion:{promotion_public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "exam.rubric_evidence.promoted",
            object_type: "exam_rubric_evidence_promotion",
            object_id: &promotion_public_id,
            object_revision: Some(adopted_assessment_revision),
            note: None,
            meta_json: Some(&audit_meta),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    Ok(RubricEvidencePromotionResult {
        outcome: "created_new_version".into(),
        promotion_id: Some(promotion_id),
        rubric_point_stable_id: source.rubric_point_stable_id,
        evidence_text,
        adopted_assessment_version_id,
        adopted_assessment_revision,
        adopted_rubric_version_id,
        adopted_link_set_id,
        carried_knowledge_link_count,
        carried_ability_link_count,
        current_grade_unchanged: true,
        current_publication_unchanged: true,
    })
}
