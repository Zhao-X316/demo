//! 旧知识点/题目到 K1 的显式映射。
//!
//! 迁移本身不会根据名称、题干或主键猜关系；只有老师确认或经过验证的导入
//! 才能调用这里的写入函数。重新映射会在同一事务里保留旧 revision 并切换 active。

use rusqlite::{Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use suite_core::domain::{ids, time};
use suite_core::error::{CoreError, CoreResult};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegacyKnowledgeSource {
    Core,
    Exam,
}

impl LegacyKnowledgeSource {
    fn table(self) -> &'static str {
        match self {
            Self::Core => "knowledge_points",
            Self::Exam => "exam_knowledge_points",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeMappingMethod {
    ExactCode,
    Manual,
    VerifiedImport,
}

impl KnowledgeMappingMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::ExactCode => "exact_code",
            Self::Manual => "manual",
            Self::VerifiedImport => "verified_import",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuestionMappingMethod {
    ExactContent,
    Manual,
    VerifiedImport,
}

impl QuestionMappingMethod {
    fn as_str(self) -> &'static str {
        match self {
            Self::ExactContent => "exact_content",
            Self::Manual => "manual",
            Self::VerifiedImport => "verified_import",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyKnowledgeMapping {
    pub id: i64,
    pub public_id: String,
    pub source_table: String,
    pub legacy_id: i64,
    pub revision: i64,
    pub knowledge_node_id: i64,
    pub mapping_method: String,
    pub state: String,
    pub verified_by: String,
    pub verified_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyQuestionMapping {
    pub id: i64,
    pub public_id: String,
    pub legacy_id: i64,
    pub revision: i64,
    pub question_version_id: i64,
    pub answer_key_version_id: Option<i64>,
    pub rubric_version_id: Option<i64>,
    pub link_set_id: Option<i64>,
    pub mapping_method: String,
    pub state: String,
    pub verified_by: String,
    pub verified_at: String,
}

pub struct LegacyQuestionTarget {
    pub question_version_id: i64,
    pub answer_key_version_id: Option<i64>,
    pub rubric_version_id: Option<i64>,
    pub link_set_id: Option<i64>,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Invalid(format!("{label}不能为空")));
    }
    Ok(())
}

fn legacy_knowledge_exists(
    tx: &Transaction<'_>,
    source: LegacyKnowledgeSource,
    legacy_id: i64,
) -> CoreResult<bool> {
    let sql = match source {
        LegacyKnowledgeSource::Core => "SELECT EXISTS(SELECT 1 FROM knowledge_points WHERE id=?1)",
        LegacyKnowledgeSource::Exam => {
            "SELECT EXISTS(SELECT 1 FROM exam_knowledge_points WHERE id=?1)"
        }
    };
    Ok(tx.query_row(sql, [legacy_id], |row| row.get(0))?)
}

fn next_revision(
    tx: &Transaction<'_>,
    table: &str,
    source_table: &str,
    legacy_id: i64,
) -> CoreResult<i64> {
    let sql = match table {
        "knowledge" => {
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM k1_legacy_knowledge_mappings
             WHERE source_table=?1 AND legacy_id=?2"
        }
        "question" => {
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM k1_legacy_question_mappings
             WHERE source_table=?1 AND legacy_id=?2"
        }
        _ => return Err(CoreError::Invalid("legacy mapping 表类型非法".into())),
    };
    Ok(tx.query_row(sql, (source_table, legacy_id), |row| row.get(0))?)
}

pub fn set_legacy_knowledge_mapping(
    conn: &Connection,
    source: LegacyKnowledgeSource,
    legacy_id: i64,
    knowledge_node_id: i64,
    method: KnowledgeMappingMethod,
    verified_by: &str,
) -> CoreResult<LegacyKnowledgeMapping> {
    if legacy_id <= 0 || knowledge_node_id <= 0 {
        return Err(CoreError::Invalid("legacy/knowledge id 必须大于 0".into()));
    }
    required(verified_by, "映射确认人")?;
    let source_table = source.table();
    let verified_at = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.unchecked_transaction()?;
    if !legacy_knowledge_exists(&tx, source, legacy_id)? {
        return Err(CoreError::NotFound(format!("{source_table}#{legacy_id}")));
    }
    let target_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM k1_knowledge_nodes WHERE id=?1)",
        [knowledge_node_id],
        |row| row.get(0),
    )?;
    if !target_exists {
        return Err(CoreError::NotFound(format!(
            "k1_knowledge_nodes#{knowledge_node_id}"
        )));
    }
    let revision = next_revision(&tx, "knowledge", source_table, legacy_id)?;
    tx.execute(
        "UPDATE k1_legacy_knowledge_mappings SET state='reverted'
         WHERE source_table=?1 AND legacy_id=?2 AND state='active'",
        (source_table, legacy_id),
    )?;
    tx.execute(
        "INSERT INTO k1_legacy_knowledge_mappings
         (public_id, source_table, legacy_id, revision, knowledge_node_id, mapping_method,
          state, verified_by, verified_at, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,'active',?7,?8,?8)",
        (
            &public_id,
            source_table,
            legacy_id,
            revision,
            knowledge_node_id,
            method.as_str(),
            verified_by.trim(),
            &verified_at,
        ),
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(LegacyKnowledgeMapping {
        id,
        public_id,
        source_table: source_table.into(),
        legacy_id,
        revision,
        knowledge_node_id,
        mapping_method: method.as_str().into(),
        state: "active".into(),
        verified_by: verified_by.trim().into(),
        verified_at,
    })
}

fn target_belongs_to_question(
    tx: &Transaction<'_>,
    table: &str,
    id: i64,
    question_version_id: i64,
) -> CoreResult<bool> {
    let sql = match table {
        "answer" => {
            "SELECT EXISTS(SELECT 1 FROM k1_answer_key_versions
             WHERE id=?1 AND question_version_id=?2)"
        }
        "rubric" => {
            "SELECT EXISTS(SELECT 1 FROM k1_rubric_versions
             WHERE id=?1 AND question_version_id=?2)"
        }
        "link_set" => {
            "SELECT EXISTS(SELECT 1 FROM k1_link_sets
             WHERE id=?1 AND question_version_id=?2)"
        }
        _ => return Err(CoreError::Invalid("K1 固定版本类型非法".into())),
    };
    Ok(tx.query_row(sql, (id, question_version_id), |row| row.get(0))?)
}

