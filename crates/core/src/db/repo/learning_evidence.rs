//! 标准化学习证据仓储。证据内容不可覆盖，改判只切换旧证据状态并追加新 revision。

use std::io;

use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::ids;
use crate::error::{CoreError, CoreResult};
use crate::models::{
    AssessmentContext, ConfirmationLevel, EvidenceKind, EvidenceSourceModule, EvidenceState,
    LearningEvidence,
};

const COLS: &str = "id, public_id, idempotency_key, student_id, source_module, source_type, \
                    source_ref_type, source_ref_id, source_revision, decision_ref_type, \
                    decision_ref_id, decision_revision, knowledge_node_id, ability_dimension_id, \
                    evidence_kind, value, confirmation_level, evidence_quality, assessment_context, \
                    occurred_at, rule_version, knowledge_map_version, state, created_at";

#[derive(Clone, Copy)]
pub struct NewLearningEvidence<'a> {
    pub idempotency_key: &'a str,
    pub student_id: i64,
    pub source_module: EvidenceSourceModule,
    pub source_type: &'a str,
    pub source_ref_type: &'a str,
    pub source_ref_id: &'a str,
    pub source_revision: i64,
    pub decision_ref_type: Option<&'a str>,
    pub decision_ref_id: Option<&'a str>,
    pub decision_revision: Option<i64>,
    pub knowledge_node_id: Option<&'a str>,
    pub ability_dimension_id: Option<&'a str>,
    pub evidence_kind: EvidenceKind,
    pub value: f64,
    pub confirmation_level: ConfirmationLevel,
    pub evidence_quality: f64,
    pub assessment_context: AssessmentContext,
    pub occurred_at: &'a str,
    pub rule_version: &'a str,
    pub knowledge_map_version: &'a str,
}

fn invalid_enum(index: usize, field: &str, value: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        Type::Text,
        Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid {field}: {value}"),
        )),
    )
}

fn row_to_evidence(row: &rusqlite::Row<'_>) -> rusqlite::Result<LearningEvidence> {
    let source_module_raw: String = row.get(4)?;
    let kind_raw: String = row.get(14)?;
    let confirmation_raw: String = row.get(16)?;
    let context_raw: String = row.get(18)?;
    let state_raw: String = row.get(22)?;
    Ok(LearningEvidence {
        id: row.get(0)?,
        public_id: row.get(1)?,
        idempotency_key: row.get(2)?,
        student_id: row.get(3)?,
        source_module: EvidenceSourceModule::from_db(&source_module_raw)
            .ok_or_else(|| invalid_enum(4, "evidence source module", source_module_raw))?,
        source_type: row.get(5)?,
        source_ref_type: row.get(6)?,
        source_ref_id: row.get(7)?,
        source_revision: row.get(8)?,
        decision_ref_type: row.get(9)?,
        decision_ref_id: row.get(10)?,
        decision_revision: row.get(11)?,
        knowledge_node_id: row.get(12)?,
        ability_dimension_id: row.get(13)?,
        evidence_kind: EvidenceKind::from_db(&kind_raw)
            .ok_or_else(|| invalid_enum(14, "evidence kind", kind_raw))?,
        value: row.get(15)?,
        confirmation_level: ConfirmationLevel::from_db(&confirmation_raw)
            .ok_or_else(|| invalid_enum(16, "confirmation level", confirmation_raw))?,
        evidence_quality: row.get(17)?,
        assessment_context: AssessmentContext::from_db(&context_raw)
            .ok_or_else(|| invalid_enum(18, "assessment context", context_raw))?,
        occurred_at: row.get(19)?,
        rule_version: row.get(20)?,
        knowledge_map_version: row.get(21)?,
        state: EvidenceState::from_db(&state_raw)
            .ok_or_else(|| invalid_enum(22, "evidence state", state_raw))?,
        created_at: row.get(23)?,
    })
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!(
            "learning evidence {field} is required"
        )))
    } else {
        Ok(())
    }
}

fn validate_new(input: &NewLearningEvidence<'_>) -> CoreResult<()> {
    for (value, field) in [
        (input.idempotency_key, "idempotency_key"),
        (input.source_type, "source_type"),
        (input.source_ref_type, "source_ref_type"),
        (input.source_ref_id, "source_ref_id"),
        (input.occurred_at, "occurred_at"),
        (input.rule_version, "rule_version"),
        (input.knowledge_map_version, "knowledge_map_version"),
    ] {
        required(value, field)?;
    }
    if input.source_revision < 1 {
        return Err(CoreError::Invalid(
            "learning evidence source_revision must be at least 1".into(),
        ));
    }
    if !(0.0..=1.0).contains(&input.value) || !(0.0..=1.0).contains(&input.evidence_quality) {
        return Err(CoreError::Invalid(
            "learning evidence value and quality must be between 0 and 1".into(),
        ));
    }
    let decision_parts = [
        input.decision_ref_type.is_some(),
        input.decision_ref_id.is_some(),
        input.decision_revision.is_some(),
    ];
    if decision_parts.iter().any(|value| *value) && !decision_parts.iter().all(|value| *value) {
        return Err(CoreError::Invalid(
            "learning evidence decision reference must be complete".into(),
        ));
    }
    if input.decision_revision.is_some_and(|revision| revision < 1) {
        return Err(CoreError::Invalid(
            "learning evidence decision_revision must be at least 1".into(),
        ));
    }
    if input.confirmation_level == ConfirmationLevel::TeacherOverall
        && (input.knowledge_node_id.is_some() || input.ability_dimension_id.is_some())
    {
        return Err(CoreError::Invalid(
            "teacher_overall evidence cannot assert a knowledge or ability node".into(),
        ));
    }
    Ok(())
}

