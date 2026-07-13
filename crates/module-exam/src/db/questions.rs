//! 题目 + 选项仓储（选项带逐项解析与知识点）。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use suite_core::error::{CoreError, CoreResult};

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
    pub max_score: f64,
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
    pub max_score: f64,
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
        max_score: r.get("max_score")?,
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
    knowledge_point_id, difficulty, analysis, max_score, enabled";

pub fn create_question(conn: &Connection, q: &NewQuestion<'_>) -> CoreResult<i64> {
    validate_question(q)?;
    conn.execute(
        "INSERT INTO exam_questions
           (subject_id, question_no, qtype, stem, image_path, correct_answer,
            knowledge_point_id, difficulty, analysis, max_score, enabled)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        (
            q.subject_id,
            q.question_no,
            q.qtype,
            q.stem,
            q.image_path,
            q.correct_answer,
            q.knowledge_point_id,
            q.difficulty,
            q.analysis,
            q.max_score,
            q.enabled,
        ),
    )?;
    Ok(conn.last_insert_rowid())
}

/// 原子创建题目和选项，避免题目已保存但选项只写入一部分。
pub fn create_with_options(
    conn: &Connection,
    q: &NewQuestion<'_>,
    options: &[NewOption<'_>],
) -> CoreResult<i64> {
    let tx = conn.unchecked_transaction()?;
    let id = create_question(&tx, q)?;
    set_options_inner(&tx, id, options)?;
    tx.commit()?;
    Ok(id)
}

/// 整体替换某题选项（录题/审核后保存）。
fn set_options_inner(
    conn: &Connection,
    question_id: i64,
    options: &[NewOption<'_>],
) -> CoreResult<()> {
    let mut labels = std::collections::HashSet::new();
    for option in options {
        let label = option.label.trim().to_ascii_uppercase();
        if label.is_empty() || option.content.trim().is_empty() {
            return Err(CoreError::Invalid("选项标签和内容不能为空".into()));
        }
        if !labels.insert(label) {
            return Err(CoreError::Invalid("同一题的选项标签不能重复".into()));
        }
    }
    conn.execute(
        "DELETE FROM exam_question_options WHERE question_id=?1",
        [question_id],
    )?;
    for o in options {
        conn.execute(
            "INSERT INTO exam_question_options
               (question_id, label, content, is_correct, knowledge_point_id, analysis, ord)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            (
                question_id,
                o.label,
                o.content,
                o.is_correct,
                o.knowledge_point_id,
                o.analysis,
                o.ord,
            ),
        )?;
    }
    Ok(())
}

pub fn set_options(
    conn: &Connection,
    question_id: i64,
    options: &[NewOption<'_>],
) -> CoreResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_questions WHERE id=?1)",
        [question_id],
        |r| r.get(0),
    )?;
    if !exists {
        return Err(CoreError::NotFound(format!("题目 {question_id}")));
    }
    let tx = conn.unchecked_transaction()?;
    set_options_inner(&tx, question_id, options)?;
    tx.commit()?;
    Ok(())
}

fn validate_question(q: &NewQuestion<'_>) -> CoreResult<()> {
    if q.stem.trim().is_empty() {
        return Err(CoreError::Invalid("题干不能为空".into()));
    }
    let qtype = q.qtype.trim().to_ascii_lowercase();
    if !["single", "multi", "judge", "fill", "subjective"].contains(&qtype.as_str()) {
        return Err(CoreError::Invalid(format!("不支持的题型：{}", q.qtype)));
    }
    if !q.max_score.is_finite() || q.max_score <= 0.0 {
        return Err(CoreError::Invalid("题目分值必须大于 0".into()));
    }
    if qtype != "subjective"
        && q.correct_answer
            .map(str::trim)
            .filter(|answer| !answer.is_empty())
            .is_none()
    {
        return Err(CoreError::Invalid("客观题必须设置标准答案".into()));
    }
    if q.difficulty
        .is_some_and(|difficulty| !(1..=5).contains(&difficulty))
    {
        return Err(CoreError::Invalid("难度必须在 1 到 5 之间".into()));
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
    let answer_count: i64 = conn.query_row(
        "SELECT count(*) FROM exam_student_answers WHERE question_id=?1",
        [id],
        |r| r.get(0),
    )?;
    if answer_count > 0 {
        return Err(CoreError::Invalid("题目已有作答历史，不能硬删除".into()));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "DELETE FROM exam_question_options WHERE question_id=?1",
        [id],
    )?;
    let deleted = tx.execute("DELETE FROM exam_questions WHERE id=?1", [id])?;
    if deleted == 0 {
        return Err(CoreError::NotFound(format!("题目 {id}")));
    }
    tx.commit()?;
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
                max_score: 2.0,
                enabled: true,
            },
        )
        .unwrap();
        set_options(
            &conn,
            qid,
            &[
                NewOption {
                    label: "A",
                    content: "动能一定守恒",
                    is_correct: false,
                    knowledge_point_id: None,
                    analysis: Some("选A说明混淆了弹性与非弹性碰撞"),
                    ord: 0,
                },
                NewOption {
                    label: "B",
                    content: "系统动量守恒",
                    is_correct: true,
                    knowledge_point_id: None,
                    analysis: Some("正确：内力不改变系统总动量"),
                    ord: 1,
                },
            ],
        )
        .unwrap();

        let (q, opts) = get(&conn, qid).unwrap().unwrap();
        assert_eq!(q.correct_answer.as_deref(), Some("B"));
        assert!(q.enabled);
        assert_eq!(q.max_score, 2.0);
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

    #[test]
    fn question_and_options_are_atomic() {
        let conn = setup();
        conn.execute_batch(
            "CREATE TRIGGER fail_second_option BEFORE INSERT ON exam_question_options
             WHEN NEW.label='B'
             BEGIN SELECT RAISE(ABORT, 'injected option failure'); END;",
        )
        .unwrap();
        let result = create_with_options(
            &conn,
            &NewQuestion {
                subject_id: None,
                question_no: Some("ATOMIC-1"),
                qtype: "single",
                stem: "原子保存测试",
                image_path: None,
                correct_answer: Some("B"),
                knowledge_point_id: None,
                difficulty: None,
                analysis: None,
                max_score: 1.0,
                enabled: true,
            },
            &[
                NewOption {
                    label: "A",
                    content: "甲",
                    is_correct: false,
                    knowledge_point_id: None,
                    analysis: None,
                    ord: 0,
                },
                NewOption {
                    label: "B",
                    content: "乙",
                    is_correct: true,
                    knowledge_point_id: None,
                    analysis: None,
                    ord: 1,
                },
            ],
        );
        assert!(result.is_err());
        let questions: i64 = conn
            .query_row("SELECT count(*) FROM exam_questions", [], |r| r.get(0))
            .unwrap();
        let options: i64 = conn
            .query_row("SELECT count(*) FROM exam_question_options", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!((questions, options), (0, 0));
    }
}