pub fn set_legacy_question_mapping(
    conn: &Connection,
    legacy_id: i64,
    target: &LegacyQuestionTarget,
    method: QuestionMappingMethod,
    verified_by: &str,
) -> CoreResult<LegacyQuestionMapping> {
    if legacy_id <= 0 || target.question_version_id <= 0 {
        return Err(CoreError::Invalid(
            "legacy/question version id 必须大于 0".into(),
        ));
    }
    required(verified_by, "映射确认人")?;
    let verified_at = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.unchecked_transaction()?;
    let legacy_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_questions WHERE id=?1)",
        [legacy_id],
        |row| row.get(0),
    )?;
    if !legacy_exists {
        return Err(CoreError::NotFound(format!("exam_questions#{legacy_id}")));
    }
    let question_exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM k1_question_versions WHERE id=?1)",
        [target.question_version_id],
        |row| row.get(0),
    )?;
    if !question_exists {
        return Err(CoreError::NotFound(format!(
            "k1_question_versions#{}",
            target.question_version_id
        )));
    }
    for (table, value) in [
        ("answer", target.answer_key_version_id),
        ("rubric", target.rubric_version_id),
        ("link_set", target.link_set_id),
    ] {
        if let Some(id) = value {
            if !target_belongs_to_question(&tx, table, id, target.question_version_id)? {
                return Err(CoreError::Invalid(format!(
                    "{table}#{id} 不属于 question_version#{}",
                    target.question_version_id
                )));
            }
        }
    }
    let revision = next_revision(&tx, "question", "exam_questions", legacy_id)?;
    tx.execute(
        "UPDATE k1_legacy_question_mappings SET state='reverted'
         WHERE source_table='exam_questions' AND legacy_id=?1 AND state='active'",
        [legacy_id],
    )?;
    tx.execute(
        "INSERT INTO k1_legacy_question_mappings
         (public_id, source_table, legacy_id, revision, question_version_id,
          answer_key_version_id, rubric_version_id, link_set_id, mapping_method, state,
          verified_by, verified_at, created_at)
         VALUES (?1,'exam_questions',?2,?3,?4,?5,?6,?7,?8,'active',?9,?10,?10)",
        (
            &public_id,
            legacy_id,
            revision,
            target.question_version_id,
            target.answer_key_version_id,
            target.rubric_version_id,
            target.link_set_id,
            method.as_str(),
            verified_by.trim(),
            &verified_at,
        ),
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    Ok(LegacyQuestionMapping {
        id,
        public_id,
        legacy_id,
        revision,
        question_version_id: target.question_version_id,
        answer_key_version_id: target.answer_key_version_id,
        rubric_version_id: target.rubric_version_id,
        link_set_id: target.link_set_id,
        mapping_method: method.as_str().into(),
        state: "active".into(),
        verified_by: verified_by.trim().into(),
        verified_at,
    })
}

