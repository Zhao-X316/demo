//! 题目 + 选项仓储（选项带逐项解析与知识点）。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use suite_core::error::CoreResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: i64,
    pub subject_id: Option<i64>,
    pub question_no: Option<String>,
    pub qtype: String,
    pub stem: String,
    pub image_path: Option<String>,
    pub correct_answer: Option<String>,
    pub knowledge_point_id: Option<i64>,
    pub difficulty: Option<i64>,
    pub analysis: Option<String>,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    pub id: i64,
    pub question_id: i64,
    pub label: String,
    pub content: String,
    pub is_correct: bool,
    pub knowledge_point_id: Option<i64>,
    pub analysis: Option<String>,
    pub ord: i64,
}

pub struct NewQuestion<'a> {
    pub subject_id: Option<i64>,
    pub question_no: Option<&'a str>,
    pub qtype: &'a str,
    pub stem: &'a str,
    pub image_path: Option<&'a str>,
    pub correct_answer: Option<&'a str>,
    pub knowledge_point_id: Option<i64>,
    pub difficulty: Option<i64>,
    pub analysis: Option<&'a str>,
    pub enabled: bool,
}

pub struct NewOption<'a> {
    pub label: &'a str,
    pub content: &'a str,
    pub is_correct: bool,
    pub knowledge_point_id: Option<i64>,
    pub analysis: Option<&'a str>,
    pub ord: i64,
}

fn q_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Question> {
    Ok(Question {
        id: r.get("id")?,
        subject_id: r.get("subject_id")?,
        question_no: r.get("question_no")?,
        qtype: r.get("qtype")?,
        stem: r.get("stem")?,
        image_path: r.get("image_path")?,
        correct_answer: r.get("correct_answer")?,
        knowledge_point_id: r.get("knowledge_point_id")?,
        difficulty: r.get("difficulty")?,
        analysis: r.get("analysis")?,
        enabled: r.get("enabled")?,
    })
}

fn o_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<QuestionOption> {
    Ok(QuestionOption {
        id: r.get("id")?,
        question_id: r.get("question_id")?,
        label: r.get("label")?,
        content: r.get("content")?,
        is_correct: r.get("is_correct")?,
        knowledge_point_id: r.get("knowledge_point_id")?,
        analysis: r.get("analysis")?,
        ord: r.get("ord")?,
    })
}

const Q_COLS: &str = "id, subject_id, question_no, qtype, stem, image_path, correct_answer, \
    knowledge_point_id, difficulty, analysis, enabled";

pub fn create_question(conn: &Connection, q: &NewQuestion<'_>) -> CoreResult<i64> {
    conn.execute(
        "INSERT INTO exam_questions
           (subject_id, question_no, qtype, stem, image_path, correct_answer,
            knowledge_point_id, difficulty, analysis, enabled)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        (
            q.subject_id, q.question_no, q.qtype, q.stem, q.image_path, q.correct_answer,
            q.knowledge_point_id, q.difficulty, q.analysis, q.enabled,
        ),
    )?;
    Ok(conn.last_insert_rowid())
}

/// 整体替换某题选项（录题/审核后保存）。
pub fn set_options(conn: &Connection, question_id: i64, options: &[NewOption<'_>]) -> CoreResult<()> {
    conn.execute("DELETE FROM exam_question_options WHERE question_id=?1", [question_id])?;
    for o in options {
        conn.execute(
            "INSERT INTO exam_question_options
               (question_id, label, content, is_correct, knowledge_point_id, analysis, ord)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            (question_id, o.label, o.content, o.is_correct, o.knowledge_point_id, o.analysis, o.ord),
        )?;
    }
    Ok(())
}

pub fn options_of(conn: &Connection, question_id: i64) -> CoreResult<Vec<QuestionOption>> {
    let mut stmt = conn.prepare(
        "SELECT id, question_id, label, content, is_correct, knowledge_point_id, analysis, ord
         FROM exam_question_options WHERE question_id=?1 ORDER BY ord, id",
    )?;
    let rows = stmt.query_map([question_id], o_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn get(conn: &Connection, id: i64) -> CoreResult<Option<(Question, Vec<QuestionOption>)>> {
    let sql = format!("SELECT {Q_COLS} FROM exam_questions WHERE id=?1");
    let q = conn.query_row(&sql, [id], q_row).optional()?;
    match q {
        Some(q) => {
            let opts = options_of(conn, id)?;
            Ok(Some((q, opts)))
        }
        None => Ok(None),
    }
}

pub fn list(conn: &Connection) -> CoreResult<Vec<Question>> {
    let sql = format!("SELECT {Q_COLS} FROM exam_questions ORDER BY id DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], q_row)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn delete(conn: &Connection, id: i64) -> CoreResult<()> {
    conn.execute("DELETE FROM exam_question_options WHERE question_id=?1", [id])?;
    conn.execute("DELETE FROM exam_questions WHERE id=?1", [id])?;
    Ok(())
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
    fn question_with_per_option_analysis() {
        let conn = setup();
        let qid = create_question(
            &conn,
            &NewQuestion {
                subject_id: None,
                question_no: Some("PHY-001"),
                qtype: "single",
                stem: "两物体碰撞，下列说法正确的是？",
                image_path: None,
                correct_answer: Some("B"),
                knowledge_point_id: None,
                difficulty: Some(3),
                analysis: Some("考查动量守恒的适用条件"),
                enabled: true,
            },
        )
        .unwrap();
        set_options(
            &conn,
            qid,
            &[
                NewOption { label: "A", content: "动能一定守恒", is_correct: false, knowledge_point_id: None, analysis: Some("选A说明混淆了弹性与非弹性碰撞"), ord: 0 },
                NewOption { label: "B", content: "系统动量守恒", is_correct: true, knowledge_point_id: None, analysis: Some("正确：内力不改变系统总动量"), ord: 1 },
            ],
        )
        .unwrap();

        let (q, opts) = get(&conn, qid).unwrap().unwrap();
        assert_eq!(q.correct_answer.as_deref(), Some("B"));
        assert!(q.enabled);
        assert_eq!(opts.len(), 2);
        let correct: Vec<_> = opts.iter().filter(|o| o.is_correct).collect();
        assert_eq!(correct.len(), 1);
        assert_eq!(correct[0].label, "B");
        assert!(opts[0].analysis.as_deref().unwrap().contains("混淆"));

        assert_eq!(list(&conn).unwrap().len(), 1);
        delete(&conn, qid).unwrap();
        assert!(get(&conn, qid).unwrap().is_none());
        assert_eq!(options_of(&conn, qid).unwrap().len(), 0);
    }
}
