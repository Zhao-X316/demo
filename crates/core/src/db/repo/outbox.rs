//! 事务 outbox 仓储。事件不可覆盖；每个 consumer 独立 claim 和确认。

use std::io;

use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::ids;
use crate::error::{CoreError, CoreResult};
use crate::models::{OutboxConsumption, OutboxConsumptionStatus, OutboxEvent};

const EVENT_COLS: &str = "id, public_id, idempotency_key, event_type, event_version, \
                          aggregate_type, aggregate_id, aggregate_revision, payload_json, \
                          occurred_at, created_at";
const CONSUMPTION_COLS: &str = "event_id, consumer, status, attempts, max_attempts, lease_token, \
                                lease_expires_at, processed_at, error_meta_json, created_at, updated_at";

#[derive(Clone, Copy)]
pub struct NewOutboxEvent<'a> {
    pub idempotency_key: &'a str,
    pub event_type: &'a str,
    pub event_version: i64,
    pub aggregate_type: &'a str,
    pub aggregate_id: &'a str,
    pub aggregate_revision: i64,
    pub payload_json: &'a str,
    pub occurred_at: &'a str,
}

fn invalid_consumption_status(value: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        2,
        Type::Text,
        Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid outbox consumption status: {value}"),
        )),
    )
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusqlite::Result<OutboxEvent> {
    Ok(OutboxEvent {
        id: row.get(0)?,
        public_id: row.get(1)?,
        idempotency_key: row.get(2)?,
        event_type: row.get(3)?,
        event_version: row.get(4)?,
        aggregate_type: row.get(5)?,
        aggregate_id: row.get(6)?,
        aggregate_revision: row.get(7)?,
        payload_json: row.get(8)?,
        occurred_at: row.get(9)?,
        created_at: row.get(10)?,
    })
}

fn row_to_consumption(row: &rusqlite::Row<'_>) -> rusqlite::Result<OutboxConsumption> {
    let status_raw: String = row.get(2)?;
    Ok(OutboxConsumption {
        event_id: row.get(0)?,
        consumer: row.get(1)?,
        status: OutboxConsumptionStatus::from_db(&status_raw)
            .ok_or_else(|| invalid_consumption_status(status_raw))?,
        attempts: row.get(3)?,
        max_attempts: row.get(4)?,
        lease_token: row.get(5)?,
        lease_expires_at: row.get(6)?,
        processed_at: row.get(7)?,
        error_meta_json: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("outbox {field} is required")))
    } else {
        Ok(())
    }
}

fn validate_event(input: &NewOutboxEvent<'_>) -> CoreResult<()> {
    for (value, field) in [
        (input.idempotency_key, "idempotency_key"),
        (input.event_type, "event_type"),
        (input.aggregate_type, "aggregate_type"),
        (input.aggregate_id, "aggregate_id"),
        (input.payload_json, "payload_json"),
        (input.occurred_at, "occurred_at"),
    ] {
        required(value, field)?;
    }
    if input.event_version < 1 || input.aggregate_revision < 1 {
        return Err(CoreError::Invalid(
            "outbox event and aggregate revisions must be at least 1".into(),
        ));
    }
    Ok(())
}

fn same_event_identity(existing: &OutboxEvent, input: &NewOutboxEvent<'_>) -> bool {
    existing.event_type == input.event_type.trim()
        && existing.event_version == input.event_version
        && existing.aggregate_type == input.aggregate_type.trim()
        && existing.aggregate_id == input.aggregate_id.trim()
        && existing.aggregate_revision == input.aggregate_revision
        && existing.payload_json == input.payload_json
        && existing.occurred_at == input.occurred_at
}

