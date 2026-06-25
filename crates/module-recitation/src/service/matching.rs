//! 智能匹配：从 ASR 文本中识别「背诵内容」与「学生」，用于"分析录音→自动命名/归档"。
//!
//! - 内容：在启用内容里取与识别文本覆盖率最高的一篇（覆盖率对录音开头的"姓名+日期"前缀稳健）。
//! - 学生：扫描花名册，命中姓名出现在文本中的学生（取最靠前者）。

use rusqlite::Connection;

use suite_core::db::repo::students;
use suite_core::domain::accuracy::{evaluate, AccuracyCfg};
use suite_core::domain::normalize::{normalize, NormalizeCfg};
use suite_core::error::CoreResult;
use suite_core::models::Student;

use crate::db::contents::{self, RecContent};

/// 与识别文本最匹配的启用内容（含匹配分 0-100，= 覆盖率%）。
pub fn best_content(
    conn: &Connection,
    asr_text: &str,
    ncfg: &NormalizeCfg,
    acfg: &AccuracyCfg,
) -> CoreResult<Option<(RecContent, f64)>> {
    let asr_norm = normalize(asr_text, ncfg);
    let mut best: Option<(RecContent, f64)> = None;
    for c in contents::list(conn, true)? {
        let ans_norm = normalize(&c.answer_text, ncfg);
        let score = evaluate(&ans_norm, &asr_norm, acfg).accuracy;
        if best.as_ref().map(|(_, b)| score > *b).unwrap_or(true) {
            best = Some((c, score));
        }
    }
    Ok(best)
}

/// 花名册中姓名出现在文本里的学生（取最靠前出现者）。
pub fn find_student(conn: &Connection, asr_text: &str) -> CoreResult<Option<Student>> {
    let mut best: Option<(usize, Student)> = None;
    for s in students::list(conn, true)? {
        if s.name.is_empty() {
            continue;
        }
        if let Some(pos) = asr_text.find(&s.name) {
            if best.as_ref().map(|(p, _)| pos < *p).unwrap_or(true) {
                best = Some((pos, s));
            }
        }
    }
    Ok(best.map(|(_, s)| s))
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::repo::students::{upsert as upsert_student, StudentInput};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> Connection {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::recitation_migrations()).unwrap();
        upsert_student(&conn, &StudentInput { student_no: "2023001", name: "张三", class_id: None, enabled: true }).unwrap();
        upsert_student(&conn, &StudentInput { student_no: "2023002", name: "李四", class_id: None, enabled: true }).unwrap();
        contents::upsert(&conn, &contents::ContentInput { content_no: "C012", title: "静夜思", answer_text: "床前明月光，疑是地上霜。举头望明月，低头思故乡。", subject_id: None, enabled: true }).unwrap();
        contents::upsert(&conn, &contents::ContentInput { content_no: "C002", title: "春晓", answer_text: "春眠不觉晓，处处闻啼鸟。", subject_id: None, enabled: true }).unwrap();
        conn
    }

    #[test]
    fn identifies_content_and_student_from_spoken_prefix() {
        let conn = setup();
        // 录音内容：姓名 + 日期 + 背诵内容
        let text = "我是张三，六月二十五日，背诵静夜思。床前明月光疑是地上霜举头望明月低头思故乡";
        let nc = NormalizeCfg::default();
        let ac = AccuracyCfg::default();

        let (content, score) = best_content(&conn, text, &nc, &ac).unwrap().unwrap();
        assert_eq!(content.content_no, "C012");
        assert!(score >= 90.0, "覆盖率应很高, got {score}");

        let student = find_student(&conn, text).unwrap().unwrap();
        assert_eq!(student.student_no, "2023001");
    }

    #[test]
    fn picks_correct_poem_among_many() {
        let conn = setup();
        let text = "李四 春眠不觉晓处处闻啼鸟";
        let (content, _) = best_content(&conn, text, &NormalizeCfg::default(), &AccuracyCfg::default()).unwrap().unwrap();
        assert_eq!(content.content_no, "C002");
        assert_eq!(find_student(&conn, text).unwrap().unwrap().student_no, "2023002");
    }
}
