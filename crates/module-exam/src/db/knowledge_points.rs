//! 知识点（板块树）仓储。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use suite_core::error::CoreResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgePoint {
    pub id: i64,
    pub subject_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub code: Option<String>,
    pub name: String,
}

pub struct KpInput<'a> {
    pub subject_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub code: Option<&'a str>,
    pub name: &'a str,
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<KnowledgePoint> {
    Ok(KnowledgePoint {
        id: r.get("id")?,
        subject_id: r.get("subject_id")?,
        parent_id: r.get("parent_id")?,
        code: r.get("code")?,
        name: r.get("name")?,
    })
}

pub fn create(conn: &Connection, input: &KpInput<'_>) -> CoreResult<KnowledgePoint> {
    conn.execute(
        "INSERT INTO exam_knowledge_points (subject_id, parent_id, code, name)
         VALUES (?1, ?2, ?3, ?4)",
        (input.subject_id, input.parent_id, input.code, input.name),
    )?;
    let id = conn.last_insert_rowid();
    Ok(get(conn, id)?.expect("just inserted"))
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> CoreResult<()> {
    conn.execute(
        "UPDATE exam_knowledge_points SET name=?2, updated_at=datetime('now') WHERE id=?1",
        (id, name),
    )?;
    Ok(())
}

pub fn delete(conn: &Connection, id: i64) -> CoreResult<()> {
    // 子节点上提为根（parent_id=NULL），再删除本节点
    conn.execute(
        "UPDATE exam_knowledge_points SET parent_id=NULL WHERE parent_id=?1",
        [id],
    )?;
    conn.execute("DELETE FROM exam_knowledge_points WHERE id=?1", [id])?;
    Ok(())
}

pub fn get(conn: &Connection, id: i64) -> CoreResult<Option<KnowledgePoint>> {
    Ok(conn
        .query_row(
            "SELECT id, subject_id, parent_id, code, name FROM exam_knowledge_points WHERE id=?1",
            [id],
            row,
        )
        .optional()?)
}

/// 全部知识点（按 parent、id 排序，便于前端建树）。
pub fn list(conn: &Connection) -> CoreResult<Vec<KnowledgePoint>> {
    let mut stmt = conn.prepare(
        "SELECT id, subject_id, parent_id, code, name FROM exam_knowledge_points
         ORDER BY COALESCE(parent_id, 0), id",
    )?;
    let rows = stmt.query_map([], row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> Connection {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        conn
    }

    #[test]
    fn tree_create_list_delete() {
        let conn = setup();
        let board = create(
            &conn,
            &KpInput {
                subject_id: None,
                parent_id: None,
                code: Some("LX"),
                name: "力学",
            },
        )
        .unwrap();
        let child = create(
            &conn,
            &KpInput {
                subject_id: None,
                parent_id: Some(board.id),
                code: None,
                name: "动量守恒",
            },
        )
        .unwrap();
        assert_eq!(list(&conn).unwrap().len(), 2);
        assert_eq!(child.parent_id, Some(board.id));

        rename(&conn, child.id, "动量守恒定律").unwrap();
        assert_eq!(get(&conn, child.id).unwrap().unwrap().name, "动量守恒定律");

        // 删父 → 子上提为根
        delete(&conn, board.id).unwrap();
        let after = list(&conn).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].parent_id, None);
    }
}
