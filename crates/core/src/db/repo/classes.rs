//! `classes` 仓储：班级（全模块共用）。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;
use crate::models::Class;

const COLS: &str = "id, name, term, textbook";

fn row_to_class(r: &rusqlite::Row<'_>) -> rusqlite::Result<Class> {
    Ok(Class {
        id: r.get("id")?,
        name: r.get("name")?,
        term: r.get("term")?,
        textbook: r.get("textbook")?,
    })
}

pub fn list(conn: &Connection) -> CoreResult<Vec<Class>> {
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM classes ORDER BY id"))?;
    let rows = stmt.query_map([], row_to_class)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 建班（按 name 去重：同名已存在则补/返回既有；新建则带教材）。
pub fn create(
    conn: &Connection,
    name: &str,
    textbook: Option<&str>,
    term: Option<&str>,
) -> CoreResult<Class> {
    if let Some(c) = conn
        .query_row(
            &format!("SELECT {COLS} FROM classes WHERE name = ?1"),
            [name],
            row_to_class,
        )
        .optional()?
    {
        // 同名已存在：若传了教材而原本没有，顺手补上
        if textbook.is_some() && c.textbook.is_none() {
            conn.execute(
                "UPDATE classes SET textbook = ?1 WHERE id = ?2",
                (textbook, c.id),
            )?;
            return Ok(Class {
                textbook: textbook.map(String::from),
                ..c
            });
        }
        return Ok(c);
    }
    conn.execute(
        "INSERT INTO classes (name, term, textbook) VALUES (?1, ?2, ?3)",
        (name, term, textbook),
    )?;
    Ok(Class {
        id: conn.last_insert_rowid(),
        name: name.to_string(),
        term: term.map(String::from),
        textbook: textbook.map(String::from),
    })
}

/// 改班：按 id 更新名称 / 教材 / 学期（用于改名「八(1)→九(1)」、调整教材绑定）。
pub fn update(
    conn: &Connection,
    id: i64,
    name: &str,
    textbook: Option<&str>,
    term: Option<&str>,
) -> CoreResult<Class> {
    conn.execute(
        "UPDATE classes SET name = ?1, textbook = ?2, term = ?3 WHERE id = ?4",
        (name, textbook, term, id),
    )?;
    Ok(Class {
        id,
        name: name.to_string(),
        term: term.map(String::from),
        textbook: textbook.map(String::from),
    })
}

/// 删班：先把该班学生移出（class_id=NULL），再删班级，避免外键失败。
pub fn delete(conn: &Connection, id: i64) -> CoreResult<()> {
    conn.execute(
        "UPDATE students SET class_id = NULL WHERE class_id = ?1",
        [id],
    )?;
    conn.execute("DELETE FROM classes WHERE id = ?1", [id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::students::{self, StudentInput};
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    #[test]
    fn create_dedup_assign_delete() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let a = create(&conn, "八(1)班", Some("道法8上"), Some("2025秋")).unwrap();
        let a2 = create(&conn, "八(1)班", None, None).unwrap(); // 同名去重
        assert_eq!(a.id, a2.id);
        assert_eq!(a.textbook.as_deref(), Some("道法8上"));
        assert_eq!(list(&conn).unwrap().len(), 1);

        let s = students::upsert(
            &conn,
            &StudentInput {
                student_no: "2023001",
                name: "张三",
                class_id: None,
                enabled: true,
            },
        )
        .unwrap();
        students::set_class(&conn, &[s.id], Some(a.id)).unwrap();
        assert_eq!(
            students::get_by_id(&conn, s.id).unwrap().unwrap().class_id,
            Some(a.id)
        );

        // 删班 → 学生回到未分班，不报外键
        delete(&conn, a.id).unwrap();
        assert_eq!(
            students::get_by_id(&conn, s.id).unwrap().unwrap().class_id,
            None
        );
        assert!(list(&conn).unwrap().is_empty());
    }
}
