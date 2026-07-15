//! 答案资料结构化候选的复核与一次确认。
//!
//! `ai_run` 保存不可变模型输出，逐题草稿复用 B3a 的答案权威候选表；老师只能一次
//! 确认“与当前作业答案一致”或明确“沿用当前作业答案”。本批不修改 assessment item，
//! 也不把冲突答案静默晋级为 K1 新版本。

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::{ai_runs, audit};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{AiRunStatus, AuditActorType};

use crate::answer_source_recognition::{
    AnswerSourceRecognitionOutput, AnswerSourceState, ANSWER_SOURCE_SCHEMA_VERSION,
};

use super::fixed_paper::{record_answer_authority_candidate, NewAnswerAuthorityCandidate};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSourceReviewItem {
    pub assessment_item_id: i64,
    pub order_index: i64,
    pub question_no: String,
    pub question_type: String,
    pub question_stem: String,
    pub bound_answer_key_version_id: i64,
    pub bound_answer_json: String,
    pub candidate_id: Option<i64>,
    pub candidate_answer_json: Option<String>,
    pub source_anchor_json: Option<String>,
    pub match_state: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnswerSourceReviewSummary {
    pub ingest_batch_id: i64,
    pub source_ai_run_id: i64,
    pub source_state: String,
    pub route: String,
    pub matched_count: i64,
    pub conflict_count: i64,
    pub missing_count: i64,
    pub resolution: Option<String>,
    pub items: Vec<AnswerSourceReviewItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnswerSourcePreflightGate {
    Ready,
    ReviewRequired,
    Blocked,
}

fn parse_object(json: &str, label: &str) -> CoreResult<Value> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| CoreError::Parse(format!("{label}无法解析：{error}")))?;
    if !value.is_object()
        || value
            .get("schema_version")
            .and_then(Value::as_i64)
            .is_none()
    {
        return Err(CoreError::Parse(format!("{label}缺少 schema_version")));
    }
    Ok(value)
}

fn output_for_run(
    conn: &Connection,
    batch_id: i64,
    run_id: i64,
) -> CoreResult<AnswerSourceRecognitionOutput> {
    let run = ai_runs::get_by_id(conn, run_id)?
        .ok_or_else(|| CoreError::NotFound(format!("ai_run#{run_id}")))?;
    if run.run_type != "answer_source_structure"
        || run.business_ref_type != "fixed_answer_source"
        || run.business_ref_id != batch_id.to_string()
        || run.status != AiRunStatus::Succeeded
    {
        return Err(CoreError::Invalid(
            "答案结构化 run 不属于当前批次或尚未成功".into(),
        ));
    }
    let output: AnswerSourceRecognitionOutput = serde_json::from_str(
        run.output_json
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("答案结构化成功 run 缺少输出".into()))?,
    )
    .map_err(|error| CoreError::Parse(format!("答案结构化 run 输出无法解析：{error}")))?;
    if output.schema_version != ANSWER_SOURCE_SCHEMA_VERSION
        || output.ingest_batch_id != batch_id
        || output.source_artifact_id != run.input_artifact_id.unwrap_or_default()
    {
        return Err(CoreError::Invalid("答案结构化 run 输出身份链不一致".into()));
    }
    Ok(output)
}

