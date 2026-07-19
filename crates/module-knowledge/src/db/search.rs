//! K1-3 当前题目目录、结构化筛选和疑似重复人工归类。
//!
//! 检索只暴露当前老师的个人题与已发布、明确允许共享的官方题。
//! 学校空间在成员权限模型落地前默认关闭。相似度只生成候选，不合并题目，
//! 老师结论也只写追加式归类账本，不修改题目身份或历史作业引用。

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, similarity, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

pub const SEARCH_SCHEMA_VERSION: i64 = 1;
pub const SEARCH_RULE_VERSION: &str = "k1-structured-search-v1";
pub const DUPLICATE_RULE_VERSION: &str = "k1-deterministic-duplicate-v1";
const LOCAL_CATALOG_LIMIT: usize = 1_000;
const SIMILARITY_THRESHOLD: f64 = 0.72;
const BIGRAM_THRESHOLD: f64 = 0.38;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuestionSearchRequest {
    pub query: String,
    pub owner_scope: String,
    pub question_type: Option<String>,
    pub minimum_quality: String,
    pub state: String,
    pub knowledge_map_public_id: Option<String>,
    pub curriculum_node_public_id: Option<String>,
    pub knowledge_node_public_id: Option<String>,
    pub duplicate_only: bool,
    pub limit: i64,
    pub offset: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionSearchResponse {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub items: Vec<QuestionSearchItem>,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionSearchOption {
    pub label: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionSearchKnowledge {
    pub public_id: String,
    pub title: String,
    pub relation_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionSearchAbility {
    pub public_id: String,
    pub title: String,
    pub evidence_strength: f64,
    pub response_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateCandidate {
    pub question_version_public_id: String,
    pub stem: String,
    pub match_kind: String,
    pub similarity: f64,
    pub decision: Option<String>,
    pub decision_note: Option<String>,
    pub decision_revision: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionSearchItem {
    pub question_public_id: String,
    pub question_version_public_id: String,
    pub revision: i64,
    pub owner_scope: String,
    pub owner_label: String,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub max_score: f64,
    pub quality_level: String,
    pub state: String,
    pub options: Vec<QuestionSearchOption>,
    pub knowledge_nodes: Vec<QuestionSearchKnowledge>,
    pub ability_dimensions: Vec<QuestionSearchAbility>,
    pub assessment_usage_count: i64,
    pub duplicate_candidates: Vec<DuplicateCandidate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewDuplicateRequest {
    pub request_key: String,
    pub left_question_version_public_id: String,
    pub right_question_version_public_id: String,
    pub decision: String,
    pub note: Option<String>,
    pub decided_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicateReviewDecision {
    pub public_id: String,
    pub left_question_version_public_id: String,
    pub right_question_version_public_id: String,
    pub revision: i64,
    pub match_kind: String,
    pub similarity: f64,
    pub decision: String,
    pub note: Option<String>,
    pub decided_by: String,
    pub decided_at: String,
    pub state: String,
    pub identity_changed: bool,
}

#[derive(Debug, Clone)]
struct CatalogQuestion {
    id: i64,
    question_public_id: String,
    version_public_id: String,
    revision: i64,
    owner_scope: String,
    owner_id: String,
    question_type: String,
    stem: String,
    material_text: Option<String>,
    max_score: f64,
    content_hash: String,
    quality_level: String,
    state: String,
    options: Vec<QuestionSearchOption>,
}

#[derive(Debug, Clone)]
struct Match {
    kind: String,
    similarity: f64,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn validate_question_type(value: Option<&str>) -> CoreResult<()> {
    if value.is_some_and(|value| {
        !matches!(
            value,
            "single" | "multiple" | "true_false" | "fill_blank" | "short_answer"
        )
    }) {
        return Err(CoreError::Invalid("题型筛选非法".into()));
    }
    Ok(())
}

fn quality_rank(value: &str) -> Option<i64> {
    match value {
        "C0" => Some(0),
        "L0" => Some(1),
        "L1" => Some(2),
        "L2" => Some(3),
        "L3" => Some(4),
        "L4" => Some(5),
        _ => None,
    }
}

fn normalize_text(value: &str) -> String {
    value
        .chars()
        .flat_map(char::to_lowercase)
        .filter(|character| character.is_alphanumeric())
        .take(600)
        .collect()
}

fn searchable_text(question: &CatalogQuestion) -> String {
    let mut value = String::new();
    if let Some(material) = &question.material_text {
        value.push_str(material);
    }
    value.push_str(&question.stem);
    for option in &question.options {
        value.push_str(&option.label);
        value.push_str(&option.content);
    }
    normalize_text(&value)
}

fn bigrams(value: &[char]) -> HashSet<(char, char)> {
    value.windows(2).map(|pair| (pair[0], pair[1])).collect()
}

fn duplicate_match(left: &CatalogQuestion, right: &CatalogQuestion) -> Option<Match> {
    if left.question_type != right.question_type {
        return None;
    }
    if left.content_hash == right.content_hash {
        return Some(Match {
            kind: "exact".into(),
            similarity: 1.0,
        });
    }
    let left_text = searchable_text(left);
    let right_text = searchable_text(right);
    if left_text.is_empty() || right_text.is_empty() {
        return None;
    }
    if left_text == right_text {
        return Some(Match {
            kind: "similar".into(),
            similarity: 0.99,
        });
    }
    let left_chars = left_text.chars().collect::<Vec<_>>();
    let right_chars = right_text.chars().collect::<Vec<_>>();
    let length_ratio = left_chars.len().min(right_chars.len()) as f64
        / left_chars.len().max(right_chars.len()) as f64;
    if length_ratio < 0.65 {
        return None;
    }
    let left_bigrams = bigrams(&left_chars);
    let right_bigrams = bigrams(&right_chars);
    let union = left_bigrams.union(&right_bigrams).count();
    let overlap = if union == 0 {
        0.0
    } else {
        left_bigrams.intersection(&right_bigrams).count() as f64 / union as f64
    };
    if overlap < BIGRAM_THRESHOLD {
        return None;
    }
    let score = similarity::edit_sim(&left_chars, &right_chars);
    (score >= SIMILARITY_THRESHOLD).then(|| Match {
        kind: "similar".into(),
        similarity: score,
    })
}

fn load_catalog(conn: &Connection, actor_id: &str) -> CoreResult<Vec<CatalogQuestion>> {
    let mut statement = conn.prepare(
        "WITH latest AS (
           SELECT question_id, MAX(revision) AS revision
           FROM k1_question_versions
           GROUP BY question_id
         )
         SELECT version.id,question.public_id,version.public_id,version.revision,
                question.owner_scope,question.owner_id,version.question_type,version.stem,
                version.material_text,version.max_score,version.content_hash,
                version.quality_level,version.state
         FROM k1_question_versions version
         JOIN latest
           ON latest.question_id=version.question_id AND latest.revision=version.revision
         JOIN k1_questions question ON question.id=version.question_id
         WHERE (question.owner_scope='personal' AND question.owner_id=?1)
            OR (question.owner_scope='official' AND question.sharing_allowed=1
                AND version.state='published')
         ORDER BY version.created_at DESC,version.id DESC
         LIMIT ?2",
    )?;
    let rows = statement.query_map(
        params![actor_id.trim(), LOCAL_CATALOG_LIMIT as i64 + 1],
        |row| {
            Ok(CatalogQuestion {
                id: row.get(0)?,
                question_public_id: row.get(1)?,
                version_public_id: row.get(2)?,
                revision: row.get(3)?,
                owner_scope: row.get(4)?,
                owner_id: row.get(5)?,
                question_type: row.get(6)?,
                stem: row.get(7)?,
                material_text: row.get(8)?,
                max_score: row.get(9)?,
                content_hash: row.get(10)?,
                quality_level: row.get(11)?,
                state: row.get(12)?,
                options: Vec::new(),
            })
        },
    )?;
    let mut result = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if result.len() > LOCAL_CATALOG_LIMIT {
        return Err(CoreError::Invalid(
            "当前题库超过本地检索上限，请先缩小题库范围".into(),
        ));
    }
    for question in &mut result {
        let mut option_statement = conn.prepare(
            "SELECT label,content FROM k1_question_options
             WHERE question_version_id=?1 ORDER BY order_index,id",
        )?;
        question.options = option_statement
            .query_map([question.id], |row| {
                Ok(QuestionSearchOption {
                    label: row.get(0)?,
                    content: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
    }
    Ok(result)
}

fn fts_ids(conn: &Connection, query: &str) -> CoreResult<HashSet<i64>> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(HashSet::new());
    }
    let escaped = trimmed.replace('"', "\"\"");
    let expression = format!("\"{escaped}\"");
    let mut statement = conn.prepare(
        "SELECT CAST(question_version_id AS INTEGER)
         FROM k1_question_search WHERE k1_question_search MATCH ?1",
    )?;
    let rows = statement.query_map([expression], |row| row.get::<_, i64>(0))?;
    rows.collect::<rusqlite::Result<HashSet<_>>>()
        .map_err(Into::into)
}

fn scoped_knowledge_ids(
    conn: &Connection,
    map_public_id: Option<&str>,
    curriculum_public_id: Option<&str>,
) -> CoreResult<Option<HashSet<i64>>> {
    let Some(map_public_id) = map_public_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        if curriculum_public_id.is_some_and(|value| !value.trim().is_empty()) {
            return Err(CoreError::Invalid(
                "选择教材范围前必须先选择知识地图".into(),
            ));
        }
        return Ok(None);
    };
    let map_id = conn
        .query_row(
            "SELECT id FROM k1_knowledge_maps WHERE public_id=?1 AND state='confirmed'",
            [map_public_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("知识地图不存在或尚未确认".into()))?;
    let ids = if let Some(curriculum_public_id) = curriculum_public_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let curriculum_id = conn
            .query_row(
                "SELECT id FROM k1_curriculum_nodes
                 WHERE public_id=?1 AND knowledge_map_id=?2 AND state='active'",
                params![curriculum_public_id, map_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
            .ok_or_else(|| CoreError::Invalid("教材范围不属于所选知识地图".into()))?;
        let mut statement = conn.prepare(
            "WITH RECURSIVE scope(id) AS (
               SELECT id FROM k1_curriculum_nodes WHERE id=?1
               UNION ALL
               SELECT child.id FROM k1_curriculum_nodes child
               JOIN scope parent ON child.parent_id=parent.id
               WHERE child.state='active'
             )
             SELECT id FROM k1_knowledge_nodes
             WHERE knowledge_map_id=?2 AND state='active'
               AND curriculum_node_id IN (SELECT id FROM scope)",
        )?;
        let values = statement
            .query_map(params![curriculum_id, map_id], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        values
    } else {
        let mut statement = conn.prepare(
            "SELECT id FROM k1_knowledge_nodes
             WHERE knowledge_map_id=?1 AND state='active'",
        )?;
        let values = statement
            .query_map([map_id], |row| row.get::<_, i64>(0))?
            .collect::<rusqlite::Result<HashSet<_>>>()?;
        values
    };
    Ok(Some(ids))
}

fn requested_knowledge_id(
    conn: &Connection,
    map_public_id: Option<&str>,
    public_id: Option<&str>,
) -> CoreResult<Option<i64>> {
    let Some(public_id) = public_id.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let Some(map_public_id) = map_public_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Err(CoreError::Invalid(
            "按知识点筛选前必须先选择知识地图".into(),
        ));
    };
    conn.query_row(
        "SELECT knowledge.id
         FROM k1_knowledge_nodes knowledge
         JOIN k1_knowledge_maps map ON map.id=knowledge.knowledge_map_id
         WHERE knowledge.public_id=?1 AND map.public_id=?2
           AND knowledge.state='active' AND map.state='confirmed'",
        params![public_id, map_public_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("知识点不属于所选知识地图".into()))
    .map(Some)
}

fn link_set_ids(
    conn: &Connection,
    question_version_id: i64,
    map_public_id: Option<&str>,
) -> CoreResult<Vec<i64>> {
    let mut statement = conn.prepare(
        "SELECT links.id
         FROM k1_link_sets links
         JOIN k1_knowledge_maps map ON map.id=links.knowledge_map_id
         WHERE links.question_version_id=?1 AND links.state='confirmed'
           AND (?2 IS NULL OR map.public_id=?2)
           AND links.revision=(
             SELECT MAX(candidate.revision) FROM k1_link_sets candidate
             WHERE candidate.question_version_id=links.question_version_id
               AND candidate.knowledge_map_id=links.knowledge_map_id
               AND candidate.state='confirmed'
           )
         ORDER BY links.id",
    )?;
    let rows = statement.query_map(
        params![
            question_version_id,
            map_public_id
                .map(str::trim)
                .filter(|value| !value.is_empty())
        ],
        |row| row.get::<_, i64>(0),
    )?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn load_links(
    conn: &Connection,
    question_version_id: i64,
    map_public_id: Option<&str>,
) -> CoreResult<(
    Vec<QuestionSearchKnowledge>,
    Vec<QuestionSearchAbility>,
    HashSet<i64>,
)> {
    let link_sets = link_set_ids(conn, question_version_id, map_public_id)?;
    let mut knowledge = Vec::new();
    let mut abilities = Vec::new();
    let mut knowledge_ids = HashSet::new();
    for link_set_id in link_sets {
        let mut knowledge_statement = conn.prepare(
            "SELECT node.id,node.public_id,node.title,link.relation_type
             FROM k1_knowledge_links link
             JOIN k1_knowledge_nodes node ON node.id=link.knowledge_node_id
             WHERE link.link_set_id=?1 AND link.confirmation_level='teacher_confirmed'
             ORDER BY node.order_index,node.id,link.id",
        )?;
        for row in knowledge_statement.query_map([link_set_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                QuestionSearchKnowledge {
                    public_id: row.get(1)?,
                    title: row.get(2)?,
                    relation_type: row.get(3)?,
                },
            ))
        })? {
            let (id, output) = row?;
            knowledge_ids.insert(id);
            knowledge.push(output);
        }
        let mut ability_statement = conn.prepare(
            "SELECT ability.public_id,ability.title,link.evidence_strength,link.response_mode
             FROM k1_ability_links link
             JOIN k1_ability_dimensions ability ON ability.id=link.ability_dimension_id
             WHERE link.link_set_id=?1 AND link.confirmation_level='teacher_confirmed'
             ORDER BY ability.title,ability.id,link.id",
        )?;
        abilities.extend(
            ability_statement
                .query_map([link_set_id], |row| {
                    Ok(QuestionSearchAbility {
                        public_id: row.get(0)?,
                        title: row.get(1)?,
                        evidence_strength: row.get(2)?,
                        response_mode: row.get(3)?,
                    })
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?,
        );
    }
    Ok((knowledge, abilities, knowledge_ids))
}

fn assessment_usage_count(conn: &Connection, question_version_id: i64) -> CoreResult<i64> {
    let table_exists: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM sqlite_master
           WHERE type='table' AND name='exam_assessment_items_v2'
         )",
        [],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(0);
    }
    Ok(conn.query_row(
        "SELECT COUNT(DISTINCT assessment_version_id)
         FROM exam_assessment_items_v2 WHERE question_version_id=?1",
        [question_version_id],
        |row| row.get(0),
    )?)
}

fn load_active_decision(
    conn: &Connection,
    left_id: i64,
    right_id: i64,
) -> CoreResult<Option<(String, Option<String>, i64)>> {
    let (left_id, right_id) = if left_id < right_id {
        (left_id, right_id)
    } else {
        (right_id, left_id)
    };
    Ok(conn
        .query_row(
            "SELECT decision,note,revision FROM k1_duplicate_review_decisions
             WHERE left_question_version_id=?1 AND right_question_version_id=?2
               AND state='active'",
            params![left_id, right_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?)
}

fn duplicates_for(
    conn: &Connection,
    question: &CatalogQuestion,
    catalog: &[CatalogQuestion],
) -> CoreResult<Vec<DuplicateCandidate>> {
    let mut result = Vec::new();
    for candidate in catalog {
        if candidate.id == question.id {
            continue;
        }
        let Some(found) = duplicate_match(question, candidate) else {
            continue;
        };
        let decision = load_active_decision(conn, question.id, candidate.id)?;
        result.push(DuplicateCandidate {
            question_version_public_id: candidate.version_public_id.clone(),
            stem: candidate.stem.clone(),
            match_kind: found.kind,
            similarity: found.similarity,
            decision: decision.as_ref().map(|value| value.0.clone()),
            decision_note: decision.as_ref().and_then(|value| value.1.clone()),
            decision_revision: decision.map(|value| value.2),
        });
    }
    result.sort_by(|left, right| {
        right.similarity.total_cmp(&left.similarity).then_with(|| {
            left.question_version_public_id
                .cmp(&right.question_version_public_id)
        })
    });
    result.truncate(5);
    Ok(result)
}

fn validate_request(input: &QuestionSearchRequest) -> CoreResult<()> {
    if !matches!(input.owner_scope.as_str(), "all" | "personal" | "official") {
        return Err(CoreError::Invalid("题库来源筛选非法".into()));
    }
    validate_question_type(input.question_type.as_deref())?;
    if quality_rank(&input.minimum_quality).is_none() {
        return Err(CoreError::Invalid("最低可用等级非法".into()));
    }
    if !matches!(
        input.state.as_str(),
        "active" | "candidate" | "draft" | "review_pending" | "published" | "all"
    ) {
        return Err(CoreError::Invalid("题目状态筛选非法".into()));
    }
    if !(1..=50).contains(&input.limit) || !(0..=500).contains(&input.offset) {
        return Err(CoreError::Invalid("分页范围非法".into()));
    }
    if input.query.chars().count() > 100 {
        return Err(CoreError::Invalid("搜索关键词不能超过 100 个字符".into()));
    }
    Ok(())
}

pub fn search_questions(
    conn: &Connection,
    actor_id: &str,
    input: &QuestionSearchRequest,
) -> CoreResult<QuestionSearchResponse> {
    required(actor_id, "当前老师")?;
    validate_request(input)?;
    let catalog = load_catalog(conn, actor_id)?;
    let query = normalize_text(&input.query);
    let fts = if query.is_empty() {
        HashSet::new()
    } else {
        fts_ids(conn, &input.query)?
    };
    let minimum_quality = quality_rank(&input.minimum_quality).unwrap();
    let scoped_knowledge = scoped_knowledge_ids(
        conn,
        input.knowledge_map_public_id.as_deref(),
        input.curriculum_node_public_id.as_deref(),
    )?;
    let requested_knowledge = requested_knowledge_id(
        conn,
        input.knowledge_map_public_id.as_deref(),
        input.knowledge_node_public_id.as_deref(),
    )?;
    let mut items = Vec::new();
    for question in &catalog {
        if input.owner_scope != "all" && question.owner_scope != input.owner_scope {
            continue;
        }
        if input
            .question_type
            .as_deref()
            .is_some_and(|value| value != question.question_type)
        {
            continue;
        }
        if quality_rank(&question.quality_level).unwrap_or(-1) < minimum_quality {
            continue;
        }
        let state_matches = match input.state.as_str() {
            "active" => !matches!(question.state.as_str(), "deprecated" | "archived"),
            "all" => true,
            value => question.state == value,
        };
        if !state_matches {
            continue;
        }
        if !query.is_empty()
            && !fts.contains(&question.id)
            && !searchable_text(question).contains(&query)
        {
            continue;
        }
        let (knowledge_nodes, ability_dimensions, linked_knowledge_ids) =
            load_links(conn, question.id, input.knowledge_map_public_id.as_deref())?;
        if scoped_knowledge
            .as_ref()
            .is_some_and(|scope| linked_knowledge_ids.is_disjoint(scope))
        {
            continue;
        }
        if requested_knowledge.is_some_and(|id| !linked_knowledge_ids.contains(&id)) {
            continue;
        }
        let duplicate_candidates = duplicates_for(conn, question, &catalog)?;
        if input.duplicate_only && duplicate_candidates.is_empty() {
            continue;
        }
        items.push(QuestionSearchItem {
            question_public_id: question.question_public_id.clone(),
            question_version_public_id: question.version_public_id.clone(),
            revision: question.revision,
            owner_scope: question.owner_scope.clone(),
            owner_label: if question.owner_scope == "official" {
                "官方精选".into()
            } else if question.owner_id == actor_id.trim() {
                "我的题库".into()
            } else {
                "不可访问".into()
            },
            question_type: question.question_type.clone(),
            stem: question.stem.clone(),
            material_text: question.material_text.clone(),
            max_score: question.max_score,
            quality_level: question.quality_level.clone(),
            state: question.state.clone(),
            options: question.options.clone(),
            knowledge_nodes,
            ability_dimensions,
            assessment_usage_count: assessment_usage_count(conn, question.id)?,
            duplicate_candidates,
        });
    }
    let total = items.len() as i64;
    let paged = items
        .into_iter()
        .skip(input.offset as usize)
        .take(input.limit as usize)
        .collect();
    Ok(QuestionSearchResponse {
        schema_version: SEARCH_SCHEMA_VERSION,
        rule_version: SEARCH_RULE_VERSION.into(),
        calculated_at: time::utc_now_rfc3339(),
        total,
        limit: input.limit,
        offset: input.offset,
        items: paged,
        boundary_note:
            "只显示我的当前题目和已发布官方题；相似结果只供老师归类，不会自动合并或改写历史作业。"
                .into(),
    })
}

fn accessible_current_by_public_id(
    conn: &Connection,
    actor_id: &str,
    public_id: &str,
) -> CoreResult<CatalogQuestion> {
    load_catalog(conn, actor_id)?
        .into_iter()
        .find(|question| question.version_public_id == public_id.trim())
        .ok_or_else(|| CoreError::Invalid("题目不存在、不是当前版本或无权访问".into()))
}

fn load_review_by_id(conn: &Connection, id: i64) -> CoreResult<DuplicateReviewDecision> {
    conn.query_row(
        "SELECT decision.public_id,left_version.public_id,right_version.public_id,
                decision.revision,decision.match_kind,decision.similarity_millis,
                decision.decision,decision.note,decision.decided_by,decision.decided_at,
                decision.state
         FROM k1_duplicate_review_decisions decision
         JOIN k1_question_versions left_version
           ON left_version.id=decision.left_question_version_id
         JOIN k1_question_versions right_version
           ON right_version.id=decision.right_question_version_id
         WHERE decision.id=?1",
        [id],
        |row| {
            Ok(DuplicateReviewDecision {
                public_id: row.get(0)?,
                left_question_version_public_id: row.get(1)?,
                right_question_version_public_id: row.get(2)?,
                revision: row.get(3)?,
                match_kind: row.get(4)?,
                similarity: row.get::<_, i64>(5)? as f64 / 1000.0,
                decision: row.get(6)?,
                note: row.get(7)?,
                decided_by: row.get(8)?,
                decided_at: row.get(9)?,
                state: row.get(10)?,
                identity_changed: false,
            })
        },
    )
    .map_err(Into::into)
}

#[derive(Serialize)]
struct ReviewHashPayload<'a> {
    schema_version: i64,
    left_question_version_public_id: &'a str,
    right_question_version_public_id: &'a str,
    match_kind: &'a str,
    similarity_millis: i64,
    decision: &'a str,
    note: &'a Option<String>,
    decided_by: &'a str,
}

pub fn review_duplicate(
    conn: &mut Connection,
    actor_id: &str,
    input: &ReviewDuplicateRequest,
) -> CoreResult<DuplicateReviewDecision> {
    for (value, label) in [
        (actor_id, "当前老师"),
        (&input.request_key, "请求标识"),
        (&input.left_question_version_public_id, "左侧题目版本"),
        (&input.right_question_version_public_id, "右侧题目版本"),
        (&input.decided_by, "确认人"),
    ] {
        required(value, label)?;
    }
    if actor_id.trim() != input.decided_by.trim() {
        return Err(CoreError::Invalid("只能以当前老师身份确认重复候选".into()));
    }
    if !matches!(input.decision.as_str(), "independent" | "same_family") {
        return Err(CoreError::Invalid("重复候选结论非法".into()));
    }
    if input
        .note
        .as_deref()
        .is_some_and(|note| note.chars().count() > 300)
    {
        return Err(CoreError::Invalid("备注不能超过 300 个字符".into()));
    }
    let left =
        accessible_current_by_public_id(conn, actor_id, &input.left_question_version_public_id)?;
    let right =
        accessible_current_by_public_id(conn, actor_id, &input.right_question_version_public_id)?;
    if left.id == right.id {
        return Err(CoreError::Invalid("不能将题目与自身比较".into()));
    }
    let found = duplicate_match(&left, &right)
        .ok_or_else(|| CoreError::Invalid("当前规则已不再把这两道题列为重复候选".into()))?;
    let (left, right) = if left.id < right.id {
        (left, right)
    } else {
        (right, left)
    };
    let similarity_millis = (found.similarity * 1000.0).round() as i64;
    let note = input
        .note
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned);
    let hash_payload = ReviewHashPayload {
        schema_version: SEARCH_SCHEMA_VERSION,
        left_question_version_public_id: &left.version_public_id,
        right_question_version_public_id: &right.version_public_id,
        match_kind: &found.kind,
        similarity_millis,
        decision: &input.decision,
        note: &note,
        decided_by: input.decided_by.trim(),
    };
    let payload_hash = hashing::sha256_hex(
        &serde_json::to_vec(&hash_payload)
            .map_err(|error| CoreError::Parse(format!("重复候选请求序列化失败：{error}")))?,
    );
    if let Some((id, existing_hash)) = conn
        .query_row(
            "SELECT id,payload_hash FROM k1_duplicate_review_decisions WHERE request_key=?1",
            [input.request_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if existing_hash == payload_hash {
            return load_review_by_id(conn, id);
        }
        return Err(CoreError::Invalid(
            "同一请求标识已用于不同的重复候选结论".into(),
        ));
    }
    let decided_at = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_duplicate_review_decisions
         WHERE left_question_version_id=?1 AND right_question_version_id=?2",
        params![left.id, right.id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE k1_duplicate_review_decisions SET state='superseded'
         WHERE left_question_version_id=?1 AND right_question_version_id=?2
           AND state='active'",
        params![left.id, right.id],
    )?;
    tx.execute(
        "INSERT INTO k1_duplicate_review_decisions
         (public_id,request_key,payload_hash,left_question_version_id,
          right_question_version_id,revision,match_kind,similarity_millis,
          decision,note,decided_by,decided_at,state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'active',?12)",
        params![
            public_id,
            input.request_key.trim(),
            payload_hash,
            left.id,
            right.id,
            revision,
            found.kind,
            similarity_millis,
            input.decision,
            note,
            input.decided_by.trim(),
            decided_at,
        ],
    )?;
    let review_id = tx.last_insert_rowid();
    let event_payload = serde_json::json!({
        "schema_version": SEARCH_SCHEMA_VERSION,
        "rule_version": DUPLICATE_RULE_VERSION,
        "decision_public_id": public_id,
        "left_question_version_public_id": left.version_public_id,
        "right_question_version_public_id": right.version_public_id,
        "match_kind": found.kind,
        "similarity": found.similarity,
        "decision": input.decision,
        "identity_changed": false,
        "questions_merged": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("k1:outbox:duplicate-review:{public_id}"),
            event_type: "k1_duplicate_candidate_reviewed",
            event_version: 1,
            aggregate_type: "k1_duplicate_review",
            aggregate_id: &public_id,
            aggregate_revision: revision,
            payload_json: &event_payload,
            occurred_at: &decided_at,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("k1:audit:duplicate-review:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.decided_by.trim()),
            action: "k1.duplicate_candidate.reviewed",
            object_type: "k1_duplicate_review",
            object_id: &public_id,
            object_revision: Some(revision),
            note: Some("老师仅确认题目关系；未合并题目身份，未改写历史作业"),
            meta_json: Some(&event_payload),
            occurred_at: &decided_at,
        },
    )?;
    tx.commit()?;
    load_review_by_id(conn, review_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::content::{
        add_knowledge_link, create_link_set, create_question, create_question_version,
        NewKnowledgeLink, NewQuestion, NewQuestionOption, NewQuestionVersion,
    };
    use crate::db::taxonomy::{
        create_curriculum_node, create_knowledge_map, create_knowledge_node,
        create_textbook_edition, NewCurriculumNode, NewKnowledgeMap, NewKnowledgeNode,
        NewTextbookEdition,
    };
    use std::collections::BTreeSet;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> Connection {
        let connection = open_in_memory().unwrap();
        run_migrations(&connection, CORE_MIGRATIONS).unwrap();
        run_migrations(&connection, crate::knowledge_migrations()).unwrap();
        connection
    }

    fn create(
        connection: &Connection,
        owner_scope: &str,
        owner_id: &str,
        sharing_allowed: bool,
        stem: &str,
        options: &[(&str, &str)],
    ) -> CatalogQuestion {
        let question = create_question(
            connection,
            &NewQuestion {
                owner_scope,
                owner_id,
                question_family_id: None,
                rights_status: "cleared",
                sharing_allowed,
            },
        )
        .unwrap();
        let option_values = options
            .iter()
            .enumerate()
            .map(|(index, (label, content))| NewQuestionOption {
                label,
                content,
                order_index: index as i64,
            })
            .collect::<Vec<_>>();
        let version = create_question_version(
            connection,
            &NewQuestionVersion {
                question_id: question.id,
                revision: 1,
                question_type: "single",
                stem,
                material_text: None,
                max_score: 1.0,
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "L0",
                state: "draft",
                options: &option_values,
            },
        )
        .unwrap();
        if owner_scope == "official" {
            let now = time::utc_now_rfc3339();
            connection
                .execute(
                    "INSERT INTO k1_question_quality_events
                     (public_id,question_version_id,revision,from_quality,to_quality,
                      from_state,to_state,reason,verified_by,verified_at,created_at)
                     VALUES (?1,?2,1,'L0','L1','draft','published',
                             '测试官方发布','official-reviewer',?3,?3)",
                    params![ids::new_public_id(), version.id, now],
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE k1_question_versions
                     SET quality_level='L1',state='published' WHERE id=?1",
                    [version.id],
                )
                .unwrap();
        }
        load_catalog(connection, "local_teacher")
            .unwrap()
            .into_iter()
            .find(|candidate| candidate.id == version.id)
            .unwrap_or_else(|| CatalogQuestion {
                id: version.id,
                question_public_id: question.public_id,
                version_public_id: version.public_id,
                revision: version.revision,
                owner_scope: owner_scope.into(),
                owner_id: owner_id.into(),
                question_type: version.question_type,
                stem: version.stem,
                material_text: version.material_text,
                max_score: version.max_score,
                content_hash: version.content_hash,
                quality_level: version.quality_level,
                state: version.state,
                options: option_values
                    .iter()
                    .map(|option| QuestionSearchOption {
                        label: option.label.into(),
                        content: option.content.into(),
                    })
                    .collect(),
            })
    }

    fn request(query: &str) -> QuestionSearchRequest {
        QuestionSearchRequest {
            query: query.into(),
            owner_scope: "all".into(),
            question_type: None,
            minimum_quality: "L0".into(),
            state: "active".into(),
            knowledge_map_public_id: None,
            curriculum_node_public_id: None,
            knowledge_node_public_id: None,
            duplicate_only: false,
            limit: 50,
            offset: 0,
        }
    }

    #[test]
    fn search_uses_current_access_boundary_and_fts_projection() {
        let connection = setup();
        let personal = create(
            &connection,
            "personal",
            "local_teacher",
            false,
            "鸦片战争爆发于哪一年？",
            &[("A", "1840年"), ("B", "1842年")],
        );
        create(
            &connection,
            "personal",
            "other_teacher",
            false,
            "别人的鸦片战争题",
            &[("A", "1840年"), ("B", "1842年")],
        );
        create(
            &connection,
            "school",
            "school-1",
            false,
            "学校题库中的鸦片战争题",
            &[("A", "1840年"), ("B", "1842年")],
        );
        let official = create(
            &connection,
            "official",
            "official",
            true,
            "鸦片战争发生时间是？",
            &[("A", "1840年"), ("B", "1842年")],
        );
        let result = search_questions(&connection, "local_teacher", &request("鸦片战争")).unwrap();
        let ids = result
            .items
            .iter()
            .map(|item| item.question_version_public_id.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(
            ids,
            BTreeSet::from([
                personal.version_public_id.as_str(),
                official.version_public_id.as_str()
            ])
        );
        assert_eq!(result.total, 2);
        let indexed: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM k1_question_search
                 WHERE k1_question_search MATCH '1840年'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(indexed >= 4);
    }

    #[test]
    fn exact_and_similar_candidates_are_read_only_until_teacher_review() {
        let connection = setup();
        let left = create(
            &connection,
            "personal",
            "local_teacher",
            false,
            "鸦片战争爆发于哪一年？",
            &[("A", "1840年"), ("B", "1842年")],
        );
        let exact = create(
            &connection,
            "personal",
            "local_teacher",
            false,
            "鸦片战争爆发于哪一年？",
            &[("A", "1840年"), ("B", "1842年")],
        );
        let similar = create(
            &connection,
            "personal",
            "local_teacher",
            false,
            "鸦片战争开始于哪一年？",
            &[("A", "1840年"), ("B", "1842年")],
        );
        let result = search_questions(&connection, "local_teacher", &request("爆发")).unwrap();
        let item = result
            .items
            .iter()
            .find(|item| item.question_version_public_id == left.version_public_id)
            .unwrap();
        assert!(item.duplicate_candidates.iter().any(|candidate| {
            candidate.question_version_public_id == exact.version_public_id
                && candidate.match_kind == "exact"
        }));
        assert!(item.duplicate_candidates.iter().any(|candidate| {
            candidate.question_version_public_id == similar.version_public_id
                && candidate.match_kind == "similar"
        }));
        let decisions: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM k1_duplicate_review_decisions",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(decisions, 0);
    }

    #[test]
    fn map_curriculum_and_knowledge_filters_require_confirmed_links() {
        let connection = setup();
        connection
            .execute("INSERT INTO subjects(name) VALUES ('历史')", [])
            .unwrap();
        let edition = create_textbook_edition(
            &connection,
            &NewTextbookEdition {
                subject_id: connection.last_insert_rowid(),
                publisher_code: "PEP",
                edition_code: "2026",
                title: "中国历史八年级上册",
                grade: "八年级",
                volume: "upper",
                curriculum_region: None,
            },
        )
        .unwrap();
        let map = create_knowledge_map(
            &connection,
            &NewKnowledgeMap {
                textbook_edition_id: edition.id,
                revision: 1,
                state: "confirmed",
                supersedes_map_id: None,
            },
        )
        .unwrap();
        let lesson = create_curriculum_node(
            &connection,
            &NewCurriculumNode {
                stable_id: Some("lesson-1"),
                knowledge_map_id: map.id,
                parent_id: None,
                node_type: "lesson",
                code: Some("1"),
                title: "鸦片战争",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        let knowledge = create_knowledge_node(
            &connection,
            &NewKnowledgeNode {
                stable_id: Some("knowledge-1"),
                knowledge_map_id: map.id,
                curriculum_node_id: Some(lesson.id),
                parent_id: None,
                code: Some("1.1"),
                title: "鸦片战争爆发时间",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        let linked = create(
            &connection,
            "personal",
            "local_teacher",
            false,
            "鸦片战争爆发于哪一年？",
            &[("A", "1840年"), ("B", "1842年")],
        );
        create(
            &connection,
            "personal",
            "local_teacher",
            false,
            "洋务运动开始于哪一年？",
            &[("A", "1861年"), ("B", "1895年")],
        );
        let link_set = create_link_set(
            &connection,
            linked.id,
            map.id,
            1,
            "confirmed",
            Some("local_teacher"),
            None,
        )
        .unwrap();
        add_knowledge_link(
            &connection,
            &NewKnowledgeLink {
                link_set_id: link_set.id,
                source_type: "question",
                source_public_id: &linked.version_public_id,
                knowledge_node_id: knowledge.id,
                relation_type: "direct_assessment",
                confirmation_level: "teacher_confirmed",
                verified_by: Some("local_teacher"),
            },
        )
        .unwrap();
        let mut input = request("");
        input.knowledge_map_public_id = Some(map.public_id);
        input.curriculum_node_public_id = Some(lesson.public_id);
        input.knowledge_node_public_id = Some(knowledge.public_id);
        let result = search_questions(&connection, "local_teacher", &input).unwrap();
        assert_eq!(result.total, 1);
        assert_eq!(
            result.items[0].question_version_public_id,
            linked.version_public_id
        );
        assert_eq!(result.items[0].knowledge_nodes.len(), 1);
    }

    #[test]
    fn teacher_review_is_idempotent_revisioned_and_never_merges_identity() {
        let mut connection = setup();
        let left = create(
            &connection,
            "personal",
            "local_teacher",
            false,
            "洋务运动后期的口号是什么？",
            &[("A", "自强"), ("B", "求富")],
        );
        let right = create(
            &connection,
            "personal",
            "local_teacher",
            false,
            "洋务运动后期口号是什么？",
            &[("A", "自强"), ("B", "求富")],
        );
        let input = ReviewDuplicateRequest {
            request_key: "review-1".into(),
            left_question_version_public_id: left.version_public_id.clone(),
            right_question_version_public_id: right.version_public_id.clone(),
            decision: "same_family".into(),
            note: Some("同题变式".into()),
            decided_by: "local_teacher".into(),
        };
        let first = review_duplicate(&mut connection, "local_teacher", &input).unwrap();
        let repeated = review_duplicate(&mut connection, "local_teacher", &input).unwrap();
        assert_eq!(first.public_id, repeated.public_id);
        assert_eq!(first.revision, 1);
        assert!(!first.identity_changed);
        let changed = review_duplicate(
            &mut connection,
            "local_teacher",
            &ReviewDuplicateRequest {
                request_key: "review-2".into(),
                decision: "independent".into(),
                note: Some("考查角度不同".into()),
                ..input
            },
        )
        .unwrap();
        assert_eq!(changed.revision, 2);
        let states = connection
            .prepare(
                "SELECT state FROM k1_duplicate_review_decisions
                 ORDER BY revision",
            )
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<rusqlite::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(states, vec!["superseded", "active"]);
        let family_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM k1_questions WHERE question_family_id IS NOT NULL",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(family_count, 0);
        let audit_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM audit_events
                 WHERE object_type='k1_duplicate_review'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let outbox_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM outbox_events
                 WHERE event_type='k1_duplicate_candidate_reviewed'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!((audit_count, outbox_count), (2, 2));
        assert!(connection
            .execute(
                "DELETE FROM k1_duplicate_review_decisions WHERE state='active'",
                [],
            )
            .is_err());
    }
}
