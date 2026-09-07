//! K1-0 教材、课程、知识、考点和能力仓储。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::domain::{ids, time};
use suite_core::error::{CoreError, CoreResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextbookEdition {
    pub id: i64,
    pub public_id: String,
    pub subject_id: i64,
    pub publisher_code: String,
    pub edition_code: String,
    pub title: String,
    pub grade: String,
    pub volume: String,
    pub curriculum_region: Option<String>,
    pub state: String,
    pub created_at: String,
}

pub struct NewTextbookEdition<'a> {
    pub subject_id: i64,
    pub publisher_code: &'a str,
    pub edition_code: &'a str,
    pub title: &'a str,
    pub grade: &'a str,
    pub volume: &'a str,
    pub curriculum_region: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeMap {
    pub id: i64,
    pub public_id: String,
    pub textbook_edition_id: i64,
    pub revision: i64,
    pub state: String,
    pub supersedes_map_id: Option<i64>,
    pub created_at: String,
    pub confirmed_at: Option<String>,
}

pub struct NewKnowledgeMap {
    pub textbook_edition_id: i64,
    pub revision: i64,
    pub state: &'static str,
    pub supersedes_map_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurriculumNode {
    pub id: i64,
    pub public_id: String,
    pub stable_id: String,
    pub knowledge_map_id: i64,
    pub parent_id: Option<i64>,
    pub node_type: String,
    pub code: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub order_index: i64,
    pub state: String,
    pub created_at: String,
}

pub struct NewCurriculumNode<'a> {
    pub stable_id: Option<&'a str>,
    pub knowledge_map_id: i64,
    pub parent_id: Option<i64>,
    pub node_type: &'a str,
    pub code: Option<&'a str>,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub order_index: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgeNode {
    pub id: i64,
    pub public_id: String,
    pub stable_id: String,
    pub knowledge_map_id: i64,
    pub curriculum_node_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub code: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub order_index: i64,
    pub state: String,
    pub created_at: String,
}

pub struct NewKnowledgeNode<'a> {
    pub stable_id: Option<&'a str>,
    pub knowledge_map_id: i64,
    pub curriculum_node_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub code: Option<&'a str>,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub order_index: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExamPoint {
    pub id: i64,
    pub public_id: String,
    pub stable_id: String,
    pub knowledge_map_id: i64,
    pub knowledge_node_id: Option<i64>,
    pub code: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub created_at: String,
}

pub struct NewExamPoint<'a> {
    pub stable_id: Option<&'a str>,
    pub knowledge_map_id: i64,
    pub knowledge_node_id: Option<i64>,
    pub code: Option<&'a str>,
    pub title: &'a str,
    pub description: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbilityDimension {
    pub id: i64,
    pub public_id: String,
    pub stable_id: String,
    pub subject_id: i64,
    pub revision: i64,
    pub code: String,
    pub title: String,
    pub description: Option<String>,
    pub state: String,
    pub supersedes_dimension_id: Option<i64>,
    pub created_at: String,
}

pub struct NewAbilityDimension<'a> {
    pub stable_id: Option<&'a str>,
    pub subject_id: i64,
    pub revision: i64,
    pub code: &'a str,
    pub title: &'a str,
    pub description: Option<&'a str>,
    pub supersedes_dimension_id: Option<i64>,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Invalid(format!("{label}不能为空")));
    }
    Ok(())
}

pub fn create_textbook_edition(
    conn: &Connection,
    input: &NewTextbookEdition<'_>,
) -> CoreResult<TextbookEdition> {
    for (value, label) in [
        (input.publisher_code, "出版社代码"),
        (input.edition_code, "教材版本代码"),
        (input.title, "教材名称"),
        (input.grade, "年级"),
    ] {
        required(value, label)?;
    }
    if !matches!(input.volume, "all" | "upper" | "lower") {
        return Err(CoreError::Invalid("册次必须是 all/upper/lower".into()));
    }
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO k1_textbook_editions
         (public_id, subject_id, publisher_code, edition_code, title, grade, volume,
          curriculum_region, state, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'active',?9)",
        (
            &public_id,
            input.subject_id,
            input.publisher_code.trim(),
            input.edition_code.trim(),
            input.title.trim(),
            input.grade.trim(),
            input.volume,
            input.curriculum_region.map(str::trim),
            &created_at,
        ),
    )?;
    get_textbook_edition(conn, conn.last_insert_rowid())?
        .ok_or_else(|| CoreError::NotFound("刚创建的教材版本".into()))
}

