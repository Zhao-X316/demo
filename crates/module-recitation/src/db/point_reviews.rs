//! M1.1 老师逐点评审仓储。
//!
//! 机器逐点评分保持不可变；老师显式“接受”或“修正”时追加新的 review revision。
//! 本层不自行开启事务，调用方必须将它与 verdict、decision_effects 的状态切换一起提交。

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

const SCHEMA_VERSION: i64 = 1;
const REVIEW_COLS: &str = "id,public_id,submission_id,score_run_id,verdict_id,\
                           decision_effect_id,revision,definition_hash,overall_result,state,\
                           created_by,created_at,superseded_at,reverted_at";
const ITEM_COLS: &str = "id,public_id,review_revision_id,rubric_point_id,\
                         source_point_result_id,confirmation_level,confirmed_state,\
                         evidence_spans_json,teacher_note,created_at";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecPointResult {
    pub id: i64,
    pub public_id: String,
    pub score_run_id: i64,
    pub rubric_point_id: i64,
    pub stable_key: String,
    pub canonical_text: String,
    pub order_index: i64,
    pub machine_state: String,
    pub confidence: f64,
    pub evidence_spans_json: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecPointReviewRevision {
    pub id: i64,
    pub public_id: String,
    pub submission_id: i64,
    pub score_run_id: i64,
    pub verdict_id: i64,
    pub decision_effect_id: i64,
    pub revision: i64,
    pub definition_hash: String,
    pub overall_result: String,
    pub state: String,
    pub created_by: String,
    pub created_at: String,
    pub superseded_at: Option<String>,
    pub reverted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecPointReviewItem {
    pub id: i64,
    pub public_id: String,
    pub review_revision_id: i64,
    pub rubric_point_id: i64,
    pub source_point_result_id: i64,
    pub confirmation_level: String,
    pub confirmed_state: String,
    pub evidence_spans_json: String,
    pub teacher_note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeacherPointReviewItemInput {
    pub point_result_id: i64,
    pub confirmation_level: String,
    pub corrected_state: Option<String>,
    pub corrected_evidence_spans_json: Option<String>,
    pub teacher_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeacherPointReviewInput {
    pub score_run_id: i64,
    pub items: Vec<TeacherPointReviewItemInput>,
}

#[derive(Debug, Clone)]
struct PreparedItem {
    source_point_result_id: i64,
    rubric_point_id: i64,
    confirmation_level: String,
    confirmed_state: String,
    evidence_spans_json: String,
    teacher_note: Option<String>,
}

fn review_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecPointReviewRevision> {
    Ok(RecPointReviewRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        submission_id: row.get(2)?,
        score_run_id: row.get(3)?,
        verdict_id: row.get(4)?,
        decision_effect_id: row.get(5)?,
        revision: row.get(6)?,
        definition_hash: row.get(7)?,
        overall_result: row.get(8)?,
        state: row.get(9)?,
        created_by: row.get(10)?,
        created_at: row.get(11)?,
        superseded_at: row.get(12)?,
        reverted_at: row.get(13)?,
    })
}

fn item_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecPointReviewItem> {
    Ok(RecPointReviewItem {
        id: row.get(0)?,
        public_id: row.get(1)?,
        review_revision_id: row.get(2)?,
        rubric_point_id: row.get(3)?,
        source_point_result_id: row.get(4)?,
        confirmation_level: row.get(5)?,
        confirmed_state: row.get(6)?,
        evidence_spans_json: row.get(7)?,
        teacher_note: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn canonical_spans(raw: &str, label: &str) -> CoreResult<String> {
    let value: Value = serde_json::from_str(raw)
        .map_err(|error| CoreError::Invalid(format!("{label}不是有效 JSON: {error}")))?;
    if !value.is_array() {
        return Err(CoreError::Invalid(format!("{label}必须是数组")));
    }
    serde_json::to_string(&value)
        .map_err(|error| CoreError::Invalid(format!("{label}序列化失败: {error}")))
}

fn normalize_note(note: Option<&str>) -> Option<String> {
    note.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub fn list_point_results(conn: &Connection, score_run_id: i64) -> CoreResult<Vec<RecPointResult>> {
    let mut statement = conn.prepare(
        "SELECT result.id,result.public_id,result.score_run_id,result.rubric_point_id,
                point.stable_key,point.canonical_text,point.order_index,
                result.machine_state,result.confidence,result.evidence_spans_json,result.reason
         FROM rec_point_results result
         JOIN rec_rubric_points point ON point.id=result.rubric_point_id
         WHERE result.score_run_id=?1
         ORDER BY point.order_index,result.id",
    )?;
    let rows = statement.query_map([score_run_id], |row| {
        Ok(RecPointResult {
            id: row.get(0)?,
            public_id: row.get(1)?,
            score_run_id: row.get(2)?,
            rubric_point_id: row.get(3)?,
            stable_key: row.get(4)?,
            canonical_text: row.get(5)?,
            order_index: row.get(6)?,
            machine_state: row.get(7)?,
            confidence: row.get(8)?,
            evidence_spans_json: row.get(9)?,
            reason: row.get(10)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn get_review(conn: &Connection, review_id: i64) -> CoreResult<Option<RecPointReviewRevision>> {
    let sql = format!("SELECT {REVIEW_COLS} FROM rec_point_review_revisions WHERE id=?1");
    Ok(conn.query_row(&sql, [review_id], review_row).optional()?)
}

pub fn active_review_for_submission(
    conn: &Connection,
    submission_id: i64,
) -> CoreResult<Option<RecPointReviewRevision>> {
    let sql = format!(
        "SELECT {REVIEW_COLS} FROM rec_point_review_revisions
         WHERE submission_id=?1 AND state='active'"
    );
    Ok(conn
        .query_row(&sql, [submission_id], review_row)
        .optional()?)
}

pub fn list_review_items(conn: &Connection, review_id: i64) -> CoreResult<Vec<RecPointReviewItem>> {
    let sql = format!(
        "SELECT {ITEM_COLS} FROM rec_point_review_items
         WHERE review_revision_id=?1 ORDER BY id"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([review_id], item_row)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

fn prepare_items(
    conn: &Connection,
    submission_id: i64,
    input: &TeacherPointReviewInput,
) -> CoreResult<Vec<PreparedItem>> {
    let score_scope = conn
        .query_row(
            "SELECT submission_id,state FROM rec_score_runs WHERE id=?1",
            [input.score_run_id],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound(format!("score run {}", input.score_run_id)))?;
    if score_scope.0 != submission_id || score_scope.1 != "active" {
        return Err(CoreError::Invalid(
            "只能确认该提交当前仍生效的结构化评分".into(),
        ));
    }

    let results = list_point_results(conn, input.score_run_id)?;
    if results.is_empty() {
        return Err(CoreError::Invalid("结构化评分没有逐点结果".into()));
    }
    if results.len() != input.items.len() {
        return Err(CoreError::Invalid(
            "逐点评审必须明确处理本次评分的全部评分点".into(),
        ));
    }
    let by_id = results
        .iter()
        .map(|result| (result.id, result))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut prepared = Vec::with_capacity(input.items.len());
    for item in &input.items {
        if !seen.insert(item.point_result_id) {
            return Err(CoreError::Invalid("逐点评审包含重复评分点".into()));
        }
        let result = by_id
            .get(&item.point_result_id)
            .ok_or_else(|| CoreError::Invalid("逐点评审引用了其他评分的结果".into()))?;
        let teacher_note = normalize_note(item.teacher_note.as_deref());
        let (confirmed_state, evidence_spans_json) = match item.confirmation_level.as_str() {
            "accepted" => (
                result.machine_state.clone(),
                canonical_spans(&result.evidence_spans_json, "机器证据时间段")?,
            ),
            "corrected" => {
                let state = item
                    .corrected_state
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("人工修正必须选择修正后的状态".into()))?;
                if !matches!(
                    state,
                    "covered" | "partial" | "omitted" | "contradiction" | "uncertain"
                ) {
                    return Err(CoreError::Invalid("人工修正后的评分点状态无效".into()));
                }
                if teacher_note.is_none() {
                    return Err(CoreError::Invalid("人工修正评分点必须填写简短说明".into()));
                }
                let spans = item
                    .corrected_evidence_spans_json
                    .as_deref()
                    .ok_or_else(|| {
                        CoreError::Invalid(
                            "人工修正必须明确提交证据时间段；无定位时使用空数组".into(),
                        )
                    })?;
                (
                    state.to_owned(),
                    canonical_spans(spans, "人工修正证据时间段")?,
                )
            }
            _ => {
                return Err(CoreError::Invalid(
                    "评分点确认方式必须是 accepted 或 corrected".into(),
                ))
            }
        };
        prepared.push(PreparedItem {
            source_point_result_id: result.id,
            rubric_point_id: result.rubric_point_id,
            confirmation_level: item.confirmation_level.clone(),
            confirmed_state,
            evidence_spans_json,
            teacher_note,
        });
    }
    prepared.sort_by_key(|item| {
        by_id
            .get(&item.source_point_result_id)
            .map(|result| result.order_index)
            .unwrap_or(i64::MAX)
    });
    Ok(prepared)
}

fn definition_hash(
    score_run_id: i64,
    overall_result: &str,
    items: &[PreparedItem],
) -> CoreResult<String> {
    let definition_items = items
        .iter()
        .map(|item| {
            let evidence_spans: Value =
                serde_json::from_str(&item.evidence_spans_json).map_err(|error| {
                    CoreError::Invalid(format!("逐点评审证据时间段解析失败: {error}"))
                })?;
            Ok(serde_json::json!({
                "source_point_result_id": item.source_point_result_id,
                "rubric_point_id": item.rubric_point_id,
                "confirmation_level": item.confirmation_level,
                "confirmed_state": item.confirmed_state,
                "evidence_spans": evidence_spans,
                "teacher_note": item.teacher_note
            }))
        })
        .collect::<CoreResult<Vec<_>>>()?;
    let value = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "score_run_id": score_run_id,
        "overall_result": overall_result,
        "items": definition_items
    });
    let encoded = serde_json::to_vec(&value)
        .map_err(|error| CoreError::Invalid(format!("逐点评审定义序列化失败: {error}")))?;
    Ok(hashing::sha256_hex(&encoded))
}

pub(crate) fn record_review_inner(
    conn: &Connection,
    submission_id: i64,
    verdict_id: i64,
    decision_effect_id: i64,
    overall_result: &str,
    decided_by: &str,
    input: &TeacherPointReviewInput,
) -> CoreResult<RecPointReviewRevision> {
    let actor = decided_by.trim();
    if actor.is_empty() {
        return Err(CoreError::Invalid("逐点评审必须记录确认人".into()));
    }
    if !matches!(overall_result, "pass" | "fail") {
        return Err(CoreError::Invalid("逐点评审总体结论无效".into()));
    }
    let prepared = prepare_items(conn, submission_id, input)?;
    let hash = definition_hash(input.score_run_id, overall_result, &prepared)?;

    let existing_sql = format!(
        "SELECT {REVIEW_COLS} FROM rec_point_review_revisions
         WHERE decision_effect_id=?1 AND definition_hash=?2 AND state='active'"
    );
    if let Some(existing) = conn
        .query_row(&existing_sql, params![decision_effect_id, hash], review_row)
        .optional()?
    {
        return Ok(existing);
    }

    let created_at = time::utc_now_rfc3339();
    conn.execute(
        "UPDATE rec_point_review_revisions
         SET state='superseded',superseded_at=?2
         WHERE submission_id=?1 AND state='active'",
        params![submission_id, created_at],
    )?;
    let revision: i64 = conn.query_row(
        "SELECT COALESCE(max(revision),0)+1
         FROM rec_point_review_revisions WHERE submission_id=?1",
        [submission_id],
        |row| row.get(0),
    )?;
    let public_id = ids::new_public_id();
    conn.execute(
        "INSERT INTO rec_point_review_revisions
          (public_id,submission_id,score_run_id,verdict_id,decision_effect_id,revision,
           item_count,definition_hash,overall_result,state,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'building',?10,?11)",
        params![
            public_id,
            submission_id,
            input.score_run_id,
            verdict_id,
            decision_effect_id,
            revision,
            prepared.len() as i64,
            hash,
            overall_result,
            actor,
            created_at,
        ],
    )?;
    let review_id = conn.last_insert_rowid();
    for item in prepared {
        conn.execute(
            "INSERT INTO rec_point_review_items
              (public_id,review_revision_id,rubric_point_id,source_point_result_id,
               confirmation_level,confirmed_state,evidence_spans_json,teacher_note,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                ids::new_public_id(),
                review_id,
                item.rubric_point_id,
                item.source_point_result_id,
                item.confirmation_level,
                item.confirmed_state,
                item.evidence_spans_json,
                item.teacher_note,
                created_at,
            ],
        )?;
    }
    let sealed = conn.execute(
        "UPDATE rec_point_review_revisions SET state='active'
         WHERE id=?1 AND state='building'",
        [review_id],
    )?;
    if sealed != 1 {
        return Err(CoreError::Invalid("逐点评审封存失败，请刷新后重试".into()));
    }
    let accepted_count: i64 = conn.query_row(
        "SELECT count(*) FROM rec_point_review_items
         WHERE review_revision_id=?1 AND confirmation_level='accepted'",
        [review_id],
        |row| row.get(0),
    )?;
    let corrected_count: i64 = conn.query_row(
        "SELECT count(*) FROM rec_point_review_items
         WHERE review_revision_id=?1 AND confirmation_level='corrected'",
        [review_id],
        |row| row.get(0),
    )?;
    let payload = serde_json::json!({
        "schema_version": SCHEMA_VERSION,
        "review_public_id": public_id,
        "submission_id": submission_id,
        "score_run_id": input.score_run_id,
        "verdict_id": verdict_id,
        "decision_effect_id": decision_effect_id,
        "revision": revision,
        "overall_result": overall_result,
        "accepted_count": accepted_count,
        "corrected_count": corrected_count
    })
    .to_string();
    outbox::create_event(
        conn,
        &NewOutboxEvent {
            idempotency_key: &format!("recitation:outbox:point-review:{public_id}"),
            event_type: "recitation_point_review_recorded",
            event_version: 1,
            aggregate_type: "recitation_point_review",
            aggregate_id: &public_id,
            aggregate_revision: revision,
            payload_json: &payload,
            occurred_at: &created_at,
        },
    )?;
    audit::append(
        conn,
        &NewAuditEvent {
            idempotency_key: &format!("recitation:audit:point-review:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(actor),
            action: "recitation.point_review.recorded",
            object_type: "recitation_point_review",
            object_id: &public_id,
            object_revision: Some(revision),
            note: Some("老师显式接受或修正本次结构化机器评分的全部评分点"),
            meta_json: Some(&payload),
            occurred_at: &created_at,
        },
    )?;
    get_review(conn, review_id)?.ok_or_else(|| CoreError::Db("逐点评审写入后无法读取".into()))
}

pub(crate) fn revert_for_effect_inner(
    conn: &Connection,
    decision_effect_id: i64,
) -> CoreResult<usize> {
    let reverted_at = time::utc_now_rfc3339();
    Ok(conn.execute(
        "UPDATE rec_point_review_revisions
         SET state='reverted',reverted_at=?2
         WHERE decision_effect_id=?1 AND state IN ('active','superseded')",
        params![decision_effect_id, reverted_at],
    )?)
}
