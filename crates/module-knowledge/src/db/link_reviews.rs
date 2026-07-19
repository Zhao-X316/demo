//! K1 L2→L3 知识/能力链接建议与教师复核。
//!
//! AI 结果只落不可变草稿。老师确认时一次提交全部可链接来源；事务内创建正式
//! link set、写老师确认链接并通过质量闸门晋级 L3，不创建作业、成绩或学习证据。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, AuditActorType};

use crate::db::content::{self, NewAbilityLink, NewKnowledgeLink};
use crate::link_suggestion::{
    input_hash, validate_output, LinkSuggestionAbilityCandidate, LinkSuggestionInput,
    LinkSuggestionKnowledgeCandidate, LinkSuggestionOutput, LinkSuggestionSource,
    LINK_SUGGESTION_INPUT_VERSION, LINK_SUGGESTION_SCHEMA_VERSION,
};

pub const LINK_REVIEW_SCHEMA_VERSION: i64 = 1;
pub const LINK_REVIEW_RULE_VERSION: &str = "k1-teacher-link-review-l2-to-l3-v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkSuggestionDraftView {
    pub public_id: String,
    pub ai_run_public_id: String,
    pub question_version_public_id: String,
    pub knowledge_map_public_id: String,
    pub suggestion: LinkSuggestionOutput,
    pub content_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkReviewInboxItem {
    pub question_version_public_id: String,
    pub question_content_hash: String,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub max_score: f64,
    pub quality_level: String,
    pub sources: Vec<LinkSuggestionSource>,
    pub latest_suggestion: Option<LinkSuggestionDraftView>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkReviewMapOption {
    pub public_id: String,
    pub title: String,
    pub revision: i64,
    pub subject_title: String,
    pub knowledge_count: i64,
    pub ability_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkReviewCatalog {
    pub maps: Vec<LinkReviewMapOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedKnowledgeLinkInput {
    pub knowledge_node_public_id: String,
    pub relation_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedAbilityLinkInput {
    pub ability_dimension_public_id: String,
    pub evidence_strength: f64,
    pub response_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmedSourceLinksInput {
    pub source_type: String,
    pub source_public_id: String,
    pub knowledge_links: Vec<ConfirmedKnowledgeLinkInput>,
    pub ability_links: Vec<ConfirmedAbilityLinkInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfirmLinkReviewRequest {
    pub request_key: String,
    pub question_version_public_id: String,
    pub expected_question_content_hash: String,
    pub knowledge_map_public_id: String,
    pub suggestion_draft_public_id: Option<String>,
    pub expected_suggestion_content_hash: Option<String>,
    pub sources: Vec<ConfirmedSourceLinksInput>,
    pub reviewed_by: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkReviewResult {
    pub public_id: String,
    pub question_version_public_id: String,
    pub knowledge_map_public_id: String,
    pub suggestion_draft_public_id: Option<String>,
    pub result_link_set_public_id: String,
    pub result_quality: String,
    pub knowledge_link_count: i64,
    pub ability_link_count: i64,
    pub reviewed_by: String,
    pub note: Option<String>,
    pub reviewed_at: String,
    pub creates_assessment: bool,
    pub creates_grade: bool,
    pub creates_learning_evidence: bool,
}

#[derive(Debug)]
struct QuestionScope {
    id: i64,
    public_id: String,
    content_hash: String,
    question_type: String,
    stem: String,
    material_text: Option<String>,
    max_score: f64,
    quality_level: String,
    created_at: String,
}

#[derive(Debug)]
struct MapScope {
    id: i64,
    public_id: String,
    revision: i64,
    textbook_title: String,
    subject_id: i64,
}

type KnowledgeLinkRow = (String, String, i64, String);
type AbilityLinkRow = (String, String, i64, f64, String);

#[derive(Serialize)]
struct ReviewHash<'a> {
    schema_version: i64,
    rule_version: &'a str,
    question_version_public_id: &'a str,
    expected_question_content_hash: &'a str,
    knowledge_map_public_id: &'a str,
    suggestion_draft_public_id: &'a Option<String>,
    expected_suggestion_content_hash: &'a Option<String>,
    sources: &'a [ConfirmedSourceLinksInput],
    reviewed_by: &'a str,
    note: Option<&'a str>,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn normalized_note(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|note| !note.is_empty())
}

fn valid_hash(value: &str, label: &str) -> CoreResult<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!("{label}必须是 SHA-256")));
    }
    Ok(normalized)
}

fn question_scope(
    conn: &Connection,
    owner_id: &str,
    question_version_public_id: &str,
) -> CoreResult<QuestionScope> {
    conn.query_row(
        "SELECT version.id,version.public_id,version.content_hash,version.question_type,
                version.stem,version.material_text,version.max_score,version.quality_level,
                version.created_at
         FROM k1_question_versions version
         JOIN k1_questions question ON question.id=version.question_id
         WHERE version.public_id=?1 AND question.owner_scope='personal'
           AND question.owner_id=?2 AND version.state NOT IN ('deprecated','archived')",
        (question_version_public_id.trim(), owner_id.trim()),
        |row| {
            Ok(QuestionScope {
                id: row.get(0)?,
                public_id: row.get(1)?,
                content_hash: row.get(2)?,
                question_type: row.get(3)?,
                stem: row.get(4)?,
                material_text: row.get(5)?,
                max_score: row.get(6)?,
                quality_level: row.get(7)?,
                created_at: row.get(8)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound("待关联的个人题目版本".into()))
}

fn map_scope(conn: &Connection, public_id: &str) -> CoreResult<MapScope> {
    conn.query_row(
        "SELECT map.id,map.public_id,map.revision,edition.title,edition.subject_id
         FROM k1_knowledge_maps map
         JOIN k1_textbook_editions edition ON edition.id=map.textbook_edition_id
         WHERE map.public_id=?1 AND map.state='confirmed' AND edition.state='active'",
        [public_id.trim()],
        |row| {
            Ok(MapScope {
                id: row.get(0)?,
                public_id: row.get(1)?,
                revision: row.get(2)?,
                textbook_title: row.get(3)?,
                subject_id: row.get(4)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound("已确认教材知识地图".into()))
}

fn latest_confirmed_answer_id(conn: &Connection, question_id: i64) -> CoreResult<i64> {
    conn.query_row(
        "SELECT id FROM k1_answer_key_versions
         WHERE question_version_id=?1 AND state='confirmed'
         ORDER BY revision DESC,id DESC LIMIT 1",
        [question_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("L2 题目缺少已确认答案版本".into()))
}

fn latest_confirmed_rubric_id(conn: &Connection, question_id: i64) -> CoreResult<i64> {
    conn.query_row(
        "SELECT id FROM k1_rubric_versions
         WHERE question_version_id=?1 AND state='confirmed'
         ORDER BY revision DESC,id DESC LIMIT 1",
        [question_id],
        |row| row.get(0),
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("L2 简答题缺少已确认评分点".into()))
}

fn link_sources(
    conn: &Connection,
    question: &QuestionScope,
) -> CoreResult<Vec<LinkSuggestionSource>> {
    match question.question_type.as_str() {
        "single" | "multiple" | "true_false" => {
            let mut result = vec![LinkSuggestionSource {
                source_type: "question".into(),
                source_public_id: question.public_id.clone(),
                label: "整道题".into(),
                detail: question.stem.clone(),
                order_index: 0,
                required_for_l3: true,
                required_knowledge_relation: Some("direct_assessment".into()),
            }];
            let mut statement = conn.prepare(
                "SELECT public_id,label,content,order_index FROM k1_question_options
                 WHERE question_version_id=?1 ORDER BY order_index,id",
            )?;
            let rows = statement.query_map([question.id], |row| {
                Ok(LinkSuggestionSource {
                    source_type: "option".into(),
                    source_public_id: row.get(0)?,
                    label: format!("选项 {}", row.get::<_, String>(1)?),
                    detail: row.get(2)?,
                    order_index: row.get::<_, i64>(3)? + 1,
                    required_for_l3: false,
                    required_knowledge_relation: None,
                })
            })?;
            result.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
            Ok(result)
        }
        "fill_blank" => {
            let answer_id = latest_confirmed_answer_id(conn, question.id)?;
            let mut statement = conn.prepare(
                "SELECT public_id,order_index,canonical_answers_json,max_score
                 FROM k1_answer_slots WHERE answer_key_version_id=?1 ORDER BY order_index,id",
            )?;
            let rows = statement.query_map([answer_id], |row| {
                let order = row.get::<_, i64>(1)?;
                Ok(LinkSuggestionSource {
                    source_type: "answer_slot".into(),
                    source_public_id: row.get(0)?,
                    label: format!("第 {} 空", order + 1),
                    detail: row.get(2)?,
                    order_index: order,
                    required_for_l3: true,
                    required_knowledge_relation: Some("direct_assessment".into()),
                })
            })?;
            let sources = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            if sources.is_empty() {
                return Err(CoreError::Invalid("填空题没有可链接答案槽位".into()));
            }
            Ok(sources)
        }
        "short_answer" => {
            let rubric_id = latest_confirmed_rubric_id(conn, question.id)?;
            let mut statement = conn.prepare(
                "SELECT public_id,order_index,canonical_text,max_score
                 FROM k1_rubric_points WHERE rubric_version_id=?1 ORDER BY order_index,id",
            )?;
            let rows = statement.query_map([rubric_id], |row| {
                let order = row.get::<_, i64>(1)?;
                let text = row.get::<_, String>(2)?;
                let score = row.get::<_, f64>(3)?;
                Ok(LinkSuggestionSource {
                    source_type: "rubric_point".into(),
                    source_public_id: row.get(0)?,
                    label: format!("评分点 {} · {} 分", order + 1, score),
                    detail: text,
                    order_index: order,
                    required_for_l3: true,
                    required_knowledge_relation: Some("rubric_basis".into()),
                })
            })?;
            let sources = rows.collect::<rusqlite::Result<Vec<_>>>()?;
            if sources.is_empty() {
                return Err(CoreError::Invalid("简答题没有可链接评分点".into()));
            }
            Ok(sources)
        }
        _ => Err(CoreError::Invalid("当前题型不能进入 L3 链接复核".into())),
    }
}

fn draft_view_by_id(conn: &Connection, id: i64) -> CoreResult<LinkSuggestionDraftView> {
    let (
        public_id,
        ai_public_id,
        question_public_id,
        map_public_id,
        suggestion_json,
        content_hash,
        created_at,
    ): (String, String, String, String, String, String, String) = conn.query_row(
        "SELECT draft.public_id,run.public_id,question.public_id,map.public_id,
                draft.suggestion_json,draft.content_hash,draft.created_at
         FROM k1_link_suggestion_drafts draft
         JOIN ai_runs run ON run.id=draft.ai_run_id
         JOIN k1_question_versions question ON question.id=draft.question_version_id
         JOIN k1_knowledge_maps map ON map.id=draft.knowledge_map_id
         WHERE draft.id=?1",
        [id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        },
    )?;
    let suggestion = serde_json::from_str(&suggestion_json)
        .map_err(|error| CoreError::Parse(format!("知识链接建议草稿无法解析：{error}")))?;
    Ok(LinkSuggestionDraftView {
        public_id,
        ai_run_public_id: ai_public_id,
        question_version_public_id: question_public_id,
        knowledge_map_public_id: map_public_id,
        suggestion,
        content_hash,
        created_at,
    })
}

fn latest_draft_for_question(
    conn: &Connection,
    question_version_id: i64,
) -> CoreResult<Option<LinkSuggestionDraftView>> {
    let id = conn
        .query_row(
            "SELECT id FROM k1_link_suggestion_drafts
             WHERE question_version_id=?1 ORDER BY id DESC LIMIT 1",
            [question_version_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    id.map(|value| draft_view_by_id(conn, value)).transpose()
}

pub fn list_review_inbox(
    conn: &Connection,
    owner_id: &str,
    limit: i64,
) -> CoreResult<Vec<LinkReviewInboxItem>> {
    required(owner_id, "题库老师")?;
    let limit = limit.clamp(1, 200);
    let mut statement = conn.prepare(
        "SELECT version.id,version.public_id,version.content_hash,version.question_type,
                version.stem,version.material_text,version.max_score,version.quality_level,
                version.created_at
         FROM k1_question_versions version
         JOIN k1_questions question ON question.id=version.question_id
         WHERE question.owner_scope='personal' AND question.owner_id=?1
           AND version.quality_level='L2' AND version.state='published'
           AND NOT EXISTS(SELECT 1 FROM k1_link_reviews review
                          WHERE review.question_version_id=version.id)
         ORDER BY version.created_at DESC,version.id DESC LIMIT ?2",
    )?;
    let rows = statement
        .query_map((owner_id.trim(), limit), |row| {
            Ok(QuestionScope {
                id: row.get(0)?,
                public_id: row.get(1)?,
                content_hash: row.get(2)?,
                question_type: row.get(3)?,
                stem: row.get(4)?,
                material_text: row.get(5)?,
                max_score: row.get(6)?,
                quality_level: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let mut result = Vec::with_capacity(rows.len());
    for question in rows {
        result.push(LinkReviewInboxItem {
            question_version_public_id: question.public_id.clone(),
            question_content_hash: question.content_hash.clone(),
            question_type: question.question_type.clone(),
            stem: question.stem.clone(),
            material_text: question.material_text.clone(),
            max_score: question.max_score,
            quality_level: question.quality_level.clone(),
            sources: link_sources(conn, &question)?,
            latest_suggestion: latest_draft_for_question(conn, question.id)?,
            created_at: question.created_at.clone(),
        });
    }
    Ok(result)
}

pub fn list_catalog(conn: &Connection) -> CoreResult<LinkReviewCatalog> {
    let mut statement = conn.prepare(
        "SELECT map.public_id,edition.title,map.revision,subject.name,
                (SELECT COUNT(*) FROM k1_knowledge_nodes node
                 WHERE node.knowledge_map_id=map.id AND node.state='active'),
                (SELECT COUNT(*) FROM k1_ability_dimensions ability
                 WHERE ability.subject_id=edition.subject_id AND ability.state='active'
                   AND ability.revision=(
                     SELECT MAX(latest.revision) FROM k1_ability_dimensions latest
                     WHERE latest.stable_id=ability.stable_id AND latest.state='active'
                   ))
         FROM k1_knowledge_maps map
         JOIN k1_textbook_editions edition ON edition.id=map.textbook_edition_id
         JOIN subjects subject ON subject.id=edition.subject_id
         WHERE map.state='confirmed' AND edition.state='active'
         ORDER BY edition.title,map.revision DESC,map.id DESC",
    )?;
    let maps = statement
        .query_map([], |row| {
            Ok(LinkReviewMapOption {
                public_id: row.get(0)?,
                title: row.get(1)?,
                revision: row.get(2)?,
                subject_title: row.get(3)?,
                knowledge_count: row.get(4)?,
                ability_count: row.get(5)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(LinkReviewCatalog { maps })
}

pub fn build_suggestion_input(
    conn: &Connection,
    owner_id: &str,
    question_version_public_id: &str,
    knowledge_map_public_id: &str,
) -> CoreResult<LinkSuggestionInput> {
    let question = question_scope(conn, owner_id, question_version_public_id)?;
    if question.quality_level != "L2" {
        return Err(CoreError::Invalid("只有 L2 题目需要关联后晋级 L3".into()));
    }
    let map = map_scope(conn, knowledge_map_public_id)?;
    let mut knowledge_statement = conn.prepare(
        "SELECT node.public_id,node.code,node.title,curriculum.title
         FROM k1_knowledge_nodes node
         LEFT JOIN k1_curriculum_nodes curriculum ON curriculum.id=node.curriculum_node_id
         WHERE node.knowledge_map_id=?1 AND node.state='active'
         ORDER BY curriculum.order_index,node.order_index,node.id",
    )?;
    let knowledge_candidates = knowledge_statement
        .query_map([map.id], |row| {
            Ok(LinkSuggestionKnowledgeCandidate {
                public_id: row.get(0)?,
                code: row.get(1)?,
                title: row.get(2)?,
                curriculum_title: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(knowledge_statement);
    let mut ability_statement = conn.prepare(
        "SELECT ability.public_id,ability.code,ability.title,ability.description
         FROM k1_ability_dimensions ability
         WHERE ability.subject_id=?1 AND ability.state='active'
           AND ability.revision=(
             SELECT MAX(latest.revision) FROM k1_ability_dimensions latest
             WHERE latest.stable_id=ability.stable_id AND latest.state='active'
           )
         ORDER BY ability.code,ability.id",
    )?;
    let ability_candidates = ability_statement
        .query_map([map.subject_id], |row| {
            Ok(LinkSuggestionAbilityCandidate {
                public_id: row.get(0)?,
                code: row.get(1)?,
                title: row.get(2)?,
                description: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let sources = link_sources(conn, &question)?;
    let input = LinkSuggestionInput {
        schema_version: LINK_SUGGESTION_SCHEMA_VERSION,
        input_version: LINK_SUGGESTION_INPUT_VERSION.into(),
        question_version_public_id: question.public_id,
        question_content_hash: question.content_hash,
        question_type: question.question_type,
        stem: question.stem,
        material_text: question.material_text,
        knowledge_map_public_id: map.public_id,
        knowledge_map_revision: map.revision,
        textbook_title: map.textbook_title,
        sources,
        knowledge_candidates,
        ability_candidates,
    };
    let _ = input_hash(&input)?;
    Ok(input)
}

pub fn materialize_suggestion(
    conn: &mut Connection,
    owner_id: &str,
    ai_run_id: i64,
    input: &LinkSuggestionInput,
    output: &LinkSuggestionOutput,
) -> CoreResult<LinkSuggestionDraftView> {
    validate_output(input, output)?;
    let question = question_scope(conn, owner_id, &input.question_version_public_id)?;
    if question.content_hash != input.question_content_hash || question.quality_level != "L2" {
        return Err(CoreError::Invalid(
            "题目内容或质量等级已变化，请重新生成链接建议".into(),
        ));
    }
    let map = map_scope(conn, &input.knowledge_map_public_id)?;
    let expected_input_hash = input_hash(input)?;
    let run = suite_core::db::repo::ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{ai_run_id}")))?;
    if run.status != AiRunStatus::Succeeded
        || run.run_type != "knowledge_link_suggest"
        || run.source_module != "knowledge"
        || run.business_ref_type != "k1_question_version"
        || run.business_ref_id != input.question_version_public_id
        || run.input_hash != expected_input_hash
    {
        return Err(CoreError::Invalid("AI run 与知识链接建议输入不一致".into()));
    }
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM k1_link_suggestion_drafts WHERE ai_run_id=?1",
            [ai_run_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
    {
        return draft_view_by_id(conn, id);
    }
    let suggestion_json = serde_json::to_string(output)
        .map_err(|error| CoreError::Parse(format!("知识链接建议序列化失败：{error}")))?;
    let issue_codes_json = serde_json::to_string(&output.issue_codes)
        .map_err(|error| CoreError::Parse(format!("知识链接问题码序列化失败：{error}")))?;
    let content_hash = hashing::sha256_hex(suggestion_json.as_bytes());
    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO k1_link_suggestion_drafts
         (public_id,question_version_id,knowledge_map_id,ai_run_id,input_hash,suggestion_json,
          suggestion_state,confidence,issue_codes_json,content_hash,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            &public_id,
            question.id,
            map.id,
            ai_run_id,
            &expected_input_hash,
            &suggestion_json,
            &output.state,
            output.confidence,
            &issue_codes_json,
            &content_hash,
            &now
        ],
    )?;
    let id = tx.last_insert_rowid();
    let meta = serde_json::json!({
        "schema_version": LINK_REVIEW_SCHEMA_VERSION,
        "question_version_public_id": input.question_version_public_id,
        "knowledge_map_public_id": input.knowledge_map_public_id,
        "suggestion_state": output.state,
        "creates_link_set": false,
        "promotes_question": false
    })
    .to_string();
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("k1:audit:link-suggestion:{public_id}"),
            actor_type: AuditActorType::System,
            actor_id: None,
            action: "k1.link_suggestion.materialized",
            object_type: "k1_link_suggestion_draft",
            object_id: &public_id,
            object_revision: Some(1),
            note: Some("AI 仅创建知识/能力链接建议草稿，老师确认前不生效"),
            meta_json: Some(&meta),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    draft_view_by_id(conn, id)
}

fn relation_allowed(source_type: &str, relation: &str) -> bool {
    match source_type {
        "question" => matches!(relation, "direct_assessment" | "context" | "prerequisite"),
        "option" => matches!(
            relation,
            "answer_basis" | "distractor" | "misconception" | "context"
        ),
        "answer_slot" => matches!(relation, "direct_assessment" | "answer_basis"),
        "rubric_point" => relation == "rubric_basis",
        _ => false,
    }
}

fn response_mode_allowed(value: &str) -> bool {
    matches!(
        value,
        "recognition" | "recall" | "structured_response" | "source_analysis" | "argumentation"
    )
}

fn validate_confirmed_sources(
    conn: &Connection,
    expected_sources: &[LinkSuggestionSource],
    map: &MapScope,
    inputs: &[ConfirmedSourceLinksInput],
) -> CoreResult<(Vec<KnowledgeLinkRow>, Vec<AbilityLinkRow>)> {
    if inputs.len() != expected_sources.len() {
        return Err(CoreError::Invalid(
            "必须一次提交本题全部链接来源，允许非必需选项显式留空".into(),
        ));
    }
    let expected = expected_sources
        .iter()
        .map(|source| {
            (
                (
                    source.source_type.as_str(),
                    source.source_public_id.as_str(),
                ),
                source,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut seen_sources = BTreeSet::new();
    let mut knowledge_rows = Vec::new();
    let mut ability_rows = Vec::new();
    for input in inputs {
        let key = (input.source_type.trim(), input.source_public_id.trim());
        let source = expected
            .get(&key)
            .ok_or_else(|| CoreError::Invalid("链接来源不属于当前题目最新版本".into()))?;
        if !seen_sources.insert(key) {
            return Err(CoreError::Invalid("链接来源不能重复提交".into()));
        }
        let mut knowledge_seen = BTreeSet::new();
        let mut core_found = false;
        for link in &input.knowledge_links {
            let relation = link.relation_type.trim();
            if !relation_allowed(&input.source_type, relation)
                || !knowledge_seen.insert((link.knowledge_node_public_id.trim(), relation))
            {
                return Err(CoreError::Invalid("知识链接关系非法或重复".into()));
            }
            let node_id: i64 = conn
                .query_row(
                    "SELECT id FROM k1_knowledge_nodes
                     WHERE public_id=?1 AND knowledge_map_id=?2 AND state='active'",
                    (link.knowledge_node_public_id.trim(), map.id),
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| CoreError::Invalid("知识点不属于所选已确认地图".into()))?;
            if source.required_knowledge_relation.as_deref() == Some(relation) {
                core_found = true;
            }
            knowledge_rows.push((
                input.source_type.trim().to_owned(),
                input.source_public_id.trim().to_owned(),
                node_id,
                relation.to_owned(),
            ));
        }
        let mut ability_seen = BTreeSet::new();
        for link in &input.ability_links {
            let response_mode = link.response_mode.trim();
            if !link.evidence_strength.is_finite()
                || !(0.0..=1.0).contains(&link.evidence_strength)
                || link.evidence_strength <= 0.0
                || !response_mode_allowed(response_mode)
                || !ability_seen.insert((link.ability_dimension_public_id.trim(), response_mode))
            {
                return Err(CoreError::Invalid("能力链接强度、模式非法或重复".into()));
            }
            let ability_id: i64 = conn
                .query_row(
                    "SELECT id FROM k1_ability_dimensions
                     WHERE public_id=?1 AND subject_id=?2 AND state='active'
                       AND revision=(
                         SELECT MAX(latest.revision) FROM k1_ability_dimensions latest
                         WHERE latest.stable_id=k1_ability_dimensions.stable_id
                           AND latest.state='active'
                       )",
                    (link.ability_dimension_public_id.trim(), map.subject_id),
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| CoreError::Invalid("能力维度不属于当前教材学科".into()))?;
            ability_rows.push((
                input.source_type.trim().to_owned(),
                input.source_public_id.trim().to_owned(),
                ability_id,
                link.evidence_strength,
                response_mode.to_owned(),
            ));
        }
        if source.required_for_l3 && (!core_found || input.ability_links.is_empty()) {
            return Err(CoreError::Invalid(format!(
                "{}必须至少确认一个“{}”知识链接和一个能力链接",
                source.label,
                source
                    .required_knowledge_relation
                    .as_deref()
                    .unwrap_or("直接考查")
            )));
        }
    }
    Ok((knowledge_rows, ability_rows))
}

fn result_by_id(conn: &Connection, id: i64) -> CoreResult<LinkReviewResult> {
    conn.query_row(
        "SELECT review.public_id,question.public_id,map.public_id,suggestion.public_id,
                links.public_id,review.result_quality,
                (SELECT COUNT(*) FROM k1_knowledge_links item
                 WHERE item.link_set_id=review.result_link_set_id),
                (SELECT COUNT(*) FROM k1_ability_links item
                 WHERE item.link_set_id=review.result_link_set_id),
                review.reviewed_by,review.note,review.reviewed_at
         FROM k1_link_reviews review
         JOIN k1_question_versions question ON question.id=review.question_version_id
         JOIN k1_knowledge_maps map ON map.id=review.knowledge_map_id
         LEFT JOIN k1_link_suggestion_drafts suggestion ON suggestion.id=review.suggestion_draft_id
         JOIN k1_link_sets links ON links.id=review.result_link_set_id
         WHERE review.id=?1",
        [id],
        |row| {
            Ok(LinkReviewResult {
                public_id: row.get(0)?,
                question_version_public_id: row.get(1)?,
                knowledge_map_public_id: row.get(2)?,
                suggestion_draft_public_id: row.get(3)?,
                result_link_set_public_id: row.get(4)?,
                result_quality: row.get(5)?,
                knowledge_link_count: row.get(6)?,
                ability_link_count: row.get(7)?,
                reviewed_by: row.get(8)?,
                note: row.get(9)?,
                reviewed_at: row.get(10)?,
                creates_assessment: false,
                creates_grade: false,
                creates_learning_evidence: false,
            })
        },
    )
    .map_err(Into::into)
}

pub fn confirm_links(
    conn: &mut Connection,
    owner_id: &str,
    request: &ConfirmLinkReviewRequest,
) -> CoreResult<LinkReviewResult> {
    for (value, label) in [
        (&request.request_key, "请求键"),
        (&request.question_version_public_id, "题目版本"),
        (&request.expected_question_content_hash, "题目内容 hash"),
        (&request.knowledge_map_public_id, "知识地图"),
        (&request.reviewed_by, "确认老师"),
    ] {
        required(value, label)?;
    }
    if owner_id.trim() != request.reviewed_by.trim() {
        return Err(CoreError::Invalid("只能由当前题库老师确认链接".into()));
    }
    let expected_question_hash =
        valid_hash(&request.expected_question_content_hash, "题目内容 hash")?;
    let expected_suggestion_hash = request
        .expected_suggestion_content_hash
        .as_deref()
        .map(|value| valid_hash(value, "建议草稿 hash"))
        .transpose()?;
    if request.suggestion_draft_public_id.is_some() != expected_suggestion_hash.is_some() {
        return Err(CoreError::Invalid(
            "引用 AI 建议时必须同时提交建议草稿和内容 hash".into(),
        ));
    }
    let mut canonical_sources = request.sources.clone();
    canonical_sources.sort_by(|left, right| {
        (&left.source_type, &left.source_public_id)
            .cmp(&(&right.source_type, &right.source_public_id))
    });
    for source in &mut canonical_sources {
        source.knowledge_links.sort_by(|left, right| {
            (&left.knowledge_node_public_id, &left.relation_type)
                .cmp(&(&right.knowledge_node_public_id, &right.relation_type))
        });
        source.ability_links.sort_by(|left, right| {
            (&left.ability_dimension_public_id, &left.response_mode)
                .cmp(&(&right.ability_dimension_public_id, &right.response_mode))
        });
    }
    let note = normalized_note(request.note.as_deref());
    let request_hash = hashing::sha256_hex(
        &serde_json::to_vec(&ReviewHash {
            schema_version: LINK_REVIEW_SCHEMA_VERSION,
            rule_version: LINK_REVIEW_RULE_VERSION,
            question_version_public_id: request.question_version_public_id.trim(),
            expected_question_content_hash: &expected_question_hash,
            knowledge_map_public_id: request.knowledge_map_public_id.trim(),
            suggestion_draft_public_id: &request.suggestion_draft_public_id,
            expected_suggestion_content_hash: &expected_suggestion_hash,
            sources: &canonical_sources,
            reviewed_by: request.reviewed_by.trim(),
            note,
        })
        .map_err(|error| CoreError::Parse(format!("链接复核请求序列化失败：{error}")))?,
    );
    if let Some((id, existing_hash)) = conn
        .query_row(
            "SELECT id,request_hash FROM k1_link_reviews WHERE request_key=?1",
            [request.request_key.trim()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
    {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid("同一请求键对应不同链接确认".into()));
        }
        return result_by_id(conn, id);
    }
    let question = question_scope(conn, owner_id, &request.question_version_public_id)?;
    if question.content_hash != expected_question_hash {
        return Err(CoreError::Invalid("题目内容已变化，请刷新后重试".into()));
    }
    if question.quality_level != "L2" {
        return Err(CoreError::Invalid(
            "只有当前 L2 题目可以确认链接并晋级 L3".into(),
        ));
    }
    let map = map_scope(conn, &request.knowledge_map_public_id)?;
    let sources = link_sources(conn, &question)?;
    let (knowledge_rows, ability_rows) =
        validate_confirmed_sources(conn, &sources, &map, &canonical_sources)?;
    let suggestion_id = if let (Some(public_id), Some(content_hash)) = (
        request.suggestion_draft_public_id.as_deref(),
        expected_suggestion_hash.as_deref(),
    ) {
        let (id, actual_hash, question_id, map_id): (i64, String, i64, i64) = conn
            .query_row(
                "SELECT id,content_hash,question_version_id,knowledge_map_id
                 FROM k1_link_suggestion_drafts WHERE public_id=?1",
                [public_id.trim()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or_else(|| CoreError::NotFound("知识链接建议草稿".into()))?;
        if actual_hash != content_hash || question_id != question.id || map_id != map.id {
            return Err(CoreError::Invalid(
                "知识链接建议已过期或不属于当前题目与地图".into(),
            ));
        }
        Some(id)
    } else {
        None
    };
    let already_reviewed: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM k1_link_reviews WHERE question_version_id=?1)",
        [question.id],
        |row| row.get(0),
    )?;
    if already_reviewed {
        return Err(CoreError::Invalid("该题已经完成 L3 链接复核".into()));
    }
    let link_revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_link_sets
         WHERE question_version_id=?1",
        [question.id],
        |row| row.get(0),
    )?;
    let supersedes_link_set_id = conn
        .query_row(
            "SELECT id FROM k1_link_sets WHERE question_version_id=?1
             ORDER BY revision DESC,id DESC LIMIT 1",
            [question.id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?;
    let confirmed_links_json = serde_json::to_string(&canonical_sources)
        .map_err(|error| CoreError::Parse(format!("老师确认链接序列化失败：{error}")))?;
    let now = time::utc_now_rfc3339();
    let review_public_id = ids::new_public_id();
    let tx = conn.transaction()?;
    let link_set = content::create_link_set(
        &tx,
        question.id,
        map.id,
        link_revision,
        "confirmed",
        Some(request.reviewed_by.trim()),
        supersedes_link_set_id,
    )?;
    for (source_type, source_public_id, knowledge_node_id, relation_type) in &knowledge_rows {
        content::add_knowledge_link(
            &tx,
            &NewKnowledgeLink {
                link_set_id: link_set.id,
                source_type,
                source_public_id,
                knowledge_node_id: *knowledge_node_id,
                relation_type,
                confirmation_level: "teacher_confirmed",
                verified_by: Some(request.reviewed_by.trim()),
            },
        )?;
    }
    for (source_type, source_public_id, ability_dimension_id, evidence_strength, response_mode) in
        &ability_rows
    {
        content::add_ability_link(
            &tx,
            &NewAbilityLink {
                link_set_id: link_set.id,
                source_type,
                source_public_id,
                ability_dimension_id: *ability_dimension_id,
                evidence_strength: *evidence_strength,
                response_mode,
                confirmation_level: "teacher_confirmed",
                verified_by: Some(request.reviewed_by.trim()),
            },
        )?;
    }
    content::promote_question_version_in_transaction(
        &tx,
        question.id,
        "L3",
        request.reviewed_by.trim(),
        Some("老师逐来源确认知识点与能力链接"),
    )?;
    tx.execute(
        "INSERT INTO k1_link_reviews
         (public_id,request_key,request_hash,question_version_id,knowledge_map_id,
          suggestion_draft_id,confirmed_links_json,result_link_set_id,result_quality,
          reviewed_by,note,reviewed_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'L3',?9,?10,?11)",
        params![
            &review_public_id,
            request.request_key.trim(),
            &request_hash,
            question.id,
            map.id,
            suggestion_id,
            &confirmed_links_json,
            link_set.id,
            request.reviewed_by.trim(),
            note,
            &now
        ],
    )?;
    let review_id = tx.last_insert_rowid();
    let payload = serde_json::json!({
        "schema_version": LINK_REVIEW_SCHEMA_VERSION,
        "rule_version": LINK_REVIEW_RULE_VERSION,
        "question_version_public_id": question.public_id,
        "knowledge_map_public_id": map.public_id,
        "suggestion_draft_public_id": request.suggestion_draft_public_id,
        "result_link_set_public_id": link_set.public_id,
        "result_quality": "L3",
        "knowledge_link_count": knowledge_rows.len(),
        "ability_link_count": ability_rows.len(),
        "creates_assessment": false,
        "creates_grade": false,
        "creates_learning_evidence": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "k1.link_review.confirmed",
            event_version: 1,
            aggregate_type: "k1_link_review",
            aggregate_id: &review_public_id,
            aggregate_revision: 1,
            payload_json: &payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("{}:audit", request.request_key.trim()),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(request.reviewed_by.trim()),
            action: "k1.link_review.confirmed",
            object_type: "k1_link_review",
            object_id: &review_public_id,
            object_revision: Some(1),
            note: Some("老师逐来源确认链接后才晋级 L3；未创建作业、成绩或学习证据"),
            meta_json: Some(&payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    result_by_id(conn, review_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::content::{
        create_answer_key_version, create_question, create_question_version, create_rubric_version,
        NewAnswerKeyVersion, NewAnswerSlot, NewQuestion, NewQuestionOption, NewQuestionVersion,
        NewRubricPoint, NewRubricVersion,
    };
    use crate::db::taxonomy::{
        create_ability_dimension, create_curriculum_node, create_knowledge_map,
        create_knowledge_node, create_textbook_edition, NewAbilityDimension, NewCurriculumNode,
        NewKnowledgeMap, NewKnowledgeNode, NewTextbookEdition,
    };
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    struct Fixture {
        conn: Connection,
        map_public_id: String,
        knowledge_public_id: String,
        ability_public_id: String,
    }

    fn fixture() -> Fixture {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, crate::knowledge_migrations()).unwrap();
        conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])
            .unwrap();
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
        let lesson = create_curriculum_node(
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
                curriculum_node_id: Some(lesson.id),
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
        Fixture {
            conn,
            map_public_id: map.public_id,
            knowledge_public_id: knowledge.public_id,
            ability_public_id: ability.public_id,
        }
    }

    fn l2_question(
        conn: &Connection,
        question_type: &str,
        stem: &str,
    ) -> crate::db::content::QuestionVersion {
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
        let options = [
            NewQuestionOption {
                label: "A",
                content: "1840",
                order_index: 0,
            },
            NewQuestionOption {
                label: "B",
                content: "1842",
                order_index: 1,
            },
        ];
        let version = create_question_version(
            conn,
            &NewQuestionVersion {
                question_id: question.id,
                revision: 1,
                question_type,
                stem,
                material_text: None,
                max_score: if question_type == "fill_blank" {
                    2.0
                } else {
                    1.0
                },
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "L0",
                state: "draft",
                options: if matches!(question_type, "single" | "multiple") {
                    &options
                } else {
                    &[]
                },
            },
        )
        .unwrap();
        let slots = [
            NewAnswerSlot {
                stable_id: None,
                order_index: 0,
                canonical_answers_json: r#"{"schema_version":1,"canonical_answers":["1840"]}"#,
                normalization_rules_json: None,
                max_score: 1.0,
            },
            NewAnswerSlot {
                stable_id: None,
                order_index: 1,
                canonical_answers_json: r#"{"schema_version":1,"canonical_answers":["英国"]}"#,
                normalization_rules_json: None,
                max_score: 1.0,
            },
        ];
        create_answer_key_version(
            conn,
            &NewAnswerKeyVersion {
                question_version_id: version.id,
                revision: 1,
                answer_json: if question_type == "fill_blank" {
                    r#"{"schema_version":1,"slots":[{"canonical_answers":["1840"]},{"canonical_answers":["英国"]}]}"#
                } else {
                    r#"{"schema_version":1,"correct_labels":["A"]}"#
                },
                state: "confirmed",
                confirmed_by: Some("local_teacher"),
                supersedes_answer_key_id: None,
                slots: if question_type == "fill_blank" {
                    &slots
                } else {
                    &[]
                },
            },
        )
        .unwrap();
        content::promote_question_version(
            conn,
            version.id,
            "L2",
            "local_teacher",
            Some("测试答案已确认"),
        )
        .unwrap()
    }

    fn l2_short_answer_question(
        conn: &Connection,
        stem: &str,
    ) -> crate::db::content::QuestionVersion {
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
                question_type: "short_answer",
                stem,
                material_text: None,
                max_score: 4.0,
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
                answer_json: r#"{"schema_version":1,"reference_answer":"洋务运动只学习西方技术，没有改变封建制度；洋务派内部腐败。"}"#,
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
                max_score: 4.0,
                state: "confirmed",
                confirmed_by: Some("local_teacher"),
                supersedes_rubric_id: None,
                points: &[
                    NewRubricPoint {
                        stable_id: None,
                        order_index: 0,
                        canonical_text: "只学习西方技术，没有改变封建制度",
                        allowed_paraphrases_json: None,
                        required_concepts_json: None,
                        max_score: 2.0,
                    },
                    NewRubricPoint {
                        stable_id: None,
                        order_index: 1,
                        canonical_text: "洋务派内部腐败",
                        allowed_paraphrases_json: None,
                        required_concepts_json: None,
                        max_score: 2.0,
                    },
                ],
            },
        )
        .unwrap();
        content::promote_question_version(
            conn,
            version.id,
            "L2",
            "local_teacher",
            Some("测试评分点已确认"),
        )
        .unwrap()
    }

    fn source_inputs(
        inbox: &LinkReviewInboxItem,
        knowledge_public_id: &str,
        ability_public_id: &str,
    ) -> Vec<ConfirmedSourceLinksInput> {
        inbox
            .sources
            .iter()
            .map(|source| ConfirmedSourceLinksInput {
                source_type: source.source_type.clone(),
                source_public_id: source.source_public_id.clone(),
                knowledge_links: source
                    .required_knowledge_relation
                    .as_ref()
                    .map(|relation| {
                        vec![ConfirmedKnowledgeLinkInput {
                            knowledge_node_public_id: knowledge_public_id.into(),
                            relation_type: relation.clone(),
                        }]
                    })
                    .unwrap_or_default(),
                ability_links: if source.required_for_l3 {
                    vec![ConfirmedAbilityLinkInput {
                        ability_dimension_public_id: ability_public_id.into(),
                        evidence_strength: 0.6,
                        response_mode: match source.source_type.as_str() {
                            "answer_slot" => "recall",
                            "rubric_point" => "structured_response",
                            _ => "recognition",
                        }
                        .into(),
                    }]
                } else {
                    Vec::new()
                },
            })
            .collect()
    }

    #[test]
    fn teacher_confirmation_promotes_objective_to_l3_without_exam_or_evidence() {
        let mut fixture = fixture();
        let question = l2_question(&fixture.conn, "single", "鸦片战争爆发于哪一年？");
        let inbox = list_review_inbox(&fixture.conn, "local_teacher", 10)
            .unwrap()
            .pop()
            .unwrap();
        let sources = source_inputs(
            &inbox,
            &fixture.knowledge_public_id,
            &fixture.ability_public_id,
        );
        let result = confirm_links(
            &mut fixture.conn,
            "local_teacher",
            &ConfirmLinkReviewRequest {
                request_key: "review-objective".into(),
                question_version_public_id: question.public_id.clone(),
                expected_question_content_hash: question.content_hash.clone(),
                knowledge_map_public_id: fixture.map_public_id.clone(),
                suggestion_draft_public_id: None,
                expected_suggestion_content_hash: None,
                sources,
                reviewed_by: "local_teacher".into(),
                note: None,
            },
        )
        .unwrap();
        assert_eq!(result.result_quality, "L3");
        assert_eq!(result.knowledge_link_count, 1);
        assert_eq!(result.ability_link_count, 1);
        assert_eq!(
            content::get_question_version(&fixture.conn, question.id)
                .unwrap()
                .unwrap()
                .quality_level,
            "L3"
        );
        let assessment_count: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM exam_assessments_v2", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);
        let evidence_count: i64 = fixture
            .conn
            .query_row("SELECT COUNT(*) FROM learning_evidence", [], |row| {
                row.get(0)
            })
            .unwrap_or(0);
        assert_eq!(assessment_count, 0);
        assert_eq!(evidence_count, 0);
    }

    #[test]
    fn fill_blank_requires_every_slot_and_idempotency_is_strict() {
        let mut fixture = fixture();
        let question = l2_question(
            &fixture.conn,
            "fill_blank",
            "鸦片战争爆发于____年，由____发动。",
        );
        let inbox = list_review_inbox(&fixture.conn, "local_teacher", 10)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(inbox.sources.len(), 2);
        let mut incomplete = source_inputs(
            &inbox,
            &fixture.knowledge_public_id,
            &fixture.ability_public_id,
        );
        incomplete[1].ability_links.clear();
        let request = ConfirmLinkReviewRequest {
            request_key: "review-fill".into(),
            question_version_public_id: question.public_id.clone(),
            expected_question_content_hash: question.content_hash.clone(),
            knowledge_map_public_id: fixture.map_public_id.clone(),
            suggestion_draft_public_id: None,
            expected_suggestion_content_hash: None,
            sources: incomplete,
            reviewed_by: "local_teacher".into(),
            note: None,
        };
        assert!(confirm_links(&mut fixture.conn, "local_teacher", &request).is_err());
        let complete = ConfirmLinkReviewRequest {
            sources: source_inputs(
                &inbox,
                &fixture.knowledge_public_id,
                &fixture.ability_public_id,
            ),
            ..request
        };
        let first = confirm_links(&mut fixture.conn, "local_teacher", &complete).unwrap();
        let same = confirm_links(&mut fixture.conn, "local_teacher", &complete).unwrap();
        assert_eq!(first.public_id, same.public_id);
        let changed = ConfirmLinkReviewRequest {
            note: Some("不同内容".into()),
            ..complete
        };
        assert!(confirm_links(&mut fixture.conn, "local_teacher", &changed).is_err());
    }

    #[test]
    fn short_answer_requires_every_rubric_point_before_l3() {
        let mut fixture = fixture();
        let question = l2_short_answer_question(&fixture.conn, "分析洋务运动失败的原因。");
        let inbox = list_review_inbox(&fixture.conn, "local_teacher", 10)
            .unwrap()
            .pop()
            .unwrap();
        assert_eq!(inbox.sources.len(), 2);
        assert!(inbox.sources.iter().all(|source| {
            source.source_type == "rubric_point"
                && source.required_knowledge_relation.as_deref() == Some("rubric_basis")
        }));

        let mut incomplete = source_inputs(
            &inbox,
            &fixture.knowledge_public_id,
            &fixture.ability_public_id,
        );
        incomplete[1].knowledge_links.clear();
        let request = ConfirmLinkReviewRequest {
            request_key: "review-short-answer".into(),
            question_version_public_id: question.public_id.clone(),
            expected_question_content_hash: question.content_hash.clone(),
            knowledge_map_public_id: fixture.map_public_id.clone(),
            suggestion_draft_public_id: None,
            expected_suggestion_content_hash: None,
            sources: incomplete,
            reviewed_by: "local_teacher".into(),
            note: None,
        };
        assert!(confirm_links(&mut fixture.conn, "local_teacher", &request).is_err());

        let result = confirm_links(
            &mut fixture.conn,
            "local_teacher",
            &ConfirmLinkReviewRequest {
                sources: source_inputs(
                    &inbox,
                    &fixture.knowledge_public_id,
                    &fixture.ability_public_id,
                ),
                ..request
            },
        )
        .unwrap();
        assert_eq!(result.result_quality, "L3");
        assert_eq!(result.knowledge_link_count, 2);
        assert_eq!(result.ability_link_count, 2);
    }
}
