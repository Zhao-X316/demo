//! 学习证据事实编排：证据、outbox 和审计必须同事务落地。

use rusqlite::Connection;

use crate::db::repo::audit::{self, NewAuditEvent};
use crate::db::repo::learning_evidence::{self, NewLearningEvidence};
use crate::db::repo::outbox::{self, NewOutboxEvent};
use crate::error::CoreResult;
use crate::models::{AuditEvent, LearningEvidence, OutboxEvent};

pub struct EvidenceFact<'a> {
    pub evidence: NewLearningEvidence<'a>,
    pub outbox_event: NewOutboxEvent<'a>,
    pub audit_event: NewAuditEvent<'a>,
}

#[derive(Debug)]
pub struct EvidenceFactResult {
    pub evidence: LearningEvidence,
    pub outbox_event: OutboxEvent,
    pub audit_event: AuditEvent,
}

pub fn record_evidence_fact(
    conn: &Connection,
    fact: &EvidenceFact<'_>,
) -> CoreResult<EvidenceFactResult> {
    let tx = conn.unchecked_transaction()?;
    let evidence = learning_evidence::create_or_get(&tx, &fact.evidence)?;
    let outbox_event = outbox::create_event(&tx, &fact.outbox_event)?;
    let audit_event = audit::append(&tx, &fact.audit_event)?;
    tx.commit()?;
    Ok(EvidenceFactResult {
        evidence,
        outbox_event,
        audit_event,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::students::{upsert, StudentInput};
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use crate::models::{
        AssessmentContext, AuditActorType, ConfirmationLevel, EvidenceKind, EvidenceSourceModule,
    };

    const NOW: &str = "2026-07-13T08:00:00.000Z";

    fn setup() -> (Connection, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let student = upsert(
            &conn,
            &StudentInput {
                student_no: "2026001",
                name: "测试学生",
                class_id: None,
                enabled: true,
            },
        )
        .unwrap();
        (conn, student.id)
    }

    fn fact(student_id: i64) -> EvidenceFact<'static> {
        EvidenceFact {
            evidence: NewLearningEvidence {
                idempotency_key: "evidence:grade:88:1",
                student_id,
                source_module: EvidenceSourceModule::Grading,
                source_type: "objective_answer",
                source_ref_type: "exam_answer",
                source_ref_id: "42",
                source_revision: 1,
                decision_ref_type: Some("grade_decision"),
                decision_ref_id: Some("88"),
                decision_revision: Some(1),
                knowledge_node_id: Some("K1:history:1840"),
                ability_dimension_id: None,
                evidence_kind: EvidenceKind::Accuracy,
                value: 1.0,
                confirmation_level: ConfirmationLevel::TeacherAccepted,
                evidence_quality: 0.9,
                assessment_context: AssessmentContext::ClosedBook,
                occurred_at: NOW,
                rule_version: "grading-v1",
                knowledge_map_version: "k1-v1",
            },
            outbox_event: NewOutboxEvent {
                idempotency_key: "event:grade:88:1",
                event_type: "learning_evidence_changed",
                event_version: 1,
                aggregate_type: "grade_decision",
                aggregate_id: "88",
                aggregate_revision: 1,
                payload_json: r#"{"schema_version":1,"student_id":1}"#,
                occurred_at: NOW,
            },
            audit_event: NewAuditEvent {
                idempotency_key: "audit:grade:88:1",
                actor_type: AuditActorType::Teacher,
                actor_id: Some("teacher-local-1"),
                action: "learning_evidence.created",
                object_type: "grade_decision",
                object_id: "88",
                object_revision: Some(1),
                note: None,
                meta_json: Some(r#"{"schema_version":1}"#),
                occurred_at: NOW,
            },
        }
    }

    #[test]
    fn evidence_outbox_and_audit_commit_together() {
        let (conn, student_id) = setup();
        let result = record_evidence_fact(&conn, &fact(student_id)).unwrap();
        assert_eq!(result.evidence.student_id, student_id);
        assert_eq!(result.outbox_event.aggregate_id, "88");
        assert_eq!(result.audit_event.object_id, "88");
    }

    #[test]
    fn invalid_outbox_rolls_back_evidence() {
        let (conn, student_id) = setup();
        let mut invalid = fact(student_id);
        invalid.outbox_event.payload_json = r#"{"student_id":1}"#;
        assert!(record_evidence_fact(&conn, &invalid).is_err());
        let count: i64 = conn
            .query_row("SELECT count(*) FROM learning_evidence", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0);
    }
}