pub fn materialize_ai_drafts(
    conn: &Connection,
    output: &AnswerSourceRecognitionOutput,
    source_ai_run_id: i64,
) -> CoreResult<()> {
    let source_artifact_id: i64 = conn.query_row(
        "SELECT source_artifact_id FROM exam_fixed_input_documents_v2
         WHERE ingest_batch_id=?1 AND document_role='answer_source' AND state<>'voided'",
        [output.ingest_batch_id],
        |row| row.get(0),
    )?;
    for entry in &output.entries {
        let mut anchor = entry.source_anchor.clone();
        let object = anchor
            .as_object_mut()
            .ok_or_else(|| CoreError::Invalid("答案来源锚点必须是对象".into()))?;
        object.insert("source_ai_run_id".into(), Value::from(source_ai_run_id));
        object.insert("confidence".into(), Value::from(entry.confidence));
        record_answer_authority_candidate(
            conn,
            &NewAnswerAuthorityCandidate {
                ingest_batch_id: output.ingest_batch_id,
                assessment_item_id: entry.assessment_item_id,
                source_kind: "ai_draft",
                answer_key_version_id: None,
                source_artifact_id: Some(source_artifact_id),
                candidate_answer_json: &entry.answer_json.to_string(),
                source_anchor_json: &anchor.to_string(),
                teacher_confirmed: false,
                idempotency_key: &format!(
                    "answer-source-run:{source_ai_run_id}:item:{}",
                    entry.assessment_item_id
                ),
                created_by_type: "system",
                created_by: None,
            },
        )?;
    }
    Ok(())
}

pub fn review_summary(
    conn: &Connection,
    batch_id: i64,
    source_ai_run_id: i64,
) -> CoreResult<AnswerSourceReviewSummary> {
    let output = output_for_run(conn, batch_id, source_ai_run_id)?;
    let resolution: Option<String> = conn
        .query_row(
            "SELECT decision FROM exam_answer_source_resolutions_v2 WHERE source_ai_run_id=?1",
            [source_ai_run_id],
            |row| row.get(0),
        )
        .optional()?;
    let mut stmt = conn.prepare(
        "SELECT i.id,i.order_index,
                COALESCE(json_extract(i.presentation_snapshot_json,'$.question_no'),CAST(i.order_index+1 AS TEXT)),
                q.question_type,q.stem,i.answer_key_version_id,a.answer_json,
                c.id,c.candidate_answer_json,c.source_anchor_json
         FROM exam_ingest_batches_v2 b
         JOIN exam_assessment_items_v2 i
           ON i.assessment_version_id=b.assessment_version_id AND i.state='active'
         JOIN k1_question_versions q ON q.id=i.question_version_id
         JOIN k1_answer_key_versions a ON a.id=i.answer_key_version_id AND a.state='confirmed'
         LEFT JOIN exam_answer_authority_candidates_v2 c
           ON c.ingest_batch_id=b.id AND c.assessment_item_id=i.id
          AND c.source_kind='ai_draft'
          AND CAST(json_extract(c.source_anchor_json,'$.source_ai_run_id') AS INTEGER)=?2
         WHERE b.id=?1
         ORDER BY i.order_index,i.id",
    )?;
    let rows = stmt.query_map((batch_id, source_ai_run_id), |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
            row.get::<_, i64>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, Option<i64>>(7)?,
            row.get::<_, Option<String>>(8)?,
            row.get::<_, Option<String>>(9)?,
        ))
    })?;
    let mut items = Vec::new();
    let mut matched_count = 0;
    let mut conflict_count = 0;
    let mut missing_count = 0;
    for row in rows {
        let (
            item_id,
            order_index,
            question_no,
            question_type,
            question_stem,
            bound_id,
            bound_json,
            candidate_id,
            candidate_json,
            anchor_json,
        ) = row?;
        let match_state = match candidate_json.as_deref() {
            None => {
                missing_count += 1;
                "missing"
            }
            Some(candidate)
                if parse_object(candidate, "答案候选")?
                    == parse_object(&bound_json, "当前答案")? =>
            {
                matched_count += 1;
                "matched"
            }
            Some(_) => {
                conflict_count += 1;
                "conflict"
            }
        };
        items.push(AnswerSourceReviewItem {
            assessment_item_id: item_id,
            order_index,
            question_no,
            question_type,
            question_stem,
            bound_answer_key_version_id: bound_id,
            bound_answer_json: bound_json,
            candidate_id,
            candidate_answer_json: candidate_json,
            source_anchor_json: anchor_json,
            match_state: match_state.into(),
        });
    }
    let source_state = match output.state {
        AnswerSourceState::Ready => "ready",
        AnswerSourceState::NeedsReview => "needs_review",
        AnswerSourceState::Blocked => "blocked",
    };
    let route = match resolution.as_deref() {
        Some("confirmed_matches") => "confirmed",
        Some("kept_bound") => "kept_bound",
        _ if source_state != "ready" || conflict_count > 0 || missing_count > 0 => "blocked",
        _ => "ready_to_confirm",
    };
    Ok(AnswerSourceReviewSummary {
        ingest_batch_id: batch_id,
        source_ai_run_id,
        source_state: source_state.into(),
        route: route.into(),
        matched_count,
        conflict_count,
        missing_count,
        resolution,
        items,
    })
}

