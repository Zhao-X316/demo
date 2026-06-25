//! `students` 仓储。

use rusqlite::{Connection, OptionalExtension};

use crate::error::CoreResult;
use crate::models::Student;

/// 新增/更新（按 student_no 唯一）入参。
pub struct StudentInput<'a> {
    pub student_no: &'a str,
    pub name: &'a str,
    pub class_id: Option<i64>,
    pub enabled: bool,
}

fn row_to_student(r: &rusqlite::Row<'_>) -> rusqlite::Result<Student> {
    Ok(Student {
        id: r.get("id")?,
        student_no: r.get("student_no")?,
        name: r.get("name")?,
        class_id: r.get("class_id")?,
        enabled: r.get::<_, i64>("enabled")? != 0,
    })
}

/// 按 student_no upsert，返回该行。
pub fn upsert(conn: &Connection, input: &StudentInput<'_>) -> CoreResult<Student> {
    conn.execute(
        "INSERT INTO students (student_no, name, class_id, enabled, updated_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(student_no) DO UPDATE SET
            name = excluded.name,
            class_id = excluded.class_id,
            enabled = excluded.enabled,
            updated_at = datetime('now')",
        (input.student_no, input.name, input.class_id, input.enabled as i64),
    )?;
    get_by_no(conn, input.student_no)?
        .ok_or_else(|| crate::error::CoreError::Db("upsert 后未取回学生".into()))
}

pub fn get_by_no(conn: &Connection, student_no: &str) -> CoreResult<Option<Student>> {
    let s = conn
        .query_row(
            "SELECT id, student_no, name, class_id, enabled FROM students WHERE student_no = ?1",
            [student_no],
            row_to_student,
        )
        .optional()?;
    Ok(s)
}

pub fn list(conn: &Connection, only_enabled: bool) -> CoreResult<Vec<Student>> {
    let sql = if only_enabled {
        "SELECT id, student_no, name, class_id, enabled FROM students WHERE enabled = 1 ORDER BY student_no"
    } else {
        "SELECT id, student_no, name, class_id, enabled FROM students ORDER BY student_no"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], row_to_student)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    #[test]
    fn upsert_and_read() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();

        let s = upsert(
            &conn,
            &StudentInput { student_no: "2023001", name: "张三", class_id: None, enabled: true },
        )
        .unwrap();
        assert_eq!(s.student_no, "2023001");
        assert!(s.enabled);

        // 同学号再 upsert → 更新姓名，不新增
        upsert(
            &conn,
            &StudentInput { student_no: "2023001", name: "张三丰", class_id: None, enabled: true },
        )
        .unwrap();
        let got = get_by_no(&conn, "2023001").unwrap().unwrap();
        assert_eq!(got.name, "张三丰");
        assert_eq!(list(&conn, true).unwrap().len(), 1);
    }
}
