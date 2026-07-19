//! K1-4 可解释组卷蓝图。
//!
//! 预览只读取已发布 L3+ 题目、已确认答案/rubric/link set 和老师确认的
//! 知识/能力链接。确认时重新计算预览并校验 hash，在同一事务中冻结蓝图、
//! 题目解释和 M2 作业版本；不创建 attempt、不布置任务、不发布成绩。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use super::assessment::{
    create_confirmed_class_assessment_in_transaction, NewClassAssessment, NewTargetedAssessmentItem,
};

pub const BLUEPRINT_SCHEMA_VERSION: i64 = 1;
pub const BLUEPRINT_RULE_VERSION: &str = "k1-explainable-blueprint-v1";
pub const BLUEPRINT_TEMPLATE_VERSION: &str = "k1-blueprint-assessment-v1";
const QUESTION_TYPES: [&str; 5] = [
    "single",
    "multiple",
    "true_false",
    "fill_blank",
    "short_answer",
];
const MAX_ITEMS: usize = 50;
const MAX_REQUIRED_KNOWLEDGE: usize = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintClassOption {
    pub id: i64,
    pub name: String,
    pub term: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintMapOption {
    pub public_id: String,
    pub title: String,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintCurriculumOption {
    pub public_id: String,
    pub knowledge_map_public_id: String,
    pub parent_public_id: Option<String>,
    pub node_type: String,
    pub title: String,
    pub order_index: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintKnowledgeOption {
    pub public_id: String,
    pub knowledge_map_public_id: String,
    pub curriculum_node_public_id: Option<String>,
    pub title: String,
    pub order_index: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintOptions {
    pub classes: Vec<BlueprintClassOption>,
    pub knowledge_maps: Vec<BlueprintMapOption>,
    pub curriculum_nodes: Vec<BlueprintCurriculumOption>,
    pub knowledge_nodes: Vec<BlueprintKnowledgeOption>,
    pub question_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintQuestionTypeTarget {
    pub question_type: String,
    pub count: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintPreviewRequest {
    pub class_id: i64,
    pub knowledge_map_public_id: String,
    pub curriculum_node_public_id: Option<String>,
    pub total_score: f64,
    pub question_type_targets: Vec<BlueprintQuestionTypeTarget>,
    pub required_knowledge_node_public_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintCandidate {
    pub question_version_public_id: String,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub score: f64,
    pub quality_level: String,
    pub knowledge_nodes: Vec<BlueprintCandidateKnowledge>,
    pub ability_dimensions: Vec<BlueprintCandidateAbility>,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintCandidateKnowledge {
    pub public_id: String,
    pub title: String,
    pub relation_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintCandidateAbility {
    pub public_id: String,
    pub title: String,
    pub evidence_strength: f64,
    pub response_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintSelectionSummary {
    pub selected_count: i64,
    pub selected_score: f64,
    pub selected_type_counts: Vec<BlueprintQuestionTypeTarget>,
    pub covered_required_knowledge_node_public_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintPreview {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub class_id: i64,
    pub class_name: String,
    pub knowledge_map_public_id: String,
    pub knowledge_map_title: String,
    pub curriculum_node_public_id: Option<String>,
    pub curriculum_node_title: Option<String>,
    pub total_score: f64,
    pub question_type_targets: Vec<BlueprintQuestionTypeTarget>,
    pub required_knowledge_node_public_ids: Vec<String>,
    pub preview_hash: String,
    pub candidates: Vec<BlueprintCandidate>,
    pub recommended_question_version_public_ids: Vec<String>,
    pub recommended_summary: BlueprintSelectionSummary,
    pub can_confirm: bool,
    pub blockers: Vec<String>,
    pub warnings: Vec<String>,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfirmBlueprintRequest {
    pub request_key: String,
    pub title: String,
    pub preview_request: BlueprintPreviewRequest,
    pub expected_preview_hash: String,
    pub selected_question_version_public_ids: Vec<String>,
    pub confirmed_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintAssemblyItem {
    pub order_index: i64,
    pub question_version_public_id: String,
    pub question_type: String,
    pub stem: String,
    pub score: f64,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintAssembly {
    pub public_id: String,
    pub class_id: i64,
    pub class_name: String,
    pub knowledge_map_public_id: String,
    pub knowledge_map_title: String,
    pub curriculum_node_public_id: Option<String>,
    pub curriculum_node_title: Option<String>,
    pub title: String,
    pub total_score: f64,
    pub question_type_targets: Vec<BlueprintQuestionTypeTarget>,
    pub required_knowledge_node_public_ids: Vec<String>,
    pub preview_hash: String,
    pub selected_set_hash: String,
    pub assessment_public_id: String,
    pub assessment_version_public_id: String,
    pub state: String,
    pub confirmed_by: String,
    pub confirmed_at: String,
    pub items: Vec<BlueprintAssemblyItem>,
}

#[derive(Debug, Clone)]
struct Scope {
    class_name: String,
    knowledge_map_id: i64,
    knowledge_map_public_id: String,
    knowledge_map_title: String,
    curriculum_node_id: Option<i64>,
    curriculum_node_public_id: Option<String>,
    curriculum_node_title: Option<String>,
    eligible_knowledge_ids: HashSet<i64>,
}

#[derive(Debug, Clone)]
struct InternalCandidate {
    output: BlueprintCandidate,
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
}

#[derive(Serialize)]
struct PreviewHashPayload<'a> {
    schema_version: i64,
    rule_version: &'a str,
    class_id: i64,
    knowledge_map_public_id: &'a str,
    curriculum_node_public_id: &'a Option<String>,
    total_score_millis: i64,
    question_type_targets: &'a [BlueprintQuestionTypeTarget],
    required_knowledge_node_public_ids: &'a [String],
    candidates: Vec<CandidateHashPayload<'a>>,
}

#[derive(Serialize)]
struct CandidateHashPayload<'a> {
    question_version_public_id: &'a str,
    question_type: &'a str,
    score_millis: i64,
    quality_level: &'a str,
    knowledge: Vec<(&'a str, &'a str)>,
    abilities: Vec<(&'a str, i64, &'a str)>,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn score_millis(value: f64) -> CoreResult<i64> {
    if !value.is_finite() || value <= 0.0 || value > 500.0 {
        return Err(CoreError::Invalid("蓝图总分必须在 0 至 500 分之间".into()));
    }
    Ok((value * 1000.0).round() as i64)
}

fn canonical_targets(
    targets: &[BlueprintQuestionTypeTarget],
) -> CoreResult<Vec<BlueprintQuestionTypeTarget>> {
    let mut counts = BTreeMap::new();
    for target in targets {
        if !QUESTION_TYPES.contains(&target.question_type.as_str()) {
            return Err(CoreError::Invalid(format!(
                "不支持的题型：{}",
                target.question_type
            )));
        }
        if !(0..=50).contains(&target.count) {
            return Err(CoreError::Invalid("题型数量必须在 0 至 50 之间".into()));
        }
        if counts
            .insert(target.question_type.as_str(), target.count)
            .is_some()
        {
            return Err(CoreError::Invalid("题型目标不能重复".into()));
        }
    }
    let canonical = QUESTION_TYPES
        .iter()
        .map(|question_type| BlueprintQuestionTypeTarget {
            question_type: (*question_type).into(),
            count: *counts.get(question_type).unwrap_or(&0),
        })
        .collect::<Vec<_>>();
    let total = canonical.iter().map(|target| target.count).sum::<i64>();
    if total <= 0 || total as usize > MAX_ITEMS {
        return Err(CoreError::Invalid(
            "组卷题目总数必须在 1 至 50 道之间".into(),
        ));
    }
    Ok(canonical)
}

fn canonical_required(values: &[String]) -> CoreResult<Vec<String>> {
    if values.len() > MAX_REQUIRED_KNOWLEDGE {
        return Err(CoreError::Invalid("必覆盖知识点不能超过 30 个".into()));
    }
    let mut result = BTreeSet::new();
    for value in values {
        required(value, "必覆盖知识点")?;
        if !result.insert(value.trim().to_string()) {
            return Err(CoreError::Invalid("必覆盖知识点不能重复".into()));
        }
    }
    Ok(result.into_iter().collect())
}

pub fn list_blueprint_options(conn: &Connection) -> CoreResult<BlueprintOptions> {
    let classes = {
        let mut stmt = conn.prepare("SELECT id,name,term FROM classes ORDER BY id")?;
        let rows = stmt
            .query_map([], |row| {
                Ok(BlueprintClassOption {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    term: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let knowledge_maps = {
        let mut stmt = conn.prepare(
            "SELECT map.public_id,edition.title,map.revision
             FROM k1_knowledge_maps map
             JOIN k1_textbook_editions edition ON edition.id=map.textbook_edition_id
             WHERE map.state='confirmed' AND edition.state='active'
             ORDER BY edition.title,map.revision DESC",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(BlueprintMapOption {
                    public_id: row.get(0)?,
                    title: row.get(1)?,
                    revision: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let curriculum_nodes = {
        let mut stmt = conn.prepare(
            "SELECT node.public_id,map.public_id,parent.public_id,node.node_type,
                    node.title,node.order_index
             FROM k1_curriculum_nodes node
             JOIN k1_knowledge_maps map ON map.id=node.knowledge_map_id
             LEFT JOIN k1_curriculum_nodes parent ON parent.id=node.parent_id
             WHERE map.state='confirmed' AND node.state='active'
             ORDER BY map.id,node.order_index,node.id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(BlueprintCurriculumOption {
                    public_id: row.get(0)?,
                    knowledge_map_public_id: row.get(1)?,
                    parent_public_id: row.get(2)?,
                    node_type: row.get(3)?,
                    title: row.get(4)?,
                    order_index: row.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let knowledge_nodes = {
        let mut stmt = conn.prepare(
            "SELECT knowledge.public_id,map.public_id,curriculum.public_id,
                    knowledge.title,knowledge.order_index
             FROM k1_knowledge_nodes knowledge
             JOIN k1_knowledge_maps map ON map.id=knowledge.knowledge_map_id
             LEFT JOIN k1_curriculum_nodes curriculum
               ON curriculum.id=knowledge.curriculum_node_id
             WHERE map.state='confirmed' AND knowledge.state='active'
             ORDER BY map.id,knowledge.order_index,knowledge.id",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(BlueprintKnowledgeOption {
                    public_id: row.get(0)?,
                    knowledge_map_public_id: row.get(1)?,
                    curriculum_node_public_id: row.get(2)?,
                    title: row.get(3)?,
                    order_index: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    Ok(BlueprintOptions {
        classes,
        knowledge_maps,
        curriculum_nodes,
        knowledge_nodes,
        question_types: QUESTION_TYPES.iter().map(|value| (*value).into()).collect(),
    })
}

fn load_scope(
    conn: &Connection,
    request: &BlueprintPreviewRequest,
    required: &[String],
) -> CoreResult<Scope> {
    let class_name: String = conn
        .query_row(
            "SELECT name FROM classes WHERE id=?1",
            [request.class_id],
            |row| row.get(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("class#{}", request.class_id)))?;
    let map: (i64, String, String) = conn
        .query_row(
            "SELECT map.id,map.public_id,edition.title
             FROM k1_knowledge_maps map
             JOIN k1_textbook_editions edition ON edition.id=map.textbook_edition_id
             WHERE map.public_id=?1 AND map.state='confirmed' AND edition.state='active'",
            [request.knowledge_map_public_id.trim()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("知识地图不存在或尚未确认".into()))?;
    let curriculum = if let Some(public_id) = request
        .curriculum_node_public_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(
            conn.query_row(
                "SELECT id,public_id,title FROM k1_curriculum_nodes
                 WHERE public_id=?1 AND knowledge_map_id=?2 AND state='active'",
                params![public_id, map.0],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| CoreError::Invalid("教材范围不属于所选知识地图".into()))?,
        )
    } else {
        None
    };
    let knowledge_rows = if let Some((curriculum_id, _, _)) = curriculum.as_ref() {
        let mut stmt = conn.prepare(
            "WITH RECURSIVE curriculum_scope(id) AS (
               SELECT id FROM k1_curriculum_nodes WHERE id=?1
               UNION ALL
               SELECT child.id FROM k1_curriculum_nodes child
               JOIN curriculum_scope parent ON child.parent_id=parent.id
               WHERE child.state='active'
             )
             SELECT knowledge.id,knowledge.public_id
             FROM k1_knowledge_nodes knowledge
             WHERE knowledge.knowledge_map_id=?2
               AND knowledge.state='active'
               AND knowledge.curriculum_node_id IN (SELECT id FROM curriculum_scope)",
        )?;
        let rows = stmt
            .query_map(params![curriculum_id, map.0], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    } else {
        let mut stmt = conn.prepare(
            "SELECT id,public_id FROM k1_knowledge_nodes
             WHERE knowledge_map_id=?1 AND state='active'",
        )?;
        let rows = stmt
            .query_map([map.0], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let eligible_knowledge_ids = knowledge_rows.iter().map(|row| row.0).collect();
    let knowledge_by_public_id = knowledge_rows
        .into_iter()
        .map(|(id, public_id)| (public_id, id))
        .collect::<HashMap<_, _>>();
    for public_id in required {
        if !knowledge_by_public_id.contains_key(public_id) {
            return Err(CoreError::Invalid(format!(
                "必覆盖知识点不在当前教材范围：{public_id}"
            )));
        }
    }
    Ok(Scope {
        class_name,
        knowledge_map_id: map.0,
        knowledge_map_public_id: map.1,
        knowledge_map_title: map.2,
        curriculum_node_id: curriculum.as_ref().map(|value| value.0),
        curriculum_node_public_id: curriculum.as_ref().map(|value| value.1.clone()),
        curriculum_node_title: curriculum.as_ref().map(|value| value.2.clone()),
        eligible_knowledge_ids,
    })
}

fn load_candidates(conn: &Connection, scope: &Scope) -> CoreResult<Vec<InternalCandidate>> {
    let base_rows = {
        let mut stmt = conn.prepare(
            "SELECT version.id,version.public_id,version.question_type,version.stem,
                    version.material_text,version.max_score,version.quality_level,
                    answer.id,rubric.id,links.id
             FROM k1_question_versions version
             JOIN k1_questions question ON question.id=version.question_id
             JOIN k1_answer_key_versions answer
               ON answer.id=(
                 SELECT candidate.id FROM k1_answer_key_versions candidate
                 WHERE candidate.question_version_id=version.id
                   AND candidate.state='confirmed'
                 ORDER BY candidate.revision DESC LIMIT 1
               )
             JOIN k1_rubric_versions rubric
               ON rubric.id=(
                 SELECT candidate.id FROM k1_rubric_versions candidate
                 WHERE candidate.question_version_id=version.id
                   AND candidate.state='confirmed'
                 ORDER BY candidate.revision DESC LIMIT 1
               )
             JOIN k1_link_sets links
               ON links.id=(
                 SELECT candidate.id FROM k1_link_sets candidate
                 WHERE candidate.question_version_id=version.id
                   AND candidate.knowledge_map_id=?1
                   AND candidate.state='confirmed'
                 ORDER BY candidate.revision DESC LIMIT 1
               )
             WHERE version.state='published'
               AND version.quality_level IN ('L3','L4')
               AND question.owner_scope IN ('personal','official')
               AND ABS(version.max_score-rubric.max_score) < 0.000001
             ORDER BY version.question_type,version.id",
        )?;
        let rows = stmt
            .query_map([scope.knowledge_map_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let mut result = Vec::new();
    for row in base_rows {
        let knowledge_nodes = {
            let mut stmt = conn.prepare(
                "SELECT knowledge.id,knowledge.public_id,knowledge.title,link.relation_type
                 FROM k1_knowledge_links link
                 JOIN k1_knowledge_nodes knowledge ON knowledge.id=link.knowledge_node_id
                 WHERE link.link_set_id=?1
                   AND link.confirmation_level='teacher_confirmed'
                   AND link.relation_type IN ('direct_assessment','rubric_basis')
                 ORDER BY knowledge.order_index,knowledge.id,link.relation_type",
            )?;
            let rows = stmt
                .query_map([row.9], |link_row| {
                    Ok((
                        link_row.get::<_, i64>(0)?,
                        BlueprintCandidateKnowledge {
                            public_id: link_row.get(1)?,
                            title: link_row.get(2)?,
                            relation_type: link_row.get(3)?,
                        },
                    ))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
                .into_iter()
                .filter(|(id, _)| scope.eligible_knowledge_ids.contains(id))
                .map(|(_, output)| output)
                .collect::<Vec<_>>();
            rows
        };
        if knowledge_nodes.is_empty() {
            continue;
        }
        let ability_dimensions = {
            let mut stmt = conn.prepare(
                "SELECT ability.public_id,ability.title,link.evidence_strength,
                        link.response_mode
                 FROM k1_ability_links link
                 JOIN k1_ability_dimensions ability ON ability.id=link.ability_dimension_id
                 WHERE link.link_set_id=?1
                   AND link.confirmation_level='teacher_confirmed'
                   AND ability.state='active'
                 ORDER BY ability.code,ability.id",
            )?;
            let rows = stmt
                .query_map([row.9], |link_row| {
                    Ok(BlueprintCandidateAbility {
                        public_id: link_row.get(0)?,
                        title: link_row.get(1)?,
                        evidence_strength: link_row.get(2)?,
                        response_mode: link_row.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        if ability_dimensions.is_empty() {
            continue;
        }
        let knowledge_titles = knowledge_nodes
            .iter()
            .map(|node| node.title.as_str())
            .collect::<Vec<_>>()
            .join("、");
        let ability_titles = ability_dimensions
            .iter()
            .map(|ability| ability.title.as_str())
            .collect::<Vec<_>>()
            .join("、");
        let quality_level = row.6.clone();
        result.push(InternalCandidate {
            output: BlueprintCandidate {
                question_version_public_id: row.1,
                question_type: row.2,
                stem: row.3,
                material_text: row.4,
                score: row.5,
                quality_level,
                knowledge_nodes,
                ability_dimensions,
                explanation: format!(
                    "已发布 {} 题；直接考查/评分依据：{}；能力证据：{}",
                    row.6, knowledge_titles, ability_titles
                ),
            },
            question_version_id: row.0,
            answer_key_version_id: row.7,
            rubric_version_id: row.8,
            link_set_id: row.9,
        });
    }
    Ok(result)
}

fn selection_summary(
    candidates: &[InternalCandidate],
    selected_public_ids: &[String],
    required: &[String],
) -> BlueprintSelectionSummary {
    let selected = selected_public_ids.iter().collect::<HashSet<_>>();
    let mut counts = BTreeMap::new();
    let mut score = 0.0;
    let mut covered = BTreeSet::new();
    for candidate in candidates {
        if !selected.contains(&candidate.output.question_version_public_id) {
            continue;
        }
        *counts
            .entry(candidate.output.question_type.as_str())
            .or_insert(0_i64) += 1;
        score += candidate.output.score;
        for knowledge in &candidate.output.knowledge_nodes {
            if required.contains(&knowledge.public_id) {
                covered.insert(knowledge.public_id.clone());
            }
        }
    }
    BlueprintSelectionSummary {
        selected_count: selected.len() as i64,
        selected_score: ((score * 1000.0).round()) / 1000.0,
        selected_type_counts: QUESTION_TYPES
            .iter()
            .map(|question_type| BlueprintQuestionTypeTarget {
                question_type: (*question_type).into(),
                count: *counts.get(question_type).unwrap_or(&0),
            })
            .collect(),
        covered_required_knowledge_node_public_ids: covered.into_iter().collect(),
    }
}

fn recommend(
    candidates: &[InternalCandidate],
    targets: &[BlueprintQuestionTypeTarget],
    required: &[String],
) -> Vec<String> {
    let target_counts = targets
        .iter()
        .map(|target| (target.question_type.as_str(), target.count))
        .collect::<HashMap<_, _>>();
    let mut selected = BTreeSet::new();
    let mut counts = HashMap::<&str, i64>::new();
    for required_public_id in required {
        if let Some((index, candidate)) =
            candidates.iter().enumerate().find(|(index, candidate)| {
                !selected.contains(index)
                    && candidate
                        .output
                        .knowledge_nodes
                        .iter()
                        .any(|node| node.public_id == *required_public_id)
                    && counts
                        .get(candidate.output.question_type.as_str())
                        .copied()
                        .unwrap_or(0)
                        < target_counts
                            .get(candidate.output.question_type.as_str())
                            .copied()
                            .unwrap_or(0)
            })
        {
            selected.insert(index);
            *counts
                .entry(candidate.output.question_type.as_str())
                .or_insert(0) += 1;
        }
    }
    for (index, candidate) in candidates.iter().enumerate() {
        let target = target_counts
            .get(candidate.output.question_type.as_str())
            .copied()
            .unwrap_or(0);
        let current = counts
            .get(candidate.output.question_type.as_str())
            .copied()
            .unwrap_or(0);
        if !selected.contains(&index) && current < target {
            selected.insert(index);
            *counts
                .entry(candidate.output.question_type.as_str())
                .or_insert(0) += 1;
        }
    }
    selected
        .into_iter()
        .map(|index| candidates[index].output.question_version_public_id.clone())
        .collect()
}

fn preview_hash(
    request: &BlueprintPreviewRequest,
    targets: &[BlueprintQuestionTypeTarget],
    required: &[String],
    scope: &Scope,
    candidates: &[InternalCandidate],
) -> CoreResult<String> {
    let payload = PreviewHashPayload {
        schema_version: BLUEPRINT_SCHEMA_VERSION,
        rule_version: BLUEPRINT_RULE_VERSION,
        class_id: request.class_id,
        knowledge_map_public_id: &scope.knowledge_map_public_id,
        curriculum_node_public_id: &scope.curriculum_node_public_id,
        total_score_millis: score_millis(request.total_score)?,
        question_type_targets: targets,
        required_knowledge_node_public_ids: required,
        candidates: candidates
            .iter()
            .map(|candidate| CandidateHashPayload {
                question_version_public_id: &candidate.output.question_version_public_id,
                question_type: &candidate.output.question_type,
                score_millis: (candidate.output.score * 1000.0).round() as i64,
                quality_level: &candidate.output.quality_level,
                knowledge: candidate
                    .output
                    .knowledge_nodes
                    .iter()
                    .map(|node| (node.public_id.as_str(), node.relation_type.as_str()))
                    .collect(),
                abilities: candidate
                    .output
                    .ability_dimensions
                    .iter()
                    .map(|ability| {
                        (
                            ability.public_id.as_str(),
                            (ability.evidence_strength * 1000.0).round() as i64,
                            ability.response_mode.as_str(),
                        )
                    })
                    .collect(),
            })
            .collect(),
    };
    let bytes = serde_json::to_vec(&payload)
        .map_err(|error| CoreError::Parse(format!("组卷预览 hash 序列化失败：{error}")))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn build_preview(
    conn: &Connection,
    request: &BlueprintPreviewRequest,
) -> CoreResult<(BlueprintPreview, Scope, Vec<InternalCandidate>)> {
    if request.class_id <= 0 {
        return Err(CoreError::Invalid("班级 ID 非法".into()));
    }
    required(&request.knowledge_map_public_id, "知识地图")?;
    let _ = score_millis(request.total_score)?;
    let targets = canonical_targets(&request.question_type_targets)?;
    let required = canonical_required(&request.required_knowledge_node_public_ids)?;
    let scope = load_scope(conn, request, &required)?;
    let candidates = load_candidates(conn, &scope)?;
    let recommended = recommend(&candidates, &targets, &required);
    let recommended_summary = selection_summary(&candidates, &recommended, &required);
    let mut blockers = Vec::new();
    for target in &targets {
        let available = candidates
            .iter()
            .filter(|candidate| candidate.output.question_type == target.question_type)
            .count() as i64;
        if available < target.count {
            blockers.push(format!(
                "{}题可用 {} 道，少于目标 {} 道",
                target.question_type, available, target.count
            ));
        }
    }
    for public_id in &required {
        if !candidates.iter().any(|candidate| {
            candidate
                .output
                .knowledge_nodes
                .iter()
                .any(|node| node.public_id == *public_id)
        }) {
            blockers.push(format!("必覆盖知识点暂无合格题目：{public_id}"));
        }
    }
    let mut warnings = Vec::new();
    if (recommended_summary.selected_score - request.total_score).abs() > 0.000_001 {
        warnings.push(format!(
            "系统初选共 {:.3} 分，与蓝图总分 {:.3} 分不同；可手动调整候选题后确认",
            recommended_summary.selected_score, request.total_score
        ));
    }
    if candidates.is_empty() {
        blockers.push("当前范围没有已发布 L3+ 且评价版本完整的题目".into());
    }
    let hash = preview_hash(request, &targets, &required, &scope, &candidates)?;
    let preview = BlueprintPreview {
        schema_version: BLUEPRINT_SCHEMA_VERSION,
        rule_version: BLUEPRINT_RULE_VERSION.into(),
        calculated_at: time::utc_now_rfc3339(),
        class_id: request.class_id,
        class_name: scope.class_name.clone(),
        knowledge_map_public_id: scope.knowledge_map_public_id.clone(),
        knowledge_map_title: scope.knowledge_map_title.clone(),
        curriculum_node_public_id: scope.curriculum_node_public_id.clone(),
        curriculum_node_title: scope.curriculum_node_title.clone(),
        total_score: request.total_score,
        question_type_targets: targets,
        required_knowledge_node_public_ids: required,
        preview_hash: hash,
        candidates: candidates
            .iter()
            .map(|candidate| candidate.output.clone())
            .collect(),
        recommended_question_version_public_ids: recommended,
        recommended_summary,
        can_confirm: blockers.is_empty(),
        blockers,
        warnings,
        boundary_note:
            "只使用已发布 L3+ 题目和老师确认的知识/能力链接；确认后仅冻结全班作业版本，不创建学生作答、不发布成绩。"
                .into(),
    };
    Ok((preview, scope, candidates))
}

pub fn preview_blueprint(
    conn: &Connection,
    request: &BlueprintPreviewRequest,
) -> CoreResult<BlueprintPreview> {
    Ok(build_preview(conn, request)?.0)
}

fn canonical_selected<'a>(
    candidates: &'a [InternalCandidate],
    selected: &[String],
) -> CoreResult<Vec<&'a InternalCandidate>> {
    if selected.is_empty() || selected.len() > MAX_ITEMS {
        return Err(CoreError::Invalid(
            "入选题目数量必须在 1 至 50 道之间".into(),
        ));
    }
    let selected_set = selected
        .iter()
        .map(|value| value.trim().to_string())
        .collect::<BTreeSet<_>>();
    if selected_set.len() != selected.len() {
        return Err(CoreError::Invalid("入选题目不能重复".into()));
    }
    let result = candidates
        .iter()
        .filter(|candidate| selected_set.contains(&candidate.output.question_version_public_id))
        .collect::<Vec<_>>();
    if result.len() != selected.len() {
        return Err(CoreError::Invalid(
            "入选题目不属于当前预览，或已不再满足质量闸门".into(),
        ));
    }
    Ok(result)
}

fn validate_selected(
    preview: &BlueprintPreview,
    selected: &[&InternalCandidate],
) -> CoreResult<()> {
    let mut counts = BTreeMap::new();
    let mut total_score = 0.0;
    let mut covered = BTreeSet::new();
    for candidate in selected {
        *counts
            .entry(candidate.output.question_type.as_str())
            .or_insert(0_i64) += 1;
        total_score += candidate.output.score;
        for knowledge in &candidate.output.knowledge_nodes {
            covered.insert(knowledge.public_id.as_str());
        }
    }
    for target in &preview.question_type_targets {
        let actual = counts
            .get(target.question_type.as_str())
            .copied()
            .unwrap_or(0);
        if actual != target.count {
            return Err(CoreError::Invalid(format!(
                "{}题数量应为 {}，当前为 {}",
                target.question_type, target.count, actual
            )));
        }
    }
    if (total_score - preview.total_score).abs() > 0.000_001 {
        return Err(CoreError::Invalid(format!(
            "入选题目总分应为 {:.3}，当前为 {:.3}",
            preview.total_score, total_score
        )));
    }
    for required in &preview.required_knowledge_node_public_ids {
        if !covered.contains(required.as_str()) {
            return Err(CoreError::Invalid(format!(
                "入选题目未覆盖必选知识点：{required}"
            )));
        }
    }
    Ok(())
}

fn request_hash(
    input: &ConfirmBlueprintRequest,
    preview: &BlueprintPreview,
    selected: &[&InternalCandidate],
) -> CoreResult<String> {
    let payload = serde_json::json!({
        "schema_version": BLUEPRINT_SCHEMA_VERSION,
        "rule_version": BLUEPRINT_RULE_VERSION,
        "title": input.title.trim(),
        "class_id": preview.class_id,
        "knowledge_map_public_id": preview.knowledge_map_public_id,
        "curriculum_node_public_id": preview.curriculum_node_public_id,
        "total_score_millis": (preview.total_score * 1000.0).round() as i64,
        "question_type_targets": preview.question_type_targets,
        "required_knowledge_node_public_ids": preview.required_knowledge_node_public_ids,
        "preview_hash": preview.preview_hash,
        "selected_question_version_public_ids": selected.iter().map(|candidate| {
            candidate.output.question_version_public_id.as_str()
        }).collect::<Vec<_>>()
    });
    let bytes = serde_json::to_vec(&payload)
        .map_err(|error| CoreError::Parse(format!("组卷确认 hash 序列化失败：{error}")))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn load_assembly_by_id(conn: &Connection, id: i64) -> CoreResult<BlueprintAssembly> {
    let mut assembly = conn
        .query_row(
            "SELECT assembly.public_id,assembly.class_id,class.name,map.public_id,
                    edition.title,curriculum.public_id,curriculum.title,assembly.title,
                    assembly.total_score,assembly.question_type_targets_json,
                    assembly.required_knowledge_node_ids_json,assembly.preview_hash,
                    assembly.selected_set_hash,assessment.public_id,version.public_id,
                    assembly.state,assembly.confirmed_by,assembly.confirmed_at
             FROM exam_blueprint_assemblies_v2 assembly
             JOIN classes class ON class.id=assembly.class_id
             JOIN k1_knowledge_maps map ON map.id=assembly.knowledge_map_id
             JOIN k1_textbook_editions edition ON edition.id=map.textbook_edition_id
             LEFT JOIN k1_curriculum_nodes curriculum
               ON curriculum.id=assembly.curriculum_node_id
             JOIN exam_assessments_v2 assessment ON assessment.id=assembly.assessment_id
             JOIN exam_assessment_versions_v2 version
               ON version.id=assembly.assessment_version_id
             WHERE assembly.id=?1",
            [id],
            |row| {
                let targets_json: String = row.get(9)?;
                let required_json: String = row.get(10)?;
                Ok(BlueprintAssembly {
                    public_id: row.get(0)?,
                    class_id: row.get(1)?,
                    class_name: row.get(2)?,
                    knowledge_map_public_id: row.get(3)?,
                    knowledge_map_title: row.get(4)?,
                    curriculum_node_public_id: row.get(5)?,
                    curriculum_node_title: row.get(6)?,
                    title: row.get(7)?,
                    total_score: row.get(8)?,
                    question_type_targets: serde_json::from_str(&targets_json).map_err(
                        |error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                targets_json.len(),
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        },
                    )?,
                    required_knowledge_node_public_ids: serde_json::from_str(&required_json)
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                required_json.len(),
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })?,
                    preview_hash: row.get(11)?,
                    selected_set_hash: row.get(12)?,
                    assessment_public_id: row.get(13)?,
                    assessment_version_public_id: row.get(14)?,
                    state: row.get(15)?,
                    confirmed_by: row.get(16)?,
                    confirmed_at: row.get(17)?,
                    items: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("blueprint_assembly#{id}")))?;
    let mut stmt = conn.prepare(
        "SELECT item.order_index,version.public_id,version.question_type,version.stem,
                item.score,item.explanation_json
         FROM exam_blueprint_items_v2 item
         JOIN k1_question_versions version ON version.id=item.question_version_id
         WHERE item.assembly_id=?1 ORDER BY item.order_index",
    )?;
    assembly.items = stmt
        .query_map([id], |row| {
            let explanation_json: String = row.get(5)?;
            let explanation = serde_json::from_str::<serde_json::Value>(&explanation_json)
                .ok()
                .and_then(|value| {
                    value
                        .get("explanation")
                        .and_then(|item| item.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| explanation_json.clone());
            Ok(BlueprintAssemblyItem {
                order_index: row.get(0)?,
                question_version_public_id: row.get(1)?,
                question_type: row.get(2)?,
                stem: row.get(3)?,
                score: row.get(4)?,
                explanation,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(assembly)
}

pub fn confirm_blueprint(
    conn: &mut Connection,
    input: &ConfirmBlueprintRequest,
) -> CoreResult<BlueprintAssembly> {
    required(&input.request_key, "请求标识")?;
    required(&input.title, "组卷名称")?;
    required(&input.expected_preview_hash, "预览校验值")?;
    required(&input.confirmed_by, "确认人")?;
    if input.expected_preview_hash.len() != 64 {
        return Err(CoreError::Invalid("预览校验值无效".into()));
    }
    if input.title.trim().chars().count() > 100 {
        return Err(CoreError::Invalid("组卷名称不能超过 100 个字符".into()));
    }
    let existing_id = conn
        .query_row(
            "SELECT id FROM exam_blueprint_assemblies_v2 WHERE request_key=?1",
            [input.request_key.trim()],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    if let Some(id) = existing_id {
        let existing = load_assembly_by_id(conn, id)?;
        let targets = canonical_targets(&input.preview_request.question_type_targets)?;
        let required =
            canonical_required(&input.preview_request.required_knowledge_node_public_ids)?;
        let requested_curriculum = input
            .preview_request
            .curriculum_node_public_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let requested_selected = input
            .selected_question_version_public_ids
            .iter()
            .map(|value| value.trim())
            .collect::<BTreeSet<_>>();
        let existing_selected = existing
            .items
            .iter()
            .map(|item| item.question_version_public_id.as_str())
            .collect::<BTreeSet<_>>();
        let same_request = existing.title == input.title.trim()
            && existing.class_id == input.preview_request.class_id
            && existing.knowledge_map_public_id
                == input.preview_request.knowledge_map_public_id.trim()
            && existing.curriculum_node_public_id.as_deref() == requested_curriculum
            && (existing.total_score - input.preview_request.total_score).abs() < 0.000_001
            && existing.question_type_targets == targets
            && existing.required_knowledge_node_public_ids == required
            && existing.preview_hash == input.expected_preview_hash
            && requested_selected.len() == input.selected_question_version_public_ids.len()
            && existing_selected == requested_selected;
        if same_request {
            return Ok(existing);
        }
        return Err(CoreError::Invalid("同一请求标识已用于不同组卷内容".into()));
    }
    let (preview, scope, candidates) = build_preview(conn, &input.preview_request)?;
    if !preview.can_confirm {
        return Err(CoreError::Invalid(preview.blockers.join("；")));
    }
    if preview.preview_hash != input.expected_preview_hash {
        return Err(CoreError::Invalid(
            "题库或蓝图条件已变化，请刷新预览后再确认".into(),
        ));
    }
    let selected = canonical_selected(&candidates, &input.selected_question_version_public_ids)?;
    validate_selected(&preview, &selected)?;
    let request_hash = request_hash(input, &preview, &selected)?;
    let existing = conn
        .query_row(
            "SELECT id,request_hash FROM exam_blueprint_assemblies_v2
             WHERE request_key=?1",
            [input.request_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    if let Some((id, stored_hash)) = existing {
        if stored_hash != request_hash {
            return Err(CoreError::Invalid("同一请求标识已用于不同组卷内容".into()));
        }
        return load_assembly_by_id(conn, id);
    }

    let presentations = selected
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            serde_json::json!({
                "schema_version": BLUEPRINT_SCHEMA_VERSION,
                "source": "k1_blueprint",
                "question_no": index + 1,
                "question_version_public_id": candidate.output.question_version_public_id
            })
            .to_string()
        })
        .collect::<Vec<_>>();
    let assessment_items = selected
        .iter()
        .zip(presentations.iter())
        .map(|(candidate, presentation)| NewTargetedAssessmentItem {
            question_version_id: candidate.question_version_id,
            answer_key_version_id: candidate.answer_key_version_id,
            rubric_version_id: candidate.rubric_version_id,
            link_set_id: candidate.link_set_id,
            score: candidate.output.score,
            option_order_json: None,
            presentation_snapshot_json: presentation,
        })
        .collect::<Vec<_>>();
    let targets_json = serde_json::to_string(&preview.question_type_targets)
        .map_err(|error| CoreError::Parse(format!("题型目标序列化失败：{error}")))?;
    let required_json = serde_json::to_string(&preview.required_knowledge_node_public_ids)
        .map_err(|error| CoreError::Parse(format!("知识点范围序列化失败：{error}")))?;
    let selected_public_ids = selected
        .iter()
        .map(|candidate| candidate.output.question_version_public_id.as_str())
        .collect::<Vec<_>>();
    let selected_bytes = serde_json::to_vec(&selected_public_ids)
        .map_err(|error| CoreError::Parse(format!("入选题目 hash 序列化失败：{error}")))?;
    let selected_set_hash = hashing::sha256_hex(&selected_bytes);
    let public_id = ids::new_public_id();
    let confirmed_at = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    let assessment = create_confirmed_class_assessment_in_transaction(
        &tx,
        &NewClassAssessment {
            title: input.title.trim(),
            class_id: preview.class_id,
            assessment_context: "quiz",
            evidence_policy: "include",
            template_version: BLUEPRINT_TEMPLATE_VERSION,
            created_by: input.confirmed_by.trim(),
            items: &assessment_items,
        },
    )?;
    tx.execute(
        "INSERT INTO exam_blueprint_assemblies_v2
          (public_id,request_key,request_hash,class_id,knowledge_map_id,
           curriculum_node_id,title,total_score,question_type_targets_json,
           required_knowledge_node_ids_json,preview_hash,selected_set_hash,
           assessment_id,assessment_version_id,state,confirmed_by,confirmed_at,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,
                 'confirmed',?15,?16,?16)",
        params![
            public_id,
            input.request_key.trim(),
            request_hash,
            preview.class_id,
            scope.knowledge_map_id,
            scope.curriculum_node_id,
            input.title.trim(),
            preview.total_score,
            targets_json,
            required_json,
            preview.preview_hash,
            selected_set_hash,
            assessment.assessment_id,
            assessment.assessment_version_id,
            input.confirmed_by.trim(),
            confirmed_at,
        ],
    )?;
    let assembly_id = tx.last_insert_rowid();
    for (order_index, candidate) in selected.iter().enumerate() {
        let explanation_json = serde_json::json!({
            "schema_version": BLUEPRINT_SCHEMA_VERSION,
            "explanation": candidate.output.explanation,
            "knowledge_nodes": candidate.output.knowledge_nodes,
            "ability_dimensions": candidate.output.ability_dimensions
        })
        .to_string();
        tx.execute(
            "INSERT INTO exam_blueprint_items_v2
              (assembly_id,question_version_id,answer_key_version_id,rubric_version_id,
               link_set_id,order_index,score,explanation_json,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                assembly_id,
                candidate.question_version_id,
                candidate.answer_key_version_id,
                candidate.rubric_version_id,
                candidate.link_set_id,
                order_index as i64,
                candidate.output.score,
                explanation_json,
                confirmed_at,
            ],
        )?;
    }
    let event_payload = serde_json::json!({
        "schema_version": BLUEPRINT_SCHEMA_VERSION,
        "assembly_public_id": public_id,
        "assessment_public_id": assessment.assessment_public_id,
        "assessment_version_public_id": assessment.assessment_version_public_id,
        "class_id": preview.class_id,
        "selected_question_count": selected.len(),
        "total_score": preview.total_score,
        "attempts_created": false,
        "tasks_created": false,
        "grades_published": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("exam:outbox:blueprint:{public_id}"),
            event_type: "k1_blueprint_assembly_confirmed",
            event_version: 1,
            aggregate_type: "exam_blueprint_assembly",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &confirmed_at,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("exam:audit:blueprint:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.confirmed_by.trim()),
            action: "exam.blueprint_assembly.confirmed",
            object_type: "exam_blueprint_assembly",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("老师确认可解释组卷蓝图；只冻结全班作业版本，未建立作答或发布成绩"),
            meta_json: Some(&event_payload),
            occurred_at: &confirmed_at,
        },
    )?;
    tx.commit()?;
    load_assembly_by_id(conn, assembly_id)
}

pub fn list_blueprint_assemblies(
    conn: &Connection,
    class_id: i64,
    limit: i64,
) -> CoreResult<Vec<BlueprintAssembly>> {
    if class_id <= 0 {
        return Err(CoreError::Invalid("班级 ID 非法".into()));
    }
    let ids = {
        let mut stmt = conn.prepare(
            "SELECT id FROM exam_blueprint_assemblies_v2
             WHERE class_id=?1 ORDER BY confirmed_at DESC,id DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![class_id, limit.clamp(1, 100)], |row| {
                row.get::<_, i64>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    ids.into_iter()
        .map(|id| load_assembly_by_id(conn, id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use module_knowledge::db::content::{
        add_ability_link, add_knowledge_link, create_answer_key_version, create_link_set,
        create_question, create_question_version, create_rubric_version, promote_question_version,
        NewAbilityLink, NewAnswerKeyVersion, NewKnowledgeLink, NewQuestion, NewQuestionVersion,
        NewRubricPoint, NewRubricVersion,
    };
    use module_knowledge::db::taxonomy::{
        create_ability_dimension, create_curriculum_node, create_knowledge_map,
        create_knowledge_node, create_textbook_edition, NewAbilityDimension, NewCurriculumNode,
        NewKnowledgeMap, NewKnowledgeNode, NewTextbookEdition,
    };
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    struct Fixture {
        conn: Connection,
        class_id: i64,
        map_id: i64,
        map_public_id: String,
        curriculum_public_id: String,
        knowledge_id: i64,
        knowledge_public_id: String,
        ability_id: i64,
        question_public_id: String,
    }

    struct QuestionSpec<'a> {
        map_id: i64,
        knowledge_id: i64,
        ability_id: i64,
        question_type: &'a str,
        stem: &'a str,
        max_score: f64,
        quality: &'a str,
    }

    fn create_versioned_question(conn: &Connection, spec: &QuestionSpec<'_>) -> String {
        let question = create_question(
            conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: "local_teacher",
                question_family_id: None,
                rights_status: "unknown",
                sharing_allowed: false,
            },
        )
        .unwrap();
        let version = create_question_version(
            conn,
            &NewQuestionVersion {
                question_id: question.id,
                revision: 1,
                question_type: spec.question_type,
                stem: spec.stem,
                material_text: None,
                max_score: spec.max_score,
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "L0",
                state: "draft",
                options: &[],
            },
        )
        .unwrap();
        create_answer_key_version(
            conn,
            &NewAnswerKeyVersion {
                question_version_id: version.id,
                revision: 1,
                answer_json: r#"{"schema_version":1,"answer":"示例"}"#,
                state: "confirmed",
                confirmed_by: Some("local_teacher"),
                supersedes_answer_key_id: None,
                slots: &[],
            },
        )
        .unwrap();
        create_rubric_version(
            conn,
            &NewRubricVersion {
                question_version_id: version.id,
                revision: 1,
                max_score: spec.max_score,
                state: "confirmed",
                confirmed_by: Some("local_teacher"),
                supersedes_rubric_id: None,
                points: &[NewRubricPoint {
                    stable_id: None,
                    order_index: 0,
                    canonical_text: "符合标准答案",
                    allowed_paraphrases_json: None,
                    required_concepts_json: None,
                    max_score: spec.max_score,
                }],
            },
        )
        .unwrap();
        let link_set = create_link_set(
            conn,
            version.id,
            spec.map_id,
            1,
            "confirmed",
            Some("local_teacher"),
            None,
        )
        .unwrap();
        add_knowledge_link(
            conn,
            &NewKnowledgeLink {
                link_set_id: link_set.id,
                source_type: "question",
                source_public_id: &version.public_id,
                knowledge_node_id: spec.knowledge_id,
                relation_type: "direct_assessment",
                confirmation_level: "teacher_confirmed",
                verified_by: Some("local_teacher"),
            },
        )
        .unwrap();
        add_ability_link(
            conn,
            &NewAbilityLink {
                link_set_id: link_set.id,
                source_type: "question",
                source_public_id: &version.public_id,
                ability_dimension_id: spec.ability_id,
                evidence_strength: 0.5,
                response_mode: "recognition",
                confirmation_level: "teacher_confirmed",
                verified_by: Some("local_teacher"),
            },
        )
        .unwrap();
        promote_question_version(conn, version.id, spec.quality, "local_teacher", None).unwrap();
        version.public_id
    }

    fn setup() -> Fixture {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        conn.execute_batch(
            "INSERT INTO subjects(name) VALUES ('历史');
             INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');",
        )
        .unwrap();
        let class_id = conn.last_insert_rowid();
        let edition = create_textbook_edition(
            &conn,
            &NewTextbookEdition {
                subject_id: 1,
                publisher_code: "PEP",
                edition_code: "2024",
                title: "中国历史八年级上册",
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
        let curriculum = create_curriculum_node(
            &conn,
            &NewCurriculumNode {
                stable_id: None,
                knowledge_map_id: map.id,
                parent_id: None,
                node_type: "lesson",
                code: Some("L1"),
                title: "鸦片战争",
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
                curriculum_node_id: Some(curriculum.id),
                parent_id: None,
                code: Some("K1"),
                title: "鸦片战争爆发时间",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        let ability = create_ability_dimension(
            &conn,
            &NewAbilityDimension {
                stable_id: None,
                subject_id: 1,
                revision: 1,
                code: "fact_recall",
                title: "事实识记与提取",
                description: None,
                supersedes_dimension_id: None,
            },
        )
        .unwrap();
        let question_public_id = create_versioned_question(
            &conn,
            &QuestionSpec {
                map_id: map.id,
                knowledge_id: knowledge.id,
                ability_id: ability.id,
                question_type: "true_false",
                stem: "鸦片战争爆发于 1840 年。",
                max_score: 1.0,
                quality: "L3",
            },
        );
        Fixture {
            conn,
            class_id,
            map_id: map.id,
            map_public_id: map.public_id,
            curriculum_public_id: curriculum.public_id,
            knowledge_id: knowledge.id,
            knowledge_public_id: knowledge.public_id,
            ability_id: ability.id,
            question_public_id,
        }
    }

    fn request(fixture: &Fixture, total_score: f64) -> BlueprintPreviewRequest {
        BlueprintPreviewRequest {
            class_id: fixture.class_id,
            knowledge_map_public_id: fixture.map_public_id.clone(),
            curriculum_node_public_id: Some(fixture.curriculum_public_id.clone()),
            total_score,
            question_type_targets: vec![BlueprintQuestionTypeTarget {
                question_type: "true_false".into(),
                count: 1,
            }],
            required_knowledge_node_public_ids: vec![fixture.knowledge_public_id.clone()],
        }
    }

    #[test]
    fn preview_and_confirm_freeze_explainable_class_assessment_idempotently() {
        let mut fixture = setup();
        let preview = preview_blueprint(&fixture.conn, &request(&fixture, 1.0)).unwrap();
        assert!(preview.can_confirm);
        assert_eq!(preview.candidates.len(), 1);
        assert_eq!(
            preview.recommended_question_version_public_ids,
            vec![fixture.question_public_id.clone()]
        );
        assert!(preview.candidates[0]
            .explanation
            .contains("鸦片战争爆发时间"));
        let input = ConfirmBlueprintRequest {
            request_key: "blueprint-happy-1".into(),
            title: "鸦片战争随堂检测".into(),
            preview_request: request(&fixture, 1.0),
            expected_preview_hash: preview.preview_hash,
            selected_question_version_public_ids: vec![fixture.question_public_id.clone()],
            confirmed_by: "local_teacher".into(),
        };
        let created = confirm_blueprint(&mut fixture.conn, &input).unwrap();
        let repeated = confirm_blueprint(&mut fixture.conn, &input).unwrap();
        assert_eq!(created.public_id, repeated.public_id);
        create_versioned_question(
            &fixture.conn,
            &QuestionSpec {
                map_id: fixture.map_id,
                knowledge_id: fixture.knowledge_id,
                ability_id: fixture.ability_id,
                question_type: "true_false",
                stem: "鸦片战争发生于十九世纪。",
                max_score: 1.0,
                quality: "L3",
            },
        );
        let repeated_after_catalog_change = confirm_blueprint(&mut fixture.conn, &input).unwrap();
        assert_eq!(created.public_id, repeated_after_catalog_change.public_id);
        assert_eq!(created.items.len(), 1);
        let counts: (i64, i64, i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_blueprint_assemblies_v2),
                   (SELECT COUNT(*) FROM exam_assessment_items_v2),
                   (SELECT COUNT(*) FROM exam_attempts_v2),
                   (SELECT COUNT(*) FROM audit_events
                    WHERE action='exam.blueprint_assembly.confirmed'),
                   (SELECT COUNT(*) FROM outbox_events
                    WHERE event_type='k1_blueprint_assembly_confirmed')",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(counts, (1, 1, 0, 1, 1));
        assert!(fixture
            .conn
            .execute(
                "UPDATE exam_blueprint_assemblies_v2 SET title='篡改' WHERE public_id=?1",
                [&created.public_id],
            )
            .is_err());
    }

    #[test]
    fn l2_question_is_not_eligible_for_blueprint() {
        let fixture = setup();
        create_versioned_question(
            &fixture.conn,
            &QuestionSpec {
                map_id: fixture.map_id,
                knowledge_id: fixture.knowledge_id,
                ability_id: fixture.ability_id,
                question_type: "true_false",
                stem: "鸦片战争结束于 1842 年。",
                max_score: 1.0,
                quality: "L2",
            },
        );
        let preview = preview_blueprint(&fixture.conn, &request(&fixture, 1.0)).unwrap();
        assert_eq!(preview.candidates.len(), 1);
        assert_eq!(
            preview.candidates[0].question_version_public_id,
            fixture.question_public_id
        );
    }

    #[test]
    fn score_mismatch_rolls_back_without_half_assessment() {
        let mut fixture = setup();
        let preview = preview_blueprint(&fixture.conn, &request(&fixture, 2.0)).unwrap();
        assert!(preview.can_confirm);
        let preview_request = request(&fixture, 2.0);
        let error = confirm_blueprint(
            &mut fixture.conn,
            &ConfirmBlueprintRequest {
                request_key: "blueprint-score-mismatch".into(),
                title: "分值不匹配".into(),
                preview_request,
                expected_preview_hash: preview.preview_hash,
                selected_question_version_public_ids: vec![fixture.question_public_id.clone()],
                confirmed_by: "local_teacher".into(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("总分"));
        let counts: (i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_blueprint_assemblies_v2),
                   (SELECT COUNT(*) FROM exam_assessments_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(counts, (0, 0));
    }

    #[test]
    fn changed_candidate_catalog_invalidates_old_preview_hash() {
        let mut fixture = setup();
        let old_preview = preview_blueprint(&fixture.conn, &request(&fixture, 1.0)).unwrap();
        create_versioned_question(
            &fixture.conn,
            &QuestionSpec {
                map_id: fixture.map_id,
                knowledge_id: fixture.knowledge_id,
                ability_id: fixture.ability_id,
                question_type: "true_false",
                stem: "《南京条约》签订于 1842 年。",
                max_score: 1.0,
                quality: "L3",
            },
        );
        let preview_request = request(&fixture, 1.0);
        let error = confirm_blueprint(
            &mut fixture.conn,
            &ConfirmBlueprintRequest {
                request_key: "blueprint-stale".into(),
                title: "旧预览".into(),
                preview_request,
                expected_preview_hash: old_preview.preview_hash,
                selected_question_version_public_ids: vec![fixture.question_public_id.clone()],
                confirmed_by: "local_teacher".into(),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("已变化"));
        let count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_blueprint_assemblies_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
}