fn same_identity(existing: &LearningEvidence, input: &NewLearningEvidence<'_>) -> bool {
    existing.student_id == input.student_id
        && existing.source_module == input.source_module
        && existing.source_type == input.source_type.trim()
        && existing.source_ref_type == input.source_ref_type.trim()
        && existing.source_ref_id == input.source_ref_id.trim()
        && existing.source_revision == input.source_revision
        && existing.decision_ref_type.as_deref() == input.decision_ref_type
        && existing.decision_ref_id.as_deref() == input.decision_ref_id
        && existing.decision_revision == input.decision_revision
        && existing.knowledge_node_id.as_deref() == input.knowledge_node_id
        && existing.ability_dimension_id.as_deref() == input.ability_dimension_id
        && existing.evidence_kind == input.evidence_kind
        && existing.value == input.value
        && existing.confirmation_level == input.confirmation_level
        && existing.evidence_quality == input.evidence_quality
        && existing.assessment_context == input.assessment_context
        && existing.occurred_at == input.occurred_at
        && existing.rule_version == input.rule_version.trim()
        && existing.knowledge_map_version == input.knowledge_map_version.trim()
}

pub fn create_or_get(
    conn: &Connection,
    input: &NewLearningEvidence<'_>,
) -> CoreResult<LearningEvidence> {
    validate_new(input)?;
    conn.execute(
        "INSERT INTO learning_evidence
            (public_id, idempotency_key, student_id, source_module, source_type,
             source_ref_type, source_ref_id, source_revision, decision_ref_type,
             decision_ref_id, decision_revision, knowledge_node_id, ability_dimension_id,
             evidence_kind, value, confirmation_level, evidence_quality, assessment_context,
             occurred_at, rule_version, knowledge_map_version, state)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                 ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, 'active')
         ON CONFLICT(idempotency_key) DO NOTHING",
        params![
            ids::new_public_id(),
            input.idempotency_key.trim(),
            input.student_id,
            input.source_module.as_str(),
            input.source_type.trim(),
            input.source_ref_type.trim(),
            input.source_ref_id.trim(),
            input.source_revision,
            input.decision_ref_type,
            input.decision_ref_id,
            input.decision_revision,
            input.knowledge_node_id,
            input.ability_dimension_id,
            input.evidence_kind.as_str(),
            input.value,
            input.confirmation_level.as_str(),
            input.evidence_quality,
            input.assessment_context.as_str(),
            input.occurred_at,
            input.rule_version.trim(),
            input.knowledge_map_version.trim(),
        ],
    )?;
    let evidence = get_by_idempotency_key(conn, input.idempotency_key.trim())?
        .ok_or_else(|| CoreError::Db("learning evidence insert did not produce a row".into()))?;
    if !same_identity(&evidence, input) {
        return Err(CoreError::Invalid(
            "learning evidence idempotency_key already belongs to different input".into(),
        ));
    }
    Ok(evidence)
}

pub fn get_by_id(conn: &Connection, id: i64) -> CoreResult<Option<LearningEvidence>> {
    let sql = format!("SELECT {COLS} FROM learning_evidence WHERE id=?1");
    Ok(conn.query_row(&sql, [id], row_to_evidence).optional()?)
}

pub fn get_by_idempotency_key(
    conn: &Connection,
    key: &str,
) -> CoreResult<Option<LearningEvidence>> {
    let sql = format!("SELECT {COLS} FROM learning_evidence WHERE idempotency_key=?1");
    Ok(conn.query_row(&sql, [key], row_to_evidence).optional()?)
}

