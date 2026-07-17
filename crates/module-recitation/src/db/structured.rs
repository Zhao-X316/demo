//! M1.1 结构化背诵证据仓储。
//!
//! 本层只管理不可变答案/rubric/ASR/逐点评分快照，不修改 M1.0 的 verdict、
//! decision_effects、任务或复习卡。机器结果只有在后续老师逐点确认后才可转成正式学习证据。

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

pub const STRUCTURED_SCORING_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecAnswerVersion {
    pub id: i64,
    pub public_id: String,
    pub content_id: i64,
    pub answer_version: i64,
    pub answer_text: String,
    pub provenance: String,
    pub supersedes_answer_version_id: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecRubricVersion {
    pub id: i64,
    pub public_id: String,
    pub answer_version_id: i64,
    pub revision: i64,
    pub definition_hash: String,
    pub status: String,
    pub generated_by_ai_run_id: Option<i64>,
    pub supersedes_rubric_version_id: Option<i64>,
    pub created_by: String,
    pub created_at: String,
    pub confirmed_by: Option<String>,
    pub confirmed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecRubricPoint {
    pub id: i64,
    pub public_id: String,
    pub stable_key: String,
    pub rubric_version_id: i64,
    pub canonical_text: String,
    pub required_entities_json: String,
    pub allowed_paraphrases_json: String,
    pub contradiction_rules_json: String,
    pub required: bool,
    pub weight: f64,
    pub order_index: i64,
    pub knowledge_node_id: Option<i64>,
    pub knowledge_link_state: String,
    pub verified_by: Option<String>,
    pub verified_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecTranscript {
    pub id: i64,
    pub public_id: String,
    pub submission_id: i64,
    pub asr_ai_run_id: i64,
    pub raw_transcript: String,
    pub normalized_transcript: String,
    pub normalization_version: String,
    pub word_segments_json: String,
    pub duration_ms: i64,
    pub output_hash: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecScoreRun {
    pub id: i64,
    pub public_id: String,
    pub submission_id: i64,
    pub transcript_id: i64,
    pub rubric_version_id: i64,
    pub score_ai_run_id: i64,
    pub overall_suggestion: String,
    pub accuracy_json: String,
    pub fluency_json: String,
    pub confidence: f64,
    pub output_hash: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct RubricPointDraftInput<'a> {
    pub stable_key: &'a str,
    pub canonical_text: &'a str,
    pub required_entities_json: &'a str,
    pub allowed_paraphrases_json: &'a str,
    pub contradiction_rules_json: &'a str,
    pub required: bool,
    pub weight: f64,
    pub order_index: i64,
    pub knowledge_node_id: Option<i64>,
    pub knowledge_link_state: &'a str,
    pub verified_by: Option<&'a str>,
    pub verified_at: Option<&'a str>,
}

#[derive(Debug, Clone)]
pub struct CreateRubricDraftInput<'a> {
    pub answer_version_id: i64,
    pub generated_by_ai_run_id: Option<i64>,
    pub created_by: &'a str,
    pub points: &'a [RubricPointDraftInput<'a>],
}

const ANSWER_COLS: &str = "id,public_id,content_id,answer_version,answer_text,provenance,\
                           supersedes_answer_version_id,created_at";
const RUBRIC_COLS: &str = "id,public_id,answer_version_id,revision,definition_hash,status,\
                           generated_by_ai_run_id,supersedes_rubric_version_id,created_by,\
                           created_at,confirmed_by,confirmed_at";
const POINT_COLS: &str = "id,public_id,stable_key,rubric_version_id,canonical_text,\
                          required_entities_json,allowed_paraphrases_json,\
                          contradiction_rules_json,required,weight,order_index,\
                          knowledge_node_id,knowledge_link_state,verified_by,verified_at,created_at";
const TRANSCRIPT_COLS: &str = "id,public_id,submission_id,asr_ai_run_id,raw_transcript,\
                               normalized_transcript,normalization_version,word_segments_json,\
                               duration_ms,output_hash,state,created_at";
const SCORE_COLS: &str = "id,public_id,submission_id,transcript_id,rubric_version_id,\
                          score_ai_run_id,overall_suggestion,accuracy_json,fluency_json,\
                          confidence,output_hash,state,created_at";

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn versioned_items_json(raw: &str, label: &str) -> CoreResult<Value> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| CoreError::Invalid(format!("{label}不是有效 JSON: {error}")))?;
    if value.get("schema_version").and_then(Value::as_i64) != Some(1)
        || !value.get("items").is_some_and(Value::is_array)
    {
        return Err(CoreError::Invalid(format!(
            "{label}必须是 schema_version=1 且包含 items 数组的对象"
        )));
    }
    Ok(value)
}

fn encode_json(value: &Value, label: &str) -> CoreResult<String> {
    serde_json::to_string(value)
        .map_err(|error| CoreError::Invalid(format!("{label}序列化失败: {error}")))
}

fn answer_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecAnswerVersion> {
    Ok(RecAnswerVersion {
        id: row.get(0)?,
        public_id: row.get(1)?,
        content_id: row.get(2)?,
        answer_version: row.get(3)?,
        answer_text: row.get(4)?,
        provenance: row.get(5)?,
        supersedes_answer_version_id: row.get(6)?,
        created_at: row.get(7)?,
    })
}