fn write_resolution(
    conn: &mut Connection,
    summary: &AnswerSourceReviewSummary,
    decision: &str,
    confirmed_by: &str,
) -> CoreResult<()> {
    if confirmed_by.trim().is_empty() {
        return Err(CoreError::Invalid("答案资料确认人不能为空".into()));
    }
    if let Some(existing) = summary.resolution.as_deref() {
        if existing == decision {
            return Ok(());
        }
        return Err(CoreError::Invalid(
            "该答案资料已经作出另一项不可变决议".into(),
        ));
    }
    if decision == "confirmed_matches"
        && (summary.route != "ready_to_confirm"
            || summary.conflict_count > 0
            || summary.missing_count > 0)
    {
        return Err(CoreError::Invalid("只有全部逐题一致时才能一次确认".into()));
    }
    if !matches!(decision, "confirmed_matches" | "kept_bound") {
        return Err(CoreError::Invalid("答案资料决议非法".into()));
    }
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let summary_json = serde_json::json!({
        "schema_version": 1,
        "source_ai_run_id": summary.source_ai_run_id,
        "decision": decision,
        "matched_count": summary.matched_count,
        "conflict_count": summary.conflict_count,
        "missing_count": summary.missing_count,
    })
    .to_string();
    let summary_hash = hashing::sha256_hex(summary_json.as_bytes());
    let tx = conn.transaction()?;
    if decision == "confirmed_matches" {
        for item in &summary.items {
            let candidate_id = item
                .candidate_id
                .ok_or_else(|| CoreError::Invalid("一致答案缺少机器候选".into()))?;
            let (artifact_id, candidate_json, fingerprint, anchor_json): (i64, String, String, String) =
                tx.query_row(
                    "SELECT source_artifact_id,candidate_answer_json,answer_fingerprint,source_anchor_json
                     FROM exam_answer_authority_candidates_v2
                     WHERE id=?1 AND source_kind='ai_draft' AND state='active'",
                    [candidate_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?;
            tx.execute(
                "INSERT INTO exam_answer_authority_candidates_v2
                 (public_id,ingest_batch_id,assessment_item_id,source_kind,source_priority,
                  answer_key_version_id,source_artifact_id,candidate_answer_json,
                  answer_fingerprint,source_anchor_json,teacher_confirmed,idempotency_key,
                  state,created_by_type,created_by,created_at)
                 VALUES (?1,?2,?3,'uploaded_official',3,?4,?5,?6,?7,?8,1,?9,
                         'active','teacher',?10,?11)",
                params![
                    ids::new_public_id(),
                    summary.ingest_batch_id,
                    item.assessment_item_id,
                    item.bound_answer_key_version_id,
                    artifact_id,
                    candidate_json,
                    fingerprint,
                    anchor_json,
                    format!(
                        "answer-source-run:{}:confirmed-item:{}",
                        summary.source_ai_run_id, item.assessment_item_id
                    ),
                    confirmed_by.trim(),
                    &now,
                ],
            )?;
        }
    }
    tx.execute(
        "UPDATE exam_answer_authority_candidates_v2 SET state='rejected'
         WHERE ingest_batch_id=?1 AND source_kind='ai_draft' AND state='active'
           AND CAST(json_extract(source_anchor_json,'$.source_ai_run_id') AS INTEGER)=?2",
        (summary.ingest_batch_id, summary.source_ai_run_id),
    )?;
    tx.execute(
        "INSERT INTO exam_answer_source_resolutions_v2
         (public_id,ingest_batch_id,source_ai_run_id,decision,summary_hash,summary_json,
          confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        (
            &public_id,
            summary.ingest_batch_id,
            summary.source_ai_run_id,
            decision,
            &summary_hash,
            &summary_json,
            confirmed_by.trim(),
            &now,
        ),
    )?;
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!("exam:answer-source-resolution:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "exam.answer_source.resolved",
            object_type: "exam_answer_source_resolution",
            object_id: &public_id,
            object_revision: None,
            note: None,
            meta_json: Some(&summary_json),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    Ok(())
}

pub fn confirm_matches(
    conn: &mut Connection,
    batch_id: i64,
    source_ai_run_id: i64,
    confirmed_by: &str,
) -> CoreResult<AnswerSourceReviewSummary> {
    let summary = review_summary(conn, batch_id, source_ai_run_id)?;
    write_resolution(conn, &summary, "confirmed_matches", confirmed_by)?;
    review_summary(conn, batch_id, source_ai_run_id)
}

pub fn keep_bound_answers(
    conn: &mut Connection,
    batch_id: i64,
    source_ai_run_id: i64,
    confirmed_by: &str,
) -> CoreResult<AnswerSourceReviewSummary> {
    let summary = review_summary(conn, batch_id, source_ai_run_id)?;
    write_resolution(conn, &summary, "kept_bound", confirmed_by)?;
    review_summary(conn, batch_id, source_ai_run_id)
}

pub fn latest_preflight_gate(
    conn: &Connection,
    batch_id: i64,
) -> CoreResult<(AnswerSourcePreflightGate, Option<&'static str>)> {
    let has_source: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_fixed_input_documents_v2
         WHERE ingest_batch_id=?1 AND document_role='answer_source' AND state<>'voided')",
        [batch_id],
        |row| row.get(0),
    )?;
    if !has_source {
        return Ok((AnswerSourcePreflightGate::Ready, None));
    }
    let (item_count, confirmed_uploaded_count): (i64, i64) = conn.query_row(
        "SELECT COUNT(DISTINCT i.id),
                COUNT(DISTINCT CASE WHEN c.teacher_confirmed=1 AND c.state='active' THEN i.id END)
         FROM exam_ingest_batches_v2 b
         JOIN exam_assessment_items_v2 i
           ON i.assessment_version_id=b.assessment_version_id AND i.state='active'
         LEFT JOIN exam_answer_authority_candidates_v2 c
           ON c.ingest_batch_id=b.id AND c.assessment_item_id=i.id
          AND c.source_kind='uploaded_official'
         WHERE b.id=?1",
        [batch_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    if item_count > 0 && confirmed_uploaded_count == item_count {
        return Ok((AnswerSourcePreflightGate::Ready, None));
    }
    let latest: Option<(i64, String)> = conn
        .query_row(
            "SELECT id,status FROM ai_runs
             WHERE run_type='answer_source_structure'
               AND business_ref_type='fixed_answer_source' AND business_ref_id=?1
             ORDER BY id DESC LIMIT 1",
            [batch_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((run_id, status)) = latest else {
        return Ok((
            AnswerSourcePreflightGate::Blocked,
            Some("ANSWER_SOURCE_STRUCTURE_PENDING"),
        ));
    };
    if status != "succeeded" {
        return Ok((
            AnswerSourcePreflightGate::Blocked,
            Some("ANSWER_SOURCE_STRUCTURE_FAILED"),
        ));
    }
    let summary = review_summary(conn, batch_id, run_id)?;
    match summary.route.as_str() {
        "confirmed" | "kept_bound" => Ok((AnswerSourcePreflightGate::Ready, None)),
        "ready_to_confirm" => Ok((
            AnswerSourcePreflightGate::ReviewRequired,
            Some("ANSWER_SOURCE_CONFIRMATION_REQUIRED"),
        )),
        _ => Ok((
            AnswerSourcePreflightGate::Blocked,
            Some("ANSWER_SOURCE_CONFLICT_OR_MISSING"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::answer_source_recognition::{AnswerSourceEntry, AnswerSourceRecognizerDescriptor};
    use crate::service::fixed_paper::{register_fixed_input_document, NewFixedInputDocument};
    use suite_core::db::repo::artifacts;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

    struct Fixture {
        conn: Connection,
        batch_id: i64,
        run_id: i64,
    }

    fn fixture(candidate_correct: bool) -> Fixture {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        let hash = "a".repeat(64);
        conn.execute_batch(&format!(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('answer-source-edition',1,'PEP','2024','中国历史八上','8','upper',
                         'active','2026-07-15T10:00:00Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('answer-source-map',1,1,'confirmed','2026-07-15T10:00:00Z',
                         '2026-07-15T10:00:00Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('answer-source-q','personal','teacher','unknown',0,'2026-07-15T10:00:00Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,
                  quality_level,state,created_at)
                 VALUES ('answer-source-qv',1,1,'true_false','鸦片战争爆发于1840年。',1,
                         '{hash}','L2','published','2026-07-15T10:00:00Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-source-ak',1,1,'{{"schema_version":1,"correct":true}}',
                         'confirmed','2026-07-15T10:00:00Z','teacher','2026-07-15T10:00:00Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-source-r',1,1,1,'confirmed','2026-07-15T10:00:00Z',
                         'teacher','2026-07-15T10:00:00Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                  confirmed_by,confirmed_at)
                 VALUES ('answer-source-links',1,1,1,'confirmed','2026-07-15T10:00:00Z',
                         'teacher','2026-07-15T10:00:00Z');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,
                  created_by,created_at,updated_at)
                 VALUES ('answer-source-a','答案资料作业',1,'quiz','include','active','teacher',
                         '2026-07-15T10:00:00Z','2026-07-15T10:00:00Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,
                  created_at,confirmed_by,confirmed_at)
                 VALUES ('answer-source-av',1,1,'{hash}','answer-source-template','confirmed',
                         '2026-07-15T10:00:00Z','teacher','2026-07-15T10:00:00Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                  rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,state,created_at)
                 VALUES ('answer-source-item',1,1,1,1,1,0,1,
                         '{{"schema_version":1,"question_no":"1"}}','active',
                         '2026-07-15T10:00:00Z');
               INSERT INTO exam_ingest_batches_v2
                 (public_id,assessment_version_id,source_kind,idempotency_key,state,
                  created_by,created_at,updated_at)
                 VALUES ('answer-source-batch',1,'image_folder','answer-source-batch','processing',
                         'teacher','2026-07-15T10:00:00Z','2026-07-15T10:00:00Z');"#
        ))
        .unwrap();
        let artifact = artifacts::create_or_get(
            &conn,
            &artifacts::NewArtifact {
                kind: ArtifactKind::Document,
                sha256: &"b".repeat(64),
                mime_type: "text/plain",
                byte_size: 4,
                original_name: Some("answer.txt"),
                original_path: None,
                archived_path: "/tmp/answer-source.txt",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "answer-source-test-v1",
                privacy_class: PrivacyClass::TeachingContent,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        register_fixed_input_document(
            &conn,
            &NewFixedInputDocument {
                ingest_batch_id: 1,
                source_artifact_id: artifact.id,
                document_role: "answer_source",
                source_format: "text",
                import_index: 0,
                page_count: 1,
                idempotency_key: "answer-source-doc",
                created_by: "teacher",
            },
        )
        .unwrap();
        let descriptor = AnswerSourceRecognizerDescriptor {
            provider: "fixture".into(),
            model_name: "fixture".into(),
            model_version: "v1".into(),
            config_version: "v1".into(),
            rule_version: "v1".into(),
        };
        let output = AnswerSourceRecognitionOutput {
            schema_version: 1,
            ingest_batch_id: 1,
            source_artifact_id: artifact.id,
            source_artifact_sha256: artifact.sha256.clone(),
            input_hash: "c".repeat(64),
            descriptor,
            state: AnswerSourceState::Ready,
            entries: vec![AnswerSourceEntry {
                assessment_item_id: 1,
                answer_json: serde_json::json!({"schema_version":1,"correct":candidate_correct}),
                source_anchor: serde_json::json!({"schema_version":1,"line":1}),
                confidence: 0.99,
            }],
            confidence: 0.99,
            issue_codes: vec![],
        };
        let run = ai_runs::create_or_get(
            &conn,
            &ai_runs::NewAiRun {
                idempotency_key: "answer-source-run",
                run_type: "answer_source_structure",
                source_module: "exam",
                business_ref_type: "fixed_answer_source",
                business_ref_id: "1",
                input_artifact_id: Some(artifact.id),
                provider: "fixture",
                model_name: "fixture",
                model_version: "v1",
                config_version: "v1",
                prompt_or_rule_version: "v1",
                input_hash: &output.input_hash,
                retry_of_ai_run_id: None,
            },
        )
        .unwrap();
        ai_runs::start(&conn, run.id, "2026-07-15T10:00:01Z", None).unwrap();
        let output_json = serde_json::to_string(&output).unwrap();
        ai_runs::finalize_succeeded(
            &conn,
            run.id,
            &hashing::sha256_hex(output_json.as_bytes()),
            Some(0.99),
            &output_json,
            "2026-07-15T10:00:02Z",
        )
        .unwrap();
        materialize_ai_drafts(&conn, &output, run.id).unwrap();
        Fixture {
            conn,
            batch_id: 1,
            run_id: run.id,
        }
    }

    #[test]
    fn matching_source_requires_one_teacher_confirmation_then_reuses_bound_k1() {
        let mut fixture = fixture(true);
        let summary = review_summary(&fixture.conn, fixture.batch_id, fixture.run_id).unwrap();
        assert_eq!(summary.route, "ready_to_confirm");
        assert_eq!(
            latest_preflight_gate(&fixture.conn, fixture.batch_id)
                .unwrap()
                .0,
            AnswerSourcePreflightGate::ReviewRequired
        );
        let confirmed = confirm_matches(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
        )
        .unwrap();
        assert_eq!(confirmed.route, "confirmed");
        let official: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_answer_authority_candidates_v2
                 WHERE source_kind='uploaded_official' AND teacher_confirmed=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(official, 1);
        assert_eq!(
            latest_preflight_gate(&fixture.conn, fixture.batch_id)
                .unwrap()
                .0,
            AnswerSourcePreflightGate::Ready
        );
        let effects: (i64, i64) = fixture
            .conn
            .query_row(
                "SELECT (SELECT COUNT(*) FROM exam_grade_decisions_v2),
                        (SELECT COUNT(*) FROM exam_grade_publications_v2)",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(effects, (0, 0));
    }

    #[test]
    fn conflict_stays_blocked_until_teacher_explicitly_keeps_bound_answer() {
        let mut fixture = fixture(false);
        let summary = review_summary(&fixture.conn, fixture.batch_id, fixture.run_id).unwrap();
        assert_eq!(summary.route, "blocked");
        assert_eq!(summary.conflict_count, 1);
        assert!(confirm_matches(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher"
        )
        .is_err());
        let resolved = keep_bound_answers(
            &mut fixture.conn,
            fixture.batch_id,
            fixture.run_id,
            "teacher",
        )
        .unwrap();
        assert_eq!(resolved.route, "kept_bound");
        assert_eq!(
            latest_preflight_gate(&fixture.conn, fixture.batch_id)
                .unwrap()
                .0,
            AnswerSourcePreflightGate::Ready
        );
    }
}