pub fn get_textbook_edition(conn: &Connection, id: i64) -> CoreResult<Option<TextbookEdition>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, subject_id, publisher_code, edition_code, title, grade,
                    volume, curriculum_region, state, created_at
             FROM k1_textbook_editions WHERE id=?1",
            [id],
            |row| {
                Ok(TextbookEdition {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    subject_id: row.get(2)?,
                    publisher_code: row.get(3)?,
                    edition_code: row.get(4)?,
                    title: row.get(5)?,
                    grade: row.get(6)?,
                    volume: row.get(7)?,
                    curriculum_region: row.get(8)?,
                    state: row.get(9)?,
                    created_at: row.get(10)?,
                })
            },
        )
        .optional()?)
}

pub fn create_knowledge_map(
    conn: &Connection,
    input: &NewKnowledgeMap,
) -> CoreResult<KnowledgeMap> {
    if input.revision <= 0 || !matches!(input.state, "draft" | "confirmed" | "retired") {
        return Err(CoreError::Invalid("知识地图 revision/state 非法".into()));
    }
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    let confirmed_at = (input.state == "confirmed").then(|| created_at.clone());
    conn.execute(
        "INSERT INTO k1_knowledge_maps
         (public_id, textbook_edition_id, revision, state, supersedes_map_id, created_at, confirmed_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        (
            &public_id,
            input.textbook_edition_id,
            input.revision,
            input.state,
            input.supersedes_map_id,
            &created_at,
            confirmed_at.as_deref(),
        ),
    )?;
    get_knowledge_map(conn, conn.last_insert_rowid())?
        .ok_or_else(|| CoreError::NotFound("刚创建的知识地图".into()))
}

pub fn get_knowledge_map(conn: &Connection, id: i64) -> CoreResult<Option<KnowledgeMap>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, textbook_edition_id, revision, state, supersedes_map_id,
                    created_at, confirmed_at FROM k1_knowledge_maps WHERE id=?1",
            [id],
            |row| {
                Ok(KnowledgeMap {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    textbook_edition_id: row.get(2)?,
                    revision: row.get(3)?,
                    state: row.get(4)?,
                    supersedes_map_id: row.get(5)?,
                    created_at: row.get(6)?,
                    confirmed_at: row.get(7)?,
                })
            },
        )
        .optional()?)
}

fn stable_or_new(stable_id: Option<&str>) -> String {
    stable_id
        .map(str::to_owned)
        .unwrap_or_else(ids::new_public_id)
}

pub fn create_curriculum_node(
    conn: &Connection,
    input: &NewCurriculumNode<'_>,
) -> CoreResult<CurriculumNode> {
    required(input.title, "课程节点名称")?;
    if !matches!(input.node_type, "unit" | "lesson" | "topic") {
        return Err(CoreError::Invalid("课程节点类型非法".into()));
    }
    let public_id = ids::new_public_id();
    let stable_id = stable_or_new(input.stable_id);
    let created_at = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO k1_curriculum_nodes
         (public_id, stable_id, knowledge_map_id, parent_id, node_type, code, title,
          description, order_index, state, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',?10)",
        (
            &public_id,
            &stable_id,
            input.knowledge_map_id,
            input.parent_id,
            input.node_type,
            input.code.map(str::trim),
            input.title.trim(),
            input.description.map(str::trim),
            input.order_index,
            &created_at,
        ),
    )?;
    get_curriculum_node(conn, conn.last_insert_rowid())?
        .ok_or_else(|| CoreError::NotFound("刚创建的课程节点".into()))
}

pub fn get_curriculum_node(conn: &Connection, id: i64) -> CoreResult<Option<CurriculumNode>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, stable_id, knowledge_map_id, parent_id, node_type, code,
                    title, description, order_index, state, created_at
             FROM k1_curriculum_nodes WHERE id=?1",
            [id],
            |row| {
                Ok(CurriculumNode {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    stable_id: row.get(2)?,
                    knowledge_map_id: row.get(3)?,
                    parent_id: row.get(4)?,
                    node_type: row.get(5)?,
                    code: row.get(6)?,
                    title: row.get(7)?,
                    description: row.get(8)?,
                    order_index: row.get(9)?,
                    state: row.get(10)?,
                    created_at: row.get(11)?,
                })
            },
        )
        .optional()?)
}