pub fn list_active_for_student(
    conn: &Connection,
    student_id: i64,
    formal_only: bool,
) -> CoreResult<Vec<LearningEvidence>> {
    let machine_filter = if formal_only {
        "AND confirmation_level <> 'machine_only'"
    } else {
        ""
    };
    let sql = format!(
        "SELECT {COLS} FROM learning_evidence
         WHERE student_id=?1 AND state='active' {machine_filter}
         ORDER BY occurred_at, id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([student_id], row_to_evidence)?;
    let mut evidence = Vec::new();
    for row in rows {
        evidence.push(row?);
    }
    Ok(evidence)
}

/// 改判时由同一上层 transaction 调用；重复回滚不会再次改变行。
pub fn revert_for_decision(
    conn: &Connection,
    decision_ref_type: &str,
    decision_ref_id: &str,
    decision_revision: i64,
) -> CoreResult<usize> {
    Ok(conn.execute(
        "UPDATE learning_evidence SET state='reverted'
         WHERE decision_ref_type=?1 AND decision_ref_id=?2 AND decision_revision=?3
           AND state='active'",
        params![decision_ref_type, decision_ref_id, decision_revision],
    )?)
}

/// 同一业务来源追加新 revision 时，将旧来源的 active 证据标记为 superseded。
///
/// 调用方必须在自己的业务 transaction 中先写新来源 revision，再调用本函数切换旧证据，
/// 最后创建新证据。重复调用不会再次改变行。
pub fn supersede_for_source(
    conn: &Connection,
    source_module: EvidenceSourceModule,
    source_ref_type: &str,
    source_ref_id: &str,
    source_revision: i64,
) -> CoreResult<usize> {
    required(source_ref_type, "source_ref_type")?;
    required(source_ref_id, "source_ref_id")?;
    if source_revision < 1 {
        return Err(CoreError::Invalid(
            "learning evidence source_revision must be at least 1".into(),
        ));
    }
    Ok(conn.execute(
        "UPDATE learning_evidence SET state='superseded'
         WHERE source_module=?1 AND source_ref_type=?2 AND source_ref_id=?3
           AND source_revision=?4 AND state='active'",
        params![
            source_module.as_str(),
            source_ref_type.trim(),
            source_ref_id.trim(),
            source_revision
        ],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::students::{upsert, StudentInput};
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    const OCCURRED: &str = "2026-07-13T08:00:00.000Z";

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

    fn input(student_id: i64, key: &'static str) -> NewLearningEvidence<'static> {
        NewLearningEvidence {
            idempotency_key: key,
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
            occurred_at: OCCURRED,
            rule_version: "grading-v1",
            knowledge_map_version: "k1-v1",
        }
    }

    #[test]
    fn idempotent_evidence_is_not_rebound() {
        let (conn, student_id) = setup();
        let first = create_or_get(&conn, &input(student_id, "grade:88:1:accuracy")).unwrap();
        let same = create_or_get(&conn, &input(student_id, "grade:88:1:accuracy")).unwrap();
        assert_eq!(first.id, same.id);
        assert!(create_or_get(
            &conn,
            &NewLearningEvidence {
                value: 0.0,
                ..input(student_id, "grade:88:1:accuracy")
            }
        )
        .is_err());
    }

    #[test]
    fn formal_query_excludes_machine_only_but_keeps_confirmed() {
        let (conn, student_id) = setup();
        create_or_get(&conn, &input(student_id, "confirmed")).unwrap();
        create_or_get(
            &conn,
            &NewLearningEvidence {
                idempotency_key: "preview",
                confirmation_level: ConfirmationLevel::MachineOnly,
                ..input(student_id, "unused")
            },
        )
        .unwrap();
        assert_eq!(
            list_active_for_student(&conn, student_id, false)
                .unwrap()
                .len(),
            2
        );
        let formal = list_active_for_student(&conn, student_id, true).unwrap();
        assert_eq!(formal.len(), 1);
        assert_eq!(
            formal[0].confirmation_level,
            ConfirmationLevel::TeacherAccepted
        );
    }

    #[test]
    fn overall_confirmation_cannot_claim_specific_node() {
        let (conn, student_id) = setup();
        assert!(create_or_get(
            &conn,
            &NewLearningEvidence {
                confirmation_level: ConfirmationLevel::TeacherOverall,
                ..input(student_id, "overall")
            }
        )
        .is_err());
    }

    #[test]
    fn decision_revert_is_scoped_and_idempotent() {
        let (conn, student_id) = setup();
        let evidence = create_or_get(&conn, &input(student_id, "confirmed")).unwrap();
        assert_eq!(
            revert_for_decision(&conn, "grade_decision", "88", 1).unwrap(),
            1
        );
        assert_eq!(
            revert_for_decision(&conn, "grade_decision", "88", 1).unwrap(),
            0
        );
        assert_eq!(
            get_by_id(&conn, evidence.id).unwrap().unwrap().state,
            EvidenceState::Reverted
        );
        assert!(conn
            .execute(
                "UPDATE learning_evidence SET value=0.0 WHERE id=?1",
                [evidence.id]
            )
            .is_err());
        assert!(conn
            .execute("DELETE FROM learning_evidence WHERE id=?1", [evidence.id])
            .is_err());
    }

    #[test]
    fn source_supersede_is_scoped_and_idempotent() {
        let (conn, student_id) = setup();
        let evidence = create_or_get(&conn, &input(student_id, "confirmed")).unwrap();
        assert_eq!(
            supersede_for_source(&conn, EvidenceSourceModule::Grading, "exam_answer", "42", 1)
                .unwrap(),
            1
        );
        assert_eq!(
            supersede_for_source(&conn, EvidenceSourceModule::Grading, "exam_answer", "42", 1)
                .unwrap(),
            0
        );
        assert_eq!(
            get_by_id(&conn, evidence.id).unwrap().unwrap().state,
            EvidenceState::Superseded
        );
    }
}
