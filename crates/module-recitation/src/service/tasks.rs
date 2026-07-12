//! 任务生成：补背、到期复习、日切结转（见 背诵批改系统 §9）。

use chrono::NaiveDate;
use rusqlite::Connection;
use serde::Serialize;
use suite_core::db::repo::{memory_cards, tasks};
use suite_core::error::CoreResult;
use suite_core::models::{ModuleKey, TaskKind, TaskStatus};

const MODULE: ModuleKey = ModuleKey::Recitation;
const REF_TYPE: &str = "content";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GenerateNormalReport {
    pub created: usize,
    pub revived: usize,
    pub skipped_open: usize,
    pub skipped_completed: usize,
    pub task_ids: Vec<i64>,
}

/// 生成补背任务（去重：同 学生+内容 已有未关闭补背则不新建）。返回新任务 id 或 None。
pub fn ensure_makeup(
    conn: &Connection,
    student_id: i64,
    content_id: i64,
    source_task_id: i64,
    due_date: &str,
) -> CoreResult<Option<i64>> {
    if tasks::exists_open_kind(conn, MODULE, student_id, content_id, TaskKind::Makeup)? {
        return Ok(None);
    }
    let id = tasks::insert(
        conn,
        &tasks::NewTask {
            module: MODULE,
            student_id,
            subject_id: None,
            ref_type: REF_TYPE,
            ref_id: content_id,
            kind: TaskKind::Makeup,
            due_date,
            source_task_id: Some(source_task_id),
            card_id: None,
        },
    )?;
    Ok(Some(id))
}

/// 批量生成"新背"任务：为给定 (学生, 内容) 列表在某日期建 normal 任务（幂等）。
pub fn generate_normal(
    conn: &Connection,
    due_date: &str,
    pairs: &[(i64, i64)],
) -> CoreResult<GenerateNormalReport> {
    let mut out = GenerateNormalReport::default();
    for &(student_id, content_id) in pairs {
        if let Some(existing) = tasks::find_by_unique_key(
            conn,
            MODULE,
            student_id,
            content_id,
            due_date,
            TaskKind::Normal,
        )? {
            match existing.status {
                TaskStatus::Closed => {
                    let id = tasks::insert(
                        conn,
                        &tasks::NewTask {
                            module: MODULE,
                            student_id,
                            subject_id: None,
                            ref_type: REF_TYPE,
                            ref_id: content_id,
                            kind: TaskKind::Normal,
                            due_date,
                            source_task_id: None,
                            card_id: None,
                        },
                    )?;
                    out.revived += 1;
                    out.task_ids.push(id);
                }
                TaskStatus::Open | TaskStatus::Submitted | TaskStatus::Reopened => {
                    out.skipped_open += 1;
                }
                TaskStatus::Passed | TaskStatus::Failed | TaskStatus::Expired => {
                    out.skipped_completed += 1;
                }
            }
            continue;
        }
        // 学生已欠这篇（有未完成的新背/补背/复习）就跳过，避免「新背 + 补背」同篇重复布置
        if tasks::exists_open_any(conn, MODULE, student_id, content_id)? {
            out.skipped_open += 1;
            continue;
        }
        let id = tasks::insert(
            conn,
            &tasks::NewTask {
                module: MODULE,
                student_id,
                subject_id: None,
                ref_type: REF_TYPE,
                ref_id: content_id,
                kind: TaskKind::Normal,
                due_date,
                source_task_id: None,
                card_id: None,
            },
        )?;
        out.created += 1;
        out.task_ids.push(id);
    }
    Ok(out)
}

/// 到期复习生成：扫描到期记忆卡片，为每张生成一条复习任务（去重）。
pub fn generate_due_reviews(conn: &Connection, today: &str) -> CoreResult<Vec<i64>> {
    let cards = memory_cards::due(conn, MODULE, today)?;
    let mut out = Vec::new();
    for c in cards {
        if tasks::exists_open_kind(conn, MODULE, c.student_id, c.ref_id, TaskKind::Review)? {
            continue;
        }
        let id = tasks::insert(
            conn,
            &tasks::NewTask {
                module: MODULE,
                student_id: c.student_id,
                subject_id: None,
                ref_type: REF_TYPE,
                ref_id: c.ref_id,
                kind: TaskKind::Review,
                due_date: today,
                source_task_id: None,
                card_id: Some(c.id),
            },
        )?;
        out.push(id);
    }
    Ok(out)
}

