//! 不可变审计事件仓储。只允许追加和查询。

use std::io;

use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::ids;
use crate::error::{CoreError, CoreResult};
use crate::models::{AuditActorType, AuditEvent};

const COLS: &str = "id, public_id, idempotency_key, actor_type, actor_id, action, object_type, \
                    object_id, object_revision, note, meta_json, occurred_at, created_at";

#[derive(Clone, Copy)]
pub struct NewAuditEvent<'a> {
    pub idempotency_key: &'a str,
    pub actor_type: AuditActorType,
    pub actor_id: Option<&'a str>,
    pub action: &'a str,
    pub object_type: &'a str,
    pub object_id: &'a str,
    pub object_revision: Option<i64>,
    pub note: Option<&'a str>,
    pub meta_json: Option<&'a str>,
    pub occurred_at: &'a str,
}

fn invalid_actor(value: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        3,
        Type::Text,
        Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid audit actor type: {value}"),
        )),
    )
}

fn row_to_audit(row: &rusqlite::Row<'_>) -> rusqlite::Result<AuditEvent> {
    let actor_raw: String = row.get(3)?;
    Ok(AuditEvent {
        id: row.get(0)?,
        public_id: row.get(1)?,
        idempotency_key: row.get(2)?,
        actor_type: AuditActorType::from_db(&actor_raw).ok_or_else(|| invalid_actor(actor_raw))?,
        actor_id: row.get(4)?,
        action: row.get(5)?,
        object_type: row.get(6)?,
        object_id: row.get(7)?,
        object_revision: row.get(8)?,
        note: row.get(9)?,
        meta_json: row.get(10)?,
        occurred_at: row.get(11)?,
        created_at: row.get(12)?,
    })
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("audit {field} is required")))
    } else {
        Ok(())
    }
}

fn validate_new(input: &NewAuditEvent<'_>) -> CoreResult<()> {
    for (value, field) in [
        (input.idempotency_key, "idempotency_key"),
        (input.action, "action"),
        (input.object_type, "object_type"),
        (input.object_id, "object_id"),
        (input.occurred_at, "occurred_at"),
    ] {
        required(value, field)?;
    }
    if input.object_revision.is_some_and(|revision| revision < 1) {
        return Err(CoreError::Invalid(
            "audit object_revision must be at least 1".into(),
        ));
    }
    match input.actor_type {
        AuditActorType::Teacher if input.actor_id.is_none() => Err(CoreError::Invalid(
            "teacher audit event requires actor_id".into(),
        )),
        AuditActorType::System | AuditActorType::Migration if input.actor_id.is_some() => Err(
            CoreError::Invalid("system or migration audit event cannot carry actor_id".into()),
        ),
        _ => Ok(()),
    }
}

fn same_identity(existing: &AuditEvent, input: &NewAuditEvent<'_>) -> bool {
    existing.actor_type == input.actor_type
        && existing.actor_id.as_deref() == input.actor_id
        && existing.action == input.action.trim()
        && existing.object_type == input.object_type.trim()
        && existing.object_id == input.object_id.trim()
        && existing.object_revision == input.object_revision
        && existing.note.as_deref() == input.note
        && existing.meta_json.as_deref() == input.meta_json
        && existing.occurred_at == input.occurred_at
}

pub fn append(conn: &Connection, input: &NewAuditEvent<'_>) -> CoreResult<AuditEvent> {
    validate_new(input)?;
    conn.execute(
        "INSERT INTO audit_events
            (public_id, idempotency_key, actor_type, actor_id, action, object_type,
             object_id, object_revision, note, meta_json, occurred_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(idempotency_key) DO NOTHING",
        params![
            ids::new_public_id(),
            input.idempotency_key.trim(),
            input.actor_type.as_str(),
            input.actor_id,
            input.action.trim(),
            input.object_type.trim(),
            input.object_id.trim(),
            input.object_revision,
            input.note,
            input.meta_json,
            input.occurred_at,
        ],
    )?;
    let event = get_by_key(conn, input.idempotency_key.trim())?
        .ok_or_else(|| CoreError::Db("audit event insert did not produce a row".into()))?;
    if !same_identity(&event, input) {
        return Err(CoreError::Invalid(
            "audit idempotency_key already belongs to different input".into(),
        ));
    }
    Ok(event)
}

pub fn get_by_key(conn: &Connection, key: &str) -> CoreResult<Option<AuditEvent>> {
    let sql = format!("SELECT {COLS} FROM audit_events WHERE idempotency_key=?1");
    Ok(conn.query_row(&sql, [key], row_to_audit).optional()?)
}

pub fn list_for_object(
    conn: &Connection,
    object_type: &str,
    object_id: &str,
) -> CoreResult<Vec<AuditEvent>> {
    let sql = format!(
        "SELECT {COLS} FROM audit_events
         WHERE object_type=?1 AND object_id=?2 ORDER BY occurred_at, id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![object_type, object_id], row_to_audit)?;
    let mut events = Vec::new();
    for row in rows {
        events.push(row?);
    }
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    const NOW: &str = "2026-07-13T08:00:00.000Z";

    fn input(key: &'static str) -> NewAuditEvent<'static> {
        NewAuditEvent {
            idempotency_key: key,
            actor_type: AuditActorType::Teacher,
            actor_id: Some("teacher-local-1"),
            action: "grade_decision.activated",
            object_type: "grade_decision",
            object_id: "88",
            object_revision: Some(1),
            note: Some("确认正确"),
            meta_json: Some(r#"{"schema_version":1}"#),
            occurred_at: NOW,
        }
    }

    #[test]
    fn audit_is_idempotent_append_only() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let first = append(&conn, &input("audit:grade:88:1")).unwrap();
        let same = append(&conn, &input("audit:grade:88:1")).unwrap();
        assert_eq!(first.id, same.id);
        assert!(conn
            .execute(
                "UPDATE audit_events SET note='tampered' WHERE id=?1",
                [first.id]
            )
            .is_err());
        assert!(conn
            .execute("DELETE FROM audit_events WHERE id=?1", [first.id])
            .is_err());
        assert_eq!(
            list_for_object(&conn, "grade_decision", "88")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn actor_identity_rules_are_enforced() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        assert!(append(
            &conn,
            &NewAuditEvent {
                actor_id: None,
                ..input("missing-teacher")
            }
        )
        .is_err());
        append(
            &conn,
            &NewAuditEvent {
                idempotency_key: "system-event",
                actor_type: AuditActorType::System,
                actor_id: None,
                ..input("unused")
            },
        )
        .unwrap();
    }
}