fn rubric_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecRubricVersion> {
    Ok(RecRubricVersion {
        id: row.get(0)?,
        public_id: row.get(1)?,
        answer_version_id: row.get(2)?,
        revision: row.get(3)?,
        definition_hash: row.get(4)?,
        status: row.get(5)?,
        generated_by_ai_run_id: row.get(6)?,
        supersedes_rubric_version_id: row.get(7)?,
        created_by: row.get(8)?,
        created_at: row.get(9)?,
        confirmed_by: row.get(10)?,
        confirmed_at: row.get(11)?,
    })
}

fn point_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecRubricPoint> {
    Ok(RecRubricPoint {
        id: row.get(0)?,
        public_id: row.get(1)?,
        stable_key: row.get(2)?,
        rubric_version_id: row.get(3)?,
        canonical_text: row.get(4)?,
        required_entities_json: row.get(5)?,
        allowed_paraphrases_json: row.get(6)?,
        contradiction_rules_json: row.get(7)?,
        required: row.get::<_, i64>(8)? != 0,
        weight: row.get(9)?,
        order_index: row.get(10)?,
        knowledge_node_id: row.get(11)?,
        knowledge_link_state: row.get(12)?,
        verified_by: row.get(13)?,
        verified_at: row.get(14)?,
        created_at: row.get(15)?,
    })
}

fn transcript_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecTranscript> {
    Ok(RecTranscript {
        id: row.get(0)?,
        public_id: row.get(1)?,
        submission_id: row.get(2)?,
        asr_ai_run_id: row.get(3)?,
        raw_transcript: row.get(4)?,
        normalized_transcript: row.get(5)?,
        normalization_version: row.get(6)?,
        word_segments_json: row.get(7)?,
        duration_ms: row.get(8)?,
        output_hash: row.get(9)?,
        state: row.get(10)?,
        created_at: row.get(11)?,
    })
}

fn score_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecScoreRun> {
    Ok(RecScoreRun {
        id: row.get(0)?,
        public_id: row.get(1)?,
        submission_id: row.get(2)?,
        transcript_id: row.get(3)?,
        rubric_version_id: row.get(4)?,
        score_ai_run_id: row.get(5)?,
        overall_suggestion: row.get(6)?,
        accuracy_json: row.get(7)?,
        fluency_json: row.get(8)?,
        confidence: row.get(9)?,
        output_hash: row.get(10)?,
        state: row.get(11)?,
        created_at: row.get(12)?,
    })
}

pub fn current_answer_version(
    conn: &Connection,
    content_id: i64,
) -> CoreResult<Option<RecAnswerVersion>> {
    let sql = format!(
        "SELECT {ANSWER_COLS} FROM rec_answer_versions
         WHERE content_id=?1
           AND answer_version=(
             SELECT answer_version FROM rec_contents WHERE id=?1
           )"
    );
    Ok(conn.query_row(&sql, [content_id], answer_row).optional()?)
}

pub fn get_answer_version(
    conn: &Connection,
    answer_version_id: i64,
) -> CoreResult<Option<RecAnswerVersion>> {
    let sql = format!("SELECT {ANSWER_COLS} FROM rec_answer_versions WHERE id=?1");
    Ok(conn
        .query_row(&sql, [answer_version_id], answer_row)
        .optional()?)
}

pub fn get_rubric(
    conn: &Connection,
    rubric_version_id: i64,
) -> CoreResult<Option<RecRubricVersion>> {
    let sql = format!("SELECT {RUBRIC_COLS} FROM rec_rubric_versions WHERE id=?1");
    Ok(conn
        .query_row(&sql, [rubric_version_id], rubric_row)
        .optional()?)
}

