//! 数据库层：连接、迁移运行器、通用表仓储。
//!
//! 单库（SQLite）。`run_migrations` 按 id 幂等执行，已应用的跳过。
//! 应用外壳用 `open` 打开应用数据目录下的 `data.db` 并用 `Mutex` 共享连接；
//! 模块通过 `ports::Module::migrations()` 提供自己的迁移，外壳汇总后一并运行。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;
use crate::ports::Migration;

pub mod repo;

/// Core 通用表迁移（外壳应在各模块迁移之前先运行它）。
pub static CORE_MIGRATIONS: &[Migration] = &[
    Migration {
        id: "core_0001",
        sql: include_str!("schema/0001_core.sql"),
    },
    Migration {
        id: "core_0002",
        sql: include_str!("schema/0002_class_textbook.sql"),
    },
    Migration {
        id: "core_0003",
        sql: include_str!("schema/0003_decision_effects.sql"),
    },
    Migration {
        id: "core_0004",
        sql: include_str!("schema/0004_memory_card_counters.sql"),
    },
    Migration {
        id: "core_0005",
        sql: include_str!("schema/0005_artifacts.sql"),
    },
    Migration {
        id: "core_0006",
        sql: include_str!("schema/0006_ai_runs_and_jobs.sql"),
    },
    Migration {
        id: "core_0007",
        sql: include_str!("schema/0007_evidence_outbox_audit.sql"),
    },
];

/// 打开磁盘数据库并开启外键。
pub fn open(path: &std::path::Path) -> CoreResult<Connection> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

/// 打开内存数据库（测试用）。
pub fn open_in_memory() -> CoreResult<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", true)?;
    Ok(conn)
}

/// 幂等执行迁移：未应用的按顺序执行并登记。
pub fn run_migrations(conn: &Connection, migrations: &[Migration]) -> CoreResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            id TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        )",
    )?;
    for m in migrations {
        let applied: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE id = ?1",
                [m.id],
                |r| r.get(0),
            )
            .optional()?;
        if applied.is_none() {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(m.sql)?;
            tx.execute("INSERT INTO schema_migrations (id) VALUES (?1)", [m.id])?;
            tx.commit()?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_idempotent() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        // 再跑一次不应报错
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 7);
        // 关键表存在
        let t: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='memory_cards'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(t, 1);
        let effects: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='decision_effects'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(effects, 1);
        let artifacts: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='artifacts'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(artifacts, 1);
        let shared_runtime_tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE type='table' AND name IN ('ai_runs', 'background_jobs')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(shared_runtime_tables, 2);
        let evidence_event_tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE type='table' AND name IN (
                     'learning_evidence', 'outbox_events', 'outbox_consumptions', 'audit_events'
                 )",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(evidence_event_tables, 4);
    }

    #[test]
    fn failed_migration_rolls_back_schema_and_registration() {
        static BROKEN: &[Migration] = &[Migration {
            id: "atomic_probe",
            sql: "CREATE TABLE atomic_probe (id INTEGER PRIMARY KEY);
                  INSERT INTO definitely_missing_table VALUES (1);",
        }];
        static FIXED: &[Migration] = &[Migration {
            id: "atomic_probe",
            sql: "CREATE TABLE atomic_probe (id INTEGER PRIMARY KEY);",
        }];

        let conn = open_in_memory().unwrap();
        assert!(run_migrations(&conn, BROKEN).is_err());
        let table_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='atomic_probe')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let registered: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE id='atomic_probe')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!table_exists);
        assert!(!registered);

        run_migrations(&conn, FIXED).unwrap();
        let table_exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='atomic_probe')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(table_exists);
    }

    #[test]
    fn artifact_migration_preserves_legacy_submission_without_guessing() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, &CORE_MIGRATIONS[..4]).unwrap();
        conn.execute(
            "INSERT INTO submissions (module, media_type, file_path, archived_path, file_hash)
             VALUES ('recitation', 'audio', '/legacy.wav', '/archive/legacy.wav', 'legacy')",
            [],
        )
        .unwrap();
        let submission_id = conn.last_insert_rowid();

        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let artifact_id: Option<i64> = conn
            .query_row(
                "SELECT artifact_id FROM submissions WHERE id=?1",
                [submission_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(artifact_id, None);
        let applied: i64 = conn
            .query_row(
                "SELECT count(*) FROM schema_migrations WHERE id='core_0005'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(applied, 1);

        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let rows: i64 = conn
            .query_row("SELECT count(*) FROM submissions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn counter_migration_repairs_one_provable_legacy_lapse() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, &CORE_MIGRATIONS[..3]).unwrap();
        conn.execute(
            "INSERT INTO students (student_no, name) VALUES ('legacy', '旧数据')",
            [],
        )
        .unwrap();
        let student_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO memory_cards
                (module, student_id, ref_type, ref_id, state, stage, interval_days, reps, lapses)
             VALUES ('recitation', ?1, 'content', 7, 'lapsed', 0, 0, 3, 0)",
            [student_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memory_cards
                (module, student_id, ref_type, ref_id, state, stage, interval_days, reps, lapses)
             VALUES ('recitation', ?1, 'content', 8, 'lapsed', 0, 0, 1, 0)",
            [student_id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO submissions
                (module, student_id, ref_id, media_type, file_path, file_hash)
             VALUES ('recitation', ?1, 8, 'audio', '/legacy.m4a', 'legacy-hash')",
            [student_id],
        )
        .unwrap();
        let submission_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO verdicts (submission_id, module, pass, answer_version)
             VALUES (?1, 'recitation', 0, 1)",
            [submission_id],
        )
        .unwrap();
        let verdict_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO decision_effects
                (verdict_id, revision, module, student_id, ref_type, ref_id, result,
                 card_before_json, task_before_json, card_after_json, task_after_json,
                 created_makeup_task_ids_json)
             VALUES (?1, 1, 'recitation', ?2, 'content', 8, 'fail',
                     'null', '[]', '{}', '[]', '[]')",
            (verdict_id, student_id),
        )
        .unwrap();

        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let counters: (i64, i64) = conn
            .query_row(
                "SELECT reps, lapses FROM memory_cards WHERE student_id=?1 AND ref_id=7",
                [student_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counters, (2, 1));
        let ledger_protected: (i64, i64) = conn
            .query_row(
                "SELECT reps, lapses FROM memory_cards WHERE student_id=?1 AND ref_id=8",
                [student_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(ledger_protected, (1, 0));

        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let counters_again: (i64, i64) = conn
            .query_row(
                "SELECT reps, lapses FROM memory_cards WHERE student_id=?1 AND ref_id=7",
                [student_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counters_again, counters);
    }
}
