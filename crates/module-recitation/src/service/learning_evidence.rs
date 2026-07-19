//! M1.2 背诵学习证据投影。
//!
//! 本层不自行开启事务。调用方必须把人工终审、效果账本、逐点评审、证据、
//! outbox 与 audit 放在同一个 SQLite transaction 中提交。

use chrono::NaiveDate;
use rusqlite::Connection;
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::decision_effects::DecisionEffect;
use suite_core::db::repo::learning_evidence::{self, NewLearningEvidence};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{
    AssessmentContext, AuditActorType, ConfirmationLevel, EvidenceKind, EvidenceSourceModule,
    EvidenceState, Task, Verdict,
};

use crate::db::point_reviews::RecPointReviewRevision;
use crate::db::retention::{self, RecRetentionWindow};

const DECISION_REF_TYPE: &str = "recitation_decision_effect";
const UNMAPPED_VERSION: &str = "unmapped:r1";
const OVERALL_RULE_VERSION: &str = "recitation-overall-v1";
const FLUENCY_RULE_VERSION: &str = "recitation-fluency-v1";
const POINT_RULE_VERSION: &str = "recitation-rubric-point-v1";
const RETENTION_RULE_VERSION: &str = "recitation-retention-v1";

pub(crate) struct DecisionTiming<'a> {
    pub decision_date: NaiveDate,
    pub task: Option<&'a Task>,
}

pub(crate) struct DecisionEvidenceInput<'a> {
    pub submission_id: i64,
    pub student_id: i64,
    pub verdict: &'a Verdict,
    pub effect: &'a DecisionEffect,
    pub review: Option<&'a RecPointReviewRevision>,
    pub timing: Option<DecisionTiming<'a>>,
    pub actor: &'a str,
}

struct PointEvidenceSource {
    point_public_id: String,
    confirmation_level: ConfirmationLevel,
    confirmed_state: String,
    confidence: f64,
    knowledge_node_public_id: Option<String>,
    knowledge_map_version: Option<String>,
}

fn confirmed_quality(machine_confidence: Option<f64>) -> f64 {
    let confidence = machine_confidence.unwrap_or(0.5).clamp(0.0, 1.0);
    (0.5 + 0.5 * confidence).clamp(0.0, 1.0)
}

