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

pub const CLASS_WRONGBOOK_SCHEMA_VERSION: i64 = 3;
pub const CLASS_WRONGBOOK_RULE_VERSION: &str = "m3-published-wrong-facts-v3";
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
    let rows = statement.query_map([class_id], |row| {
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

pub fn class_wrongbook_dashboard(
    conn: &Connection,
    class_id: i64,
) -> CoreResult<ClassWrongbookDashboard> {
    let class = load_class(conn, class_id)?;
    let responses = load_published_responses(conn, class_id)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = "2026-07-16T08:00:00Z";

    struct Fixture {
        conn: Connection,
        class_one: i64,
        class_two: i64,
        students: Vec<i64>,
        assessment_version_id: i64,
        assessment_item_id: i64,
        next_publication_revision: i64,
    }

    impl Fixture {
        fn new() -> Self {
            let conn = Connection::open_in_memory().unwrap();
            suite_core::db::run_migrations(&conn, suite_core::db::CORE_MIGRATIONS).unwrap();
            suite_core::db::run_migrations(&conn, module_knowledge::knowledge_migrations())
                .unwrap();
            suite_core::db::run_migrations(&conn, module_exam::exam_migrations()).unwrap();
            suite_core::db::run_migrations(&conn, crate::wrongbook_migrations()).unwrap();
            conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])
                .unwrap();
            let subject_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO classes(name,term,textbook) VALUES ('八年级一班','2026秋','中国历史八上')",
                [],
            )
            .unwrap();
            let class_one = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO classes(name,term,textbook) VALUES ('八年级二班','2026秋','中国历史八上')",
                [],
            )
            .unwrap();
            let class_two = conn.last_insert_rowid();
            let mut students = Vec::new();
            for (student_no, name, class_id) in [
                ("10", "小十", class_one),
                ("02", "小二", class_one),
                ("03", "小三", class_one),
                ("04", "小四", class_one),
                ("05", "外班", class_two),
            ] {
                conn.execute(
                    "INSERT INTO students(student_no,name,class_id,enabled) VALUES (?1,?2,?3,1)",
                    (student_no, name, class_id),
                )
                .unwrap();
                students.push(conn.last_insert_rowid());
            }
            conn.execute(
                "INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('edition-1',?1,'PEP','2026','中国历史八上','八年级','upper','active',?2)",
                (subject_id, NOW),
            )
            .unwrap();
            let edition_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('map-1',?1,1,'confirmed',?2,?2)",
                (edition_id, NOW),
            )
            .unwrap();
            let map_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO k1_knowledge_nodes
                 (public_id,stable_id,knowledge_map_id,title,created_at)
                 VALUES ('knowledge-1','knowledge-stable-1',?1,'洋务运动失败原因',?2)",
                (map_id, NOW),
            )
            .unwrap();
            let knowledge_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO k1_ability_dimensions
                 (public_id,stable_id,subject_id,revision,code,title,state,created_at)
                 VALUES ('ability-1','ability-stable-1',?1,1,'CAUSE','因果分析','active',?2)",
                (subject_id, NOW),
            )
            .unwrap();
            let ability_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('question-1','personal','teacher','cleared',0,?1)",
                [NOW],
            )
            .unwrap();
            let question_id = conn.last_insert_rowid();
            let question_version_public_id = "question-version-1".to_string();
            conn.execute(
                "INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES (?1,?2,1,'single','洋务运动失败的根本原因是？',2,
                  'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa',
                  'L3','published',?3)",
                (&question_version_public_id, question_id, NOW),
            )
            .unwrap();
            let question_version_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('answer-1',?1,1,'{\"schema_version\":1}','confirmed',?2,'teacher',?2)",
                (question_version_id, NOW),
            )
            .unwrap();
            let answer_key_version_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('rubric-1',?1,1,2,'confirmed',?2,'teacher',?2)",
                (question_version_id, NOW),
            )
            .unwrap();
            let rubric_version_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('links-1',?1,?2,1,'confirmed',?3,'teacher',?3)",
                (question_version_id, map_id, NOW),
            )
            .unwrap();
            let link_set_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO k1_knowledge_links
                 (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,
                  relation_type,confirmation_level,verified_by,verified_at,created_at)
                 VALUES ('knowledge-link-1',?1,'question',?2,?3,'direct_assessment',
                  'teacher_confirmed','teacher',?4,?4)",
                (link_set_id, &question_version_public_id, knowledge_id, NOW),
            )
            .unwrap();
            conn.execute(
                "INSERT INTO k1_ability_links
                 (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
                  evidence_strength,response_mode,confirmation_level,verified_by,verified_at,created_at)
                 VALUES ('ability-link-1',?1,'question',?2,?3,0.7,'recognition',
                  'teacher_confirmed','teacher',?4,?4)",
                (
                    link_set_id,
                    &question_version_public_id,
                    ability_id,
                    NOW,
                ),
            )
            .unwrap();
            conn.execute(
                "INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,created_by,created_at,updated_at)
                 VALUES ('assessment-1','近代化单元测验',?1,'quiz','include','active','teacher',?2,?2)",
                (class_one, NOW),
            )
            .unwrap();
            let assessment_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('assessment-version-1',?1,1,
                  'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb',
                  'v1','confirmed',?2,'teacher',?2)",
                (assessment_id, NOW),
            )
            .unwrap();
            let assessment_version_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,state,created_at)
                 VALUES ('item-1',?1,?2,?3,?4,?5,0,2,'{\"schema_version\":1}','active',?6)",
                (
                    assessment_version_id,
                    question_version_id,
                    answer_key_version_id,
                    rubric_version_id,
                    link_set_id,
                    NOW,
                ),
            )
            .unwrap();
            let assessment_item_id = conn.last_insert_rowid();
            Self {
                conn,
                class_one,
                class_two,
                students,
                assessment_version_id,
                assessment_item_id,
                next_publication_revision: 1,
            }
        }

        fn publish_response(
            &mut self,
            student_id: i64,
            attempt_no: i64,
            attempt_kind: &str,
            score: f64,
            published_at: &str,
        ) -> (String, String) {
            let attempt_public_id = format!("attempt-{student_id}-{attempt_no}");
            self.conn
                .execute(
                    "INSERT INTO exam_attempts_v2
                     (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                      attempt_kind,state,created_at,updated_at)
                     VALUES (?1,?2,?3,?4,'image',?5,'published',?6,?6)",
                    (
                        &attempt_public_id,
                        self.assessment_version_id,
                        student_id,
                        attempt_no,
                        attempt_kind,
                        published_at,
                    ),
                )
                .unwrap();
            let attempt_id = self.conn.last_insert_rowid();
            let decision_public_id = format!("decision-{student_id}-{attempt_no}");
            self.conn
                .execute(
                    "INSERT INTO exam_grade_decisions_v2
                     (public_id,attempt_id,assessment_item_id,revision,teacher_score,
                      point_results_json,confirmation_level,state,decided_by,decided_at,created_at)
                     VALUES (?1,?2,?3,1,?4,'{\"schema_version\":1}',
                      'teacher_accepted','active','teacher',?5,?5)",
                    (
                        &decision_public_id,
                        attempt_id,
                        self.assessment_item_id,
                        score,
                        published_at,
                    ),
                )
                .unwrap();
            let decision_id = self.conn.last_insert_rowid();
            let publication_public_id = format!("publication-{}", self.next_publication_revision);
            self.conn
                .execute(
                    "INSERT INTO exam_grade_publications_v2
                     (public_id,assessment_version_id,revision,state,published_by,published_at,created_at)
                     VALUES (?1,?2,?3,'published','teacher',?4,?4)",
                    (
                        &publication_public_id,
                        self.assessment_version_id,
                        self.next_publication_revision,
                        published_at,
                    ),
                )
                .unwrap();
            self.next_publication_revision += 1;
            let publication_id = self.conn.last_insert_rowid();
            self.conn
                .execute(
                    "INSERT INTO exam_grade_publication_items_v2
                     (publication_id,attempt_id,grade_decision_set_hash,total_score,created_at)
                     VALUES (?1,?2,
                      'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc',
                      ?3,?4)",
                    (publication_id, attempt_id, score, published_at),
                )
                .unwrap();
            let publication_item_id = self.conn.last_insert_rowid();
            self.conn
                .execute(
                    "INSERT INTO exam_grade_publication_decisions_v2
                     (publication_item_id,attempt_id,grade_decision_id,created_at)
                     VALUES (?1,?2,?3,?4)",
                    (publication_item_id, attempt_id, decision_id, published_at),
                )
                .unwrap();
            self.conn
                .execute(
                    "UPDATE exam_attempts_v2
                     SET active_publication_id=?1 WHERE id=?2",
                    (publication_id, attempt_id),
                )
                .unwrap();
            (decision_public_id, publication_public_id)
        }
    }

    #[test]
    fn published_wrong_facts_have_explicit_correction_states_without_mastery_claims() {
        let mut fixture = Fixture::new();
        fixture.publish_response(fixture.students[0], 1, "first", 0.0, "2026-07-01T08:00:00Z");
        fixture.publish_response(
            fixture.students[0],
            2,
            "correction",
            2.0,
            "2026-07-02T08:00:00Z",
        );
        fixture.publish_response(fixture.students[1], 1, "first", 1.0, "2026-07-03T08:00:00Z");
        fixture.publish_response(fixture.students[1], 2, "retry", 2.0, "2026-07-04T08:00:00Z");
        fixture.publish_response(fixture.students[2], 1, "first", 0.0, "2026-07-05T08:00:00Z");
        fixture.publish_response(
            fixture.students[2],
            2,
            "correction",
            1.0,
            "2026-07-06T08:00:00Z",
        );
        fixture.publish_response(fixture.students[3], 1, "first", 2.0, "2026-07-07T08:00:00Z");

        let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
        assert_eq!(dashboard.summary.wrong_question_count, 3);
        assert_eq!(dashboard.summary.affected_student_count, 3);
        assert_eq!(dashboard.summary.corrected_once_count, 1);
        assert_eq!(dashboard.summary.rechecked_correct_count, 1);
        assert_eq!(dashboard.summary.needs_correction_count, 1);
        assert_eq!(dashboard.summary.repeated_error_count, 1);
        assert!(dashboard
            .summary
            .denominator_note
            .contains("不等于知识点已掌握"));
        assert_eq!(
            dashboard
                .items
                .iter()
                .find(|item| item.student_name == "小十")
                .unwrap()
                .status,
            "corrected_once"
        );
        assert_eq!(
            dashboard
                .items
                .iter()
                .find(|item| item.student_name == "小二")
                .unwrap()
                .status,
            "rechecked_correct"
        );
        let repeated = dashboard
            .items
            .iter()
            .find(|item| item.student_name == "小三")
            .unwrap();
        assert_eq!(repeated.status, "needs_correction");
        assert!(repeated.repeated_error);
        assert_eq!(repeated.knowledge_nodes[0].title, "洋务运动失败原因");
        assert_eq!(repeated.ability_dimensions[0].title, "因果分析");
        assert!(dashboard
            .items
            .iter()
            .all(|item| item.student_name != "小四"));
    }

    #[test]
    fn scope_uses_enabled_students_current_publications_and_student_number_order() {
        let mut fixture = Fixture::new();
        fixture.publish_response(fixture.students[0], 1, "first", 0.0, "2026-07-01T08:00:00Z");
        fixture.publish_response(fixture.students[1], 1, "first", 0.0, "2026-07-02T08:00:00Z");
        fixture.publish_response(fixture.students[4], 1, "first", 0.0, "2026-07-03T08:00:00Z");
        fixture
            .conn
            .execute(
                "UPDATE students SET enabled=0 WHERE id=?1",
                [fixture.students[2]],
            )
            .unwrap();

        let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
        assert_eq!(
            dashboard
                .items
                .iter()
                .map(|item| item.student_no.as_str())
                .collect::<Vec<_>>(),
            vec!["02", "10"]
        );
        assert_eq!(
            class_wrongbook_dashboard(&fixture.conn, fixture.class_two)
                .unwrap()
                .summary
                .wrong_question_count,
            0
        );
    }

    #[test]
    fn current_publication_snapshot_wins_over_superseded_history() {
        let mut fixture = Fixture::new();
        fixture.publish_response(fixture.students[0], 1, "first", 0.0, "2026-07-01T08:00:00Z");
        let attempt_id: i64 = fixture
            .conn
            .query_row(
                "SELECT id FROM exam_attempts_v2 WHERE student_id=?1 AND attempt_no=1",
                [fixture.students[0]],
                |row| row.get(0),
            )
            .unwrap();
        let old_publication_id: i64 = fixture
            .conn
            .query_row(
                "SELECT active_publication_id FROM exam_attempts_v2 WHERE id=?1",
                [attempt_id],
                |row| row.get(0),
            )
            .unwrap();
        fixture
            .conn
            .execute(
                "UPDATE exam_grade_publications_v2 SET state='superseded' WHERE id=?1",
                [old_publication_id],
            )
            .unwrap();
        fixture
            .conn
            .execute(
                "UPDATE exam_attempts_v2 SET active_publication_id=NULL WHERE id=?1",
                [attempt_id],
            )
            .unwrap();

        assert_eq!(
            class_wrongbook_dashboard(&fixture.conn, fixture.class_one)
                .unwrap()
                .summary
                .wrong_question_count,
            0
        );
    }

    #[test]
    fn legacy_wrong_item_and_mastery_tables_are_not_read() {
        let fixture = Fixture::new();
        fixture
            .conn
            .execute(
                "INSERT INTO exam_wrong_items
                 (student_id,question_id,times_wrong,status,first_seen,last_seen)
                 VALUES (?1,999,8,'open',?2,?2)",
                (fixture.students[0], NOW),
            )
            .unwrap();
        fixture
            .conn
            .execute(
                "INSERT INTO exam_knowledge_mastery
                 (student_id,knowledge_point_id,total,correct,updated_at)
                 VALUES (?1,999,10,0,?2)",
                (fixture.students[0], NOW),
            )
            .unwrap();

        let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
        assert_eq!(dashboard.summary.wrong_question_count, 0);
        assert!(dashboard.items.is_empty());
    }

    #[test]
    fn read_model_does_not_write_database() {
        let mut fixture = Fixture::new();
        fixture.publish_response(fixture.students[0], 1, "first", 0.0, "2026-07-01T08:00:00Z");
        let before: i64 = fixture
            .conn
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap();
        let _ = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
        let after: i64 = fixture
            .conn
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap();
        assert_eq!(after, before);
    }

    #[test]
    fn missing_class_is_rejected() {
        let fixture = Fixture::new();
        assert!(matches!(
            class_wrongbook_dashboard(&fixture.conn, 999),
            Err(CoreError::NotFound(_))
        ));
    }

    #[test]
    fn teacher_confirmed_error_causes_are_versioned_and_same_input_is_noop() {
        use crate::error_cause::{confirm_error_causes, ConfirmErrorCausesInput};

        let mut fixture = Fixture::new();
        let student_id = fixture.students[0];
        let (decision_public_id, publication_public_id) =
            fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
        let causes = vec!["concept_confusion".to_string(), "fact_error".to_string()];
        let input = ConfirmErrorCausesInput {
            class_id: fixture.class_one,
            student_id,
            question_version_public_id: "question-version-1",
            grade_decision_public_id: &decision_public_id,
            publication_public_id: &publication_public_id,
            cause_codes: &causes,
            teacher_note: Some("把根本原因和直接原因混淆"),
            confirmed_by: "teacher",
        };
        let first = confirm_error_causes(&mut fixture.conn, &input).unwrap();
        let repeated = confirm_error_causes(&mut fixture.conn, &input).unwrap();
        assert_eq!(first, repeated);
        assert_eq!(first.revision, 1);
        assert_eq!(
            first.cause_codes,
            vec!["fact_error".to_string(), "concept_confusion".to_string()]
        );
        assert_eq!(
            fixture
                .conn
                .query_row("SELECT COUNT(*) FROM wb_error_cause_revisions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            1
        );

        let changed_causes = vec!["fact_error".to_string()];
        let changed = confirm_error_causes(
            &mut fixture.conn,
            &ConfirmErrorCausesInput {
                cause_codes: &changed_causes,
                teacher_note: None,
                ..input
            },
        )
        .unwrap();
        assert_eq!(changed.revision, 2);
        assert_eq!(
            fixture
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM wb_error_cause_revisions WHERE state='active'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM wb_error_cause_revisions WHERE state='superseded'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
        assert_eq!(
            dashboard.items[0]
                .cause_review
                .as_ref()
                .unwrap()
                .cause_codes,
            vec!["fact_error"]
        );
        assert!(dashboard.items[0]
            .cause_options
            .iter()
            .any(|option| option.code == "misread_prompt"));
    }

    #[test]
    fn error_cause_confirmation_rejects_stale_scope_without_partial_rows() {
        use crate::error_cause::{confirm_error_causes, ConfirmErrorCausesInput};

        let mut fixture = Fixture::new();
        let student_id = fixture.students[0];
        let (old_decision, old_publication) =
            fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
        let (_latest_decision, _latest_publication) =
            fixture.publish_response(student_id, 2, "correction", 0.0, "2026-07-02T08:00:00Z");
        let causes = vec!["fact_error".to_string()];
        let stale = confirm_error_causes(
            &mut fixture.conn,
            &ConfirmErrorCausesInput {
                class_id: fixture.class_one,
                student_id,
                question_version_public_id: "question-version-1",
                grade_decision_public_id: &old_decision,
                publication_public_id: &old_publication,
                cause_codes: &causes,
                teacher_note: None,
                confirmed_by: "teacher",
            },
        );
        assert!(matches!(stale, Err(CoreError::Invalid(_))));
        assert_eq!(
            fixture
                .conn
                .query_row("SELECT COUNT(*) FROM wb_error_cause_revisions", [], |row| {
                    row.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
    }

    #[test]
    fn other_error_cause_requires_note_and_revision_rows_are_immutable() {
        use crate::error_cause::{confirm_error_causes, ConfirmErrorCausesInput};

        let mut fixture = Fixture::new();
        let student_id = fixture.students[0];
        let (decision_public_id, publication_public_id) =
            fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
        let causes = vec!["other".to_string()];
        let base = ConfirmErrorCausesInput {
            class_id: fixture.class_one,
            student_id,
            question_version_public_id: "question-version-1",
            grade_decision_public_id: &decision_public_id,
            publication_public_id: &publication_public_id,
            cause_codes: &causes,
            teacher_note: None,
            confirmed_by: "teacher",
        };
        assert!(matches!(
            confirm_error_causes(&mut fixture.conn, &base),
            Err(CoreError::Invalid(_))
        ));
        let saved = confirm_error_causes(
            &mut fixture.conn,
            &ConfirmErrorCausesInput {
                teacher_note: Some("课堂用语理解偏差"),
                ..base
            },
        )
        .unwrap();
        assert_eq!(saved.revision, 1);
        assert!(fixture
            .conn
            .execute(
                "UPDATE wb_error_cause_revisions SET teacher_note='覆盖原备注' WHERE state='active'",
                [],
            )
            .is_err());
        assert!(fixture
            .conn
            .execute("DELETE FROM wb_error_cause_items", [])
            .is_err());
    }

    #[test]
    fn single_correction_is_atomic_idempotent_and_does_not_precreate_attempt() {
        use crate::correction::{create_single_correction, CreateCorrectionInput};

        let mut fixture = Fixture::new();
        let student_id = fixture.students[0];
        let (decision_public_id, publication_public_id) =
            fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
        let source_attempt_count: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM exam_attempts_v2", [], |row| {
                row.get(0)
            })
            .unwrap();
        let input = CreateCorrectionInput {
            class_id: fixture.class_one,
            student_id,
            question_version_public_id: "question-version-1",
            source_grade_decision_public_id: &decision_public_id,
            source_publication_public_id: &publication_public_id,
            created_by: "teacher",
        };

        let created = create_single_correction(&mut fixture.conn, &input).unwrap();
        let repeated = create_single_correction(&mut fixture.conn, &input).unwrap();
        assert_eq!(created, repeated);
        assert_eq!(created.status, "waiting_upload");
        assert_eq!(created.latest_attempt_public_id, None);
        assert_eq!(
            fixture
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM wb_correction_assignments",
                    [],
                    |row| { row.get::<_, i64>(0) }
                )
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .conn
                .query_row(
                    "SELECT COUNT(*)
                     FROM exam_assessment_targets_v2 target
                     JOIN exam_assessment_versions_v2 version
                       ON version.id=target.assessment_version_id
                     WHERE version.public_id=?1 AND target.student_id=?2",
                    (&created.assessment_version_public_id, student_id),
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM exam_assessments_v2
                     WHERE assessment_context='correction'
                       AND evidence_policy='progress_only' AND state='active'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .conn
                .query_row(
                    "SELECT COUNT(*)
                     FROM exam_assessment_versions_v2 version
                     JOIN exam_assessment_items_v2 item
                       ON item.assessment_version_id=version.id AND item.state='active'
                     JOIN k1_question_versions question
                       ON question.id=item.question_version_id
                     WHERE version.public_id=?1 AND version.state='confirmed'
                       AND question.public_id='question-version-1'",
                    [&created.assessment_version_public_id],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert_eq!(
            fixture
                .conn
                .query_row("SELECT COUNT(*) FROM exam_attempts_v2", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            source_attempt_count
        );
        assert!(fixture
            .conn
            .execute(
                "UPDATE wb_correction_assignments SET created_by='other' WHERE public_id=?1",
                [&created.public_id],
            )
            .is_err());
        assert!(fixture
            .conn
            .execute(
                "DELETE FROM wb_correction_assignments WHERE public_id=?1",
                [&created.public_id],
            )
            .is_err());

        let dashboard = class_wrongbook_dashboard(&fixture.conn, fixture.class_one).unwrap();
        assert_eq!(
            dashboard.items[0].correction_assignment.as_ref().unwrap(),
            &created
        );

        let mismatched = CreateCorrectionInput {
            student_id: fixture.students[1],
            ..input
        };
        assert!(matches!(
            create_single_correction(&mut fixture.conn, &mismatched),
            Err(CoreError::Invalid(_))
        ));
    }

    #[test]
    fn correction_assignment_failure_rolls_back_m2_assessment_and_target() {
        use crate::correction::{create_single_correction, CreateCorrectionInput};

        let mut fixture = Fixture::new();
        let student_id = fixture.students[0];
        let (decision_public_id, publication_public_id) =
            fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
        fixture
            .conn
            .execute_batch(
                "CREATE TRIGGER fail_correction_assignment
                 BEFORE INSERT ON wb_correction_assignments
                 BEGIN SELECT RAISE(ABORT,'injected correction assignment failure'); END;",
            )
            .unwrap();
        let result = create_single_correction(
            &mut fixture.conn,
            &CreateCorrectionInput {
                class_id: fixture.class_one,
                student_id,
                question_version_public_id: "question-version-1",
                source_grade_decision_public_id: &decision_public_id,
                source_publication_public_id: &publication_public_id,
                created_by: "teacher",
            },
        );
        assert!(result.is_err());
        let counts: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_assessments_v2
                    WHERE assessment_context='correction'),
                   (SELECT COUNT(*) FROM exam_assessment_targets_v2),
                   (SELECT COUNT(*) FROM wb_correction_assignments)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0, 0));
    }

    #[test]
    fn stale_correction_source_rolls_back_assessment_and_assignment() {
        use crate::correction::{create_single_correction, CreateCorrectionInput};

        let mut fixture = Fixture::new();
        let student_id = fixture.students[0];
        let (old_decision, old_publication) =
            fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
        fixture.publish_response(student_id, 2, "correction", 2.0, "2026-07-02T08:00:00Z");
        let result = create_single_correction(
            &mut fixture.conn,
            &CreateCorrectionInput {
                class_id: fixture.class_one,
                student_id,
                question_version_public_id: "question-version-1",
                source_grade_decision_public_id: &old_decision,
                source_publication_public_id: &old_publication,
                created_by: "teacher",
            },
        );
        assert!(matches!(result, Err(CoreError::Invalid(_))));
        assert_eq!(
            fixture
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM wb_correction_assignments",
                    [],
                    |row| { row.get::<_, i64>(0) }
                )
                .unwrap(),
            0
        );
        assert_eq!(
            fixture
                .conn
                .query_row(
                    "SELECT COUNT(*) FROM exam_assessments_v2
                     WHERE assessment_context='correction'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }

    #[test]
    fn correction_attempt_guard_accepts_only_target_student_and_kind() {
        use crate::correction::{
            create_single_correction, load_for_source_decision, CreateCorrectionInput,
        };

        let mut fixture = Fixture::new();
        let student_id = fixture.students[0];
        let (decision_public_id, publication_public_id) =
            fixture.publish_response(student_id, 1, "first", 0.0, "2026-07-01T08:00:00Z");
        let created = create_single_correction(
            &mut fixture.conn,
            &CreateCorrectionInput {
                class_id: fixture.class_one,
                student_id,
                question_version_public_id: "question-version-1",
                source_grade_decision_public_id: &decision_public_id,
                source_publication_public_id: &publication_public_id,
                created_by: "teacher",
            },
        )
        .unwrap();
        let version_id: i64 = fixture
            .conn
            .query_row(
                "SELECT id FROM exam_assessment_versions_v2 WHERE public_id=?1",
                [&created.assessment_version_public_id],
                |row| row.get(0),
            )
            .unwrap();
        let insert_attempt =
            |conn: &Connection, public_id: &str, target_student_id: i64, attempt_kind: &str| {
                conn.execute(
                    "INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES (?1,?2,?3,1,'image',?4,'ingesting',?5,?5)",
                    (public_id, version_id, target_student_id, attempt_kind, NOW),
                )
            };

        assert!(insert_attempt(
            &fixture.conn,
            "wrong-student-attempt",
            fixture.students[1],
            "correction"
        )
        .is_err());
        assert!(insert_attempt(&fixture.conn, "wrong-kind-attempt", student_id, "retry").is_err());
        insert_attempt(
            &fixture.conn,
            "target-correction-attempt",
            student_id,
            "correction",
        )
        .unwrap();

        let loaded = load_for_source_decision(&fixture.conn, &decision_public_id)
            .unwrap()
            .unwrap();
        assert_eq!(loaded.status, "in_progress");
        assert_eq!(
            loaded.latest_attempt_public_id.as_deref(),
            Some("target-correction-attempt")
        );
    }
}
