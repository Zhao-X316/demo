//! SQLite 在线备份与受保护恢复。备份目录只包含 data.db 快照，不包含 secrets.json。

use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, Utc};
use rusqlite::{Connection, DatabaseName};
use serde::Serialize;
use suite_core::error::{CoreError, CoreResult};

pub const RETENTION_LIMIT: usize = 14;
const PREFIX: &str = "jiaofu-";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupKind {
    Daily,
    PreMigration,
    Manual,
    BeforeRestore,
}

impl BackupKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::PreMigration => "pre-migration",
            Self::Manual => "manual",
            Self::BeforeRestore => "before-restore",
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackupInfo {
    pub file_name: String,
    pub created_at: String,
    pub kind: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct BackupCatalog {
    pub items: Vec<BackupInfo>,
    pub retention_limit: usize,
    pub older_retained: usize,
}

fn io_error(context: &str, err: std::io::Error) -> CoreError {
    CoreError::Io(format!("{context}: {err}"))
}

fn now_shanghai() -> DateTime<FixedOffset> {
    let offset = FixedOffset::east_opt(8 * 60 * 60).expect("valid UTC+8 offset");
    Utc::now().with_timezone(&offset)
}

fn timestamp(now: DateTime<FixedOffset>) -> String {
    now.format("%Y%m%dT%H%M%S%z").to_string()
}

fn unique_path(dir: &Path, kind: BackupKind, now: DateTime<FixedOffset>) -> PathBuf {
    let stem = format!("{PREFIX}{}--{}", timestamp(now), kind.as_str());
    let first = dir.join(format!("{stem}.db"));
    if !first.exists() {
        return first;
    }
    for suffix in 1..=99 {
        let candidate = dir.join(format!("{stem}-{suffix:02}.db"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{stem}-overflow.db"))
}

fn parse_info(path: &Path) -> Option<BackupInfo> {
    let file_name = path.file_name()?.to_str()?.to_string();
    let raw = file_name.strip_prefix(PREFIX)?.strip_suffix(".db")?;
    let (stamp, kind_with_suffix) = raw.split_once("--")?;
    let parsed = DateTime::parse_from_str(stamp, "%Y%m%dT%H%M%S%z").ok()?;
    let kind = ["daily", "pre-migration", "manual", "before-restore"]
        .into_iter()
        .find(|kind| {
            kind_with_suffix == *kind || kind_with_suffix.starts_with(&format!("{kind}-"))
        })?
        .to_string();
    let size_bytes = path.metadata().ok()?.len();
    Some(BackupInfo {
        file_name,
        created_at: parsed.to_rfc3339(),
        kind,
        size_bytes,
    })
}

pub fn list_backups(dir: &Path) -> CoreResult<BackupCatalog> {
    std::fs::create_dir_all(dir).map_err(|err| io_error("创建备份目录失败", err))?;
    let mut items = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|err| io_error("读取备份目录失败", err))? {
        let entry = entry.map_err(|err| io_error("读取备份条目失败", err))?;
        if entry
            .file_type()
            .map_err(|err| io_error("读取备份类型失败", err))?
            .is_file()
        {
            if let Some(info) = parse_info(&entry.path()) {
                items.push(info);
            }
        }
    }
    items.sort_by(|left, right| right.file_name.cmp(&left.file_name));
    let older_retained = items.len().saturating_sub(RETENTION_LIMIT);
    Ok(BackupCatalog {
        items,
        retention_limit: RETENTION_LIMIT,
        older_retained,
    })
}

pub fn create_backup_at(
    conn: &Connection,
    dir: &Path,
    kind: BackupKind,
    now: DateTime<FixedOffset>,
) -> CoreResult<BackupInfo> {
    std::fs::create_dir_all(dir).map_err(|err| io_error("创建备份目录失败", err))?;
    let path = unique_path(dir, kind, now);
    let result = conn
        .backup(DatabaseName::Main, &path, None)
        .map_err(CoreError::from)
        .and_then(|_| verify_path(&path));
    if let Err(err) = result {
        let _ = std::fs::remove_file(&path);
        return Err(err);
    }
    parse_info(&path).ok_or_else(|| CoreError::Invalid("备份文件名无法解析".into()))
}

pub fn create_backup(conn: &Connection, dir: &Path, kind: BackupKind) -> CoreResult<BackupInfo> {
    create_backup_at(conn, dir, kind, now_shanghai())
}

pub fn ensure_daily_backup(conn: &Connection, dir: &Path) -> CoreResult<BackupInfo> {
    let now = now_shanghai();
    let today = now.date_naive();
    for existing in list_backups(dir)?.items {
        let same_day = existing.kind == BackupKind::Daily.as_str()
            && DateTime::parse_from_rfc3339(&existing.created_at)
                .map(|created| created.date_naive() == today)
                .unwrap_or(false);
        if same_day && verify_path(&dir.join(&existing.file_name)).is_ok() {
            return Ok(existing);
        }
    }
    create_backup_at(conn, dir, BackupKind::Daily, now)
}

pub fn verify_connection(conn: &Connection) -> CoreResult<()> {
    let result: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    if result == "ok" {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!(
            "数据库完整性检查失败: {result}"
        )))
    }
}

fn verify_path(path: &Path) -> CoreResult<()> {
    let conn = Connection::open(path)?;
    verify_connection(&conn)
}

fn safe_backup_path(dir: &Path, file_name: &str) -> CoreResult<PathBuf> {
    let name = Path::new(file_name);
    if name.file_name().and_then(|value| value.to_str()) != Some(file_name)
        || !file_name.starts_with(PREFIX)
        || !file_name.ends_with(".db")
    {
        return Err(CoreError::Invalid("备份文件名非法".into()));
    }
    let path = dir.join(file_name);
    if !path.is_file() || parse_info(&path).is_none() {
        return Err(CoreError::NotFound(format!("backup {file_name}")));
    }
    Ok(path)
}

/// 恢复前自动保护当前库；恢复、迁移或完整性检查任一步失败，都自动恢复保护快照。
pub fn restore_with<F>(
    conn: &mut Connection,
    dir: &Path,
    file_name: &str,
    after_restore: F,
) -> CoreResult<BackupInfo>
where
    F: FnOnce(&Connection) -> CoreResult<()>,
{
    let source = safe_backup_path(dir, file_name)?;
    verify_path(&source)?;
    let protective = create_backup(conn, dir, BackupKind::BeforeRestore)?;
    let protective_path = dir.join(&protective.file_name);

    let restored = conn
        .restore(
            DatabaseName::Main,
            &source,
            None::<fn(rusqlite::backup::Progress)>,
        )
        .map_err(CoreError::from)
        .and_then(|_| after_restore(conn))
        .and_then(|_| verify_connection(conn));
    if let Err(err) = restored {
        conn.restore(
            DatabaseName::Main,
            &protective_path,
            None::<fn(rusqlite::backup::Progress)>,
        )?;
        verify_connection(conn)?;
        return Err(CoreError::Invalid(format!(
            "恢复失败，已自动回到恢复前状态: {err}"
        )));
    }
    Ok(protective)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dir(label: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("jiaofu-backup-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixed_time(hour: u32) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(&format!("2026-07-12T{hour:02}:00:00+08:00")).unwrap()
    }

    #[test]
    fn online_backup_and_restore_return_database_to_snapshot() {
        let dir = test_dir("restore");
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sample(value TEXT); INSERT INTO sample VALUES ('before');",
        )
        .unwrap();
        let backup = create_backup_at(&conn, &dir, BackupKind::Manual, fixed_time(10)).unwrap();
        conn.execute("UPDATE sample SET value='after'", []).unwrap();

        let protective = restore_with(&mut conn, &dir, &backup.file_name, |_| Ok(())).unwrap();
        let value: String = conn
            .query_row("SELECT value FROM sample", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "before");
        assert_eq!(protective.kind, "before-restore");
        assert!(dir.join(protective.file_name).is_file());
    }

    #[test]
    fn failed_post_restore_check_automatically_restores_protective_snapshot() {
        let dir = test_dir("rollback");
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE sample(value TEXT); INSERT INTO sample VALUES ('backup');",
        )
        .unwrap();
        let backup = create_backup_at(&conn, &dir, BackupKind::Manual, fixed_time(11)).unwrap();
        conn.execute("UPDATE sample SET value='current'", [])
            .unwrap();

        let result = restore_with(&mut conn, &dir, &backup.file_name, |_| {
            Err(CoreError::Invalid("injected migration failure".into()))
        });
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("已自动回到恢复前状态"));
        let value: String = conn
            .query_row("SELECT value FROM sample", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, "current");
    }

    #[test]
    fn catalog_keeps_older_files_and_reports_retention_warning() {
        let dir = test_dir("retention");
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE sample(value TEXT);")
            .unwrap();
        for hour in 0..=14 {
            create_backup_at(&conn, &dir, BackupKind::Manual, fixed_time(hour)).unwrap();
        }
        let catalog = list_backups(&dir).unwrap();
        assert_eq!(catalog.items.len(), 15);
        assert_eq!(catalog.older_retained, 1);
        assert!(catalog
            .items
            .iter()
            .all(|item| dir.join(&item.file_name).is_file()));
    }

    #[test]
    fn restore_rejects_paths_outside_backup_directory() {
        let dir = test_dir("path-traversal");
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE sample(value TEXT);")
            .unwrap();
        let result = restore_with(&mut conn, &dir, "../secrets.json", |_| Ok(()));
        assert!(result.unwrap_err().to_string().contains("备份文件名非法"));
        assert!(list_backups(&dir).unwrap().items.is_empty());
    }
}
