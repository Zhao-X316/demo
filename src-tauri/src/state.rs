//! 应用状态：共享 SQLite 连接（启动时建库 + 跑迁移）+ 数据目录。

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use tauri::{App, Manager};

pub struct AppState {
    pub db: Mutex<Connection>,
    /// 应用数据目录（放 data.db 与 secrets.json）。
    pub data_dir: PathBuf,
}

/// 在应用数据目录打开 data.db，运行 core + 各模块迁移。
pub fn init(app: &App) -> Result<AppState, Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?;
    std::fs::create_dir_all(&dir)?;
    let db_path = dir.join("data.db");

    let conn = suite_core::db::open(&db_path)?;
    suite_core::db::run_migrations(&conn, suite_core::db::CORE_MIGRATIONS)?;
    suite_core::db::run_migrations(&conn, module_recitation::recitation_migrations())?;
    suite_core::db::run_migrations(&conn, module_exam::exam_migrations())?;
    let recovered = module_recitation::service::recognition::recover_stale_processing(&conn)?;
    if recovered > 0 {
        eprintln!("[ASR恢复] {recovered} 条中断的 processing 已转为 failed");
    }

    Ok(AppState { db: Mutex::new(conn), data_dir: dir })
}
