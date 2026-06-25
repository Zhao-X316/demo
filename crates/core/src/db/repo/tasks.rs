//! `tasks` 仓储：通用任务（新背/补背/复习）。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;
use crate::models::{ModuleKey, Task, TaskKind, TaskStatus};

fn kind_str(k: TaskKind) -> &'static str {
    match k {
        TaskKind::Normal => "normal",
        TaskKind::Makeup => "makeup",
        TaskKind::Review => "review",
    }
}
fn kind_from(s: &str) -> TaskKind {
    match s {
        "makeup" => TaskKind::Makeup,
        "review" => TaskKind::Review,
        _ => TaskKind::Normal,
    }
}
fn status_str(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Open => "open",
        TaskStatus::Submitted => "submitted",
        TaskStatus::Passed => "passed",
        TaskStatus::Failed => "failed",
        TaskStatus::Reopened => "reopened",
        TaskStatus::Closed => "closed",
        TaskStatus::Expired => "expired",
    }
}
fn status_from(s: &str) -> TaskStatus {
    match s {
        "submitted" => TaskStatus::Submitted,
        "passed" => TaskStatus::Passed,
        "failed" => TaskStatus::Failed,
        "reopened" => TaskStatus::Reopened,
        "closed" => TaskStatus::Closed,
        "expired" => TaskStatus::Expired,
        _ => TaskStatus::Open,
    }
}

const COLS: &str = "id, module, student_id, subject_id, ref_type, ref_id, kind, due_date, \
    status, source_task_id, card_id";

fn row_to_task(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get("id")?,
        module: ModuleKey::from_db(&r.get::<_, String>("module")?),
        student_id: r.get("student_id")?,
        subject_id: r.get("subject_id")?,
        ref_type: r.get("ref_type")?,
        ref_id: r.get("ref_id")?,
        kind: kind_from(&r.get::<_, String>("kind")?),
        due_date: r.get("due_date")?,
        status: status_from(&r.get::<_, String>("status")?),
        source_task_id: r.get("source_task_id")?,
        card_id: r.get("card_id")?,
    })
}

pub struct NewTask<'a> {
    pub module: ModuleKey,
    pub student_id: i64,
    pub subject_id: Option<i64>,
    pub ref_type: &'a str,
    pub ref_id: i64,
    pub kind: TaskKind,
    pub due_date: &'a str,
    pub source_task_id: Option<i64>,
    pub card_id: Option<i64>,
}