pub fn create_event(conn: &Connection, input: &NewOutboxEvent<'_>) -> CoreResult<OutboxEvent> {
    validate_event(input)?;
    conn.execute(
        "INSERT INTO outbox_events
            (public_id, idempotency_key, event_type, event_version, aggregate_type,
             aggregate_id, aggregate_revision, payload_json, occurred_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(idempotency_key) DO NOTHING",
        params![
            ids::new_public_id(),
            input.idempotency_key.trim(),
            input.event_type.trim(),
            input.event_version,
            input.aggregate_type.trim(),
            input.aggregate_id.trim(),
            input.aggregate_revision,
            input.payload_json,
            input.occurred_at,
        ],
    )?;
    let event = get_event_by_key(conn, input.idempotency_key.trim())?
        .ok_or_else(|| CoreError::Db("outbox event insert did not produce a row".into()))?;
    if !same_event_identity(&event, input) {
        return Err(CoreError::Invalid(
            "outbox event idempotency_key already belongs to different input".into(),
        ));
    }
    Ok(event)
}

pub fn get_event(conn: &Connection, id: i64) -> CoreResult<Option<OutboxEvent>> {
    let sql = format!("SELECT {EVENT_COLS} FROM outbox_events WHERE id=?1");
    Ok(conn.query_row(&sql, [id], row_to_event).optional()?)
}

pub fn get_event_by_key(conn: &Connection, key: &str) -> CoreResult<Option<OutboxEvent>> {
    let sql = format!("SELECT {EVENT_COLS} FROM outbox_events WHERE idempotency_key=?1");
    Ok(conn.query_row(&sql, [key], row_to_event).optional()?)
}

/// 为 consumer 发现指定类型的全部历史事件；重复扫描不会重建消费行。
pub fn ensure_pending_for_consumer(
    conn: &Connection,
    consumer: &str,
    event_type: &str,
    max_attempts: i64,
) -> CoreResult<usize> {
    required(consumer, "consumer")?;
    required(event_type, "event_type")?;
    if max_attempts < 1 {
        return Err(CoreError::Invalid(
            "outbox max_attempts must be at least 1".into(),
        ));
    }
    Ok(conn.execute(
        "INSERT INTO outbox_consumptions (event_id, consumer, status, max_attempts)
         SELECT id, ?1, 'pending', ?3 FROM outbox_events WHERE event_type=?2
         ON CONFLICT(event_id, consumer) DO NOTHING",
        params![consumer.trim(), event_type.trim(), max_attempts],
    )?)
}

pub fn get_consumption(
    conn: &Connection,
    event_id: i64,
    consumer: &str,
) -> CoreResult<Option<OutboxConsumption>> {
    let sql = format!(
        "SELECT {CONSUMPTION_COLS} FROM outbox_consumptions
         WHERE event_id=?1 AND consumer=?2"
    );
    Ok(conn
        .query_row(&sql, params![event_id, consumer], row_to_consumption)
        .optional()?)
}

/// 同一 consumer 并发 worker 只会有一个拿到事件；不同 consumer 相互独立。
pub fn claim_next(
    conn: &Connection,
    consumer: &str,
    now: &str,
    lease_token: &str,
    lease_expires_at: &str,
) -> CoreResult<Option<(OutboxEvent, OutboxConsumption)>> {
    let sql = format!(
        "UPDATE outbox_consumptions
         SET status='processing', attempts=attempts+1, lease_token=?3,
             lease_expires_at=?4, updated_at=?2
         WHERE event_id=(
             SELECT event_id FROM outbox_consumptions
             WHERE consumer=?1 AND status='pending' AND attempts < max_attempts
             ORDER BY event_id LIMIT 1
         ) AND consumer=?1
         RETURNING {CONSUMPTION_COLS}"
    );
    let consumption = conn
        .query_row(
            &sql,
            params![consumer, now, lease_token, lease_expires_at],
            row_to_consumption,
        )
        .optional()?;
    let Some(consumption) = consumption else {
        return Ok(None);
    };
    let event = get_event(conn, consumption.event_id)?.ok_or_else(|| {
        CoreError::Db(format!(
            "outbox consumption references missing event {}",
            consumption.event_id
        ))
    })?;
    Ok(Some((event, consumption)))
}

fn transition_error(conn: &Connection, event_id: i64, consumer: &str, expected: &str) -> CoreError {
    match get_consumption(conn, event_id, consumer) {
        Ok(None) => CoreError::NotFound(format!("outbox consumption {event_id}/{consumer}")),
        Ok(Some(consumption)) => CoreError::Invalid(format!(
            "outbox consumption {event_id}/{consumer} is {}, expected {expected}",
            consumption.status.as_str()
        )),
        Err(err) => err,
    }
}

pub fn finalize_succeeded(
    conn: &Connection,
    event_id: i64,
    consumer: &str,
    lease_token: &str,
    processed_at: &str,
) -> CoreResult<OutboxConsumption> {
    let changed = conn.execute(
        "UPDATE outbox_consumptions
         SET status='succeeded', lease_token=NULL, lease_expires_at=NULL,
             processed_at=?4, error_meta_json=NULL, updated_at=?4
         WHERE event_id=?1 AND consumer=?2 AND status='processing' AND lease_token=?3",
        params![event_id, consumer, lease_token, processed_at],
    )?;
    if changed != 1 {
        return Err(transition_error(conn, event_id, consumer, "processing"));
    }
    get_consumption(conn, event_id, consumer)?
        .ok_or_else(|| CoreError::NotFound(format!("outbox consumption {event_id}/{consumer}")))
}

pub fn finalize_failed(
    conn: &Connection,
    event_id: i64,
    consumer: &str,
    lease_token: &str,
    error_meta_json: &str,
    processed_at: &str,
) -> CoreResult<OutboxConsumption> {
    let changed = conn.execute(
        "UPDATE outbox_consumptions
         SET status='failed', lease_token=NULL, lease_expires_at=NULL,
             processed_at=?5, error_meta_json=?4, updated_at=?5
         WHERE event_id=?1 AND consumer=?2 AND status='processing' AND lease_token=?3",
        params![
            event_id,
            consumer,
            lease_token,
            error_meta_json,
            processed_at
        ],
    )?;
    if changed != 1 {
        return Err(transition_error(conn, event_id, consumer, "processing"));
    }
    get_consumption(conn, event_id, consumer)?
        .ok_or_else(|| CoreError::NotFound(format!("outbox consumption {event_id}/{consumer}")))
}

/// 消费者崩溃后的保守恢复：过期 lease 只标失败，不自动重复副作用。
pub fn fail_expired(conn: &Connection, now: &str, error_meta_json: &str) -> CoreResult<usize> {
    Ok(conn.execute(
        "UPDATE outbox_consumptions
         SET status='failed', lease_token=NULL, lease_expires_at=NULL,
             processed_at=?1, error_meta_json=?2, updated_at=?1
         WHERE status='processing' AND lease_expires_at <= ?1",
        params![now, error_meta_json],
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    const NOW: &str = "2026-07-13T08:00:00.000Z";
    const LEASE_END: &str = "2026-07-13T08:01:00.000Z";

    fn input(key: &'static str) -> NewOutboxEvent<'static> {
        NewOutboxEvent {
            idempotency_key: key,
            event_type: "grade_decision_activated",
            event_version: 1,
            aggregate_type: "grade_decision",
            aggregate_id: "88",
            aggregate_revision: 1,
            payload_json: r#"{"schema_version":1,"answer_id":42}"#,
            occurred_at: NOW,
        }
    }

    #[test]
    fn event_is_idempotent_and_requires_versioned_payload() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let first = create_event(&conn, &input("grade:88:1")).unwrap();
        let same = create_event(&conn, &input("grade:88:1")).unwrap();
        assert_eq!(first.id, same.id);
        assert!(create_event(
            &conn,
            &NewOutboxEvent {
                idempotency_key: "bad-payload",
                payload_json: r#"{"answer_id":42}"#,
                ..input("unused")
            }
        )
        .is_err());
    }

    #[test]
    fn consumers_are_independent_and_lease_guarded() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let event = create_event(&conn, &input("grade:88:1")).unwrap();
        assert_eq!(
            ensure_pending_for_consumer(&conn, "m2_5", "grade_decision_activated", 1).unwrap(),
            1
        );
        assert_eq!(
            ensure_pending_for_consumer(&conn, "m6", "grade_decision_activated", 1).unwrap(),
            1
        );
        assert_eq!(
            ensure_pending_for_consumer(&conn, "m6", "grade_decision_activated", 1).unwrap(),
            0
        );

        let (_, m25) = claim_next(&conn, "m2_5", NOW, "lease-a", LEASE_END)
            .unwrap()
            .unwrap();
        assert!(claim_next(&conn, "m2_5", NOW, "lease-b", LEASE_END)
            .unwrap()
            .is_none());
        assert!(finalize_succeeded(&conn, event.id, "m2_5", "wrong", NOW).is_err());
        let done = finalize_succeeded(&conn, event.id, "m2_5", "lease-a", NOW).unwrap();
        assert_eq!(done.status, OutboxConsumptionStatus::Succeeded);
        assert_eq!(m25.attempts, 1);

        let (_, m6) = claim_next(&conn, "m6", NOW, "lease-c", LEASE_END)
            .unwrap()
            .unwrap();
        assert_eq!(m6.event_id, event.id);
    }

    #[test]
    fn expired_consumption_fails_without_redelivery() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        create_event(&conn, &input("grade:88:1")).unwrap();
        ensure_pending_for_consumer(&conn, "m6", "grade_decision_activated", 1).unwrap();
        claim_next(&conn, "m6", NOW, "lease-a", LEASE_END)
            .unwrap()
            .unwrap();
        assert_eq!(
            fail_expired(
                &conn,
                "2026-07-13T08:02:00.000Z",
                r#"{"schema_version":1,"code":"LEASE_EXPIRED"}"#
            )
            .unwrap(),
            1
        );
        assert!(claim_next(
            &conn,
            "m6",
            "2026-07-13T08:02:01.000Z",
            "lease-b",
            "2026-07-13T08:03:01.000Z"
        )
        .unwrap()
        .is_none());
    }
}