/// 日切：早于今天仍 open 的非补背任务 → expired，并结转为今天到期的补背。
/// 已有补背（包括更早已逾期的补背）保持原日期与 open 状态，不重复滚动。
pub fn rollover(conn: &Connection, today: NaiveDate) -> CoreResult<usize> {
    let today_str = today.format("%Y-%m-%d").to_string();
    let stale: Vec<_> = tasks::list_open_before(conn, MODULE, &today_str)?
        .into_iter()
        .filter(|task| task.kind != TaskKind::Makeup)
        .collect();
    for t in &stale {
        tasks::set_status(conn, t.id, TaskStatus::Expired)?;
        ensure_makeup(conn, t.student_id, t.ref_id, t.id, &today_str)?;
    }
    Ok(stale.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::repo::memory_cards::{upsert_after_pass, CardUpdate};
    use suite_core::db::repo::students::{upsert as upsert_student, StudentInput};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::CardState;

    fn setup() -> (Connection, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::recitation_migrations()).unwrap();
        let s = upsert_student(
            &conn,
            &StudentInput {
                student_no: "2023001",
                name: "张三",
                class_id: None,
                enabled: true,
            },
        )
        .unwrap();
        (conn, s.id)
    }

    #[test]
    fn makeup_is_deduped() {
        let (conn, sid) = setup();
        let a = ensure_makeup(&conn, sid, 12, 1, "2026-06-26").unwrap();
        assert!(a.is_some());
        let b = ensure_makeup(&conn, sid, 12, 1, "2026-06-26").unwrap();
        assert!(b.is_none()); // 已存在未关闭补背
    }

    #[test]
    fn due_reviews_generated_once() {
        let (conn, sid) = setup();
        upsert_after_pass(
            &conn,
            &CardUpdate {
                module: MODULE,
                student_id: sid,
                ref_type: REF_TYPE,
                ref_id: 7,
                state: CardState::Review,
                stage: 2,
                interval_days: 4,
                quality: "A",
                due_date: "2026-06-20",
            },
        )
        .unwrap();
        let made = generate_due_reviews(&conn, "2026-06-25").unwrap();
        assert_eq!(made.len(), 1);
        // 再跑一次：已有未关闭复习任务 → 不重复
        let again = generate_due_reviews(&conn, "2026-06-25").unwrap();
        assert!(again.is_empty());
    }

    #[test]
    fn normal_generation_reports_completed_duplicates() {
        let (conn, sid) = setup();
        let first = generate_normal(&conn, "2026-06-25", &[(sid, 12)]).unwrap();
        assert_eq!(first.created, 1);
        let id = first.task_ids[0];
        tasks::set_status(&conn, id, TaskStatus::Passed).unwrap();

        let again = generate_normal(&conn, "2026-06-25", &[(sid, 12)]).unwrap();
        assert_eq!(again.created, 0);
        assert_eq!(again.revived, 0);
        assert_eq!(again.skipped_completed, 1);
        assert!(again.task_ids.is_empty());
    }

    #[test]
    fn rollover_expires_and_makes_up_today_without_duplication() {
        let (conn, sid) = setup();
        let tid = tasks::insert(
            &conn,
            &tasks::NewTask {
                module: MODULE,
                student_id: sid,
                subject_id: None,
                ref_type: REF_TYPE,
                ref_id: 9,
                kind: TaskKind::Normal,
                due_date: "2026-06-20",
                source_task_id: None,
                card_id: None,
            },
        )
        .unwrap();
        let n = rollover(&conn, NaiveDate::from_ymd_opt(2026, 6, 25).unwrap()).unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            tasks::get(&conn, tid).unwrap().unwrap().status,
            TaskStatus::Expired
        );
        let scope = tasks::list_for_scope(&conn, MODULE, sid, REF_TYPE, 9).unwrap();
        let makeups: Vec<_> = scope
            .iter()
            .filter(|task| task.kind == TaskKind::Makeup)
            .collect();
        assert_eq!(makeups.len(), 1);
        assert_eq!(makeups[0].due_date, "2026-06-25");
        assert_eq!(makeups[0].status, TaskStatus::Open);
        assert_eq!(makeups[0].source_task_id, Some(tid));
        assert!(tasks::list_by_date(&conn, MODULE, "2026-06-25")
            .unwrap()
            .iter()
            .any(|task| task.id == makeups[0].id));

        let repeated = rollover(&conn, NaiveDate::from_ymd_opt(2026, 6, 25).unwrap()).unwrap();
        assert_eq!(repeated, 0);
        let scope_after = tasks::list_for_scope(&conn, MODULE, sid, REF_TYPE, 9).unwrap();
        assert_eq!(
            scope_after
                .iter()
                .filter(|task| task.kind == TaskKind::Makeup)
                .count(),
            1
        );
    }

    #[test]
    fn rollover_preserves_existing_earlier_makeup() {
        let (conn, sid) = setup();
        let source_id = tasks::insert(
            &conn,
            &tasks::NewTask {
                module: MODULE,
                student_id: sid,
                subject_id: None,
                ref_type: REF_TYPE,
                ref_id: 10,
                kind: TaskKind::Normal,
                due_date: "2026-06-20",
                source_task_id: None,
                card_id: None,
            },
        )
        .unwrap();
        let earlier_makeup = ensure_makeup(&conn, sid, 10, source_id, "2026-06-23")
            .unwrap()
            .unwrap();

        let rolled = rollover(&conn, NaiveDate::from_ymd_opt(2026, 6, 25).unwrap()).unwrap();
        assert_eq!(rolled, 1);
        assert_eq!(
            tasks::get(&conn, source_id).unwrap().unwrap().status,
            TaskStatus::Expired
        );
        let preserved = tasks::get(&conn, earlier_makeup).unwrap().unwrap();
        assert_eq!(preserved.status, TaskStatus::Open);
        assert_eq!(preserved.due_date, "2026-06-23");
        let scope = tasks::list_for_scope(&conn, MODULE, sid, REF_TYPE, 10).unwrap();
        assert_eq!(
            scope
                .iter()
                .filter(|task| task.kind == TaskKind::Makeup)
                .count(),
            1
        );
    }
}