pub fn create_knowledge_node(
    conn: &Connection,
    input: &NewKnowledgeNode<'_>,
) -> CoreResult<KnowledgeNode> {
    required(input.title, "知识点名称")?;
    let public_id = ids::new_public_id();
    let stable_id = stable_or_new(input.stable_id);
    let created_at = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO k1_knowledge_nodes
         (public_id, stable_id, knowledge_map_id, curriculum_node_id, parent_id, code, title,
          description, order_index, state, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',?10)",
        (
            &public_id,
            &stable_id,
            input.knowledge_map_id,
            input.curriculum_node_id,
            input.parent_id,
            input.code.map(str::trim),
            input.title.trim(),
            input.description.map(str::trim),
            input.order_index,
            &created_at,
        ),
    )?;
    get_knowledge_node(conn, conn.last_insert_rowid())?
        .ok_or_else(|| CoreError::NotFound("刚创建的知识点".into()))
}

pub fn get_knowledge_node(conn: &Connection, id: i64) -> CoreResult<Option<KnowledgeNode>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, stable_id, knowledge_map_id, curriculum_node_id, parent_id,
                    code, title, description, order_index, state, created_at
             FROM k1_knowledge_nodes WHERE id=?1",
            [id],
            |row| {
                Ok(KnowledgeNode {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    stable_id: row.get(2)?,
                    knowledge_map_id: row.get(3)?,
                    curriculum_node_id: row.get(4)?,
                    parent_id: row.get(5)?,
                    code: row.get(6)?,
                    title: row.get(7)?,
                    description: row.get(8)?,
                    order_index: row.get(9)?,
                    state: row.get(10)?,
                    created_at: row.get(11)?,
                })
            },
        )
        .optional()?)
}

pub fn create_exam_point(conn: &Connection, input: &NewExamPoint<'_>) -> CoreResult<ExamPoint> {
    required(input.title, "考点名称")?;
    let public_id = ids::new_public_id();
    let stable_id = stable_or_new(input.stable_id);
    let created_at = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO k1_exam_points
         (public_id, stable_id, knowledge_map_id, knowledge_node_id, code, title,
          description, state, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'active',?8)",
        (
            &public_id,
            &stable_id,
            input.knowledge_map_id,
            input.knowledge_node_id,
            input.code.map(str::trim),
            input.title.trim(),
            input.description.map(str::trim),
            &created_at,
        ),
    )?;
    get_exam_point(conn, conn.last_insert_rowid())?
        .ok_or_else(|| CoreError::NotFound("刚创建的考点".into()))
}

pub fn get_exam_point(conn: &Connection, id: i64) -> CoreResult<Option<ExamPoint>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, stable_id, knowledge_map_id, knowledge_node_id, code, title,
                    description, state, created_at FROM k1_exam_points WHERE id=?1",
            [id],
            |row| {
                Ok(ExamPoint {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    stable_id: row.get(2)?,
                    knowledge_map_id: row.get(3)?,
                    knowledge_node_id: row.get(4)?,
                    code: row.get(5)?,
                    title: row.get(6)?,
                    description: row.get(7)?,
                    state: row.get(8)?,
                    created_at: row.get(9)?,
                })
            },
        )
        .optional()?)
}

pub fn create_ability_dimension(
    conn: &Connection,
    input: &NewAbilityDimension<'_>,
) -> CoreResult<AbilityDimension> {
    required(input.code, "能力代码")?;
    required(input.title, "能力名称")?;
    if input.revision <= 0 {
        return Err(CoreError::Invalid("能力 revision 必须大于 0".into()));
    }
    let public_id = ids::new_public_id();
    let stable_id = stable_or_new(input.stable_id);
    let created_at = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO k1_ability_dimensions
         (public_id, stable_id, subject_id, revision, code, title, description, state,
          supersedes_dimension_id, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'active',?8,?9)",
        (
            &public_id,
            &stable_id,
            input.subject_id,
            input.revision,
            input.code.trim(),
            input.title.trim(),
            input.description.map(str::trim),
            input.supersedes_dimension_id,
            &created_at,
        ),
    )?;
    get_ability_dimension(conn, conn.last_insert_rowid())?
        .ok_or_else(|| CoreError::NotFound("刚创建的能力维度".into()))
}

