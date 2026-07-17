//! M3-1 老师确认错因。
//!
//! 机器或题型只能提供候选；正式错因必须由老师显式选择，并绑定当前有效
//! 发布快照中的具体非满分 grade decision。重复提交相同内容是 no-op。

use std::collections::BTreeSet;

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::domain::{ids, time};
use suite_core::error::{CoreError, CoreResult};

const MAX_CAUSES: usize = 3;
const MAX_NOTE_CHARS: usize = 500;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorCauseOption {
    pub code: String,
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorCauseReview {
    pub public_id: String,
    pub revision: i64,
    pub grade_decision_public_id: String,
    pub publication_public_id: String,
    pub cause_codes: Vec<String>,
    pub teacher_note: Option<String>,
    pub confirmed_by: String,
    pub confirmed_at: String,
}

#[derive(Debug, Clone)]
pub struct ConfirmErrorCausesInput<'a> {
    pub class_id: i64,
    pub student_id: i64,
    pub question_version_public_id: &'a str,
    pub grade_decision_public_id: &'a str,
    pub publication_public_id: &'a str,
    pub cause_codes: &'a [String],
    pub teacher_note: Option<&'a str>,
    pub confirmed_by: &'a str,
}

#[derive(Debug)]
struct ErrorScope {
    grade_decision_id: i64,
    publication_id: i64,
    question_type: String,
}

const OPTIONS: &[(&str, &str, &str)] = &[
    ("missing_answer", "未作答", "学生没有写出可评分答案"),
    (
        "fact_error",
        "史实错误",
        "人物、时间、地点、事件或制度表述错误",
    ),
    (
        "concept_confusion",
        "概念混淆",
        "把相近概念、事件或人物混在一起",
    ),
    ("chronology_error", "时序错误", "历史事件先后顺序错误"),
    ("causal_gap", "因果缺漏", "原因、结果或影响遗漏或关系不完整"),
    ("rubric_omission", "评分点遗漏", "简答题缺少必要评分点"),
    ("contradiction", "前后矛盾", "答案同时出现互相冲突的表述"),
    (
        "incomplete_expression",
        "表达不完整",
        "方向基本正确，但内容不足以得满分",
    ),
    ("misread_prompt", "审题偏差", "回答方向与题目要求不一致"),
    ("other", "其他", "由老师补充说明"),
];

fn allowed_codes(question_type: &str) -> &'static [&'static str] {
    match question_type {
        "single" | "multiple" | "multi" | "true_false" | "judge" => &[
            "missing_answer",
            "fact_error",
            "concept_confusion",
            "misread_prompt",
            "other",
        ],
        "fill_blank" | "fill" => &[
            "missing_answer",
            "fact_error",
            "concept_confusion",
            "incomplete_expression",
            "other",
        ],
        "short_answer" | "subjective" => &[
            "missing_answer",
            "rubric_omission",
            "fact_error",
            "concept_confusion",
            "chronology_error",
            "causal_gap",
            "contradiction",
            "incomplete_expression",
            "misread_prompt",
            "other",
        ],
        _ => &[
            "missing_answer",
            "fact_error",
            "concept_confusion",
            "incomplete_expression",
            "misread_prompt",
            "other",
        ],
    }
}

pub fn cause_options(question_type: &str) -> Vec<ErrorCauseOption> {
    let allowed = allowed_codes(question_type);
    OPTIONS
        .iter()
        .filter(|(code, _, _)| allowed.contains(code))
        .map(|(code, label, description)| ErrorCauseOption {
            code: (*code).into(),
            label: (*label).into(),
            description: (*description).into(),
        })
        .collect()
}

fn normalize_note(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|note| !note.is_empty())
        .map(str::to_string)
}

fn normalize_causes(question_type: &str, values: &[String]) -> CoreResult<Vec<String>> {
    if values.is_empty() {
        return Err(CoreError::Invalid("请至少选择一个错因".into()));
    }
    if values.len() > MAX_CAUSES {
        return Err(CoreError::Invalid(format!(
            "一次最多选择 {MAX_CAUSES} 个主要错因"
        )));
    }
    let allowed = allowed_codes(question_type);
    let input = values.iter().map(|value| value.trim()).collect::<Vec<_>>();
    if input.iter().any(|value| value.is_empty()) {
        return Err(CoreError::Invalid("错因代码不能为空".into()));
    }
    let unique = input.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != input.len() {
        return Err(CoreError::Invalid("错因不能重复选择".into()));
    }
    if let Some(invalid) = input.iter().find(|code| !allowed.contains(code)) {
        return Err(CoreError::Invalid(format!("当前题型不支持错因：{invalid}")));
    }
    Ok(allowed
        .iter()
        .filter(|code| unique.contains(**code))
        .map(|code| (*code).to_string())
        .collect())
}

