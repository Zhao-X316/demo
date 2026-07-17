//! M6.1-3 班级教学事件。
//!
//! 教学事件只解释快照所处的课堂背景。创建、修正和作废均追加不可变
//! revision；它们不会修改学习证据、个人/班级快照或自动生成因果结论。

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

const MAX_EVENT_DAYS: i64 = 366;
const EVENT_TYPES: &[&str] = &[
    "new_lesson",
    "review",
    "quiz",
    "exam",
    "holiday",
    "schedule_pause",
];

#[derive(Debug, Clone)]
pub struct CreateTeachingEventInput<'a> {
    pub request_key: &'a str,
    pub class_id: i64,
    pub event_type: &'a str,
    pub title: &'a str,
    pub range_start: &'a str,
    pub range_end: &'a str,
    pub note: Option<&'a str>,
    pub actor_id: &'a str,
}

#[derive(Debug, Clone)]
pub struct ReviseTeachingEventInput<'a> {
    pub request_key: &'a str,
    pub event_key: &'a str,
    pub expected_revision: i64,
    pub event_type: &'a str,
    pub title: &'a str,
    pub range_start: &'a str,
    pub range_end: &'a str,
    pub note: Option<&'a str>,
    pub actor_id: &'a str,
}

#[derive(Debug, Clone)]
pub struct VoidTeachingEventInput<'a> {
    pub request_key: &'a str,
    pub event_key: &'a str,
    pub expected_revision: i64,
    pub actor_id: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassTeachingEvent {
    pub public_id: String,
    pub event_key: String,
    pub class_id: i64,
    pub revision: i64,
    pub event_type: String,
    pub title: String,
    pub range_start: String,
    pub range_end: String,
    pub note: Option<String>,
    pub state: String,
    pub supersedes_public_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
struct EventPayload<'a> {
    event_type: &'a str,
    title: &'a str,
    range_start: &'a str,
    range_end: &'a str,
    note: Option<&'a str>,
    state: &'a str,
}

struct EventInsert<'a, 'payload> {
    request_key: &'a str,
    event_key: &'a str,
    class_id: i64,
    revision: i64,
    payload: &'payload EventPayload<'a>,
    supersedes_public_id: Option<&'a str>,
    actor_id: &'a str,
    action: &'a str,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Invalid(format!("{label}不能为空")));
    }
    Ok(())
}

fn validate_class(conn: &Connection, class_id: i64) -> CoreResult<()> {
    let exists = conn
        .query_row("SELECT 1 FROM classes WHERE id=?1", [class_id], |row| {
            row.get::<_, i64>(0)
        })
        .optional()?
        .is_some();
    if !exists {
        return Err(CoreError::Invalid("班级不存在".into()));
    }
    Ok(())
}

