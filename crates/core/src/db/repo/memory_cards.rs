//! `memory_cards` 仓储：间隔重复卡片（背诵/错题共用）。
//! 业务编排（何时前进/脱档）在 services 层；这里只做读写。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;
use crate::models::{CardState, MemoryCard, ModuleKey};

fn parse_state(s: &str) -> CardState {
    match s {
        "review" => CardState::Review,
        "lapsed" => CardState::Lapsed,
        _ => CardState::Learning,
    }
}

fn state_str(s: CardState) -> &'static str {
    match s {
        CardState::Learning => "learning",
        CardState::Review => "review",
        CardState::Lapsed => "lapsed",
    }
}

fn row_to_card(r: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryCard> {
    Ok(MemoryCard {
        id: r.get("id")?,
        module: ModuleKey::from_db(&r.get::<_, String>("module")?),
        student_id: r.get("student_id")?,
        ref_type: r.get("ref_type")?,
        ref_id: r.get("ref_id")?,
        state: parse_state(&r.get::<_, String>("state")?),
        stage: r.get("stage")?,
        interval_days: r.get("interval_days")?,
        ease: r.get("ease")?,
        last_quality: r.get("last_quality")?,
        last_reviewed_at: r.get("last_reviewed_at")?,
        due_date: r.get("due_date")?,
        reps: r.get("reps")?,
        lapses: r.get("lapses")?,
    })
}

const SELECT_COLS: &str = "id, module, student_id, ref_type, ref_id, state, stage, \
    interval_days, ease, last_quality, last_reviewed_at, due_date, reps, lapses";

pub fn get(
    conn: &Connection,
    module: ModuleKey,
    student_id: i64,
    ref_type: &str,
    ref_id: i64,
) -> CoreResult<Option<MemoryCard>> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM memory_cards \
         WHERE module=?1 AND student_id=?2 AND ref_type=?3 AND ref_id=?4"
    );
    let c = conn
        .query_row(&sql, (module.as_str(), student_id, ref_type, ref_id), row_to_card)
        .optional()?;
    Ok(c)
}

/// 一次复习后的卡片业务状态。
pub struct CardUpdate<'a> {
    pub module: ModuleKey,
    pub student_id: i64,
    pub ref_type: &'a str,
    pub ref_id: i64,
    pub state: CardState,
    pub stage: i32,
    pub interval_days: i32,
    pub quality: &'a str,
    pub due_date: &'a str,
}

/// 通过后的排程结果落库：只增加 reps，保留 lapses。
pub fn upsert_after_pass(conn: &Connection, u: &CardUpdate<'_>) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO memory_cards
            (module, student_id, ref_type, ref_id, state, stage, interval_days,
             last_quality, last_reviewed_at, due_date, reps, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8, datetime('now'), ?9, 1, datetime('now'))
         ON CONFLICT(module, student_id, ref_type, ref_id) DO UPDATE SET
            state = excluded.state,
            stage = excluded.stage,
            interval_days = excluded.interval_days,
            last_quality = excluded.last_quality,
            last_reviewed_at = datetime('now'),
            due_date = excluded.due_date,
            reps = memory_cards.reps + 1,
            updated_at = datetime('now')",
        (
            u.module.as_str(),
            u.student_id,
            u.ref_type,
            u.ref_id,
            state_str(u.state),
            u.stage,
            u.interval_days,
            u.quality,
            u.due_date,
        ),
    )?;
    Ok(())
}

/// 未达标后的脱档结果落库：只增加 lapses，保留 reps。
pub fn upsert_after_lapse(conn: &Connection, u: &CardUpdate<'_>) -> CoreResult<()> {
    conn.execute(
        "INSERT INTO memory_cards
            (module, student_id, ref_type, ref_id, state, stage, interval_days,
             last_quality, last_reviewed_at, due_date, lapses, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8, datetime('now'), ?9, 1, datetime('now'))
         ON CONFLICT(module, student_id, ref_type, ref_id) DO UPDATE SET
            state = excluded.state,
            stage = excluded.stage,
            interval_days = excluded.interval_days,
            last_quality = excluded.last_quality,
            last_reviewed_at = datetime('now'),
            due_date = excluded.due_date,
            lapses = memory_cards.lapses + 1,
            updated_at = datetime('now')",
        (
            u.module.as_str(),
            u.student_id,
            u.ref_type,
            u.ref_id,
            state_str(u.state),
            u.stage,
            u.interval_days,
            u.quality,
            u.due_date,
        ),
    )?;
    Ok(())
}

