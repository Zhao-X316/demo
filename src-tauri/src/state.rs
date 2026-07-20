//! 应用状态：共享 SQLite 连接（启动时建库 + 跑迁移）+ 数据目录。

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};
use suite_core::error::CoreResult;
use suite_core::ports::Migration;
use tauri::{App, Manager};

use crate::backup::{self, BackupKind};

pub struct AppState {
    pub db: Mutex<Connection>,
    /// 应用数据目录（放 data.db 与 secrets.json）。
    pub data_dir: PathBuf,
}

pub fn run_all_migrations(conn: &Connection) -> CoreResult<()> {
    suite_core::db::run_migrations(conn, suite_core::db::CORE_MIGRATIONS)?;
    suite_core::db::run_migrations(conn, module_knowledge::knowledge_migrations())?;
    suite_core::db::run_migrations(conn, module_recitation::recitation_migrations())?;
    suite_core::db::run_migrations(conn, module_exam::exam_migrations())?;
    suite_core::db::run_migrations(conn, module_wrongbook::wrongbook_migrations())?;
    suite_core::db::run_migrations(conn, module_profile::profile_migrations())?;
    Ok(())
}

/// 将数据库中已经登记过的媒体文件按“精确路径”加入 asset 协议白名单。
///
/// 新导入的录音会进入 `$APPDATA/archive`，由静态 scope 覆盖；这里主要兼容
/// A10 归档能力上线前只保存原始路径的历史数据。只放行数据库中真实存在的
/// 单个文件，不恢复过去的全盘通配符权限。
pub fn allow_existing_media<R: tauri::Runtime, M: Manager<R>>(
    manager: &M,
    conn: &Connection,
) -> Result<usize, String> {
    let mut stmt = conn
        .prepare("SELECT file_path, archived_path FROM submissions")
        .map_err(|err| err.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })
        .map_err(|err| err.to_string())?;
    let scope = manager.asset_protocol_scope();
    let mut allowed = 0;

    for row in rows {
        let (file_path, archived_path) = row.map_err(|err| err.to_string())?;
        for path in archived_path
            .as_deref()
            .into_iter()
            .chain([file_path.as_str()])
        {
            let path = std::path::Path::new(path);
            if path.is_file() {
                scope.allow_file(path).map_err(|err| err.to_string())?;
                allowed += 1;
            }
        }
    }

    Ok(allowed)
}

fn group_has_pending(conn: &Connection, migrations: &[Migration]) -> CoreResult<bool> {
    let has_table: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='schema_migrations')",
        [],
        |row| row.get(0),
    )?;
    if !has_table {
        return Ok(!migrations.is_empty());
    }
    for migration in migrations {
        let applied: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM schema_migrations WHERE id=?1",
                [migration.id],
                |row| row.get(0),
            )
            .optional()?;
        if applied.is_none() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn has_pending_migrations(conn: &Connection) -> CoreResult<bool> {
    Ok(group_has_pending(conn, suite_core::db::CORE_MIGRATIONS)?
        || group_has_pending(conn, module_knowledge::knowledge_migrations())?
        || group_has_pending(conn, module_recitation::recitation_migrations())?
        || group_has_pending(conn, module_exam::exam_migrations())?
        || group_has_pending(conn, module_wrongbook::wrongbook_migrations())?
        || group_has_pending(conn, module_profile::profile_migrations())?)
}

fn open_managed_database(
    db_path: &std::path::Path,
    backup_dir: &std::path::Path,
) -> CoreResult<Connection> {
    let had_data = db_path
        .metadata()
        .map(|meta| meta.len() > 0)
        .unwrap_or(false);
    let conn = suite_core::db::open(db_path)?;
    if had_data && has_pending_migrations(&conn)? {
        backup::create_backup(&conn, backup_dir, BackupKind::PreMigration)?;
    }
    run_all_migrations(&conn)?;
    backup::ensure_daily_backup(&conn, backup_dir)?;
    Ok(conn)
}

/// 在应用数据目录打开 data.db；现有库迁移前先备份，迁移后确保当天快照存在。
pub fn init(app: &App) -> Result<AppState, Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    let db_path = dir.join("data.db");
    let backup_dir = dir.join("backups");

    let conn = open_managed_database(&db_path, &backup_dir)?;
    let recovered = module_recitation::service::recognition::recover_stale_processing(&conn)?;
    if recovered > 0 {
        eprintln!("[ASR恢复] {recovered} 条中断的 processing 已转为 failed");
    }
    let allowed = allow_existing_media(app, &conn).map_err(std::io::Error::other)?;
    if allowed > 0 {
        eprintln!("[媒体白名单] 已按精确路径放行 {allowed} 个历史媒体文件");
    }

    Ok(AppState {
        db: Mutex::new(conn),
        data_dir: dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_creates_pre_migration_then_only_one_daily_backup() {
        let root =
            std::env::temp_dir().join(format!("jiaofu-startup-backup-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let db_path = root.join("data.db");
        let backup_dir = root.join("backups");
        {
            let conn = suite_core::db::open(&db_path).unwrap();
            suite_core::db::run_migrations(&conn, &suite_core::db::CORE_MIGRATIONS[..3]).unwrap();
        }

        drop(open_managed_database(&db_path, &backup_dir).unwrap());
        let first = backup::list_backups(&backup_dir).unwrap();
        assert_eq!(
            first
                .items
                .iter()
                .filter(|item| item.kind == "pre-migration")
                .count(),
            1
        );
        assert_eq!(
            first
                .items
                .iter()
                .filter(|item| item.kind == "daily")
                .count(),
            1
        );

        drop(open_managed_database(&db_path, &backup_dir).unwrap());
        let second = backup::list_backups(&backup_dir).unwrap();
        assert_eq!(second.items.len(), first.items.len());
    }
}
