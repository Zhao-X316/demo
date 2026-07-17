//! M3 错题事实 / M6 掌握分析教师入口命令。

use module_wrongbook::read_model::{self, ClassWrongbookDashboard};
use tauri::State;

use crate::state::AppState;

#[tauri::command]
pub fn class_wrongbook_dashboard(
    state: State<'_, AppState>,
    class_id: i64,
) -> Result<ClassWrongbookDashboard, String> {
    let connection = state.db.lock().map_err(|_| "数据库忙".to_string())?;
    read_model::class_wrongbook_dashboard(&connection, class_id).map_err(|error| error.to_string())
}