pub fn get_ability_dimension(conn: &Connection, id: i64) -> CoreResult<Option<AbilityDimension>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, stable_id, subject_id, revision, code, title, description,
                    state, supersedes_dimension_id, created_at
             FROM k1_ability_dimensions WHERE id=?1",
            [id],
            |row| {
                Ok(AbilityDimension {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    stable_id: row.get(2)?,
                    subject_id: row.get(3)?,
                    revision: row.get(4)?,
                    code: row.get(5)?,
                    title: row.get(6)?,
                    description: row.get(7)?,
                    state: row.get(8)?,
                    supersedes_dimension_id: row.get(9)?,
                    created_at: row.get(10)?,
                })
            },
        )
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> Connection {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::knowledge_migrations()).unwrap();
        conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])
            .unwrap();
        conn
    }

    fn edition_and_map(conn: &Connection, revision: i64) -> (TextbookEdition, KnowledgeMap) {
        let edition = create_textbook_edition(
            conn,
            &NewTextbookEdition {
                subject_id: 1,
                publisher_code: "PEP",
                edition_code: "2024",
                title: "中国历史八年级上册",
                grade: "8",
                volume: "upper",
                curriculum_region: Some("CN"),
            },
        )
        .unwrap_or_else(|_| get_textbook_edition(conn, 1).unwrap().unwrap());
        let map = create_knowledge_map(
            conn,
            &NewKnowledgeMap {
                textbook_edition_id: edition.id,
                revision,
                state: "confirmed",
                supersedes_map_id: (revision > 1).then_some(revision - 1),
            },
        )
        .unwrap();
        (edition, map)
    }

    #[test]
    fn creates_versioned_taxonomy_with_stable_ids() {
        let conn = setup();
        let (_, map1) = edition_and_map(&conn, 1);
        let unit = create_curriculum_node(
            &conn,
            &NewCurriculumNode {
                stable_id: None,
                knowledge_map_id: map1.id,
                parent_id: None,
                node_type: "unit",
                code: Some("U1"),
                title: "中国开始沦为半殖民地半封建社会",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        let knowledge = create_knowledge_node(
            &conn,
            &NewKnowledgeNode {
                stable_id: None,
                knowledge_map_id: map1.id,
                curriculum_node_id: Some(unit.id),
                parent_id: None,
                code: Some("K-OPIUM-CAUSE"),
                title: "鸦片战争爆发原因",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        create_exam_point(
            &conn,
            &NewExamPoint {
                stable_id: None,
                knowledge_map_id: map1.id,
                knowledge_node_id: Some(knowledge.id),
                code: Some("EP-CAUSE"),
                title: "近代侵略战争原因",
                description: None,
            },
        )
        .unwrap();
        let ability = create_ability_dimension(
            &conn,
            &NewAbilityDimension {
                stable_id: None,
                subject_id: 1,
                revision: 1,
                code: "causal_analysis",
                title: "因果分析",
                description: None,
                supersedes_dimension_id: None,
            },
        )
        .unwrap();

        assert!(!unit.stable_id.is_empty());
        let (_, map2) = edition_and_map(&conn, 2);
        let revised_unit = create_curriculum_node(
            &conn,
            &NewCurriculumNode {
                stable_id: Some(&unit.stable_id),
                knowledge_map_id: map2.id,
                parent_id: None,
                node_type: "unit",
                code: Some("U1"),
                title: &unit.title,
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        assert_ne!(revised_unit.id, unit.id);
        assert_eq!(revised_unit.stable_id, unit.stable_id);
        let original_unit = get_curriculum_node(&conn, unit.id).unwrap().unwrap();
        assert_eq!(original_unit.stable_id, unit.stable_id);
        assert_eq!(original_unit.knowledge_map_id, map1.id);
        assert_eq!(knowledge.knowledge_map_id, map1.id);
        assert_eq!(ability.revision, 1);
        assert!(conn
            .execute(
                "UPDATE k1_knowledge_nodes SET title='静默覆盖' WHERE id=?1",
                [knowledge.id]
            )
            .is_err());
    }

    #[test]
    fn rejects_cross_map_parent_and_anchor() {
        let conn = setup();
        let (_, map1) = edition_and_map(&conn, 1);
        let (_, map2) = edition_and_map(&conn, 2);
        let unit = create_curriculum_node(
            &conn,
            &NewCurriculumNode {
                stable_id: None,
                knowledge_map_id: map1.id,
                parent_id: None,
                node_type: "unit",
                code: Some("U1"),
                title: "第一单元",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        let result = create_knowledge_node(
            &conn,
            &NewKnowledgeNode {
                stable_id: None,
                knowledge_map_id: map2.id,
                curriculum_node_id: Some(unit.id),
                parent_id: None,
                code: None,
                title: "跨版本错误锚点",
                description: None,
                order_index: 1,
            },
        );
        assert!(result.is_err());
    }
}