fn effect_occurred_at(conn: &Connection, effect_id: i64) -> CoreResult<String> {
    conn.query_row(
        "SELECT strftime('%Y-%m-%dT%H:%M:%fZ',created_at)
         FROM decision_effects WHERE id=?1",
        [effect_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

fn emit_activation(
    conn: &Connection,
    evidence: &suite_core::models::LearningEvidence,
    actor: &str,
) -> CoreResult<()> {
    let payload = serde_json::json!({
        "schema_version": 1,
        "evidence_public_id": evidence.public_id,
        "student_id": evidence.student_id,
        "source_module": evidence.source_module.as_str(),
        "source_type": evidence.source_type,
        "source_ref_type": evidence.source_ref_type,
        "source_ref_id": evidence.source_ref_id,
        "source_revision": evidence.source_revision,
        "decision_ref_type": evidence.decision_ref_type,
        "decision_ref_id": evidence.decision_ref_id,
        "decision_revision": evidence.decision_revision
    })
    .to_string();
    outbox::create_event(
        conn,
        &NewOutboxEvent {
            idempotency_key: &format!("recitation:outbox:evidence:{}", evidence.public_id),
            event_type: "learning_evidence_changed",
            event_version: 1,
            aggregate_type: "learning_evidence",
            aggregate_id: &evidence.public_id,
            aggregate_revision: evidence.source_revision,
            payload_json: &payload,
            occurred_at: &evidence.occurred_at,
        },
    )?;
    audit::append(
        conn,
        &NewAuditEvent {
            idempotency_key: &format!("recitation:audit:evidence:{}", evidence.public_id),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(actor),
            action: "recitation.learning_evidence.activated",
            object_type: "learning_evidence",
            object_id: &evidence.public_id,
            object_revision: Some(evidence.source_revision),
            note: Some(match evidence.source_type.as_str() {
                "recitation_rubric_point" => "老师接受或修正逐点评审后激活背诵评分点正式学习证据",
                "recitation_fluency" => "老师确认总体结论后激活背诵流畅度证据",
                "recitation_retention" => "跨日期老师终审形成背诵保持稳定性证据",
                _ => "老师确认总体结论后激活背诵内容级证据",
            }),
            meta_json: Some(&payload),
            occurred_at: &evidence.occurred_at,
        },
    )?;
    Ok(())
}

fn create_and_emit(
    conn: &Connection,
    input: &NewLearningEvidence<'_>,
    actor: &str,
) -> CoreResult<()> {
    let existed = learning_evidence::get_by_idempotency_key(conn, input.idempotency_key)?.is_some();
    let evidence = learning_evidence::create_or_get(conn, input)?;
    if existed {
        if evidence.state != EvidenceState::Active {
            return Err(CoreError::Invalid(
                "同一背诵证据键已经失效，拒绝静默重新激活".into(),
            ));
        }
        return Ok(());
    }
    emit_activation(conn, &evidence, actor)
}

fn list_point_sources(conn: &Connection, review_id: i64) -> CoreResult<Vec<PointEvidenceSource>> {
    let mut statement = conn.prepare(
        "SELECT point.public_id,item.confirmation_level,item.confirmed_state,
                result.confidence,
                CASE WHEN map.id IS NOT NULL THEN node.public_id END,
                map.public_id,map.revision
         FROM rec_point_review_items item
         JOIN rec_point_results result ON result.id=item.source_point_result_id
         JOIN rec_rubric_points point ON point.id=item.rubric_point_id
         LEFT JOIN k1_knowledge_nodes node
           ON node.id=point.knowledge_node_id
          AND point.knowledge_link_state='confirmed'
          AND node.state='active'
         LEFT JOIN k1_knowledge_maps map
           ON map.id=node.knowledge_map_id
          AND map.state='confirmed'
         WHERE item.review_revision_id=?1
         ORDER BY point.order_index,item.id",
    )?;
    let rows = statement.query_map([review_id], |row| {
        let confirmation: String = row.get(1)?;
        let confirmation_level = match confirmation.as_str() {
            "accepted" => ConfirmationLevel::TeacherAccepted,
            "corrected" => ConfirmationLevel::TeacherCorrected,
            _ => {
                return Err(rusqlite::Error::InvalidColumnType(
                    1,
                    "confirmation_level".into(),
                    rusqlite::types::Type::Text,
                ))
            }
        };
        let map_public_id: Option<String> = row.get(5)?;
        let map_revision: Option<i64> = row.get(6)?;
        Ok(PointEvidenceSource {
            point_public_id: row.get(0)?,
            confirmation_level,
            confirmed_state: row.get(2)?,
            confidence: row.get(3)?,
            knowledge_node_public_id: row.get(4)?,
            knowledge_map_version: map_public_id
                .zip(map_revision)
                .map(|(public_id, revision)| format!("{public_id}:r{revision}")),
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn activate_overall(
    conn: &Connection,
    submission_id: i64,
    student_id: i64,
    verdict: &Verdict,
    effect: &DecisionEffect,
    actor: &str,
    occurred_at: &str,
) -> CoreResult<()> {
    let effect_id = effect.id.to_string();
    let submission_ref = submission_id.to_string();
    let overall_key = format!(
        "recitation:evidence:effect:{}:r{}:overall:accuracy",
        effect.id, effect.revision
    );
    create_and_emit(
        conn,
        &NewLearningEvidence {
            idempotency_key: &overall_key,
            student_id,
            source_module: EvidenceSourceModule::Recitation,
            source_type: "recitation_overall",
            source_ref_type: "recitation_submission",
            source_ref_id: &submission_ref,
            source_revision: effect.revision,
            decision_ref_type: Some(DECISION_REF_TYPE),
            decision_ref_id: Some(&effect_id),
            decision_revision: Some(effect.revision),
            knowledge_node_id: None,
            ability_dimension_id: None,
            evidence_kind: EvidenceKind::Accuracy,
            value: if effect.result == "pass" { 1.0 } else { 0.0 },
            confirmation_level: ConfirmationLevel::TeacherOverall,
            evidence_quality: confirmed_quality(verdict.confidence),
            assessment_context: AssessmentContext::Homework,
            occurred_at,
            rule_version: OVERALL_RULE_VERSION,
            knowledge_map_version: UNMAPPED_VERSION,
        },
        actor,
    )?;

    let Some(fluency) = verdict.secondary_score else {
        return Ok(());
    };
    if !fluency.is_finite() || !(0.0..=100.0).contains(&fluency) {
        return Err(CoreError::Invalid(
            "背诵流畅度不在 0 到 100 的有效范围内".into(),
        ));
    }
    let fluency_key = format!(
        "recitation:evidence:effect:{}:r{}:overall:fluency",
        effect.id, effect.revision
    );
    create_and_emit(
        conn,
        &NewLearningEvidence {
            idempotency_key: &fluency_key,
            student_id,
            source_module: EvidenceSourceModule::Recitation,
            source_type: "recitation_fluency",
            source_ref_type: "recitation_submission",
            source_ref_id: &submission_ref,
            source_revision: effect.revision,
            decision_ref_type: Some(DECISION_REF_TYPE),
            decision_ref_id: Some(&effect_id),
            decision_revision: Some(effect.revision),
            knowledge_node_id: None,
            ability_dimension_id: None,
            evidence_kind: EvidenceKind::Fluency,
            value: fluency / 100.0,
            confirmation_level: ConfirmationLevel::TeacherOverall,
            evidence_quality: confirmed_quality(verdict.confidence),
            assessment_context: AssessmentContext::Homework,
            occurred_at,
            rule_version: FLUENCY_RULE_VERSION,
            knowledge_map_version: UNMAPPED_VERSION,
        },
        actor,
    )
}

fn activate_points(
    conn: &Connection,
    student_id: i64,
    effect: &DecisionEffect,
    review: &RecPointReviewRevision,
    actor: &str,
) -> CoreResult<()> {
    if review.state != "active" || review.decision_effect_id != effect.id {
        return Err(CoreError::Invalid(
            "只能从当前终审效果下仍生效的逐点评审生成正式证据".into(),
        ));
    }
    let effect_id = effect.id.to_string();
    for source in list_point_sources(conn, review.id)? {
        let point_quality = match source.confirmation_level {
            ConfirmationLevel::TeacherCorrected => 1.0,
            ConfirmationLevel::TeacherAccepted => confirmed_quality(Some(source.confidence)),
            _ => {
                return Err(CoreError::Invalid(
                    "逐点评审确认级别不能生成正式背诵证据".into(),
                ))
            }
        };
        let projections: &[(EvidenceKind, f64)] = match source.confirmed_state.as_str() {
            "covered" => &[(EvidenceKind::Coverage, 1.0), (EvidenceKind::Accuracy, 1.0)],
            "partial" => &[(EvidenceKind::Coverage, 0.5), (EvidenceKind::Accuracy, 0.5)],
            "omitted" => &[(EvidenceKind::Coverage, 0.0)],
            "contradiction" => &[
                (EvidenceKind::Coverage, 1.0),
                (EvidenceKind::Accuracy, 0.0),
                (EvidenceKind::Contradiction, 1.0),
            ],
            "uncertain" => &[],
            _ => return Err(CoreError::Invalid("逐点评审状态无效".into())),
        };
        let knowledge_map_version = source
            .knowledge_map_version
            .as_deref()
            .unwrap_or(UNMAPPED_VERSION);
        for (kind, value) in projections {
            let idempotency_key = format!(
                "recitation:evidence:review:{}:r{}:point:{}:{}",
                review.public_id,
                review.revision,
                source.point_public_id,
                kind.as_str()
            );
            create_and_emit(
                conn,
                &NewLearningEvidence {
                    idempotency_key: &idempotency_key,
                    student_id,
                    source_module: EvidenceSourceModule::Recitation,
                    source_type: "recitation_rubric_point",
                    source_ref_type: "recitation_point_review",
                    source_ref_id: &review.public_id,
                    source_revision: review.revision,
                    decision_ref_type: Some(DECISION_REF_TYPE),
                    decision_ref_id: Some(&effect_id),
                    decision_revision: Some(effect.revision),
                    knowledge_node_id: source.knowledge_node_public_id.as_deref(),
                    ability_dimension_id: None,
                    evidence_kind: *kind,
                    value: *value,
                    confirmation_level: source.confirmation_level,
                    evidence_quality: point_quality,
                    assessment_context: AssessmentContext::Homework,
                    occurred_at: &review.created_at,
                    rule_version: POINT_RULE_VERSION,
                    knowledge_map_version,
                },
                actor,
            )?;
        }
    }
    Ok(())
}

fn retention_interval_strength(actual_interval_days: i64) -> f64 {
    match actual_interval_days {
        21.. => 1.0,
        7..=20 => 0.8,
        2..=6 => 0.6,
        _ => 0.4,
    }
}

fn activate_retention(
    conn: &Connection,
    verdict: &Verdict,
    effect: &DecisionEffect,
    window: &RecRetentionWindow,
    actor: &str,
    occurred_at: &str,
) -> CoreResult<()> {
    if window.state != "active" || window.current_effect_id != effect.id {
        return Err(CoreError::Invalid(
            "只能从当前终审效果下仍生效的跨日期窗口生成保持证据".into(),
        ));
    }
    let effect_id = effect.id.to_string();
    let key = format!(
        "recitation:evidence:retention:{}:r{}",
        window.public_id, window.revision
    );
    create_and_emit(
        conn,
        &NewLearningEvidence {
            idempotency_key: &key,
            student_id: effect.student_id,
            source_module: EvidenceSourceModule::Recitation,
            source_type: "recitation_retention",
            source_ref_type: "recitation_retention_window",
            source_ref_id: &window.public_id,
            source_revision: window.revision,
            decision_ref_type: Some(DECISION_REF_TYPE),
            decision_ref_id: Some(&effect_id),
            decision_revision: Some(effect.revision),
            knowledge_node_id: None,
            ability_dimension_id: None,
            evidence_kind: EvidenceKind::Retention,
            value: if effect.result == "pass" { 1.0 } else { 0.0 },
            confirmation_level: ConfirmationLevel::TeacherOverall,
            evidence_quality: (confirmed_quality(verdict.confidence)
                * retention_interval_strength(window.actual_interval_days))
            .clamp(0.0, 1.0),
            assessment_context: AssessmentContext::Homework,
            occurred_at,
            rule_version: RETENTION_RULE_VERSION,
            knowledge_map_version: UNMAPPED_VERSION,
        },
        actor,
    )
}

pub(crate) fn activate_for_decision(
    conn: &Connection,
    input: &DecisionEvidenceInput<'_>,
) -> CoreResult<()> {
    let actor = input.actor.trim();
    if actor.is_empty() {
        return Err(CoreError::Invalid(
            "正式背诵学习证据必须记录终审老师".into(),
        ));
    }
    let occurred_at = effect_occurred_at(conn, input.effect.id)?;
    activate_overall(
        conn,
        input.submission_id,
        input.student_id,
        input.verdict,
        input.effect,
        actor,
        &occurred_at,
    )?;
    if let Some(review) = input.review {
        activate_points(conn, input.student_id, input.effect, review, actor)?;
    }
    if let Some(timing) = input.timing.as_ref() {
        let context =
            retention::record_context(conn, input.effect, timing.decision_date, timing.task)?;
        if let Some(window) = retention::record_window(conn, input.effect, &context, timing.task)? {
            activate_retention(
                conn,
                input.verdict,
                input.effect,
                &window,
                actor,
                &occurred_at,
            )?;
        }
    }
    Ok(())
}

pub(crate) fn revert_for_effect(conn: &Connection, effect: &DecisionEffect) -> CoreResult<usize> {
    learning_evidence::revert_for_decision(
        conn,
        DECISION_REF_TYPE,
        &effect.id.to_string(),
        effect.revision,
    )
}

pub(crate) fn supersede_for_review(
    conn: &Connection,
    review: &RecPointReviewRevision,
) -> CoreResult<usize> {
    learning_evidence::supersede_for_source(
        conn,
        EvidenceSourceModule::Recitation,
        "recitation_point_review",
        &review.public_id,
        review.revision,
    )
}