/// 插入任务；唯一键冲突（同 module+student+ref+due+kind）则忽略并返回既有 id。
pub fn insert(conn: &Connection, t: &NewTask<'_>) -> CoreResult<i64> {
    conn.execute(
        "INSERT OR IGNORE INTO tasks
            (module, student_id, subject_id, ref_type, ref_id, kind, due_date, source_task_id, card_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        (
            t.module.as_str(), t.student_id, t.subject_id, t.ref_type, t.ref_id,
            kind_str(t.kind), t.due_date, t.source_task_id, t.card_id,
        ),
    )?;
    // 取回 id（无论是新插入还是已存在）
    let id: i64 = conn.query_row(
        "SELECT id FROM tasks WHERE module=?1 AND student_id=?2 AND ref_type=?3 AND ref_id=?4 \
         AND due_date=?5 AND kind=?6",
        (t.module.as_str(), t.student_id, t.ref_type, t.ref_id, t.due_date, kind_str(t.kind)),
        |r| r.get(0),
    )?;
    Ok(id)
}

pub fn get(conn: &Connection, id: i64) -> CoreResult<Option<Task>> {
    let sql = format!("SELECT {COLS} FROM tasks WHERE id=?1");
    Ok(conn.query_row(&sql, [id], row_to_task).optional()?)
}

pub fn set_status(conn: &Connection, id: i64, status: TaskStatus) -> CoreResult<()> {
    conn.execute(
        "UPDATE tasks SET status=?2, updated_at=datetime('now') WHERE id=?1",
        (id, status_str(status)),
    )?;
    Ok(())
}

/// 导入匹配：找该 学生+内容+日期 下任意未关闭任务（open/submitted/reopened），优先匹配。
/// 改派用：不限日期，找该 学生+内容 最近一条未关闭任务。
pub fn find_latest_open(
    conn: &Connection,
    module: ModuleKey,
    student_id: i64,
    ref_id: i64,
) -> CoreResult<Option<Task>> {
    let sql = format!(
        "SELECT {COLS} FROM tasks WHERE module=?1 AND student_id=?2 AND ref_id=?3 \
         AND status IN ('open','submitted','reopened') ORDER BY due_date DESC, id DESC LIMIT 1"
    );
    Ok(conn
        .query_row(&sql, (module.as_str(), student_id, ref_id), row_to_task)
        .optional()?)
}

pub fn find_open_match(
    conn: &Connection,
    module: ModuleKey,
    student_id: i64,
    ref_id: i64,
    due_date: &str,
) -> CoreResult<Option<Task>> {
    let sql = format!(
        "SELECT {COLS} FROM tasks WHERE module=?1 AND student_id=?2 AND ref_id=?3 \
         AND due_date=?4 AND status IN ('open','submitted','reopened') ORDER BY id LIMIT 1"
    );
    Ok(conn
        .query_row(&sql, (module.as_str(), student_id, ref_id, due_date), row_to_task)
        .optional()?)
}

/// 去重护栏：该 学生+内容 是否已有未关闭的指定种类任务。
pub fn exists_open_kind(
    conn: &Connection,
    module: ModuleKey,
    student_id: i64,
    ref_id: i64,
    kind: TaskKind,
) -> CoreResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT count(*) FROM tasks WHERE module=?1 AND student_id=?2 AND ref_id=?3 \
         AND kind=?4 AND status IN ('open','submitted','reopened')",
        (module.as_str(), student_id, ref_id, kind_str(kind)),
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

/// 关闭某 学生+内容 下所有未关闭的指定种类任务（如人工改判后作废补背）。返回关闭数量。
pub fn close_open_kind(
    conn: &Connection,
    module: ModuleKey,
    student_id: i64,
    ref_id: i64,
    kind: TaskKind,
) -> CoreResult<usize> {
    let n = conn.execute(
        "UPDATE tasks SET status='closed', updated_at=datetime('now')
         WHERE module=?1 AND student_id=?2 AND ref_id=?3 AND kind=?4
           AND status IN ('open','submitted','reopened')",
        (module.as_str(), student_id, ref_id, kind_str(kind)),
    )?;
    Ok(n)
}

/// 今日任务列表。
pub fn list_by_date(conn: &Connection, module: ModuleKey, date: &str) -> CoreResult<Vec<Task>> {
    let sql = format!("SELECT {COLS} FROM tasks WHERE module=?1 AND due_date=?2 ORDER BY kind, id");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map((module.as_str(), date), row_to_task)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 日切：早于 today 仍 open 的任务（用于结转/过期）。
pub fn list_open_before(conn: &Connection, module: ModuleKey, today: &str) -> CoreResult<Vec<Task>> {
    let sql = format!(
        "SELECT {COLS} FROM tasks WHERE module=?1 AND status='open' AND due_date < ?2 ORDER BY due_date"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map((module.as_str(), today), row_to_task)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::students::{upsert as upsert_student, StudentInput};
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> (Connection, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let s = upsert_student(
            &conn,
            &StudentInput { student_no: "2023001", name: "张三", class_id: None, enabled: true },
        )
        .unwrap();
        (conn, s.id)
    }

    #[test]
    fn insert_is_idempotent_and_matchable() {
        let (conn, sid) = setup();
        let nt = NewTask {
            module: ModuleKey::Recitation, student_id: sid, subject_id: None,
            ref_type: "content", ref_id: 12, kind: TaskKind::Normal,
            due_date: "2026-06-25", source_task_id: None, card_id: None,
        };
        let id1 = insert(&conn, &nt).unwrap();
        let id2 = insert(&conn, &nt).unwrap(); // 唯一键冲突 → 同 id
        assert_eq!(id1, id2);

        let m = find_open_match(&conn, ModuleKey::Recitation, sid, 12, "2026-06-25").unwrap();
        assert_eq!(m.unwrap().id, id1);
    }

    #[test]
    fn makeup_dedup_guard() {
        let (conn, sid) = setup();
        assert!(!exists_open_kind(&conn, ModuleKey::Recitation, sid, 12, TaskKind::Makeup).unwrap());
        insert(
            &conn,
            &NewTask {
                module: ModuleKey::Recitation, student_id: sid, subject_id: None,
                ref_type: "content", ref_id: 12, kind: TaskKind::Makeup,
                due_date: "2026-06-26", source_task_id: Some(1), card_id: None,
            },
        )
        .unwrap();
        assert!(exists_open_kind(&conn, ModuleKey::Recitation, sid, 12, TaskKind::Makeup).unwrap());
    }

    #[test]
    fn rollover_finds_stale_open() {
        let (conn, sid) = setup();
        insert(
            &conn,
            &NewTask {
                module: ModuleKey::Recitation, student_id: sid, subject_id: None,
                ref_type: "content", ref_id: 12, kind: TaskKind::Normal,
                due_date: "2026-06-20", source_task_id: None, card_id: None,
            },
        )
        .unwrap();
        let stale = list_open_before(&conn, ModuleKey::Recitation, "2026-06-25").unwrap();
        assert_eq!(stale.len(), 1);
    }
}
