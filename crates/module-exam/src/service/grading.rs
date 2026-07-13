//! 客观题批改闭环。
//!
//! 机器对比只创建 `pending_review` 建议；教师确认后，才在同一事务中更新
//! 最终成绩、错题本和知识点掌握度。重复确认或改判均通过重算聚合保持幂等。

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};

use suite_core::error::{CoreError, CoreResult};

const OBJECTIVE_TYPES: &[&str] = &["single", "multi", "judge", "fill"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnswerDetail {
    pub id: i64,
    pub student_id: i64,
    pub student_name: String,
    pub question_id: i64,
    pub question_no: Option<String>,
    pub question_stem: String,
    pub question_type: String,
    pub picked: Option<String>,
    pub correct_answer: Option<String>,
    pub machine_correct: Option<bool>,
    pub human_correct: Option<bool>,
    pub is_correct: Option<bool>,
    pub score: Option<f64>,
    pub max_score: f64,
    pub knowledge_point_id: Option<i64>,
    pub knowledge_point_name: Option<String>,
    pub status: String,
    pub machine_note: Option<String>,
    pub human_note: Option<String>,
    pub created_at: String,
    pub decided_at: Option<String>,
}

struct QuestionForGrade {
    qtype: String,
    correct_answer: Option<String>,
    knowledge_point_id: Option<i64>,
    max_score: f64,
}

fn bool_opt(v: Option<i64>) -> Option<bool> {
    v.map(|x| x != 0)
}

fn answer_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<AnswerDetail> {
    Ok(AnswerDetail {
        id: r.get("id")?,
        student_id: r.get("student_id")?,
        student_name: r.get("student_name")?,
        question_id: r.get("question_id")?,
        question_no: r.get("question_no")?,
        question_stem: r.get("question_stem")?,
        question_type: r.get("question_type")?,
        picked: r.get("picked")?,
        correct_answer: r.get("correct_answer")?,
        machine_correct: bool_opt(r.get("machine_correct")?),
        human_correct: bool_opt(r.get("human_correct")?),
        is_correct: bool_opt(r.get("is_correct")?),
        score: r.get("score")?,
        max_score: r.get("max_score")?,
        knowledge_point_id: r.get("knowledge_point_id")?,
        knowledge_point_name: r.get("knowledge_point_name")?,
        status: r.get("status")?,
        machine_note: r.get("machine_note")?,
        human_note: r.get("human_note")?,
        created_at: r.get("created_at")?,
        decided_at: r.get("decided_at")?,
    })
}

const ANSWER_SELECT: &str = "
    SELECT a.id, a.student_id, s.name AS student_name,
           a.question_id, q.question_no, q.stem AS question_stem,
           q.qtype AS question_type, a.picked, q.correct_answer,
           a.machine_correct, a.human_correct, a.is_correct,
           a.score, a.max_score, a.knowledge_point_id,
           kp.name AS knowledge_point_name, a.status,
           a.note AS machine_note, a.human_note, a.created_at, a.decided_at
    FROM exam_student_answers a
    JOIN students s ON s.id = a.student_id
    JOIN exam_questions q ON q.id = a.question_id
    LEFT JOIN exam_knowledge_points kp ON kp.id = a.knowledge_point_id";

pub fn get_answer(conn: &Connection, id: i64) -> CoreResult<Option<AnswerDetail>> {
    let sql = format!("{ANSWER_SELECT} WHERE a.id=?1");
    Ok(conn.query_row(&sql, [id], answer_row).optional()?)
}

pub fn list_answers(conn: &Connection, limit: i64) -> CoreResult<Vec<AnswerDetail>> {
    let limit = limit.clamp(1, 200);
    let sql = format!("{ANSWER_SELECT} ORDER BY a.id DESC LIMIT ?1");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([limit], answer_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 创建机器判分建议。这里不会写错题本或掌握度。
pub fn suggest_answer(
    conn: &Connection,
    student_id: i64,
    question_id: i64,
    picked: &str,
) -> CoreResult<AnswerDetail> {
    let picked = picked.trim();
    if picked.is_empty() {
        return Err(CoreError::Invalid("学生答案不能为空".into()));
    }

    let student_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM students WHERE id=?1 AND enabled=1)",
        [student_id],
        |r| r.get(0),
    )?;
    if !student_exists {
        return Err(CoreError::NotFound(format!("启用中的学生 {student_id}")));
    }

    let q = conn
        .query_row(
            "SELECT qtype, correct_answer, knowledge_point_id, max_score
             FROM exam_questions WHERE id=?1 AND enabled=1",
            [question_id],
            |r| {
                Ok(QuestionForGrade {
                    qtype: r.get(0)?,
                    correct_answer: r.get(1)?,
                    knowledge_point_id: r.get(2)?,
                    max_score: r.get(3)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("启用中的题目 {question_id}")))?;

    let qtype = q.qtype.trim().to_ascii_lowercase();
    if !OBJECTIVE_TYPES.contains(&qtype.as_str()) {
        return Err(CoreError::Invalid(
            "主观题必须由老师直接判定，不能生成机器对错建议".into(),
        ));
    }
    if !q.max_score.is_finite() || q.max_score <= 0.0 {
        return Err(CoreError::Invalid("题目分值必须大于 0".into()));
    }
    let correct_answer = q
        .correct_answer
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| CoreError::Invalid("题目还没有标准答案".into()))?;
    let machine_correct = compare_answer(&qtype, picked, correct_answer)?;
    let (picked_option_id, diagnostic_kp) = if machine_correct || qtype != "single" {
        (None, q.knowledge_point_id)
    } else {
        selected_option_diagnosis(conn, question_id, picked)?
            .map_or((None, q.knowledge_point_id), |(id, kp)| {
                (Some(id), kp.or(q.knowledge_point_id))
            })
    };
    let note = if machine_correct {
        "机器建议：答案与标准答案一致"
    } else {
        "机器建议：答案与标准答案不一致，请老师确认"
    };

    conn.execute(
        "INSERT INTO exam_student_answers
           (student_id, question_id, picked, picked_option_id, max_score,
            machine_correct, knowledge_point_id, note, status, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'pending_review',datetime('now'))",
        (
            student_id,
            question_id,
            picked,
            picked_option_id,
            q.max_score,
            machine_correct as i64,
            diagnostic_kp,
            note,
        ),
    )?;
    let id = conn.last_insert_rowid();
    get_answer(conn, id)?.ok_or_else(|| CoreError::Db("创建机器建议后未取回作答".into()))
}

/// 教师确认或改判；所有派生数据在同一事务里重算。
pub fn human_decide(
    conn: &Connection,
    answer_id: i64,
    is_correct: bool,
    note: Option<&str>,
) -> CoreResult<AnswerDetail> {
    let tx = conn.unchecked_transaction()?;
    let affected = tx.execute(
        "UPDATE exam_student_answers
         SET human_correct=?1, is_correct=?1,
             score=CASE WHEN ?1=1 THEN max_score ELSE 0 END,
             status='confirmed', human_note=?2,
             decided_at=datetime('now'), updated_at=datetime('now')
         WHERE id=?3",
        (is_correct as i64, note.map(str::trim), answer_id),
    )?;
    if affected == 0 {
        return Err(CoreError::NotFound(format!("作答 {answer_id}")));
    }

    let (student_id, question_id, knowledge_point_id): (i64, i64, Option<i64>) = tx.query_row(
        "SELECT student_id, question_id, knowledge_point_id
         FROM exam_student_answers WHERE id=?1",
        [answer_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    if let Some(kp_id) = knowledge_point_id {
        rebuild_mastery(&tx, student_id, kp_id)?;
    }
    rebuild_wrong_item(&tx, student_id, question_id)?;
    tx.commit()?;

    get_answer(conn, answer_id)?.ok_or_else(|| CoreError::Db("教师判定后未取回作答".into()))
}

fn selected_option_diagnosis(
    conn: &Connection,
    question_id: i64,
    picked: &str,
) -> CoreResult<Option<(i64, Option<i64>)>> {
    let label = normalize_choice(picked);
    if label.chars().count() != 1 {
        return Ok(None);
    }
    Ok(conn
        .query_row(
            "SELECT id, knowledge_point_id FROM exam_question_options
             WHERE question_id=?1 AND upper(label)=?2 LIMIT 1",
            (question_id, label),
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?)
}

fn rebuild_mastery(tx: &Transaction<'_>, student_id: i64, kp_id: i64) -> CoreResult<()> {
    let (total, correct): (i64, i64) = tx.query_row(
        "SELECT count(*), coalesce(sum(CASE WHEN is_correct=1 THEN 1 ELSE 0 END), 0)
         FROM exam_student_answers
         WHERE student_id=?1 AND knowledge_point_id=?2 AND status='confirmed'",
        (student_id, kp_id),
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if total == 0 {
        tx.execute(
            "DELETE FROM exam_knowledge_mastery WHERE student_id=?1 AND knowledge_point_id=?2",
            (student_id, kp_id),
        )?;
    } else {
        tx.execute(
            "INSERT INTO exam_knowledge_mastery
               (student_id, knowledge_point_id, total, correct, updated_at)
             VALUES (?1,?2,?3,?4,datetime('now'))
             ON CONFLICT(student_id, knowledge_point_id) DO UPDATE SET
               total=excluded.total, correct=excluded.correct, updated_at=datetime('now')",
            (student_id, kp_id, total, correct),
        )?;
    }
    Ok(())
}

fn rebuild_wrong_item(tx: &Transaction<'_>, student_id: i64, question_id: i64) -> CoreResult<()> {
    let wrong_count: i64 = tx.query_row(
        "SELECT count(*) FROM exam_student_answers
         WHERE student_id=?1 AND question_id=?2
           AND status='confirmed' AND is_correct=0",
        (student_id, question_id),
        |r| r.get(0),
    )?;
    if wrong_count == 0 {
        tx.execute(
            "DELETE FROM exam_wrong_items WHERE student_id=?1 AND question_id=?2",
            (student_id, question_id),
        )?;
        return Ok(());
    }

    let kp_id: Option<i64> = tx
        .query_row(
            "SELECT knowledge_point_id FROM exam_student_answers
             WHERE student_id=?1 AND question_id=?2
               AND status='confirmed' AND is_correct=0
             ORDER BY id DESC LIMIT 1",
            (student_id, question_id),
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    tx.execute(
        "INSERT INTO exam_wrong_items
           (student_id, question_id, knowledge_point_id, times_wrong, status, first_seen, last_seen)
         VALUES (?1,?2,?3,?4,'open',datetime('now'),datetime('now'))
         ON CONFLICT(student_id, question_id) DO UPDATE SET
           knowledge_point_id=excluded.knowledge_point_id,
           times_wrong=excluded.times_wrong,
           status='open', last_seen=datetime('now')",
        (student_id, question_id, kp_id, wrong_count),
    )?;
    Ok(())
}

fn compare_answer(qtype: &str, picked: &str, correct: &str) -> CoreResult<bool> {
    match qtype {
        "single" => Ok(normalize_choice(picked) == normalize_choice(correct)),
        "multi" => Ok(normalize_multi(picked) == normalize_multi(correct)),
        "judge" => Ok(normalize_judge(picked)? == normalize_judge(correct)?),
        "fill" => Ok(normalize_fill(picked) == normalize_fill(correct)),
        _ => Err(CoreError::Invalid(format!("不支持自动对比的题型：{qtype}"))),
    }
}

fn normalize_choice(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect()
}

fn normalize_multi(raw: &str) -> String {
    let mut chars: Vec<char> = normalize_choice(raw).chars().collect();
    chars.sort_unstable();
    chars.dedup();
    chars.into_iter().collect()
}

fn normalize_judge(raw: &str) -> CoreResult<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "true" | "t" | "1" | "yes" | "y" | "对" | "正确" | "是" | "√" => Ok(true),
        "false" | "f" | "0" | "no" | "n" | "错" | "错误" | "否" | "×" | "x" => Ok(false),
        _ => Err(CoreError::Invalid(format!(
            "无法识别判断题答案：{}",
            raw.trim()
        ))),
    }
}

fn normalize_fill(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::knowledge_points::{self, KpInput};
    use crate::db::questions::{self, NewOption, NewQuestion};
    use suite_core::db::repo::students::{self, StudentInput};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> (Connection, i64, i64, i64, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        let student = students::upsert(
            &conn,
            &StudentInput {
                student_no: "S001",
                name: "小林",
                class_id: None,
                enabled: true,
            },
        )
        .unwrap();
        let main_kp = knowledge_points::create(
            &conn,
            &KpInput {
                subject_id: None,
                parent_id: None,
                code: Some("M"),
                name: "动量守恒",
            },
        )
        .unwrap();
        let trap_kp = knowledge_points::create(
            &conn,
            &KpInput {
                subject_id: None,
                parent_id: None,
                code: Some("T"),
                name: "动能守恒",
            },
        )
        .unwrap();
        let question = questions::create_with_options(
            &conn,
            &NewQuestion {
                subject_id: None,
                question_no: Some("PHY-001"),
                qtype: "single",
                stem: "碰撞中一定守恒的是？",
                image_path: None,
                correct_answer: Some("B"),
                knowledge_point_id: Some(main_kp.id),
                difficulty: Some(2),
                analysis: None,
                max_score: 2.0,
                enabled: true,
            },
            &[
                NewOption {
                    label: "A",
                    content: "动能",
                    is_correct: false,
                    knowledge_point_id: Some(trap_kp.id),
                    analysis: None,
                    ord: 0,
                },
                NewOption {
                    label: "B",
                    content: "动量",
                    is_correct: true,
                    knowledge_point_id: Some(main_kp.id),
                    analysis: None,
                    ord: 1,
                },
            ],
        )
        .unwrap();
        (conn, student.id, question, main_kp.id, trap_kp.id)
    }

    fn counts(conn: &Connection) -> (i64, i64) {
        let mastery = conn
            .query_row("SELECT count(*) FROM exam_knowledge_mastery", [], |r| {
                r.get(0)
            })
            .unwrap();
        let wrong = conn
            .query_row("SELECT count(*) FROM exam_wrong_items", [], |r| r.get(0))
            .unwrap();
        (mastery, wrong)
    }

    #[test]
    fn suggestion_waits_for_teacher_before_side_effects() {
        let (conn, student_id, question_id, _main_kp, trap_kp) = setup();
        let answer = suggest_answer(&conn, student_id, question_id, "A").unwrap();
        assert_eq!(answer.machine_correct, Some(false));
        assert_eq!(answer.human_correct, None);
        assert_eq!(answer.status, "pending_review");
        assert_eq!(answer.knowledge_point_id, Some(trap_kp));
        assert_eq!(counts(&conn), (0, 0));
    }

    #[test]
    fn teacher_confirmation_and_rejudge_rebuild_derivatives() {
        let (conn, student_id, question_id, _main_kp, trap_kp) = setup();
        let answer = suggest_answer(&conn, student_id, question_id, "A").unwrap();

        let confirmed = human_decide(&conn, answer.id, false, Some("错项诊断确认")).unwrap();
        assert_eq!(confirmed.score, Some(0.0));
        assert_eq!(confirmed.status, "confirmed");
        let mastery: (i64, i64) = conn.query_row(
            "SELECT total, correct FROM exam_knowledge_mastery WHERE student_id=?1 AND knowledge_point_id=?2",
            (student_id, trap_kp), |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(mastery, (1, 0));
        let wrong: i64 = conn
            .query_row(
                "SELECT times_wrong FROM exam_wrong_items WHERE student_id=?1 AND question_id=?2",
                (student_id, question_id),
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(wrong, 1);

        let rejudged = human_decide(&conn, answer.id, true, Some("老师复核后改为正确")).unwrap();
        assert_eq!(rejudged.score, Some(2.0));
        let mastery: (i64, i64) = conn.query_row(
            "SELECT total, correct FROM exam_knowledge_mastery WHERE student_id=?1 AND knowledge_point_id=?2",
            (student_id, trap_kp), |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(mastery, (1, 1));
        assert_eq!(counts(&conn), (1, 0));

        human_decide(&conn, answer.id, true, None).unwrap();
        let mastery: (i64, i64) = conn.query_row(
            "SELECT total, correct FROM exam_knowledge_mastery WHERE student_id=?1 AND knowledge_point_id=?2",
            (student_id, trap_kp), |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap();
        assert_eq!(mastery, (1, 1));
    }

    #[test]
    fn transaction_rolls_back_if_derivative_write_fails() {
        let (conn, student_id, question_id, _main_kp, _trap_kp) = setup();
        let answer = suggest_answer(&conn, student_id, question_id, "A").unwrap();
        conn.execute_batch(
            "CREATE TRIGGER fail_mastery BEFORE INSERT ON exam_knowledge_mastery
             BEGIN SELECT RAISE(ABORT, 'injected mastery failure'); END;",
        )
        .unwrap();

        assert!(human_decide(&conn, answer.id, false, None).is_err());
        let unchanged = get_answer(&conn, answer.id).unwrap().unwrap();
        assert_eq!(unchanged.status, "pending_review");
        assert_eq!(unchanged.is_correct, None);
        assert_eq!(counts(&conn), (0, 0));
    }

    #[test]
    fn objective_normalization_is_explicit_and_bounded() {
        assert!(compare_answer("single", " b. ", "B").unwrap());
        assert!(compare_answer("multi", "C, A", "AC").unwrap());
        assert!(compare_answer("judge", "对", "TRUE").unwrap());
        assert!(compare_answer("fill", "牛 顿", "牛顿").unwrap());
        assert!(compare_answer("judge", "也许", "TRUE").is_err());
        assert!(compare_answer("subjective", "x", "x").is_err());
    }
}