pub fn current_confirmed_rubric_for_content(
    conn: &Connection,
    content_id: i64,
) -> CoreResult<Option<RecRubricVersion>> {
    let rubric_cols = RUBRIC_COLS
        .split(',')
        .map(|column| format!("rubric.{column}"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT {rubric_cols}
         FROM rec_rubric_versions rubric
         JOIN rec_answer_versions answer ON answer.id=rubric.answer_version_id
         JOIN rec_contents content
           ON content.id=answer.content_id
          AND content.answer_version=answer.answer_version
         WHERE content.id=?1 AND rubric.status='confirmed'"
    );
    Ok(conn.query_row(&sql, [content_id], rubric_row).optional()?)
}

pub fn list_rubric_points(
    conn: &Connection,
    rubric_version_id: i64,
) -> CoreResult<Vec<RecRubricPoint>> {
    let sql = format!(
        "SELECT {POINT_COLS} FROM rec_rubric_points
         WHERE rubric_version_id=?1 ORDER BY order_index"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([rubric_version_id], point_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn validate_point(point: &RubricPointDraftInput<'_>) -> CoreResult<Value> {
    required(point.stable_key, "评分点稳定标识")?;
    required(point.canonical_text, "评分点标准表述")?;
    if point.weight <= 0.0 || !point.weight.is_finite() {
        return Err(CoreError::Invalid("评分点权重必须大于 0".into()));
    }
    if point.order_index < 0 {
        return Err(CoreError::Invalid("评分点顺序不能小于 0".into()));
    }
    if !matches!(
        point.knowledge_link_state,
        "none" | "suggested" | "confirmed"
    ) {
        return Err(CoreError::Invalid("知识点链接状态无效".into()));
    }
    match point.knowledge_link_state {
        "none"
            if point.knowledge_node_id.is_some()
                || point.verified_by.is_some()
                || point.verified_at.is_some() =>
        {
            return Err(CoreError::Invalid(
                "未链接评分点不能携带知识点或确认人".into(),
            ));
        }
        "suggested"
            if point.knowledge_node_id.is_none()
                || point.verified_by.is_some()
                || point.verified_at.is_some() =>
        {
            return Err(CoreError::Invalid(
                "机器建议链接必须有知识点且不能伪装成人工确认".into(),
            ));
        }
        "confirmed"
            if point.knowledge_node_id.is_none()
                || point.verified_by.is_none()
                || point.verified_at.is_none() =>
        {
            return Err(CoreError::Invalid(
                "已确认知识点链接必须有知识点、确认人和确认时间".into(),
            ));
        }
        _ => {}
    }
    Ok(serde_json::json!({
        "stable_key": point.stable_key.trim(),
        "canonical_text": point.canonical_text.trim(),
        "required_entities": versioned_items_json(
            point.required_entities_json,
            "必需实体"
        )?,
        "allowed_paraphrases": versioned_items_json(
            point.allowed_paraphrases_json,
            "允许改述"
        )?,
        "contradiction_rules": versioned_items_json(
            point.contradiction_rules_json,
            "矛盾规则"
        )?,
        "required": point.required,
        "weight": point.weight,
        "order_index": point.order_index,
        "knowledge_node_id": point.knowledge_node_id,
        "knowledge_link_state": point.knowledge_link_state,
        "verified_by": point.verified_by,
        "verified_at": point.verified_at
    }))
}

pub(crate) fn create_rubric_draft_inner(
    conn: &Connection,
    input: &CreateRubricDraftInput<'_>,
) -> CoreResult<RecRubricVersion> {
    required(input.created_by, "创建人")?;
    if input.points.is_empty() {
        return Err(CoreError::Invalid("rubric 至少需要一个评分点".into()));
    }
    get_answer_version(conn, input.answer_version_id)?.ok_or_else(|| {
        CoreError::NotFound(format!("answer version {}", input.answer_version_id))
    })?;

    let mut stable_keys = HashSet::new();
    let mut order_indexes = HashSet::new();
    let definitions = input
        .points
        .iter()
        .map(|point| {
            if !stable_keys.insert(point.stable_key.trim().to_string()) {
                return Err(CoreError::Invalid("评分点稳定标识重复".into()));
            }
            if !order_indexes.insert(point.order_index) {
                return Err(CoreError::Invalid("评分点顺序重复".into()));
            }
            validate_point(point)
        })
        .collect::<CoreResult<Vec<_>>>()?;
    let definition_json = serde_json::to_string(&serde_json::json!({
        "schema_version": STRUCTURED_SCORING_SCHEMA_VERSION,
        "points": definitions
    }))
    .map_err(|error| CoreError::Invalid(format!("rubric 定义序列化失败: {error}")))?;
    let definition_hash = hashing::sha256_hex(definition_json.as_bytes());

    let existing_sql = format!(
        "SELECT {RUBRIC_COLS} FROM rec_rubric_versions
         WHERE answer_version_id=?1 AND definition_hash=?2"
    );
    if let Some(existing) = conn
        .query_row(
            &existing_sql,
            params![input.answer_version_id, definition_hash],
            rubric_row,
        )
        .optional()?
    {
        return Ok(existing);
    }

    let (revision, supersedes): (i64, Option<i64>) = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1,
                (SELECT id FROM rec_rubric_versions
                 WHERE answer_version_id=?1 ORDER BY revision DESC LIMIT 1)
         FROM rec_rubric_versions WHERE answer_version_id=?1",
        [input.answer_version_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO rec_rubric_versions
          (public_id,answer_version_id,revision,definition_hash,status,
           generated_by_ai_run_id,supersedes_rubric_version_id,created_by,created_at)
         VALUES (?1,?2,?3,?4,'draft',?5,?6,?7,?8)",
        params![
            public_id,
            input.answer_version_id,
            revision,
            definition_hash,
            input.generated_by_ai_run_id,
            supersedes,
            input.created_by.trim(),
            created_at,
        ],
    )?;
    let rubric_id = conn.last_insert_rowid();
    for point in input.points {
        conn.execute(
            "INSERT INTO rec_rubric_points
              (public_id,stable_key,rubric_version_id,canonical_text,
               required_entities_json,allowed_paraphrases_json,contradiction_rules_json,
               required,weight,order_index,knowledge_node_id,knowledge_link_state,
               verified_by,verified_at,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                ids::new_public_id(),
                point.stable_key.trim(),
                rubric_id,
                point.canonical_text.trim(),
                encode_json(
                    &versioned_items_json(point.required_entities_json, "必需实体")?,
                    "必需实体"
                )?,
                encode_json(
                    &versioned_items_json(point.allowed_paraphrases_json, "允许改述")?,
                    "允许改述"
                )?,
                encode_json(
                    &versioned_items_json(point.contradiction_rules_json, "矛盾规则")?,
                    "矛盾规则"
                )?,
                i64::from(point.required),
                point.weight,
                point.order_index,
                point.knowledge_node_id,
                point.knowledge_link_state,
                point.verified_by,
                point.verified_at,
                created_at,
            ],
        )?;
    }
    let event_payload = serde_json::json!({
        "schema_version": STRUCTURED_SCORING_SCHEMA_VERSION,
        "rubric_public_id": public_id,
        "answer_version_id": input.answer_version_id,
        "revision": revision,
        "definition_hash": definition_hash,
        "point_count": input.points.len(),
        "status": "draft"
    })
    .to_string();
    outbox::create_event(
        conn,
        &NewOutboxEvent {
            idempotency_key: &format!("recitation:outbox:rubric-draft:{public_id}"),
            event_type: "recitation_rubric_draft_created",
            event_version: 1,
            aggregate_type: "recitation_rubric_version",
            aggregate_id: &public_id,
            aggregate_revision: revision,
            payload_json: &event_payload,
            occurred_at: &created_at,
        },
    )?;
    audit::append(
        conn,
        &NewAuditEvent {
            idempotency_key: &format!("recitation:audit:rubric-draft:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.created_by.trim()),
            action: "recitation.rubric_draft.created",
            object_type: "recitation_rubric_version",
            object_id: &public_id,
            object_revision: Some(revision),
            note: Some("创建背诵评分点草稿；尚未用于机器评分或老师终审"),
            meta_json: Some(&event_payload),
            occurred_at: &created_at,
        },
    )?;
    get_rubric(conn, rubric_id)?.ok_or_else(|| CoreError::Db("rubric 草稿创建后无法读取".into()))
}

/// 创建不可变 rubric 草稿。相同答案版本 + 相同定义 hash 返回同一草稿。
pub fn create_rubric_draft(
    conn: &Connection,
    input: &CreateRubricDraftInput<'_>,
) -> CoreResult<RecRubricVersion> {
    let transaction = conn.unchecked_transaction()?;
    let rubric = create_rubric_draft_inner(&transaction, input)?;
    transaction.commit()?;
    Ok(rubric)
}

pub(crate) fn confirm_rubric_inner(
    conn: &Connection,
    rubric_version_id: i64,
    confirmed_by: &str,
) -> CoreResult<RecRubricVersion> {
    required(confirmed_by, "确认人")?;
    let rubric = get_rubric(conn, rubric_version_id)?
        .ok_or_else(|| CoreError::NotFound(format!("rubric version {rubric_version_id}")))?;
    if rubric.status == "confirmed" {
        return Ok(rubric);
    }
    if rubric.status != "draft" {
        return Err(CoreError::Invalid("已退役 rubric 不能重新确认".into()));
    }
    let point_count = list_rubric_points(conn, rubric_version_id)?.len();
    if point_count == 0 {
        return Err(CoreError::Invalid("没有评分点的 rubric 不能确认".into()));
    }

    let confirmed_at = time::utc_now_rfc3339();
    conn.execute(
        "UPDATE rec_rubric_versions SET status='retired'
         WHERE answer_version_id=?1 AND status='confirmed' AND id<>?2",
        params![rubric.answer_version_id, rubric_version_id],
    )?;
    let changed = conn.execute(
        "UPDATE rec_rubric_versions
         SET status='confirmed',confirmed_by=?2,confirmed_at=?3
         WHERE id=?1 AND status='draft'",
        params![rubric_version_id, confirmed_by.trim(), confirmed_at],
    )?;
    if changed != 1 {
        return Err(CoreError::Invalid("rubric 状态已变化，请刷新后重试".into()));
    }
    let event_payload = serde_json::json!({
        "schema_version": STRUCTURED_SCORING_SCHEMA_VERSION,
        "rubric_public_id": rubric.public_id,
        "answer_version_id": rubric.answer_version_id,
        "revision": rubric.revision,
        "definition_hash": rubric.definition_hash,
        "point_count": point_count,
        "status": "confirmed"
    })
    .to_string();
    outbox::create_event(
        conn,
        &NewOutboxEvent {
            idempotency_key: &format!("recitation:outbox:rubric-confirmed:{}", rubric.public_id),
            event_type: "recitation_rubric_confirmed",
            event_version: 1,
            aggregate_type: "recitation_rubric_version",
            aggregate_id: &rubric.public_id,
            aggregate_revision: rubric.revision,
            payload_json: &event_payload,
            occurred_at: &confirmed_at,
        },
    )?;
    audit::append(
        conn,
        &NewAuditEvent {
            idempotency_key: &format!("recitation:audit:rubric-confirmed:{}", rubric.public_id),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "recitation.rubric.confirmed",
            object_type: "recitation_rubric_version",
            object_id: &rubric.public_id,
            object_revision: Some(rubric.revision),
            note: Some("老师确认背诵评分点定义；机器分析仍只作为建议"),
            meta_json: Some(&event_payload),
            occurred_at: &confirmed_at,
        },
    )?;
    get_rubric(conn, rubric_version_id)?
        .ok_or_else(|| CoreError::Db("rubric 确认后无法读取".into()))
}

/// 老师确认 rubric；同答案版本旧 confirmed 自动退役，但旧行保持可追溯。
pub fn confirm_rubric(
    conn: &Connection,
    rubric_version_id: i64,
    confirmed_by: &str,
) -> CoreResult<RecRubricVersion> {
    let transaction = conn.unchecked_transaction()?;
    let rubric = confirm_rubric_inner(&transaction, rubric_version_id, confirmed_by)?;
    transaction.commit()?;
    Ok(rubric)
}

#[derive(Debug, Deserialize)]
struct TranscriptAiOutput {
    schema_version: i64,
    submission_id: i64,
    raw_transcript: String,
    normalized_transcript: String,
    normalization_version: String,
    word_segments: Value,
    duration_ms: i64,
}

pub fn get_transcript_by_ai_run(
    conn: &Connection,
    ai_run_id: i64,
) -> CoreResult<Option<RecTranscript>> {
    let sql = format!("SELECT {TRANSCRIPT_COLS} FROM rec_transcripts WHERE asr_ai_run_id=?1");
    Ok(conn
        .query_row(&sql, [ai_run_id], transcript_row)
        .optional()?)
}

pub fn active_transcript_for_submission(
    conn: &Connection,
    submission_id: i64,
) -> CoreResult<Option<RecTranscript>> {
    let sql = format!(
        "SELECT {TRANSCRIPT_COLS} FROM rec_transcripts
         WHERE submission_id=?1 AND state='active'"
    );
    Ok(conn
        .query_row(&sql, [submission_id], transcript_row)
        .optional()?)
}

pub(crate) fn record_transcript_from_ai_run_inner(
    conn: &Connection,
    ai_run_id: i64,
) -> CoreResult<RecTranscript> {
    if let Some(existing) = get_transcript_by_ai_run(conn, ai_run_id)? {
        return Ok(existing);
    }
    let run = suite_core::db::repo::ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai run {ai_run_id}")))?;
    let output_json = run
        .output_json
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("ASR run 没有成功输出".into()))?;
    let output: TranscriptAiOutput = serde_json::from_str(output_json)
        .map_err(|error| CoreError::Invalid(format!("ASR 输出合同无效: {error}")))?;
    if output.schema_version != STRUCTURED_SCORING_SCHEMA_VERSION
        || !output.word_segments.is_array()
        || output.duration_ms < 0
    {
        return Err(CoreError::Invalid("ASR 输出合同版本或字段无效".into()));
    }
    required(&output.normalization_version, "规范化规则版本")?;
    let output_hash = run
        .output_hash
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("ASR run 缺少输出 hash".into()))?;
    let segments_json = serde_json::to_string(&output.word_segments)
        .map_err(|error| CoreError::Invalid(format!("ASR 时间段序列化失败: {error}")))?;
    let created_at = run
        .finished_at
        .clone()
        .unwrap_or_else(time::utc_now_rfc3339);

    conn.execute(
        "UPDATE rec_score_runs SET state='superseded'
         WHERE submission_id=?1 AND state='active'",
        [output.submission_id],
    )?;
    conn.execute(
        "UPDATE rec_transcripts SET state='superseded'
         WHERE submission_id=?1 AND state='active'",
        [output.submission_id],
    )?;
    let public_id = ids::new_public_id();
    conn.execute(
        "INSERT INTO rec_transcripts
          (public_id,submission_id,asr_ai_run_id,raw_transcript,normalized_transcript,
           normalization_version,word_segments_json,duration_ms,output_hash,state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'active',?10)",
        params![
            public_id,
            output.submission_id,
            ai_run_id,
            output.raw_transcript,
            output.normalized_transcript,
            output.normalization_version,
            segments_json,
            output.duration_ms,
            output_hash,
            created_at,
        ],
    )?;
    let transcript_id = conn.last_insert_rowid();
    let sql = format!("SELECT {TRANSCRIPT_COLS} FROM rec_transcripts WHERE id=?1");
    conn.query_row(&sql, [transcript_id], transcript_row)
        .map_err(Into::into)
}

/// 将 succeeded ASR run 物化为不可变 transcript。重复物化同一 run 返回原行。
pub fn record_transcript_from_ai_run(
    conn: &Connection,
    ai_run_id: i64,
) -> CoreResult<RecTranscript> {
    let transaction = conn.unchecked_transaction()?;
    let transcript = record_transcript_from_ai_run_inner(&transaction, ai_run_id)?;
    transaction.commit()?;
    Ok(transcript)
}

#[derive(Debug, Deserialize)]
struct ScorePointAiOutput {
    rubric_point_id: i64,
    machine_state: String,
    confidence: f64,
    evidence_spans: Value,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct ScoreAiOutput {
    schema_version: i64,
    submission_id: i64,
    transcript_id: i64,
    rubric_version_id: i64,
    overall_suggestion: String,
    accuracy: Value,
    fluency: Value,
    confidence: f64,
    point_results: Vec<ScorePointAiOutput>,
}

pub fn get_score_by_ai_run(conn: &Connection, ai_run_id: i64) -> CoreResult<Option<RecScoreRun>> {
    let sql = format!("SELECT {SCORE_COLS} FROM rec_score_runs WHERE score_ai_run_id=?1");
    Ok(conn.query_row(&sql, [ai_run_id], score_row).optional()?)
}

pub fn active_score_for_submission(
    conn: &Connection,
    submission_id: i64,
) -> CoreResult<Option<RecScoreRun>> {
    let sql = format!(
        "SELECT {SCORE_COLS} FROM rec_score_runs
         WHERE submission_id=?1 AND state='active'"
    );
    Ok(conn
        .query_row(&sql, [submission_id], score_row)
        .optional()?)
}

pub(crate) fn record_score_from_ai_run_inner(
    conn: &Connection,
    ai_run_id: i64,
) -> CoreResult<RecScoreRun> {
    if let Some(existing) = get_score_by_ai_run(conn, ai_run_id)? {
        return Ok(existing);
    }
    let run = suite_core::db::repo::ai_runs::get_by_id(conn, ai_run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai run {ai_run_id}")))?;
    let output_json = run
        .output_json
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("评分 run 没有成功输出".into()))?;
    let output: ScoreAiOutput = serde_json::from_str(output_json)
        .map_err(|error| CoreError::Invalid(format!("结构化评分输出合同无效: {error}")))?;
    if output.schema_version != STRUCTURED_SCORING_SCHEMA_VERSION
        || !matches!(
            output.overall_suggestion.as_str(),
            "pass" | "fail" | "unable_to_score"
        )
        || !output.accuracy.is_object()
        || output
            .accuracy
            .get("schema_version")
            .and_then(Value::as_i64)
            != Some(1)
        || !output.fluency.is_object()
        || output.fluency.get("schema_version").and_then(Value::as_i64) != Some(1)
        || !(0.0..=1.0).contains(&output.confidence)
    {
        return Err(CoreError::Invalid(
            "结构化评分输出合同版本或字段无效".into(),
        ));
    }
    let rubric_points = list_rubric_points(conn, output.rubric_version_id)?;
    if rubric_points.len() != output.point_results.len() {
        return Err(CoreError::Invalid(
            "结构化评分必须逐一返回全部 rubric 评分点".into(),
        ));
    }
    let expected_ids = rubric_points
        .iter()
        .map(|point| point.id)
        .collect::<HashSet<_>>();
    let mut actual_ids = HashSet::new();
    for point in &output.point_results {
        if !expected_ids.contains(&point.rubric_point_id)
            || !actual_ids.insert(point.rubric_point_id)
            || !matches!(
                point.machine_state.as_str(),
                "covered" | "partial" | "omitted" | "contradiction" | "uncertain"
            )
            || !(0.0..=1.0).contains(&point.confidence)
            || !point.evidence_spans.is_array()
        {
            return Err(CoreError::Invalid("逐点评分结果范围或字段无效".into()));
        }
    }
    let output_hash = run
        .output_hash
        .as_deref()
        .ok_or_else(|| CoreError::Invalid("评分 run 缺少输出 hash".into()))?;
    let accuracy_json = serde_json::to_string(&output.accuracy)
        .map_err(|error| CoreError::Invalid(format!("准确性结果序列化失败: {error}")))?;
    let fluency_json = serde_json::to_string(&output.fluency)
        .map_err(|error| CoreError::Invalid(format!("流畅度结果序列化失败: {error}")))?;
    let created_at = run
        .finished_at
        .clone()
        .unwrap_or_else(time::utc_now_rfc3339);

    conn.execute(
        "UPDATE rec_score_runs SET state='superseded'
         WHERE submission_id=?1 AND state='active'",
        [output.submission_id],
    )?;
    let public_id = ids::new_public_id();
    conn.execute(
        "INSERT INTO rec_score_runs
          (public_id,submission_id,transcript_id,rubric_version_id,score_ai_run_id,
           overall_suggestion,accuracy_json,fluency_json,confidence,output_hash,state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11)",
        params![
            public_id,
            output.submission_id,
            output.transcript_id,
            output.rubric_version_id,
            ai_run_id,
            output.overall_suggestion,
            accuracy_json,
            fluency_json,
            output.confidence,
            output_hash,
            created_at,
        ],
    )?;
    let score_id = conn.last_insert_rowid();
    for point in output.point_results {
        let evidence_json = serde_json::to_string(&point.evidence_spans)
            .map_err(|error| CoreError::Invalid(format!("证据时间段序列化失败: {error}")))?;
        conn.execute(
            "INSERT INTO rec_point_results
              (public_id,score_run_id,rubric_point_id,machine_state,confidence,
               evidence_spans_json,reason,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                ids::new_public_id(),
                score_id,
                point.rubric_point_id,
                point.machine_state,
                point.confidence,
                evidence_json,
                point.reason,
                created_at,
            ],
        )?;
    }
    let sql = format!("SELECT {SCORE_COLS} FROM rec_score_runs WHERE id=?1");
    conn.query_row(&sql, [score_id], score_row)
        .map_err(Into::into)
}

/// 将 succeeded recitation_score run 物化为机器建议与逐点结果。
/// 不写 verdict，不触发老师终审，也不生成学习证据。
pub fn record_score_from_ai_run(conn: &Connection, ai_run_id: i64) -> CoreResult<RecScoreRun> {
    let transaction = conn.unchecked_transaction()?;
    let score = record_score_from_ai_run_inner(&transaction, ai_run_id)?;
    transaction.commit()?;
    Ok(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::repo::ai_runs::{self, NewAiRun};
    use suite_core::db::repo::submissions::{self, NewSubmission};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{MediaType, ModuleKey};

    use crate::db::contents::{self, ContentInput};

    const ITEMS_EMPTY: &str = r#"{"schema_version":1,"items":[]}"#;
    const INPUT_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const STARTED: &str = "2026-07-17T08:00:00.000Z";
    const FINISHED: &str = "2026-07-17T08:00:01.000Z";

    fn setup() -> Connection {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::recitation_migrations()).unwrap();
        conn
    }

    fn add_content(conn: &Connection) -> RecAnswerVersion {
        let content = contents::upsert(
            conn,
            &ContentInput {
                content_no: "M1-1",
                title: "洋务运动",
                answer_text: "前期以自强为口号，后期以求富为口号。",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        current_answer_version(conn, content.id).unwrap().unwrap()
    }

    fn point<'a>(
        stable_key: &'a str,
        text: &'a str,
        order_index: i64,
    ) -> RubricPointDraftInput<'a> {
        RubricPointDraftInput {
            stable_key,
            canonical_text: text,
            required_entities_json: ITEMS_EMPTY,
            allowed_paraphrases_json: ITEMS_EMPTY,
            contradiction_rules_json: ITEMS_EMPTY,
            required: true,
            weight: 1.0,
            order_index,
            knowledge_node_id: None,
            knowledge_link_state: "none",
            verified_by: None,
            verified_at: None,
        }
    }

    fn add_confirmed_rubric(conn: &Connection, answer_version_id: i64) -> RecRubricVersion {
        let points = [point("front-slogan", "前期口号是自强", 0)];
        let draft = create_rubric_draft(
            conn,
            &CreateRubricDraftInput {
                answer_version_id,
                generated_by_ai_run_id: None,
                created_by: "teacher-1",
                points: &points,
            },
        )
        .unwrap();
        confirm_rubric(conn, draft.id, "teacher-1").unwrap()
    }

    fn add_submission(conn: &Connection, content_id: i64, hash: &str) -> i64 {
        submissions::insert(
            conn,
            &NewSubmission {
                module: ModuleKey::Recitation,
                task_id: None,
                student_id: None,
                ref_id: Some(content_id),
                media_type: MediaType::Audio,
                file_path: "/tmp/m1-structured.m4a",
                file_hash: hash,
                duration_ms: Some(1_200),
                parsed_meta: None,
                anomaly_type: None,
                status: "pending",
            },
        )
        .unwrap()
    }

    fn succeeded_run(
        conn: &Connection,
        key: &str,
        run_type: &str,
        business_ref_type: &str,
        business_ref_id: &str,
        output_json: &str,
    ) -> i64 {
        let output_hash = hashing::sha256_hex(output_json.as_bytes());
        let run = ai_runs::create_or_get(
            conn,
            &NewAiRun {
                idempotency_key: key,
                run_type,
                source_module: "recitation",
                business_ref_type,
                business_ref_id,
                input_artifact_id: None,
                provider: "fixture",
                model_name: "fixture",
                model_version: "1",
                config_version: "1",
                prompt_or_rule_version: "1",
                input_hash: INPUT_HASH,
                retry_of_ai_run_id: None,
            },
        )
        .unwrap();
        ai_runs::start(conn, run.id, STARTED, None).unwrap();
        ai_runs::finalize_succeeded(conn, run.id, &output_hash, Some(0.9), output_json, FINISHED)
            .unwrap();
        run.id
    }

    #[test]
    fn legacy_migration_preserves_only_provable_current_answer() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, &crate::recitation_migrations()[..1]).unwrap();
        conn.execute(
            "INSERT INTO rec_contents
              (content_no,title,answer_text,answer_version,enabled,created_at,updated_at)
             VALUES ('legacy-1','旧内容','当前仅存答案',3,1,'2026-07-01','2026-07-02')",
            [],
        )
        .unwrap();
        let content_id = conn.last_insert_rowid();
        let submission_id = add_submission(&conn, content_id, "legacy-audio");
        conn.execute(
            "INSERT INTO verdicts
              (submission_id,module,primary_score,pass,scorer,answer_version)
             VALUES (?1,'recitation',0.8,1,'legacy',3)",
            [submission_id],
        )
        .unwrap();

        run_migrations(&conn, &crate::recitation_migrations()[1..]).unwrap();
        let answer = current_answer_version(&conn, content_id).unwrap().unwrap();
        assert_eq!(answer.answer_version, 3);
        assert_eq!(answer.answer_text, "当前仅存答案");
        assert_eq!(answer.provenance, "legacy_current_only");
        let answer_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM rec_answer_versions WHERE content_id=?1",
                [content_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(answer_count, 1, "不能猜测已经丢失的 v1/v2 文本");
        let verdict_version: i64 = conn
            .query_row(
                "SELECT answer_version FROM verdicts WHERE submission_id=?1",
                [submission_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(verdict_version, 3);
        let invented: i64 = conn
            .query_row(
                "SELECT
                  (SELECT count(*) FROM rec_rubric_versions) +
                  (SELECT count(*) FROM rec_transcripts) +
                  (SELECT count(*) FROM rec_score_runs)",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(invented, 0);
    }

    #[test]
    fn new_answer_versions_append_and_invalid_steps_are_rejected() {
        let conn = setup();
        let first = add_content(&conn);
        assert_eq!(first.answer_version, 1);
        assert_eq!(first.provenance, "system_versioned");

        let same = contents::upsert(
            &conn,
            &ContentInput {
                content_no: "M1-1",
                title: "洋务运动（修改标题）",
                answer_text: "前期以自强为口号，后期以求富为口号。",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        assert_eq!(same.answer_version, 1);
        let changed = contents::upsert(
            &conn,
            &ContentInput {
                content_no: "M1-1",
                title: "洋务运动（修改标题）",
                answer_text: "前期自强，后期求富。",
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap();
        assert_eq!(changed.answer_version, 2);
        let second = current_answer_version(&conn, changed.id).unwrap().unwrap();
        assert_eq!(second.supersedes_answer_version_id, Some(first.id));
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM rec_answer_versions WHERE content_id=?1",
                [changed.id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
        assert!(conn
            .execute(
                "UPDATE rec_contents SET answer_text='越级修改',answer_version=4 WHERE id=?1",
                [changed.id],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE rec_answer_versions SET answer_text='覆盖历史' WHERE id=?1",
                [first.id],
            )
            .is_err());
    }

    #[test]
    fn rubric_drafts_are_content_idempotent_and_confirmation_is_audited() {
        let conn = setup();
        let answer = add_content(&conn);
        let first_points = [point("front-slogan", "前期口号是自强", 0)];
        let first = create_rubric_draft(
            &conn,
            &CreateRubricDraftInput {
                answer_version_id: answer.id,
                generated_by_ai_run_id: None,
                created_by: "teacher-1",
                points: &first_points,
            },
        )
        .unwrap();
        let same = create_rubric_draft(
            &conn,
            &CreateRubricDraftInput {
                answer_version_id: answer.id,
                generated_by_ai_run_id: None,
                created_by: "teacher-1",
                points: &first_points,
            },
        )
        .unwrap();
        assert_eq!(first.id, same.id);
        let first = confirm_rubric(&conn, first.id, "teacher-1").unwrap();
        assert_eq!(first.status, "confirmed");

        let second_points = [
            point("front-slogan", "前期口号是自强", 0),
            point("back-slogan", "后期口号是求富", 1),
        ];
        let second = create_rubric_draft(
            &conn,
            &CreateRubricDraftInput {
                answer_version_id: answer.id,
                generated_by_ai_run_id: None,
                created_by: "teacher-1",
                points: &second_points,
            },
        )
        .unwrap();
        assert_eq!(second.revision, 2);
        let second = confirm_rubric(&conn, second.id, "teacher-1").unwrap();
        assert_eq!(second.status, "confirmed");
        assert_eq!(
            get_rubric(&conn, first.id).unwrap().unwrap().status,
            "retired"
        );
        assert_eq!(list_rubric_points(&conn, second.id).unwrap().len(), 2);
        assert!(conn
            .execute(
                "UPDATE rec_rubric_points SET canonical_text='静默覆盖' WHERE rubric_version_id=?1",
                [second.id],
            )
            .is_err());
        let audit_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM audit_events
                 WHERE object_type='recitation_rubric_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let outbox_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM outbox_events
                 WHERE aggregate_type='recitation_rubric_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(audit_count, 4);
        assert_eq!(outbox_count, 4);
    }

    #[test]
    fn structured_ai_snapshots_require_exact_scope_and_do_not_create_verdicts() {
        let conn = setup();
        let answer = add_content(&conn);
        let rubric = add_confirmed_rubric(&conn, answer.id);
        let point_id = list_rubric_points(&conn, rubric.id).unwrap()[0].id;
        let submission_id = add_submission(&conn, answer.content_id, "structured-audio");

        let transcript_output = serde_json::json!({
            "schema_version": 1,
            "submission_id": submission_id,
            "raw_transcript": "前期自强",
            "normalized_transcript": "前期自强",
            "normalization_version": "m1-normalize-v1",
            "word_segments": [
                {"text":"前期","start_ms":0,"end_ms":300,"confidence":0.98},
                {"text":"自强","start_ms":320,"end_ms":700,"confidence":0.96}
            ],
            "duration_ms": 1200
        })
        .to_string();
        let transcript_run_id = succeeded_run(
            &conn,
            "m1-asr-1",
            "asr",
            "submission",
            &submission_id.to_string(),
            &transcript_output,
        );
        let transcript = record_transcript_from_ai_run(&conn, transcript_run_id).unwrap();
        assert_eq!(transcript.submission_id, submission_id);
        assert_eq!(
            record_transcript_from_ai_run(&conn, transcript_run_id)
                .unwrap()
                .id,
            transcript.id
        );

        let score_output = serde_json::json!({
            "schema_version": 1,
            "submission_id": submission_id,
            "transcript_id": transcript.id,
            "rubric_version_id": rubric.id,
            "overall_suggestion": "pass",
            "accuracy": {"schema_version":1,"coverage":1.0},
            "fluency": {"schema_version":1,"score":0.86},
            "confidence": 0.91,
            "point_results": [{
                "rubric_point_id": point_id,
                "machine_state": "covered",
                "confidence": 0.94,
                "evidence_spans": [{"start_ms":0,"end_ms":700}],
                "reason": "明确覆盖前期口号"
            }]
        })
        .to_string();
        let score_run_id = succeeded_run(
            &conn,
            "m1-score-1",
            "recitation_score",
            "recitation_transcript",
            &transcript.id.to_string(),
            &score_output,
        );
        let score = record_score_from_ai_run(&conn, score_run_id).unwrap();
        assert_eq!(score.overall_suggestion, "pass");
        assert_eq!(
            record_score_from_ai_run(&conn, score_run_id).unwrap().id,
            score.id
        );
        let point_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM rec_point_results WHERE score_run_id=?1",
                [score.id],
                |row| row.get(0),
            )
            .unwrap();
        let verdict_count: i64 = conn
            .query_row("SELECT count(*) FROM verdicts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(point_count, 1);
        assert_eq!(verdict_count, 0, "结构化机器建议不能越过老师创建 verdict");

        let wrong_scope_output = serde_json::json!({
            "schema_version": 1,
            "submission_id": submission_id,
            "raw_transcript": "错误作用域",
            "normalized_transcript": "错误作用域",
            "normalization_version": "m1-normalize-v1",
            "word_segments": [],
            "duration_ms": 100
        })
        .to_string();
        let wrong_run_id = succeeded_run(
            &conn,
            "m1-asr-wrong-scope",
            "asr",
            "submission",
            "999999",
            &wrong_scope_output,
        );
        assert!(record_transcript_from_ai_run(&conn, wrong_run_id).is_err());
        let transcript_count: i64 = conn
            .query_row("SELECT count(*) FROM rec_transcripts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(transcript_count, 1, "失败物化必须整体回滚");
    }
}