fn validate_payload(payload: &EventPayload<'_>) -> CoreResult<()> {
    required(payload.event_type, "事件类型")?;
    required(payload.title, "事件标题")?;
    if !EVENT_TYPES.contains(&payload.event_type) {
        return Err(CoreError::Invalid("不支持的教学事件类型".into()));
    }
    if payload.title.chars().count() > 80 {
        return Err(CoreError::Invalid("事件标题不能超过 80 个字符".into()));
    }
    if payload.note.unwrap_or_default().chars().count() > 500 {
        return Err(CoreError::Invalid("事件备注不能超过 500 个字符".into()));
    }
    let start = NaiveDate::parse_from_str(payload.range_start, "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid("事件开始日期格式应为 YYYY-MM-DD".into()))?;
    let end = NaiveDate::parse_from_str(payload.range_end, "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid("事件结束日期格式应为 YYYY-MM-DD".into()))?;
    if end < start {
        return Err(CoreError::Invalid("事件结束日期不能早于开始日期".into()));
    }
    if (end - start).num_days() > MAX_EVENT_DAYS {
        return Err(CoreError::Invalid("单个教学事件范围不能超过 366 天".into()));
    }
    Ok(())
}

fn payload_hash(class_id: i64, payload: &EventPayload<'_>) -> CoreResult<String> {
    let value = serde_json::json!({
        "schema_version": 1,
        "class_id": class_id,
        "event_type": payload.event_type,
        "title": payload.title.trim(),
        "range_start": payload.range_start,
        "range_end": payload.range_end,
        "note": payload.note.map(str::trim).filter(|value| !value.is_empty()),
        "state": payload.state,
        "causal_claim_allowed": false
    });
    let bytes =
        serde_json::to_vec(&value).map_err(|error| CoreError::Invalid(error.to_string()))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn event_row(row: &Row<'_>) -> rusqlite::Result<ClassTeachingEvent> {
    Ok(ClassTeachingEvent {
        public_id: row.get(0)?,
        event_key: row.get(1)?,
        class_id: row.get(2)?,
        revision: row.get(3)?,
        event_type: row.get(4)?,
        title: row.get(5)?,
        range_start: row.get(6)?,
        range_end: row.get(7)?,
        note: row.get(8)?,
        state: row.get(9)?,
        supersedes_public_id: row.get(10)?,
        created_by: row.get(11)?,
        created_at: row.get(12)?,
    })
}

const EVENT_SELECT: &str =
    "SELECT public_id,event_key,class_id,revision,event_type,title,range_start,range_end,
            note,state,supersedes_public_id,created_by,created_at
     FROM class_teaching_event_revisions";

fn event_by_request(
    conn: &Connection,
    request_key: &str,
) -> CoreResult<Option<(ClassTeachingEvent, String)>> {
    Ok(conn
        .query_row(
            "SELECT public_id,event_key,class_id,revision,event_type,title,range_start,
                    range_end,note,state,supersedes_public_id,created_by,created_at,
                    payload_sha256
             FROM class_teaching_event_revisions
             WHERE request_key=?1",
            [request_key],
            |row| Ok((event_row(row)?, row.get::<_, String>(13)?)),
        )
        .optional()?)
}

fn latest_event(conn: &Connection, event_key: &str) -> CoreResult<Option<ClassTeachingEvent>> {
    Ok(conn
        .query_row(
            &format!("{EVENT_SELECT} WHERE event_key=?1 ORDER BY revision DESC LIMIT 1"),
            [event_key],
            event_row,
        )
        .optional()?)
}

fn insert_event(
    tx: &Transaction<'_>,
    input: &EventInsert<'_, '_>,
) -> CoreResult<ClassTeachingEvent> {
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    let digest = payload_hash(input.class_id, input.payload)?;
    tx.execute(
        "INSERT INTO class_teaching_event_revisions
          (public_id,request_key,event_key,class_id,revision,event_type,title,range_start,
           range_end,note,state,supersedes_public_id,payload_sha256,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
        params![
            public_id,
            input.request_key.trim(),
            input.event_key,
            input.class_id,
            input.revision,
            input.payload.event_type,
            input.payload.title.trim(),
            input.payload.range_start,
            input.payload.range_end,
            input
                .payload
                .note
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            input.payload.state,
            input.supersedes_public_id,
            digest,
            input.actor_id.trim(),
            created_at,
        ],
    )?;
    let event_payload = serde_json::json!({
        "schema_version": 1,
        "event_key": input.event_key,
        "event_public_id": public_id,
        "class_id": input.class_id,
        "revision": input.revision,
        "state": input.payload.state,
        "causal_claim_allowed": false
    })
    .to_string();
    outbox::create_event(
        tx,
        &NewOutboxEvent {
            idempotency_key: &format!("profile:outbox:teaching-event:{public_id}"),
            event_type: "class_teaching_event_revised",
            event_version: 1,
            aggregate_type: "class_teaching_event",
            aggregate_id: input.event_key,
            aggregate_revision: input.revision,
            payload_json: &event_payload,
            occurred_at: &created_at,
        },
    )?;
    audit::append(
        tx,
        &NewAuditEvent {
            idempotency_key: &format!("profile:audit:teaching-event:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.actor_id.trim()),
            action: input.action,
            object_type: "class_teaching_event",
            object_id: input.event_key,
            object_revision: Some(input.revision),
            note: Some("教学事件只作为快照变化背景，不自动形成因果结论"),
            meta_json: Some(&event_payload),
            occurred_at: &created_at,
        },
    )?;
    Ok(ClassTeachingEvent {
        public_id,
        event_key: input.event_key.to_string(),
        class_id: input.class_id,
        revision: input.revision,
        event_type: input.payload.event_type.to_string(),
        title: input.payload.title.trim().to_string(),
        range_start: input.payload.range_start.to_string(),
        range_end: input.payload.range_end.to_string(),
        note: input
            .payload
            .note
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        state: input.payload.state.to_string(),
        supersedes_public_id: input.supersedes_public_id.map(str::to_string),
        created_by: input.actor_id.trim().to_string(),
        created_at,
    })
}

fn return_existing_request(
    conn: &Connection,
    request_key: &str,
    expected_hash: &str,
) -> CoreResult<Option<ClassTeachingEvent>> {
    let Some((event, stored_hash)) = event_by_request(conn, request_key)? else {
        return Ok(None);
    };
    if stored_hash != expected_hash {
        return Err(CoreError::Invalid(
            "同一请求标识已用于不同教学事件内容".into(),
        ));
    }
    Ok(Some(event))
}

pub fn create_teaching_event(
    conn: &mut Connection,
    input: &CreateTeachingEventInput<'_>,
) -> CoreResult<ClassTeachingEvent> {
    required(input.request_key, "请求标识")?;
    required(input.actor_id, "操作人")?;
    let payload = EventPayload {
        event_type: input.event_type.trim(),
        title: input.title,
        range_start: input.range_start,
        range_end: input.range_end,
        note: input.note,
        state: "active",
    };
    validate_payload(&payload)?;
    validate_class(conn, input.class_id)?;
    let digest = payload_hash(input.class_id, &payload)?;
    if let Some(existing) = return_existing_request(conn, input.request_key, &digest)? {
        if existing.class_id != input.class_id || existing.revision != 1 {
            return Err(CoreError::Invalid(
                "同一请求标识不能跨教学事件操作复用".into(),
            ));
        }
        return Ok(existing);
    }
    let tx = conn.transaction()?;
    let event_key = ids::new_public_id();
    let event = insert_event(
        &tx,
        &EventInsert {
            request_key: input.request_key,
            event_key: &event_key,
            class_id: input.class_id,
            revision: 1,
            payload: &payload,
            supersedes_public_id: None,
            actor_id: input.actor_id,
            action: "profile.teaching_event.created",
        },
    )?;
    tx.commit()?;
    Ok(event)
}

pub fn revise_teaching_event(
    conn: &mut Connection,
    input: &ReviseTeachingEventInput<'_>,
) -> CoreResult<ClassTeachingEvent> {
    required(input.request_key, "请求标识")?;
    required(input.event_key, "教学事件")?;
    required(input.actor_id, "操作人")?;
    let payload = EventPayload {
        event_type: input.event_type.trim(),
        title: input.title,
        range_start: input.range_start,
        range_end: input.range_end,
        note: input.note,
        state: "active",
    };
    validate_payload(&payload)?;
    let current = latest_event(conn, input.event_key)?
        .ok_or_else(|| CoreError::Invalid("教学事件不存在".into()))?;
    let digest = payload_hash(current.class_id, &payload)?;
    if let Some(existing) = return_existing_request(conn, input.request_key, &digest)? {
        if existing.event_key != input.event_key {
            return Err(CoreError::Invalid("同一请求标识不能跨教学事件复用".into()));
        }
        return Ok(existing);
    }
    if current.state != "active" {
        return Err(CoreError::Invalid("已作废的教学事件不能继续修改".into()));
    }
    if current.revision != input.expected_revision {
        return Err(CoreError::Invalid(
            "教学事件已被其他修改更新，请刷新后重试".into(),
        ));
    }
    let tx = conn.transaction()?;
    let event = insert_event(
        &tx,
        &EventInsert {
            request_key: input.request_key,
            event_key: input.event_key,
            class_id: current.class_id,
            revision: current.revision + 1,
            payload: &payload,
            supersedes_public_id: Some(&current.public_id),
            actor_id: input.actor_id,
            action: "profile.teaching_event.revised",
        },
    )?;
    tx.commit()?;
    Ok(event)
}

pub fn void_teaching_event(
    conn: &mut Connection,
    input: &VoidTeachingEventInput<'_>,
) -> CoreResult<ClassTeachingEvent> {
    required(input.request_key, "请求标识")?;
    required(input.event_key, "教学事件")?;
    required(input.actor_id, "操作人")?;
    let current = latest_event(conn, input.event_key)?
        .ok_or_else(|| CoreError::Invalid("教学事件不存在".into()))?;
    let payload = EventPayload {
        event_type: &current.event_type,
        title: &current.title,
        range_start: &current.range_start,
        range_end: &current.range_end,
        note: current.note.as_deref(),
        state: "voided",
    };
    let digest = payload_hash(current.class_id, &payload)?;
    if let Some(existing) = return_existing_request(conn, input.request_key, &digest)? {
        if existing.event_key != input.event_key {
            return Err(CoreError::Invalid("同一请求标识不能跨教学事件复用".into()));
        }
        return Ok(existing);
    }
    if current.state == "voided" {
        return Ok(current);
    }
    if current.revision != input.expected_revision {
        return Err(CoreError::Invalid(
            "教学事件已被其他修改更新，请刷新后重试".into(),
        ));
    }
    let tx = conn.transaction()?;
    let event = insert_event(
        &tx,
        &EventInsert {
            request_key: input.request_key,
            event_key: input.event_key,
            class_id: current.class_id,
            revision: current.revision + 1,
            payload: &payload,
            supersedes_public_id: Some(&current.public_id),
            actor_id: input.actor_id,
            action: "profile.teaching_event.voided",
        },
    )?;
    tx.commit()?;
    Ok(event)
}

pub fn list_teaching_events(
    conn: &Connection,
    class_id: i64,
    range_start: &str,
    range_end: &str,
) -> CoreResult<Vec<ClassTeachingEvent>> {
    let scope = EventPayload {
        event_type: "review",
        title: "scope",
        range_start,
        range_end,
        note: None,
        state: "active",
    };
    validate_payload(&scope)?;
    validate_class(conn, class_id)?;
    let mut stmt = conn.prepare(&format!(
        "{EVENT_SELECT}
         WHERE class_id=?1
           AND revision=(SELECT MAX(latest.revision)
                         FROM class_teaching_event_revisions latest
                         WHERE latest.event_key=class_teaching_event_revisions.event_key)
           AND state='active'
           AND range_start<=?3 AND range_end>=?2
         ORDER BY range_start DESC,range_end DESC,event_key"
    ))?;
    let rows = stmt.query_map(params![class_id, range_start, range_end], event_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn fixture() -> (Connection, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::profile_migrations()).unwrap();
        conn.execute(
            "INSERT INTO classes(name,term,textbook) VALUES ('一班','2026秋','历史')",
            [],
        )
        .unwrap();
        let class_id = conn.last_insert_rowid();
        (conn, class_id)
    }

    #[test]
    fn create_list_and_repeat_are_idempotent() {
        let (mut conn, class_id) = fixture();
        let input = CreateTeachingEventInput {
            request_key: "event-create-1",
            class_id,
            event_type: "review",
            title: "复习洋务运动",
            range_start: "2026-07-16",
            range_end: "2026-07-16",
            note: Some("课堂回顾"),
            actor_id: "teacher-1",
        };
        let first = create_teaching_event(&mut conn, &input).unwrap();
        let repeated = create_teaching_event(&mut conn, &input).unwrap();
        assert_eq!(first, repeated);
        assert_eq!(
            list_teaching_events(&conn, class_id, "2026-07-01", "2026-07-31")
                .unwrap()
                .len(),
            1
        );
        let revision_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM class_teaching_event_revisions",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(revision_count, 1);
    }

    #[test]
    fn revise_and_void_append_revisions_without_overwriting_history() {
        let (mut conn, class_id) = fixture();
        let created = create_teaching_event(
            &mut conn,
            &CreateTeachingEventInput {
                request_key: "event-create-2",
                class_id,
                event_type: "new_lesson",
                title: "洋务运动新课",
                range_start: "2026-07-15",
                range_end: "2026-07-15",
                note: None,
                actor_id: "teacher-1",
            },
        )
        .unwrap();
        let revised = revise_teaching_event(
            &mut conn,
            &ReviseTeachingEventInput {
                request_key: "event-revise-2",
                event_key: &created.event_key,
                expected_revision: 1,
                event_type: "review",
                title: "洋务运动复习",
                range_start: "2026-07-16",
                range_end: "2026-07-16",
                note: Some("修正事件类型"),
                actor_id: "teacher-1",
            },
        )
        .unwrap();
        assert_eq!(revised.revision, 2);
        let voided = void_teaching_event(
            &mut conn,
            &VoidTeachingEventInput {
                request_key: "event-void-2",
                event_key: &created.event_key,
                expected_revision: 2,
                actor_id: "teacher-1",
            },
        )
        .unwrap();
        assert_eq!(voided.revision, 3);
        assert_eq!(voided.state, "voided");
        assert!(
            list_teaching_events(&conn, class_id, "2026-07-01", "2026-07-31")
                .unwrap()
                .is_empty()
        );
        let revisions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM class_teaching_event_revisions WHERE event_key=?1",
                [&created.event_key],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(revisions, 3);
        assert!(conn
            .execute(
                "UPDATE class_teaching_event_revisions SET title='改写' WHERE public_id=?1",
                [&created.public_id],
            )
            .is_err());
    }

    #[test]
    fn stale_revision_and_reused_request_with_other_payload_are_rejected() {
        let (mut conn, class_id) = fixture();
        let created = create_teaching_event(
            &mut conn,
            &CreateTeachingEventInput {
                request_key: "event-create-3",
                class_id,
                event_type: "quiz",
                title: "随堂测验",
                range_start: "2026-07-17",
                range_end: "2026-07-17",
                note: None,
                actor_id: "teacher-1",
            },
        )
        .unwrap();
        let changed_request = create_teaching_event(
            &mut conn,
            &CreateTeachingEventInput {
                request_key: "event-create-3",
                class_id,
                event_type: "exam",
                title: "月考",
                range_start: "2026-07-17",
                range_end: "2026-07-17",
                note: None,
                actor_id: "teacher-1",
            },
        );
        assert!(changed_request.is_err());
        let stale = revise_teaching_event(
            &mut conn,
            &ReviseTeachingEventInput {
                request_key: "event-revise-stale",
                event_key: &created.event_key,
                expected_revision: 9,
                event_type: "quiz",
                title: "随堂测验",
                range_start: "2026-07-17",
                range_end: "2026-07-17",
                note: None,
                actor_id: "teacher-1",
            },
        );
        assert!(stale.is_err());
    }

    #[test]
    fn event_writes_audit_and_outbox_but_no_learning_or_profile_rows() {
        let (mut conn, class_id) = fixture();
        create_teaching_event(
            &mut conn,
            &CreateTeachingEventInput {
                request_key: "event-create-4",
                class_id,
                event_type: "holiday",
                title: "学校放假",
                range_start: "2026-07-18",
                range_end: "2026-07-19",
                note: None,
                actor_id: "teacher-1",
            },
        )
        .unwrap();
        let audit_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM audit_events WHERE action='profile.teaching_event.created'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let outbox_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM outbox_events WHERE event_type='class_teaching_event_revised'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let evidence_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM learning_evidence", [], |row| {
                row.get(0)
            })
            .unwrap();
        let snapshot_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM class_profile_snapshots", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(
            (audit_count, outbox_count, evidence_count, snapshot_count),
            (1, 1, 0, 0)
        );
    }
}
