//! M3-0 班级错题事实只读模型。
//!
//! 事实来源只能是 attempt 当前指向的有效发布快照及其中的老师评分 revision。
//! 不读取 legacy `exam_wrong_items` / `exam_knowledge_mastery`，不写任何业务表。

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::domain::time;
use suite_core::error::{CoreError, CoreResult};

use crate::correction::{load_for_source_decision, CorrectionAssignment};
use crate::error_cause::{
    cause_options, load_review_for_decision_public_id, ErrorCauseOption, ErrorCauseReview,
};
use crate::reinforcement::{
    load_for_source_decision as load_reinforcement, ReinforcementAssignment,
};

pub const CLASS_WRONGBOOK_SCHEMA_VERSION: i64 = 4;
pub const CLASS_WRONGBOOK_RULE_VERSION: &str = "m3-published-wrong-facts-v4";
const FULL_SCORE_EPSILON: f64 = 0.000_001;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrongbookMeta {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub exam_watermark: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrongbookClass {
    pub id: i64,
    pub name: String,
    pub term: Option<String>,
    pub textbook: Option<String>,
    pub enabled_student_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedReference {
    pub public_id: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WrongbookQuestion {
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub question_version_id: String,
    pub question_type: String,
    pub stem: String,
    pub status: String,
    pub first_error_at: String,
    pub last_error_at: String,
    pub latest_response_at: String,
    pub latest_score: f64,
    pub latest_max_score: f64,
    pub latest_score_ratio: f64,
    pub published_response_count: i64,
    pub error_response_count: i64,
    pub repeated_error: bool,
    pub latest_assessment_title: String,
    pub latest_assessment_context: String,
    pub latest_error_grade_decision_public_id: String,
    pub latest_error_publication_public_id: String,
    pub cause_options: Vec<ErrorCauseOption>,
    pub cause_review: Option<ErrorCauseReview>,
    pub correction_assignment: Option<CorrectionAssignment>,
    pub reinforcement_assignment: Option<ReinforcementAssignment>,
    pub knowledge_nodes: Vec<NamedReference>,
    pub ability_dimensions: Vec<NamedReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrongbookSummary {
    pub affected_student_count: i64,
    pub wrong_question_count: i64,
    pub needs_correction_count: i64,
    pub corrected_once_count: i64,
    pub rechecked_correct_count: i64,
    pub repeated_error_count: i64,
    pub denominator_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassWrongbookDashboard {
    pub meta: WrongbookMeta,
    pub class: WrongbookClass,
    pub summary: WrongbookSummary,
    pub items: Vec<WrongbookQuestion>,
}

#[derive(Debug)]
struct PublishedResponse {
    student_id: i64,
    student_no: String,
    student_name: String,
    question_version_id: String,
    question_type: String,
    stem: String,
    grade_decision_public_id: String,
    publication_public_id: String,
    attempt_kind: String,
    teacher_score: f64,
    max_score: f64,
    published_at: String,
    assessment_title: String,
    assessment_context: String,
    link_set_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct QuestionKey {
    student_id: i64,
    question_version_id: String,
}

type LabelsByLinkSet = BTreeMap<i64, Vec<NamedReference>>;

fn natural_student_no(left: &str, right: &str) -> Ordering {
    match (left.parse::<u64>(), right.parse::<u64>()) {
        (Ok(left_number), Ok(right_number)) => {
            left_number.cmp(&right_number).then_with(|| left.cmp(right))
        }
        _ => left.cmp(right),
    }
}

fn load_class(conn: &Connection, class_id: i64) -> CoreResult<WrongbookClass> {
    conn.query_row(
        "SELECT c.id,c.name,c.term,c.textbook,
                (SELECT COUNT(*) FROM students s
                 WHERE s.class_id=c.id AND s.enabled=1)
         FROM classes c WHERE c.id=?1",
        [class_id],
        |row| {
            Ok(WrongbookClass {
                id: row.get(0)?,
                name: row.get(1)?,
                term: row.get(2)?,
                textbook: row.get(3)?,
                enabled_student_count: row.get(4)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound(format!("class#{class_id}")))
}

fn load_published_responses(
    conn: &Connection,
    class_id: i64,
    student_id: Option<i64>,
) -> CoreResult<Vec<PublishedResponse>> {
    let mut statement = conn.prepare(
        "SELECT s.id,s.student_no,s.name,
                question.public_id,question.question_type,question.stem,
                decision.public_id,publication.public_id,
                attempt.attempt_kind,decision.teacher_score,item.score,
                publication.published_at,assessment.title,assessment.assessment_context,
                item.link_set_id
         FROM exam_attempts_v2 attempt
         JOIN students s
           ON s.id=attempt.student_id AND s.class_id=?1 AND s.enabled=1
          AND (?2 IS NULL OR s.id=?2)
         JOIN exam_grade_publications_v2 publication
           ON publication.id=attempt.active_publication_id
          AND publication.state='published'
         JOIN exam_grade_publication_items_v2 publication_item
           ON publication_item.publication_id=publication.id
          AND publication_item.attempt_id=attempt.id
         JOIN exam_grade_publication_decisions_v2 publication_decision
           ON publication_decision.publication_item_id=publication_item.id
          AND publication_decision.attempt_id=attempt.id
         JOIN exam_grade_decisions_v2 decision
           ON decision.id=publication_decision.grade_decision_id
          AND decision.attempt_id=attempt.id
          AND decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
         JOIN exam_assessment_items_v2 item
           ON item.id=decision.assessment_item_id
          AND item.assessment_version_id=attempt.assessment_version_id
         JOIN k1_question_versions question ON question.id=item.question_version_id
         JOIN exam_assessment_versions_v2 assessment_version
           ON assessment_version.id=attempt.assessment_version_id
         JOIN exam_assessments_v2 assessment
           ON assessment.id=assessment_version.assessment_id
          AND assessment.class_id=?1
         ORDER BY s.id,question.public_id,publication.published_at,decision.id",
    )?;
    let rows = statement.query_map((class_id, student_id), |row| {
        Ok(PublishedResponse {
            student_id: row.get(0)?,
            student_no: row.get(1)?,
            student_name: row.get(2)?,
            question_version_id: row.get(3)?,
            question_type: row.get(4)?,
            stem: row.get(5)?,
            grade_decision_public_id: row.get(6)?,
            publication_public_id: row.get(7)?,
            attempt_kind: row.get(8)?,
            teacher_score: row.get(9)?,
            max_score: row.get(10)?,
            published_at: row.get(11)?,
            assessment_title: row.get(12)?,
            assessment_context: row.get(13)?,
            link_set_id: row.get(14)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn load_confirmed_labels(
    conn: &Connection,
    link_set_ids: &BTreeSet<i64>,
) -> CoreResult<(LabelsByLinkSet, LabelsByLinkSet)> {
    let mut knowledge = BTreeMap::new();
    let mut abilities = BTreeMap::new();
    let mut knowledge_statement = conn.prepare(
        "SELECT node.public_id,node.title
         FROM k1_knowledge_links link
         JOIN k1_knowledge_nodes node
           ON node.id=link.knowledge_node_id AND node.state='active'
         WHERE link.link_set_id=?1
           AND link.confirmation_level='teacher_confirmed'
           AND link.relation_type IN ('direct_assessment','rubric_basis')
         ORDER BY node.order_index,node.public_id",
    )?;
    let mut ability_statement = conn.prepare(
        "SELECT dimension.public_id,dimension.title
         FROM k1_ability_links link
         JOIN k1_ability_dimensions dimension
           ON dimension.id=link.ability_dimension_id AND dimension.state='active'
         WHERE link.link_set_id=?1
           AND link.confirmation_level='teacher_confirmed'
         ORDER BY dimension.code,dimension.public_id",
    )?;
    for link_set_id in link_set_ids {
        let knowledge_rows = knowledge_statement.query_map([link_set_id], |row| {
            Ok(NamedReference {
                public_id: row.get(0)?,
                title: row.get(1)?,
            })
        })?;
        knowledge.insert(
            *link_set_id,
            knowledge_rows.collect::<rusqlite::Result<Vec<_>>>()?,
        );
        let ability_rows = ability_statement.query_map([link_set_id], |row| {
            Ok(NamedReference {
                public_id: row.get(0)?,
                title: row.get(1)?,
            })
        })?;
        abilities.insert(
            *link_set_id,
            ability_rows.collect::<rusqlite::Result<Vec<_>>>()?,
        );
    }
    Ok((knowledge, abilities))
}

fn add_unique_labels(target: &mut BTreeMap<String, String>, labels: &[NamedReference]) {
    for label in labels {
        target
            .entry(label.public_id.clone())
            .or_insert_with(|| label.title.clone());
    }
}

fn map_labels(labels: BTreeMap<String, String>) -> Vec<NamedReference> {
    labels
        .into_iter()
        .map(|(public_id, title)| NamedReference { public_id, title })
        .collect()
}

fn wrongbook_dashboard(
    conn: &Connection,
    class_id: i64,
    student_id: Option<i64>,
) -> CoreResult<ClassWrongbookDashboard> {
    let class = load_class(conn, class_id)?;
    let responses = load_published_responses(conn, class_id, student_id)?;
    let exam_watermark = responses
        .iter()
        .map(|response| response.published_at.as_str())
        .max()
        .map(str::to_string);
    let link_set_ids = responses
        .iter()
        .map(|response| response.link_set_id)
        .collect::<BTreeSet<_>>();
    let (knowledge_by_link_set, abilities_by_link_set) =
        load_confirmed_labels(conn, &link_set_ids)?;

    let mut grouped: BTreeMap<QuestionKey, Vec<PublishedResponse>> = BTreeMap::new();
    for response in responses {
        grouped
            .entry(QuestionKey {
                student_id: response.student_id,
                question_version_id: response.question_version_id.clone(),
            })
            .or_default()
            .push(response);
    }

    let mut items = Vec::new();
    for (_, mut group) in grouped {
        group.sort_by(|left, right| {
            left.published_at
                .cmp(&right.published_at)
                .then_with(|| left.attempt_kind.cmp(&right.attempt_kind))
        });
        let errors = group
            .iter()
            .filter(|response| response.teacher_score + FULL_SCORE_EPSILON < response.max_score)
            .collect::<Vec<_>>();
        if errors.is_empty() {
            continue;
        }
        let latest = group.last().expect("non-empty published response group");
        let latest_error = errors.last().expect("at least one error");
        let latest_is_error = latest.teacher_score + FULL_SCORE_EPSILON < latest.max_score;
        let status = if latest_is_error {
            "needs_correction"
        } else if latest.attempt_kind == "correction" {
            "corrected_once"
        } else {
            "rechecked_correct"
        };
        let ratio = if latest.max_score > 0.0 {
            (latest.teacher_score / latest.max_score).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut knowledge_labels = BTreeMap::new();
        let mut ability_labels = BTreeMap::new();
        for response in &group {
            if let Some(labels) = knowledge_by_link_set.get(&response.link_set_id) {
                add_unique_labels(&mut knowledge_labels, labels);
            }
            if let Some(labels) = abilities_by_link_set.get(&response.link_set_id) {
                add_unique_labels(&mut ability_labels, labels);
            }
        }
        items.push(WrongbookQuestion {
            student_id: latest.student_id,
            student_no: latest.student_no.clone(),
            student_name: latest.student_name.clone(),
            question_version_id: latest.question_version_id.clone(),
            question_type: latest.question_type.clone(),
            stem: latest.stem.clone(),
            status: status.into(),
            first_error_at: errors
                .first()
                .expect("at least one error")
                .published_at
                .clone(),
            last_error_at: errors
                .last()
                .expect("at least one error")
                .published_at
                .clone(),
            latest_response_at: latest.published_at.clone(),
            latest_score: latest.teacher_score,
            latest_max_score: latest.max_score,
            latest_score_ratio: ratio,
            published_response_count: group.len() as i64,
            error_response_count: errors.len() as i64,
            repeated_error: errors.len() > 1,
            latest_assessment_title: latest.assessment_title.clone(),
            latest_assessment_context: latest.assessment_context.clone(),
            latest_error_grade_decision_public_id: latest_error.grade_decision_public_id.clone(),
            latest_error_publication_public_id: latest_error.publication_public_id.clone(),
            cause_options: cause_options(&latest_error.question_type),
            cause_review: load_review_for_decision_public_id(
                conn,
                &latest_error.grade_decision_public_id,
            )?,
            correction_assignment: load_for_source_decision(
                conn,
                &latest_error.grade_decision_public_id,
            )?,
            reinforcement_assignment: load_reinforcement(
                conn,
                &latest_error.grade_decision_public_id,
            )?,
            knowledge_nodes: map_labels(knowledge_labels),
            ability_dimensions: map_labels(ability_labels),
        });
    }

    items.sort_by(|left, right| {
        natural_student_no(&left.student_no, &right.student_no)
            .then_with(|| left.student_id.cmp(&right.student_id))
            .then_with(|| right.last_error_at.cmp(&left.last_error_at))
            .then_with(|| left.question_version_id.cmp(&right.question_version_id))
    });
    let affected_student_count = items
        .iter()
        .map(|item| item.student_id)
        .collect::<BTreeSet<_>>()
        .len() as i64;
    let summary = WrongbookSummary {
        affected_student_count,
        wrong_question_count: items.len() as i64,
        needs_correction_count: items
            .iter()
            .filter(|item| item.status == "needs_correction")
            .count() as i64,
        corrected_once_count: items
            .iter()
            .filter(|item| item.status == "corrected_once")
            .count() as i64,
        rechecked_correct_count: items
            .iter()
            .filter(|item| item.status == "rechecked_correct")
            .count() as i64,
        repeated_error_count: items.iter().filter(|item| item.repeated_error).count() as i64,
        denominator_note:
            "仅统计本班启用学生当前有效发布快照中的老师评分；非满分记为错题。订正一次和再次答对都只是题目事实，不等于知识点已掌握。"
                .into(),
    };
    Ok(ClassWrongbookDashboard {
        meta: WrongbookMeta {
            schema_version: CLASS_WRONGBOOK_SCHEMA_VERSION,
            rule_version: CLASS_WRONGBOOK_RULE_VERSION.into(),
            calculated_at: time::utc_now_rfc3339(),
            exam_watermark,
        },
        class,
        summary,
        items,
    })
}

pub fn class_wrongbook_dashboard(
    conn: &Connection,
    class_id: i64,
) -> CoreResult<ClassWrongbookDashboard> {
    wrongbook_dashboard(conn, class_id, None)
}

pub fn student_wrongbook_items(
    conn: &Connection,
    class_id: i64,
    student_id: i64,
) -> CoreResult<Vec<WrongbookQuestion>> {
    let belongs = conn
        .query_row(
            "SELECT 1 FROM students
             WHERE id=?1 AND class_id=?2 AND enabled=1",
            (student_id, class_id),
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !belongs {
        return Err(CoreError::Invalid("学生不属于当前班级或已停用".into()));
    }
    Ok(wrongbook_dashboard(conn, class_id, Some(student_id))?.items)
}

#[cfg(test)]
#[path = "read_model_tests.rs"]
mod tests;