fn resolve_error_scope(
    conn: &Connection,
    input: &ConfirmErrorCausesInput<'_>,
) -> CoreResult<ErrorScope> {
    let scope = conn
        .query_row(
            "SELECT decision.id,publication.id,question.question_type
             FROM exam_grade_decisions_v2 decision
             JOIN exam_attempts_v2 attempt
               ON attempt.id=decision.attempt_id
             JOIN students student
               ON student.id=attempt.student_id
              AND student.id=?2
              AND student.class_id=?1
              AND student.enabled=1
             JOIN exam_grade_publications_v2 publication
               ON publication.id=attempt.active_publication_id
              AND publication.public_id=?5
              AND publication.state='published'
             JOIN exam_grade_publication_items_v2 publication_item
               ON publication_item.publication_id=publication.id
              AND publication_item.attempt_id=attempt.id
             JOIN exam_grade_publication_decisions_v2 publication_decision
               ON publication_decision.publication_item_id=publication_item.id
              AND publication_decision.grade_decision_id=decision.id
             JOIN exam_assessment_items_v2 assessment_item
               ON assessment_item.id=decision.assessment_item_id
              AND assessment_item.assessment_version_id=attempt.assessment_version_id
             JOIN k1_question_versions question
               ON question.id=assessment_item.question_version_id
              AND question.public_id=?3
             JOIN exam_assessment_versions_v2 assessment_version
               ON assessment_version.id=attempt.assessment_version_id
             JOIN exam_assessments_v2 assessment
               ON assessment.id=assessment_version.assessment_id
              AND assessment.class_id=?1
             WHERE decision.public_id=?4
               AND decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
               AND decision.teacher_score < assessment_item.score - 0.000001",
            (
                input.class_id,
                input.student_id,
                input.question_version_public_id,
                input.grade_decision_public_id,
                input.publication_public_id,
            ),
            |row| {
                Ok(ErrorScope {
                    grade_decision_id: row.get(0)?,
                    publication_id: row.get(1)?,
                    question_type: row.get(2)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("该评分不是当前班级可确认的已发布非满分事实".into()))?;

    let latest_error_decision_id: i64 = conn.query_row(
        "SELECT decision.id
         FROM exam_grade_decisions_v2 decision
         JOIN exam_attempts_v2 attempt
           ON attempt.id=decision.attempt_id
          AND attempt.student_id=?1
         JOIN exam_grade_publications_v2 publication
           ON publication.id=attempt.active_publication_id
          AND publication.state='published'
         JOIN exam_grade_publication_items_v2 publication_item
           ON publication_item.publication_id=publication.id
          AND publication_item.attempt_id=attempt.id
         JOIN exam_grade_publication_decisions_v2 publication_decision
           ON publication_decision.publication_item_id=publication_item.id
          AND publication_decision.grade_decision_id=decision.id
         JOIN exam_assessment_items_v2 assessment_item
           ON assessment_item.id=decision.assessment_item_id
         JOIN k1_question_versions question
           ON question.id=assessment_item.question_version_id
          AND question.public_id=?2
         JOIN exam_assessment_versions_v2 assessment_version
           ON assessment_version.id=attempt.assessment_version_id
         JOIN exam_assessments_v2 assessment
           ON assessment.id=assessment_version.assessment_id
          AND assessment.class_id=?3
         WHERE decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
           AND decision.teacher_score < assessment_item.score - 0.000001
         ORDER BY publication.published_at DESC,decision.id DESC
         LIMIT 1",
        (
            input.student_id,
            input.question_version_public_id,
            input.class_id,
        ),
        |row| row.get(0),
    )?;
    if latest_error_decision_id != scope.grade_decision_id {
        return Err(CoreError::Invalid(
            "该错因对象已不是当前最新错误，请刷新后重试".into(),
        ));
    }
    Ok(scope)
}

fn load_review_by_decision_id(
    conn: &Connection,
    grade_decision_id: i64,
) -> CoreResult<Option<ErrorCauseReview>> {
    let header = conn
        .query_row(
            "SELECT revision_row.id,revision_row.public_id,revision_row.revision,
                    decision.public_id,publication.public_id,revision_row.teacher_note,
                    revision_row.confirmed_by,revision_row.confirmed_at
             FROM wb_error_cause_revisions revision_row
             JOIN exam_grade_decisions_v2 decision
               ON decision.id=revision_row.grade_decision_id
             JOIN exam_grade_publications_v2 publication
               ON publication.id=revision_row.publication_id
             WHERE revision_row.grade_decision_id=?1 AND revision_row.state='active'",
            [grade_decision_id],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((
        revision_id,
        public_id,
        revision,
        grade_decision_public_id,
        publication_public_id,
        teacher_note,
        confirmed_by,
        confirmed_at,
    )) = header
    else {
        return Ok(None);
    };
    let mut statement = conn.prepare(
        "SELECT cause_code FROM wb_error_cause_items
         WHERE revision_id=?1 ORDER BY order_index",
    )?;
    let causes = statement
        .query_map([revision_id], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    Ok(Some(ErrorCauseReview {
        public_id,
        revision,
        grade_decision_public_id,
        publication_public_id,
        cause_codes: causes,
        teacher_note,
        confirmed_by,
        confirmed_at,
    }))
}

pub fn load_review_for_decision_public_id(
    conn: &Connection,
    grade_decision_public_id: &str,
) -> CoreResult<Option<ErrorCauseReview>> {
    let decision_id = conn
        .query_row(
            "SELECT id FROM exam_grade_decisions_v2 WHERE public_id=?1",
            [grade_decision_public_id],
            |row| row.get(0),
        )
        .optional()?;
    match decision_id {
        Some(id) => load_review_by_decision_id(conn, id),
        None => Ok(None),
    }
}

pub fn confirm_error_causes(
    conn: &mut Connection,
    input: &ConfirmErrorCausesInput<'_>,
) -> CoreResult<ErrorCauseReview> {
    if input.confirmed_by.trim().is_empty() {
        return Err(CoreError::Invalid("错因确认人不能为空".into()));
    }
    let teacher_note = normalize_note(input.teacher_note);
    if teacher_note
        .as_deref()
        .map(|note| note.chars().count() > MAX_NOTE_CHARS)
        .unwrap_or(false)
    {
        return Err(CoreError::Invalid(format!(
            "错因备注不能超过 {MAX_NOTE_CHARS} 个字符"
        )));
    }

    let transaction = conn.transaction()?;
    let scope = resolve_error_scope(&transaction, input)?;
    let cause_codes = normalize_causes(&scope.question_type, input.cause_codes)?;
    if cause_codes.iter().any(|code| code == "other") && teacher_note.is_none() {
        return Err(CoreError::Invalid("选择“其他”时请补充一句说明".into()));
    }
    if let Some(current) = load_review_by_decision_id(&transaction, scope.grade_decision_id)? {
        if current.cause_codes == cause_codes && current.teacher_note == teacher_note {
            transaction.commit()?;
            return Ok(current);
        }
    }

    let revision: i64 = transaction.query_row(
        "SELECT COALESCE(MAX(revision),0)+1
         FROM wb_error_cause_revisions WHERE grade_decision_id=?1",
        [scope.grade_decision_id],
        |row| row.get(0),
    )?;
    transaction.execute(
        "UPDATE wb_error_cause_revisions SET state='superseded'
         WHERE grade_decision_id=?1 AND state='active'",
        [scope.grade_decision_id],
    )?;
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    transaction.execute(
        "INSERT INTO wb_error_cause_revisions
         (public_id,grade_decision_id,publication_id,revision,teacher_note,
          confirmed_by,confirmed_at,created_at,state)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?7,'active')",
        (
            &public_id,
            scope.grade_decision_id,
            scope.publication_id,
            revision,
            teacher_note.as_deref(),
            input.confirmed_by.trim(),
            &now,
        ),
    )?;
    let revision_id = transaction.last_insert_rowid();
    for (order_index, cause_code) in cause_codes.iter().enumerate() {
        transaction.execute(
            "INSERT INTO wb_error_cause_items
             (revision_id,cause_code,order_index,created_at)
             VALUES (?1,?2,?3,?4)",
            (revision_id, cause_code, order_index as i64, &now),
        )?;
    }
    let result = load_review_by_decision_id(&transaction, scope.grade_decision_id)?
        .ok_or_else(|| CoreError::NotFound("刚确认的错因 revision".into()))?;
    transaction.commit()?;
    Ok(result)
}