pub fn active_legacy_question_mapping(
    conn: &Connection,
    legacy_id: i64,
) -> CoreResult<Option<LegacyQuestionMapping>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, legacy_id, revision, question_version_id,
                    answer_key_version_id, rubric_version_id, link_set_id, mapping_method,
                    state, verified_by, verified_at
             FROM k1_legacy_question_mappings
             WHERE source_table='exam_questions' AND legacy_id=?1 AND state='active'",
            [legacy_id],
            |row| {
                Ok(LegacyQuestionMapping {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    legacy_id: row.get(2)?,
                    revision: row.get(3)?,
                    question_version_id: row.get(4)?,
                    answer_key_version_id: row.get(5)?,
                    rubric_version_id: row.get(6)?,
                    link_set_id: row.get(7)?,
                    mapping_method: row.get(8)?,
                    state: row.get(9)?,
                    verified_by: row.get(10)?,
                    verified_at: row.get(11)?,
                })
            },
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::content::{
        create_answer_key_version, create_question, create_question_version, NewAnswerKeyVersion,
        NewQuestion, NewQuestionVersion,
    };
    use crate::db::taxonomy::{
        create_curriculum_node, create_knowledge_map, create_knowledge_node,
        create_textbook_edition, NewCurriculumNode, NewKnowledgeMap, NewKnowledgeNode,
        NewTextbookEdition,
    };
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> (Connection, i64, i64) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::knowledge_migrations()).unwrap();
        conn.execute_batch(
            "CREATE TABLE exam_knowledge_points(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             CREATE TABLE exam_questions(id INTEGER PRIMARY KEY, stem TEXT NOT NULL);
             INSERT INTO subjects(name) VALUES ('历史');
             INSERT INTO knowledge_points(subject_id, name) VALUES (1, '旧核心知识点');
             INSERT INTO exam_knowledge_points(id, name) VALUES (7, '旧考试知识点');
             INSERT INTO exam_questions(id, stem) VALUES (9, '旧题目');",
        )
        .unwrap();
        let edition = create_textbook_edition(
            &conn,
            &NewTextbookEdition {
                subject_id: 1,
                publisher_code: "PEP",
                edition_code: "2024",
                title: "八上历史",
                grade: "8",
                volume: "upper",
                curriculum_region: None,
            },
        )
        .unwrap();
        let map = create_knowledge_map(
            &conn,
            &NewKnowledgeMap {
                textbook_edition_id: edition.id,
                revision: 1,
                state: "confirmed",
                supersedes_map_id: None,
            },
        )
        .unwrap();
        let lesson = create_curriculum_node(
            &conn,
            &NewCurriculumNode {
                stable_id: None,
                knowledge_map_id: map.id,
                parent_id: None,
                node_type: "lesson",
                code: None,
                title: "第一课",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        let knowledge = create_knowledge_node(
            &conn,
            &NewKnowledgeNode {
                stable_id: None,
                knowledge_map_id: map.id,
                curriculum_node_id: Some(lesson.id),
                parent_id: None,
                code: None,
                title: "鸦片战争时间",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        let question = create_question(
            &conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: "teacher",
                question_family_id: None,
                rights_status: "unknown",
                sharing_allowed: false,
            },
        )
        .unwrap();
        let question_version = create_question_version(
            &conn,
            &NewQuestionVersion {
                question_id: question.id,
                revision: 1,
                question_type: "true_false",
                stem: "鸦片战争爆发于 1840 年。",
                material_text: None,
                max_score: 1.0,
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "L0",
                state: "draft",
                options: &[],
            },
        )
        .unwrap();
        (conn, knowledge.id, question_version.id)
    }

    #[test]
    fn legacy_rows_remain_unmapped_until_explicitly_verified() {
        let (conn, knowledge_id, _) = setup();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM k1_legacy_knowledge_mappings",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);

        let first = set_legacy_knowledge_mapping(
            &conn,
            LegacyKnowledgeSource::Core,
            1,
            knowledge_id,
            KnowledgeMappingMethod::Manual,
            "teacher",
        )
        .unwrap();
        let second = set_legacy_knowledge_mapping(
            &conn,
            LegacyKnowledgeSource::Core,
            1,
            knowledge_id,
            KnowledgeMappingMethod::VerifiedImport,
            "reviewer",
        )
        .unwrap();
        assert_eq!(first.revision, 1);
        assert_eq!(second.revision, 2);
        let active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM k1_legacy_knowledge_mappings WHERE state='active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(active, 1);
    }

    #[test]
    fn question_mapping_freezes_only_versions_from_the_same_question() {
        let (conn, _, question_version_id) = setup();
        let answer = create_answer_key_version(
            &conn,
            &NewAnswerKeyVersion {
                question_version_id,
                revision: 1,
                answer_json: r#"{"schema_version":1,"correct":true}"#,
                state: "confirmed",
                confirmed_by: Some("teacher"),
                supersedes_answer_key_id: None,
                slots: &[],
            },
        )
        .unwrap();
        let mapping = set_legacy_question_mapping(
            &conn,
            9,
            &LegacyQuestionTarget {
                question_version_id,
                answer_key_version_id: Some(answer.id),
                rubric_version_id: None,
                link_set_id: None,
            },
            QuestionMappingMethod::Manual,
            "teacher",
        )
        .unwrap();
        assert_eq!(mapping.answer_key_version_id, Some(answer.id));
        assert_eq!(
            active_legacy_question_mapping(&conn, 9)
                .unwrap()
                .unwrap()
                .question_version_id,
            question_version_id
        );
    }
}