/// 恢复一次人工判定应用前的完整卡片业务状态。
pub fn restore(
    conn: &Connection,
    module: ModuleKey,
    student_id: i64,
    ref_type: &str,
    ref_id: i64,
    snapshot: Option<&MemoryCard>,
) -> CoreResult<()> {
    if let Some(card) = snapshot {
        conn.execute(
            "INSERT INTO memory_cards
                (id, module, student_id, ref_type, ref_id, state, stage, interval_days,
                 ease, last_quality, last_reviewed_at, due_date, reps, lapses, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,datetime('now'))
             ON CONFLICT(module, student_id, ref_type, ref_id) DO UPDATE SET
                state=excluded.state,
                stage=excluded.stage,
                interval_days=excluded.interval_days,
                ease=excluded.ease,
                last_quality=excluded.last_quality,
                last_reviewed_at=excluded.last_reviewed_at,
                due_date=excluded.due_date,
                reps=excluded.reps,
                lapses=excluded.lapses,
                updated_at=datetime('now')",
            (
                card.id,
                card.module.as_str(),
                card.student_id,
                card.ref_type.as_str(),
                card.ref_id,
                state_str(card.state),
                card.stage,
                card.interval_days,
                card.ease,
                card.last_quality.as_deref(),
                card.last_reviewed_at.as_deref(),
                card.due_date.as_deref(),
                card.reps,
                card.lapses,
            ),
        )?;
    } else {
        conn.execute(
            "DELETE FROM memory_cards
             WHERE module=?1 AND student_id=?2 AND ref_type=?3 AND ref_id=?4",
            (module.as_str(), student_id, ref_type, ref_id),
        )?;
    }
    Ok(())
}

/// 到期可复习的卡片：due_date <= today 且 state='review'。
pub fn due(conn: &Connection, module: ModuleKey, today: &str) -> CoreResult<Vec<MemoryCard>> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM memory_cards \
         WHERE module=?1 AND state='review' AND due_date IS NOT NULL AND due_date <= ?2 \
         ORDER BY due_date"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map((module.as_str(), today), row_to_card)?;
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

    #[test]
    fn upsert_and_due_query() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let stu = upsert_student(
            &conn,
            &StudentInput { student_no: "2023001", name: "张三", class_id: None, enabled: true },
        )
        .unwrap();

        upsert_after_pass(
            &conn,
            &CardUpdate {
                module: ModuleKey::Recitation,
                student_id: stu.id,
                ref_type: "content",
                ref_id: 12,
                state: CardState::Review,
                stage: 2,
                interval_days: 4,
                quality: "A",
                due_date: "2026-06-20",
            },
        )
        .unwrap();

        // 到期（今天 >= due_date）能查到
        let d = due(&conn, ModuleKey::Recitation, "2026-06-25").unwrap();
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].stage, 2);
        assert_eq!(d[0].reps, 1);

        // 未到期查不到
        let none = due(&conn, ModuleKey::Recitation, "2026-06-19").unwrap();
        assert!(none.is_empty());

        // 再次复习 → reps 自增、stage 推进
        upsert_after_pass(
            &conn,
            &CardUpdate {
                module: ModuleKey::Recitation,
                student_id: stu.id,
                ref_type: "content",
                ref_id: 12,
                state: CardState::Review,
                stage: 4,
                interval_days: 15,
                quality: "A",
                due_date: "2026-07-10",
            },
        )
        .unwrap();
        let card = get(&conn, ModuleKey::Recitation, stu.id, "content", 12)
            .unwrap()
            .unwrap();
        assert_eq!(card.reps, 2);
        assert_eq!(card.stage, 4);
    }
}
