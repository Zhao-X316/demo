//! M2.5 批改题目自动沉淀。
//!
//! 批改证据始终留在 M2；可复用题目只进入老师私有候选库。精确匹配只能复用当前
//! assessment item 已固定的 K1 版本，其他匹配一律进入待确认，不静默改题或改答案。

use chrono::{Duration, Utc};
use module_knowledge::db::content::{self, NewQuestion, NewQuestionOption, NewQuestionVersion};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::{artifacts, audit, outbox};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{ArchiveStatus, AuditActorType, PrivacyClass};

use super::papers::trace_answer_region;

const EVENT_TYPE: &str = "exam.question_ingest.requested";
const CONSUMER: &str = "m2_5_question_library";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOptionDraft {
    pub label: String,
    pub content: String,
    pub order_index: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionDraft {
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub max_score: f64,
    pub options: Vec<QuestionOptionDraft>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacyScan {
    pub schema_version: i64,
    pub sanitized: bool,
    pub student_identity_detected: bool,
    pub student_answer_detected: bool,
    pub teacher_mark_detected: bool,
    pub score_detected: bool,
}

pub struct NewQuestionIngest<'a> {
    pub answer_region_revision_id: i64,
    pub owner_id: &'a str,
    pub source_type: &'a str,
    pub extraction_version: &'a str,
    pub draft: &'a QuestionDraft,
    pub privacy_scan: &'a PrivacyScan,
    /// 仅允许引用独立的干净题面资产；学生答案裁剪永远不能传入这里。
    pub reusable_artifact_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionIngestJob {
    pub id: i64,
    pub public_id: String,
    pub idempotency_key: String,
    pub answer_region_revision_id: i64,
    pub assessment_item_id: i64,
    pub owner_id: String,
    pub source_type: String,
    pub extraction_version: String,
    pub privacy_status: String,
    pub state: String,
    pub attempts: i64,
    pub active_event_id: Option<i64>,
    pub error_meta_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionIngestResult {
    pub job_id: i64,
    pub candidate_id: i64,
    pub candidate_status: String,
    pub question_version_id: Option<i64>,
    pub item_link_id: Option<i64>,
}

#[derive(Debug, Serialize)]
struct NaturalIdentity<'a> {
    schema_version: i64,
    assessment_version_id: i64,
    source_page_hash: &'a str,
    region_crop_hash: &'a str,
    region_revision: i64,
    extraction_version: &'a str,
    owner_id: &'a str,
}

#[derive(Debug, Serialize)]
struct RequestIdentity<'a> {
    schema_version: i64,
    natural_key: &'a str,
    source_type: &'a str,
    draft: &'a QuestionDraft,
    privacy_scan: &'a PrivacyScan,
    reusable_artifact_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredQuestionDraft {
    schema_version: i64,
    question_type: String,
    stem: String,
    material_text: Option<String>,
    max_score: f64,
    options: Vec<QuestionOptionDraft>,
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{field}不能为空")))
    } else {
        Ok(())
    }
}

fn validate_draft(draft: &QuestionDraft) -> CoreResult<()> {
    if !matches!(
        draft.question_type.as_str(),
        "single" | "multiple" | "true_false" | "fill_blank" | "short_answer"
    ) {
        return Err(CoreError::Invalid("题目候选题型非法".into()));
    }
    required(&draft.stem, "候选题干")?;
    if !draft.max_score.is_finite() || draft.max_score <= 0.0 {
        return Err(CoreError::Invalid("候选题分值必须大于 0".into()));
    }
    if matches!(draft.question_type.as_str(), "single" | "multiple") && draft.options.len() < 2 {
        return Err(CoreError::Invalid("选择题候选至少需要两个选项".into()));
    }
    let mut labels = std::collections::HashSet::new();
    for option in &draft.options {
        required(&option.label, "候选选项标签")?;
        required(&option.content, "候选选项内容")?;
        if !labels.insert(option.label.trim().to_ascii_uppercase()) {
            return Err(CoreError::Invalid("候选题选项标签不能重复".into()));
        }
    }
    Ok(())
}

fn privacy_passed(scan: &PrivacyScan) -> bool {
    scan.schema_version == 1
        && scan.sanitized
        && !scan.student_identity_detected
        && !scan.student_answer_detected
        && !scan.teacher_mark_detected
        && !scan.score_detected
}

fn borrowed_options(draft: &QuestionDraft) -> Vec<NewQuestionOption<'_>> {
    draft
        .options
        .iter()
        .map(|option| NewQuestionOption {
            label: &option.label,
            content: &option.content,
            order_index: option.order_index,
        })
        .collect()
}

fn content_hash_for(draft: &QuestionDraft) -> CoreResult<String> {
    let options = borrowed_options(draft);
    content::content_hash(&NewQuestionVersion {
        question_id: 0,
        revision: 1,
        question_type: &draft.question_type,
        stem: &draft.stem,
        material_text: draft.material_text.as_deref(),
        max_score: draft.max_score,
        source_artifact_id: None,
        source_anchor_json: None,
        supersedes_version_id: None,
        quality_level: "C0",
        state: "candidate",
        options: &options,
    })
}

fn job_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<QuestionIngestJob> {
    Ok(QuestionIngestJob {
        id: row.get(0)?,
        public_id: row.get(1)?,
        idempotency_key: row.get(2)?,
        answer_region_revision_id: row.get(3)?,
        assessment_item_id: row.get(4)?,
        owner_id: row.get(5)?,
        source_type: row.get(6)?,
        extraction_version: row.get(7)?,
        privacy_status: row.get(8)?,
        state: row.get(9)?,
        attempts: row.get(10)?,
        active_event_id: row.get(11)?,
        error_meta_json: row.get(12)?,
    })
}

fn get_job(conn: &Connection, id: i64) -> CoreResult<Option<QuestionIngestJob>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, idempotency_key, answer_region_revision_id,
                    assessment_item_id, owner_id, source_type, extraction_version,
                    privacy_status, state, attempts, active_event_id, error_meta_json
             FROM exam_question_ingest_jobs_v2 WHERE id=?1",
            [id],
            job_row,
        )
        .optional()?)
}

fn get_job_by_key(conn: &Connection, key: &str) -> CoreResult<Option<(QuestionIngestJob, String)>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, idempotency_key, answer_region_revision_id,
                    assessment_item_id, owner_id, source_type, extraction_version,
                    privacy_status, state, attempts, active_event_id, error_meta_json,
                    request_hash
             FROM exam_question_ingest_jobs_v2 WHERE idempotency_key=?1",
            [key],
            |row| Ok((job_row(row)?, row.get(13)?)),
        )
        .optional()?)
}

fn append_system_audit(
    conn: &Connection,
    key: &str,
    action: &str,
    job_public_id: &str,
    meta_json: Option<&str>,
    now: &str,
) -> CoreResult<()> {
    audit::append(
        conn,
        &audit::NewAuditEvent {
            idempotency_key: key,
            actor_type: AuditActorType::System,
            actor_id: None,
            action,
            object_type: "exam_question_ingest_job",
            object_id: job_public_id,
            object_revision: None,
            note: None,
            meta_json,
            occurred_at: now,
        },
    )?;
    Ok(())
}

/// 将一次题目提取请求与 outbox 事件原子入库。隐私未通过时只保留脱敏失败事实，
/// 不保存题干、不创建 outbox，也不影响原批改页面继续工作。
pub fn enqueue_question_ingest(
    conn: &Connection,
    input: &NewQuestionIngest<'_>,
) -> CoreResult<QuestionIngestJob> {
    required(input.owner_id, "题库所有者")?;
    required(input.extraction_version, "提取规则版本")?;
    if !matches!(
        input.source_type,
        "blank_paper" | "source_document" | "student_paper" | "workbook"
    ) {
        return Err(CoreError::Invalid("候选题来源类型非法".into()));
    }
    validate_draft(input.draft)?;

    let trace = trace_answer_region(conn, input.answer_region_revision_id)?.ok_or_else(|| {
        CoreError::NotFound(format!("answer_region#{}", input.answer_region_revision_id))
    })?;
    if trace.region_decision != "teacher_confirmed" {
        return Err(CoreError::Invalid(
            "只有老师确认且可追溯的题区才能沉淀题目".into(),
        ));
    }
    let source_page = artifacts::get_by_id(conn, trace.source_page_artifact_id)?
        .ok_or_else(|| CoreError::NotFound("题区来源页面 artifact".into()))?;
    let crop = artifacts::get_by_id(conn, trace.crop_artifact_id)?
        .ok_or_else(|| CoreError::NotFound("题区裁剪 artifact".into()))?;

    let natural_bytes = serde_json::to_vec(&NaturalIdentity {
        schema_version: 1,
        assessment_version_id: trace.assessment_version_id,
        source_page_hash: &source_page.sha256,
        region_crop_hash: &crop.sha256,
        region_revision: trace.region_revision,
        extraction_version: input.extraction_version.trim(),
        owner_id: input.owner_id.trim(),
    })
    .map_err(|error| CoreError::Parse(format!("题目沉淀幂等键序列化失败：{error}")))?;
    let natural_hash = hashing::sha256_hex(&natural_bytes);
    let idempotency_key = format!("m2.5:question-ingest:{natural_hash}");
    let request_bytes = serde_json::to_vec(&RequestIdentity {
        schema_version: 1,
        natural_key: &idempotency_key,
        source_type: input.source_type,
        draft: input.draft,
        privacy_scan: input.privacy_scan,
        reusable_artifact_id: input.reusable_artifact_id,
    })
    .map_err(|error| CoreError::Parse(format!("题目沉淀请求序列化失败：{error}")))?;
    let request_hash = hashing::sha256_hex(&request_bytes);
    if let Some((existing, existing_hash)) = get_job_by_key(conn, &idempotency_key)? {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid(
                "同一题区和提取版本已对应不同内容，请提升 extraction_version 后重试".into(),
            ));
        }
        return Ok(existing);
    }

    let mut privacy_rejection = None;
    if !privacy_passed(input.privacy_scan) {
        privacy_rejection = Some("PRIVACY_SCAN_REJECTED");
    }
    if input.source_type == "student_paper" && input.reusable_artifact_id.is_some() {
        privacy_rejection = Some("STUDENT_ASSET_REUSE_FORBIDDEN");
    }
    let reusable_artifact = if let Some(artifact_id) = input.reusable_artifact_id {
        let artifact = artifacts::get_by_id(conn, artifact_id)?
            .ok_or_else(|| CoreError::NotFound(format!("artifact#{artifact_id}")))?;
        if artifact.archive_status != ArchiveStatus::Ready
            || !matches!(
                artifact.privacy_class,
                PrivacyClass::TeachingContent | PrivacyClass::PublicSafe
            )
        {
            privacy_rejection = Some("REUSABLE_ASSET_NOT_PRIVACY_SAFE");
        }
        Some(artifact)
    } else {
        None
    };

    let public_id = ids::new_public_id();
    let now = time::utc_now_rfc3339();
    if let Some(code) = privacy_rejection {
        let error_meta = serde_json::json!({
            "schema_version": 1,
            "error_code": code,
            "retryable": false
        })
        .to_string();
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO exam_question_ingest_jobs_v2
             (public_id,idempotency_key,request_hash,answer_region_revision_id,
              assessment_item_id,owner_id,source_type,extraction_version,
              privacy_status,state,error_meta_json,created_at,updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'rejected','failed',?9,?10,?10)",
            params![
                &public_id,
                &idempotency_key,
                &request_hash,
                input.answer_region_revision_id,
                trace.assessment_item_id,
                input.owner_id.trim(),
                input.source_type,
                input.extraction_version.trim(),
                &error_meta,
                &now,
            ],
        )?;
        let id = tx.last_insert_rowid();
        append_system_audit(
            &tx,
            &format!("exam:question-ingest:{public_id}:privacy-rejected"),
            "exam.question_ingest.privacy_rejected",
            &public_id,
            Some(&error_meta),
            &now,
        )?;
        tx.commit()?;
        return get_job(conn, id)?.ok_or_else(|| CoreError::NotFound("隐私拒绝任务".into()));
    }

    let privacy_status = if reusable_artifact.is_some() {
        "reusable_asset"
    } else {
        "text_only"
    };
    let content_hash = content_hash_for(input.draft)?;
    let stored = StoredQuestionDraft {
        schema_version: 1,
        question_type: input.draft.question_type.trim().to_owned(),
        stem: input.draft.stem.trim().to_owned(),
        material_text: input
            .draft
            .material_text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned),
        max_score: input.draft.max_score,
        options: input.draft.options.clone(),
    };
    let payload = serde_json::to_string(&stored)
        .map_err(|error| CoreError::Parse(format!("候选题 payload 序列化失败：{error}")))?;
    let event_payload = serde_json::json!({
        "schema_version": 1,
        "job_public_id": public_id
    })
    .to_string();

    let tx = conn.unchecked_transaction()?;
    let event = outbox::create_event(
        &tx,
        &outbox::NewOutboxEvent {
            idempotency_key: &format!("{idempotency_key}:attempt:1"),
            event_type: EVENT_TYPE,
            event_version: 1,
            aggregate_type: "exam_question_ingest_job",
            aggregate_id: &public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &now,
        },
    )?;
    tx.execute(
        "INSERT INTO exam_question_ingest_jobs_v2
         (public_id,idempotency_key,request_hash,answer_region_revision_id,
          assessment_item_id,owner_id,source_type,extraction_version,
          structured_payload_json,content_hash,privacy_status,reusable_artifact_id,
          state,active_event_id,created_at,updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'pending',?13,?14,?14)",
        params![
            &public_id,
            &idempotency_key,
            &request_hash,
            input.answer_region_revision_id,
            trace.assessment_item_id,
            input.owner_id.trim(),
            input.source_type,
            input.extraction_version.trim(),
            &payload,
            &content_hash,
            privacy_status,
            input.reusable_artifact_id,
            event.id,
            &now,
        ],
    )?;
    let id = tx.last_insert_rowid();
    append_system_audit(
        &tx,
        &format!("exam:question-ingest:{public_id}:queued:1"),
        "exam.question_ingest.queued",
        &public_id,
        Some(r#"{"schema_version":1,"visibility":"personal_candidate"}"#),
        &now,
    )?;
    tx.commit()?;
    get_job(conn, id)?.ok_or_else(|| CoreError::NotFound("刚创建的题目沉淀任务".into()))
}

fn options_json(draft: &StoredQuestionDraft) -> String {
    serde_json::json!({
        "schema_version": 1,
        "options": draft.options
    })
    .to_string()
}

struct CandidateInsert<'a> {
    exact_match_id: Option<i64>,
    created_version_id: Option<i64>,
    status: &'a str,
    quality_issues_json: &'a str,
    reusable_artifact_id: Option<i64>,
}

fn insert_candidate(
    conn: &Connection,
    job: &QuestionIngestJob,
    draft: &StoredQuestionDraft,
    content_hash: &str,
    input: &CandidateInsert<'_>,
    now: &str,
) -> CoreResult<i64> {
    conn.execute(
        "INSERT INTO exam_question_candidates_v2
         (public_id,job_id,owner_id,answer_region_revision_id,assessment_item_id,
          source_type,question_type,normalized_stem,material_text,max_score,
          options_json,content_hash,privacy_status,reusable_artifact_id,
          exact_match_question_version_id,created_question_version_id,status,
          quality_issues_json,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19)",
        params![
            ids::new_public_id(),
            job.id,
            &job.owner_id,
            job.answer_region_revision_id,
            job.assessment_item_id,
            &job.source_type,
            &draft.question_type,
            &draft.stem,
            &draft.material_text,
            draft.max_score,
            options_json(draft),
            content_hash,
            &job.privacy_status,
            input.reusable_artifact_id,
            input.exact_match_id,
            input.created_version_id,
            input.status,
            input.quality_issues_json,
            now,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

fn process_claimed(
    conn: &Connection,
    event_id: i64,
    job_public_id: &str,
    lease_token: &str,
    fail_after_question_create: bool,
) -> CoreResult<QuestionIngestResult> {
    let tx = conn.unchecked_transaction()?;
    let row: Option<(QuestionIngestJob, String, String, Option<i64>, i64)> = tx
        .query_row(
            "SELECT id, public_id, idempotency_key, answer_region_revision_id,
                    assessment_item_id, owner_id, source_type, extraction_version,
                    privacy_status, state, attempts, active_event_id, error_meta_json,
                    structured_payload_json, content_hash, reusable_artifact_id,
                    (SELECT question_version_id FROM exam_assessment_items_v2
                     WHERE id=exam_question_ingest_jobs_v2.assessment_item_id)
             FROM exam_question_ingest_jobs_v2
             WHERE public_id=?1 AND active_event_id=?2",
            params![job_public_id, event_id],
            |row| {
                Ok((
                    job_row(row)?,
                    row.get(13)?,
                    row.get(14)?,
                    row.get(15)?,
                    row.get(16)?,
                ))
            },
        )
        .optional()?;
    let Some((job, payload, expected_hash, reusable_artifact_id, item_question_version_id)) = row
    else {
        return Err(CoreError::NotFound(format!(
            "question_ingest_job#{job_public_id}"
        )));
    };
    if !matches!(job.state.as_str(), "pending" | "processing") {
        return Err(CoreError::Invalid(format!(
            "题目沉淀任务状态 {} 不可处理",
            job.state
        )));
    }
    let draft: StoredQuestionDraft = serde_json::from_str(&payload)
        .map_err(|error| CoreError::Parse(format!("候选题 payload 无效：{error}")))?;
    let public_draft = QuestionDraft {
        question_type: draft.question_type.clone(),
        stem: draft.stem.clone(),
        material_text: draft.material_text.clone(),
        max_score: draft.max_score,
        options: draft.options.clone(),
    };
    validate_draft(&public_draft)?;
    if content_hash_for(&public_draft)? != expected_hash {
        return Err(CoreError::Invalid(
            "候选题内容 hash 与入队快照不一致".into(),
        ));
    }
    let now = time::utc_now_rfc3339();
    tx.execute(
        "UPDATE exam_question_ingest_jobs_v2
         SET state='processing', attempts=attempts+1, error_meta_json=NULL, updated_at=?1
         WHERE id=?2",
        params![&now, job.id],
    )?;

    let matches = content::find_exact_versions_for_owner(
        &tx,
        &expected_hash,
        &draft.question_type,
        "personal",
        &job.owner_id,
    )?;
    let mut item_link_id = None;
    let (candidate_id, status, question_version_id) = if matches.is_empty() {
        let question = content::create_question(
            &tx,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: &job.owner_id,
                question_family_id: None,
                rights_status: "unknown",
                sharing_allowed: false,
            },
        )?;
        let options = draft
            .options
            .iter()
            .map(|option| NewQuestionOption {
                label: &option.label,
                content: &option.content,
                order_index: option.order_index,
            })
            .collect::<Vec<_>>();
        let source_anchor = serde_json::json!({
            "schema_version": 1,
            "source": "m2_question_candidate",
            "job_public_id": job.public_id
        })
        .to_string();
        let version = content::create_question_version_in_transaction(
            &tx,
            &NewQuestionVersion {
                question_id: question.id,
                revision: 1,
                question_type: &draft.question_type,
                stem: &draft.stem,
                material_text: draft.material_text.as_deref(),
                max_score: draft.max_score,
                source_artifact_id: reusable_artifact_id,
                source_anchor_json: Some(&source_anchor),
                supersedes_version_id: None,
                quality_level: "C0",
                state: "candidate",
                options: &options,
            },
        )?;
        if fail_after_question_create {
            return Err(CoreError::Db(
                "injected candidate persistence failure".into(),
            ));
        }
        let candidate_id = insert_candidate(
            &tx,
            &job,
            &draft,
            &expected_hash,
            &CandidateInsert {
                exact_match_id: None,
                created_version_id: Some(version.id),
                status: "candidate_created",
                quality_issues_json: r#"{"schema_version":1,"issues":[]}"#,
                reusable_artifact_id,
            },
            &now,
        )?;
        (candidate_id, "candidate_created", Some(version.id))
    } else if matches.len() == 1 && matches[0].id == item_question_version_id {
        let version_id = matches[0].id;
        let active_link: Option<(i64, i64)> = tx
            .query_row(
                "SELECT id,question_version_id
                 FROM exam_assessment_item_question_links_v2
                 WHERE assessment_item_id=?1 AND state='active'",
                [job.assessment_item_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if active_link.is_some_and(|(_, active_version_id)| active_version_id != version_id) {
            let issues = r#"{"schema_version":1,"issues":["ACTIVE_TEACHER_LINK_CONFLICT"]}"#;
            let candidate_id = insert_candidate(
                &tx,
                &job,
                &draft,
                &expected_hash,
                &CandidateInsert {
                    exact_match_id: None,
                    created_version_id: None,
                    status: "needs_review",
                    quality_issues_json: issues,
                    reusable_artifact_id,
                },
                &now,
            )?;
            tx.execute(
                "INSERT INTO exam_duplicate_candidates_v2
                 (public_id,candidate_id,target_question_version_id,match_kind,
                  score,reason_code,state,created_at)
                 VALUES (?1,?2,?3,'exact',1.0,'ACTIVE_TEACHER_LINK_CONFLICT','open',?4)",
                params![ids::new_public_id(), candidate_id, version_id, &now],
            )?;
            (candidate_id, "needs_review", None)
        } else {
            let candidate_id = insert_candidate(
                &tx,
                &job,
                &draft,
                &expected_hash,
                &CandidateInsert {
                    exact_match_id: Some(version_id),
                    created_version_id: None,
                    status: "matched",
                    quality_issues_json: r#"{"schema_version":1,"issues":[]}"#,
                    reusable_artifact_id,
                },
                &now,
            )?;
            if let Some((link_id, _)) = active_link {
                item_link_id = Some(link_id);
            } else {
                let revision: i64 = tx.query_row(
                    "SELECT COALESCE(MAX(revision),0)+1
                     FROM exam_assessment_item_question_links_v2 WHERE assessment_item_id=?1",
                    [job.assessment_item_id],
                    |row| row.get(0),
                )?;
                tx.execute(
                    "INSERT INTO exam_assessment_item_question_links_v2
                     (public_id,assessment_item_id,revision,question_version_id,candidate_id,
                      link_source,state,created_at)
                     VALUES (?1,?2,?3,?4,?5,'exact_auto','active',?6)",
                    params![
                        ids::new_public_id(),
                        job.assessment_item_id,
                        revision,
                        version_id,
                        candidate_id,
                        &now
                    ],
                )?;
                item_link_id = Some(tx.last_insert_rowid());
            }
            (candidate_id, "matched", Some(version_id))
        }
    } else {
        let quality_issues = serde_json::json!({
            "schema_version": 1,
            "issues": ["EXACT_MATCH_REQUIRES_TEACHER_SELECTION"]
        })
        .to_string();
        let candidate_id = insert_candidate(
            &tx,
            &job,
            &draft,
            &expected_hash,
            &CandidateInsert {
                exact_match_id: None,
                created_version_id: None,
                status: "needs_review",
                quality_issues_json: &quality_issues,
                reusable_artifact_id,
            },
            &now,
        )?;
        for version in &matches {
            tx.execute(
                "INSERT INTO exam_duplicate_candidates_v2
                 (public_id,candidate_id,target_question_version_id,match_kind,
                  score,reason_code,state,created_at)
                 VALUES (?1,?2,?3,'exact',1.0,'EXACT_CONTENT_DIFFERENT_BINDING','open',?4)",
                params![ids::new_public_id(), candidate_id, version.id, &now],
            )?;
        }
        (candidate_id, "needs_review", None)
    };

    tx.execute(
        "UPDATE exam_question_ingest_jobs_v2
         SET state='completed', error_meta_json=NULL, updated_at=?1 WHERE id=?2",
        params![&now, job.id],
    )?;
    outbox::finalize_succeeded(&tx, event_id, CONSUMER, lease_token, &now)?;
    append_system_audit(
        &tx,
        &format!("exam:question-ingest:{}:completed", job.public_id),
        "exam.question_ingest.completed",
        &job.public_id,
        Some(
            &serde_json::json!({
                "schema_version": 1,
                "candidate_status": status
            })
            .to_string(),
        ),
        &now,
    )?;
    tx.commit()?;
    Ok(QuestionIngestResult {
        job_id: job.id,
        candidate_id,
        candidate_status: status.into(),
        question_version_id,
        item_link_id,
    })
}

fn mark_claim_failed(
    conn: &Connection,
    event_id: i64,
    job_public_id: &str,
    lease_token: &str,
) -> CoreResult<()> {
    let error_meta =
        r#"{"schema_version":1,"error_code":"QUESTION_INGEST_PROCESSING_FAILED","retryable":true}"#;
    let now = time::utc_now_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE exam_question_ingest_jobs_v2
         SET state='failed', attempts=attempts+1, error_meta_json=?1, updated_at=?2
         WHERE public_id=?3 AND active_event_id=?4",
        params![error_meta, &now, job_public_id, event_id],
    )?;
    outbox::finalize_failed(&tx, event_id, CONSUMER, lease_token, error_meta, &now)?;
    append_system_audit(
        &tx,
        &format!("exam:question-ingest:{job_public_id}:failed:{event_id}"),
        "exam.question_ingest.failed",
        job_public_id,
        Some(error_meta),
        &now,
    )?;
    tx.commit()?;
    Ok(())
}

fn process_next_internal(
    conn: &Connection,
    fail_after_question_create: bool,
) -> CoreResult<Option<QuestionIngestResult>> {
    outbox::ensure_pending_for_consumer(conn, CONSUMER, EVENT_TYPE, 1)?;
    let now = time::utc_now_rfc3339();
    let lease_expires = suite_core::domain::time::utc_rfc3339(Utc::now() + Duration::minutes(5));
    let lease_token = ids::new_public_id();
    let Some((event, _)) = outbox::claim_next(conn, CONSUMER, &now, &lease_token, &lease_expires)?
    else {
        return Ok(None);
    };
    let payload_result = serde_json::from_str::<Value>(&event.payload_json)
        .map_err(|error| CoreError::Parse(format!("题目沉淀 outbox payload 无效：{error}")))
        .and_then(|payload| {
            let job_public_id = payload
                .get("job_public_id")
                .and_then(Value::as_str)
                .ok_or_else(|| CoreError::Parse("题目沉淀 outbox 缺少 job_public_id".into()))?;
            if job_public_id != event.aggregate_id {
                return Err(CoreError::Invalid(
                    "题目沉淀 outbox aggregate 与 payload 不一致".into(),
                ));
            }
            Ok(job_public_id.to_owned())
        });
    let job_public_id = match payload_result {
        Ok(value) => value,
        Err(error) => {
            mark_claim_failed(conn, event.id, &event.aggregate_id, &lease_token)?;
            return Err(error);
        }
    };
    match process_claimed(
        conn,
        event.id,
        &job_public_id,
        &lease_token,
        fail_after_question_create,
    ) {
        Ok(result) => Ok(Some(result)),
        Err(error) => {
            mark_claim_failed(conn, event.id, &job_public_id, &lease_token)?;
            Err(error)
        }
    }
}

/// 消费一个待沉淀题目。无待办时返回 `None`。
pub fn process_next_question_ingest(conn: &Connection) -> CoreResult<Option<QuestionIngestResult>> {
    process_next_internal(conn, false)
}

/// 失败重试追加新的 outbox event，不复用已失败消费行。
pub fn retry_question_ingest(conn: &Connection, job_id: i64) -> CoreResult<QuestionIngestJob> {
    let job = get_job(conn, job_id)?
        .ok_or_else(|| CoreError::NotFound(format!("question_ingest_job#{job_id}")))?;
    if job.state != "failed" || job.privacy_status == "rejected" {
        return Err(CoreError::Invalid(
            "只有可重试的处理失败任务才能重新入队".into(),
        ));
    }
    let payload_exists: bool = conn.query_row(
        "SELECT structured_payload_json IS NOT NULL FROM exam_question_ingest_jobs_v2 WHERE id=?1",
        [job_id],
        |row| row.get(0),
    )?;
    if !payload_exists {
        return Err(CoreError::Invalid(
            "失败任务没有可恢复的结构化 payload".into(),
        ));
    }
    let attempt = job.attempts + 1;
    let now = time::utc_now_rfc3339();
    let event_payload = serde_json::json!({
        "schema_version": 1,
        "job_public_id": job.public_id
    })
    .to_string();
    let tx = conn.unchecked_transaction()?;
    let event = outbox::create_event(
        &tx,
        &outbox::NewOutboxEvent {
            idempotency_key: &format!("{}:attempt:{attempt}", job.idempotency_key),
            event_type: EVENT_TYPE,
            event_version: 1,
            aggregate_type: "exam_question_ingest_job",
            aggregate_id: &job.public_id,
            aggregate_revision: attempt,
            payload_json: &event_payload,
            occurred_at: &now,
        },
    )?;
    tx.execute(
        "UPDATE exam_question_ingest_jobs_v2
         SET state='pending', active_event_id=?1, error_meta_json=NULL, updated_at=?2
         WHERE id=?3 AND state='failed'",
        params![event.id, &now, job_id],
    )?;
    append_system_audit(
        &tx,
        &format!("exam:question-ingest:{}:queued:{attempt}", job.public_id),
        "exam.question_ingest.retried",
        &job.public_id,
        Some(r#"{"schema_version":1,"retryable":true}"#),
        &now,
    )?;
    tx.commit()?;
    get_job(conn, job_id)?.ok_or_else(|| CoreError::NotFound(format!("job#{job_id}")))
}

/// 启动恢复：处理中断只标记失败并保留 payload，等待老师或系统显式重试。
pub fn recover_interrupted_question_ingest(conn: &Connection) -> CoreResult<usize> {
    let mut stmt = conn.prepare(
        "SELECT j.id, j.public_id, j.active_event_id
         FROM exam_question_ingest_jobs_v2 j
         JOIN outbox_consumptions c ON c.event_id=j.active_event_id
         WHERE j.state IN ('pending','processing')
           AND c.consumer='m2_5_question_library' AND c.status='processing'
         ORDER BY j.id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    })?;
    let jobs = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    if jobs.is_empty() {
        return Ok(0);
    }
    let now = time::utc_now_rfc3339();
    let error_meta =
        r#"{"schema_version":1,"error_code":"INTERRUPTED_PROCESSING","retryable":true}"#;
    let tx = conn.unchecked_transaction()?;
    for (id, public_id, event_id) in &jobs {
        if let Some(event_id) = event_id {
            if let Some(consumption) = outbox::get_consumption(&tx, *event_id, CONSUMER)? {
                if let Some(lease_token) = consumption.lease_token.as_deref() {
                    outbox::finalize_failed(
                        &tx,
                        *event_id,
                        CONSUMER,
                        lease_token,
                        error_meta,
                        &now,
                    )?;
                }
            }
        }
        tx.execute(
            "UPDATE exam_question_ingest_jobs_v2
             SET state='failed', attempts=attempts+1, error_meta_json=?1, updated_at=?2
             WHERE id=?3",
            params![error_meta, &now, id],
        )?;
        append_system_audit(
            &tx,
            &format!("exam:question-ingest:{public_id}:recovered"),
            "exam.question_ingest.recovered",
            public_id,
            Some(error_meta),
            &now,
        )?;
    }
    tx.commit()?;
    Ok(jobs.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::repo::artifacts::{create_or_get, NewArtifact};
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
    use suite_core::models::ArtifactKind;

    struct Fixture {
        conn: Connection,
        region_id: i64,
        current_question_version_id: i64,
        reusable_artifact_id: i64,
    }

    fn exact_draft() -> QuestionDraft {
        QuestionDraft {
            question_type: "true_false".into(),
            stem: "鸦片战争爆发于1840年。".into(),
            material_text: None,
            max_score: 1.0,
            options: Vec::new(),
        }
    }

    fn new_draft() -> QuestionDraft {
        QuestionDraft {
            question_type: "single".into(),
            stem: "洋务运动后期提出的口号是？".into(),
            material_text: None,
            max_score: 2.0,
            options: vec![
                QuestionOptionDraft {
                    label: "A".into(),
                    content: "自强".into(),
                    order_index: 0,
                },
                QuestionOptionDraft {
                    label: "B".into(),
                    content: "求富".into(),
                    order_index: 1,
                },
            ],
        }
    }

    fn safe_scan() -> PrivacyScan {
        PrivacyScan {
            schema_version: 1,
            sanitized: true,
            student_identity_detected: false,
            student_answer_detected: false,
            teacher_mark_detected: false,
            score_detected: false,
        }
    }

    fn setup() -> Fixture {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        conn.execute_batch(
            "INSERT INTO subjects(name) VALUES ('历史');
             INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
             INSERT INTO students(student_no,name,class_id) VALUES ('S001','小林',1);
             INSERT INTO k1_textbook_editions
               (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
               VALUES ('edition-a',1,'PEP','2024','中国历史八上','8','upper','active',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO k1_knowledge_maps
               (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
               VALUES ('map-a',1,1,'confirmed','2026-07-13T08:00:00.000Z',
                       '2026-07-13T08:00:00.000Z');",
        )
        .unwrap();

        let question = content::create_question(
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
        let exact = exact_draft();
        let no_options: Vec<NewQuestionOption<'_>> = Vec::new();
        let version = content::create_question_version(
            &conn,
            &NewQuestionVersion {
                question_id: question.id,
                revision: 1,
                question_type: &exact.question_type,
                stem: &exact.stem,
                material_text: None,
                max_score: exact.max_score,
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "C0",
                state: "candidate",
                options: &no_options,
            },
        )
        .unwrap();
        conn.execute_batch(&format!(
            "INSERT INTO k1_answer_key_versions
               (public_id,question_version_id,revision,answer_json,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('answer-a',{},1,'{{\"schema_version\":1,\"correct\":true}}','confirmed',
                       '2026-07-13T08:00:00.000Z','teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO k1_rubric_versions
               (public_id,question_version_id,revision,max_score,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('rubric-a',{},1,1,'confirmed','2026-07-13T08:00:00.000Z',
                       'teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO k1_link_sets
               (public_id,question_version_id,knowledge_map_id,revision,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('link-a',{},1,1,'confirmed','2026-07-13T08:00:00.000Z',
                       'teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_assessments_v2
               (public_id,title,class_id,assessment_context,evidence_policy,state,
                created_by,created_at,updated_at)
               VALUES ('assessment-a','随堂测',1,'quiz','include','active','teacher',
                       '2026-07-13T08:00:00.000Z','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_assessment_versions_v2
               (public_id,assessment_id,revision,item_set_hash,state,created_at,
                confirmed_by,confirmed_at)
               VALUES ('assessment-version-a',1,1,'{}','confirmed',
                       '2026-07-13T08:00:00.000Z','teacher','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_assessment_items_v2
               (public_id,assessment_version_id,question_version_id,answer_key_version_id,
                rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,
                state,created_at)
               VALUES ('item-a',1,{},1,1,1,0,1,
                       '{{\"schema_version\":1,\"question_no\":\"1\"}}','active',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO exam_attempts_v2
               (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                attempt_kind,state,created_at,updated_at)
               VALUES ('attempt-a',1,1,1,'image','first','grading',
                       '2026-07-13T08:00:00.000Z','2026-07-13T08:00:00.000Z');",
            version.id,
            version.id,
            version.id,
            "a".repeat(64),
            version.id,
        ))
        .unwrap();

        let page = create_or_get(
            &conn,
            &NewArtifact {
                kind: ArtifactKind::Page,
                sha256: &"1".repeat(64),
                mime_type: "image/png",
                byte_size: 100,
                original_name: Some("student-page.png"),
                original_path: Some("/fixture/student-page.png"),
                archived_path: "/archive/student-page.png",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "fixture-v1",
                privacy_class: PrivacyClass::StudentSensitive,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        let aligned = create_or_get(
            &conn,
            &NewArtifact {
                kind: ArtifactKind::Page,
                sha256: &"2".repeat(64),
                mime_type: "image/png",
                byte_size: 90,
                original_name: None,
                original_path: None,
                archived_path: "/archive/aligned.png",
                parent_artifact_id: Some(page.id),
                derivative_type: Some("aligned"),
                processing_version: "fixture-v1",
                privacy_class: PrivacyClass::StudentSensitive,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        let crop = create_or_get(
            &conn,
            &NewArtifact {
                kind: ArtifactKind::Crop,
                sha256: &"3".repeat(64),
                mime_type: "image/png",
                byte_size: 40,
                original_name: None,
                original_path: None,
                archived_path: "/archive/crop.png",
                parent_artifact_id: Some(aligned.id),
                derivative_type: Some("answer_region"),
                processing_version: "fixture-v1",
                privacy_class: PrivacyClass::StudentSensitive,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        let reusable = create_or_get(
            &conn,
            &NewArtifact {
                kind: ArtifactKind::Crop,
                sha256: &"4".repeat(64),
                mime_type: "image/png",
                byte_size: 30,
                original_name: Some("clean-question.png"),
                original_path: Some("/fixture/clean-question.png"),
                archived_path: "/archive/clean-question.png",
                parent_artifact_id: None,
                derivative_type: None,
                processing_version: "fixture-v1",
                privacy_class: PrivacyClass::TeachingContent,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();
        conn.execute_batch(&format!(
            "INSERT INTO exam_ingest_batches_v2
               (public_id,assessment_version_id,source_kind,idempotency_key,state,
                created_by,created_at,updated_at)
               VALUES ('batch-a',1,'fixed_fixture','batch-a','ready','teacher',
                       '2026-07-13T08:00:00.000Z','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_ingest_pages_v2
               (public_id,batch_id,source_artifact_id,import_index,expected_page_no,state,
                created_at,updated_at)
               VALUES ('page-a',1,{},0,1,'segmented','2026-07-13T08:00:00.000Z',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO exam_page_match_revisions_v2
               (public_id,page_id,revision,attempt_id,page_no,student_confidence,
                page_no_confidence,template_confidence,decision,confirmed_by,state,created_at)
               VALUES ('match-a',1,1,1,1,1,1,1,'teacher_confirmed','teacher','active',
                       '2026-07-13T08:00:00.000Z');
             INSERT INTO exam_page_alignment_revisions_v2
               (public_id,page_id,revision,match_revision_id,template_version,transform_json,
                confidence,aligned_artifact_id,decision,confirmed_by,state,created_at)
               VALUES ('alignment-a',1,1,1,'fixture-v1',
                       '{{\"schema_version\":1,\"matrix\":[1,0,0,0,1,0,0,0,1]}}',
                       1,{},'teacher_confirmed','teacher','active','2026-07-13T08:00:00.000Z');
             INSERT INTO exam_answer_region_revisions_v2
               (public_id,page_id,assessment_item_id,region_index,revision,
                alignment_revision_id,bbox_json,crop_artifact_id,mapping_confidence,
                decision,confirmed_by,state,created_at)
               VALUES ('region-a',1,1,0,1,1,
                       '{{\"schema_version\":1,\"x\":0,\"y\":0,\"width\":1,\"height\":1}}',
                       {},1,'teacher_confirmed','teacher','active','2026-07-13T08:00:00.000Z');",
            page.id, aligned.id, crop.id
        ))
        .unwrap();
        Fixture {
            conn,
            region_id: 1,
            current_question_version_id: version.id,
            reusable_artifact_id: reusable.id,
        }
    }

    fn enqueue<'a>(
        fixture: &Fixture,
        draft: &'a QuestionDraft,
        scan: &'a PrivacyScan,
        source_type: &'a str,
        reusable_artifact_id: Option<i64>,
    ) -> CoreResult<QuestionIngestJob> {
        enqueue_question_ingest(
            &fixture.conn,
            &NewQuestionIngest {
                answer_region_revision_id: fixture.region_id,
                owner_id: "teacher",
                source_type,
                extraction_version: "extract-v1",
                draft,
                privacy_scan: scan,
                reusable_artifact_id,
            },
        )
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    #[test]
    fn exact_current_version_is_reused_without_duplicate_question() {
        let fixture = setup();
        let draft = exact_draft();
        let job = enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
        assert_eq!(job.state, "pending");
        assert_eq!(job.privacy_status, "text_only");
        let before = count(&fixture.conn, "k1_questions");
        let result = process_next_question_ingest(&fixture.conn)
            .unwrap()
            .unwrap();
        assert_eq!(result.candidate_status, "matched");
        assert_eq!(
            result.question_version_id,
            Some(fixture.current_question_version_id)
        );
        assert!(result.item_link_id.is_some());
        assert_eq!(count(&fixture.conn, "k1_questions"), before);
        let source_artifact: Option<i64> = fixture
            .conn
            .query_row(
                "SELECT reusable_artifact_id FROM exam_question_candidates_v2 WHERE id=?1",
                [result.candidate_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_artifact, None);
    }

    #[test]
    fn new_student_paper_question_creates_private_text_only_c0_once() {
        let fixture = setup();
        let draft = new_draft();
        let first = enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
        let same = enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
        assert_eq!(first.id, same.id);
        let result = process_next_question_ingest(&fixture.conn)
            .unwrap()
            .unwrap();
        assert_eq!(result.candidate_status, "candidate_created");
        let version_id = result.question_version_id.unwrap();
        let row: (String, String, bool, String, String, Option<i64>) = fixture
            .conn
            .query_row(
                "SELECT q.owner_scope,q.owner_id,q.sharing_allowed,v.quality_level,v.state,
                        v.source_artifact_id
                 FROM k1_question_versions v JOIN k1_questions q ON q.id=v.question_id
                 WHERE v.id=?1",
                [version_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                "personal".into(),
                "teacher".into(),
                false,
                "C0".into(),
                "candidate".into(),
                None
            )
        );
        assert!(process_next_question_ingest(&fixture.conn)
            .unwrap()
            .is_none());
        assert_eq!(count(&fixture.conn, "exam_question_candidates_v2"), 1);
    }

    #[test]
    fn privacy_rejection_persists_no_content_or_outbox_and_keeps_grading_state() {
        let fixture = setup();
        let mut scan = safe_scan();
        scan.student_answer_detected = true;
        scan.sanitized = false;
        let job = enqueue(
            &fixture,
            &new_draft(),
            &scan,
            "student_paper",
            Some(fixture.reusable_artifact_id),
        )
        .unwrap();
        assert_eq!(job.state, "failed");
        assert_eq!(job.privacy_status, "rejected");
        let stored: (Option<String>, Option<String>, Option<i64>) = fixture
            .conn
            .query_row(
                "SELECT structured_payload_json,content_hash,active_event_id
                 FROM exam_question_ingest_jobs_v2 WHERE id=?1",
                [job.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(stored, (None, None, None));
        assert_eq!(count(&fixture.conn, "outbox_events"), 0);
        assert_eq!(count(&fixture.conn, "exam_question_candidates_v2"), 0);
        let region_state: (String, String) = fixture
            .conn
            .query_row(
                "SELECT decision,state FROM exam_answer_region_revisions_v2 WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(region_state, ("teacher_confirmed".into(), "active".into()));
    }

    #[test]
    fn outbox_and_job_enqueue_are_atomic() {
        let fixture = setup();
        fixture
            .conn
            .execute_batch(
                "CREATE TRIGGER fail_question_job BEFORE INSERT ON exam_question_ingest_jobs_v2
                 BEGIN SELECT RAISE(ABORT, 'injected job failure'); END;",
            )
            .unwrap();
        assert!(enqueue(
            &fixture,
            &new_draft(),
            &safe_scan(),
            "source_document",
            Some(fixture.reusable_artifact_id)
        )
        .is_err());
        assert_eq!(count(&fixture.conn, "outbox_events"), 0);
        assert_eq!(count(&fixture.conn, "exam_question_ingest_jobs_v2"), 0);
    }

    #[test]
    fn processing_failure_rolls_back_k1_and_retry_finishes_once() {
        let fixture = setup();
        let draft = new_draft();
        let job = enqueue(
            &fixture,
            &draft,
            &safe_scan(),
            "source_document",
            Some(fixture.reusable_artifact_id),
        )
        .unwrap();
        let questions_before = count(&fixture.conn, "k1_questions");
        assert!(process_next_internal(&fixture.conn, true).is_err());
        assert_eq!(count(&fixture.conn, "k1_questions"), questions_before);
        assert_eq!(count(&fixture.conn, "exam_question_candidates_v2"), 0);
        let failed = get_job(&fixture.conn, job.id).unwrap().unwrap();
        assert_eq!(failed.state, "failed");
        assert_eq!(failed.attempts, 1);
        retry_question_ingest(&fixture.conn, job.id).unwrap();
        let result = process_next_question_ingest(&fixture.conn)
            .unwrap()
            .unwrap();
        assert_eq!(result.candidate_status, "candidate_created");
        assert_eq!(count(&fixture.conn, "k1_questions"), questions_before + 1);
        assert_eq!(count(&fixture.conn, "exam_question_candidates_v2"), 1);
        let completed = get_job(&fixture.conn, job.id).unwrap().unwrap();
        assert_eq!(completed.state, "completed");
        assert_eq!(completed.attempts, 2);
        let source_artifact_id: Option<i64> = fixture
            .conn
            .query_row(
                "SELECT source_artifact_id FROM k1_question_versions WHERE id=?1",
                [result.question_version_id.unwrap()],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source_artifact_id, Some(fixture.reusable_artifact_id));
    }

    #[test]
    fn ambiguous_exact_matches_require_teacher_selection() {
        let fixture = setup();
        let second = content::create_question(
            &fixture.conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: "teacher",
                question_family_id: None,
                rights_status: "unknown",
                sharing_allowed: false,
            },
        )
        .unwrap();
        let draft = exact_draft();
        content::create_question_version(
            &fixture.conn,
            &NewQuestionVersion {
                question_id: second.id,
                revision: 1,
                question_type: &draft.question_type,
                stem: &draft.stem,
                material_text: None,
                max_score: draft.max_score,
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "C0",
                state: "candidate",
                options: &[],
            },
        )
        .unwrap();
        enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
        let result = process_next_question_ingest(&fixture.conn)
            .unwrap()
            .unwrap();
        assert_eq!(result.candidate_status, "needs_review");
        assert_eq!(result.question_version_id, None);
        assert_eq!(result.item_link_id, None);
        assert_eq!(count(&fixture.conn, "exam_duplicate_candidates_v2"), 2);
        assert_eq!(
            count(&fixture.conn, "exam_assessment_item_question_links_v2"),
            0
        );
    }

    #[test]
    fn exact_auto_never_overwrites_an_active_teacher_link() {
        let fixture = setup();
        let other_question = content::create_question(
            &fixture.conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: "teacher",
                question_family_id: None,
                rights_status: "unknown",
                sharing_allowed: false,
            },
        )
        .unwrap();
        let other_draft = new_draft();
        let options = borrowed_options(&other_draft);
        let other_version = content::create_question_version(
            &fixture.conn,
            &NewQuestionVersion {
                question_id: other_question.id,
                revision: 1,
                question_type: &other_draft.question_type,
                stem: &other_draft.stem,
                material_text: None,
                max_score: other_draft.max_score,
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "C0",
                state: "candidate",
                options: &options,
            },
        )
        .unwrap();
        assert!(fixture
            .conn
            .execute(
                "INSERT INTO exam_assessment_item_question_links_v2
                 (public_id,assessment_item_id,revision,question_version_id,link_source,
                  state,created_at)
                 VALUES ('illegal-auto-link',1,1,?1,'exact_auto','active',
                         '2026-07-13T08:00:00.000Z')",
                [other_version.id],
            )
            .is_err());
        fixture
            .conn
            .execute(
                "INSERT INTO exam_assessment_item_question_links_v2
                 (public_id,assessment_item_id,revision,question_version_id,link_source,
                  state,linked_by,created_at)
                 VALUES ('teacher-link',1,1,?1,'teacher_selected','active','teacher',
                         '2026-07-13T08:00:00.000Z')",
                [other_version.id],
            )
            .unwrap();
        let draft = exact_draft();
        enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
        let result = process_next_question_ingest(&fixture.conn)
            .unwrap()
            .unwrap();
        assert_eq!(result.candidate_status, "needs_review");
        assert_eq!(result.item_link_id, None);
        let active: (i64, String) = fixture
            .conn
            .query_row(
                "SELECT question_version_id,state
                 FROM exam_assessment_item_question_links_v2 WHERE public_id='teacher-link'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(active, (other_version.id, "active".into()));
    }

    #[test]
    fn claimed_but_interrupted_event_is_failed_and_can_be_retried() {
        let fixture = setup();
        let draft = new_draft();
        let job = enqueue(&fixture, &draft, &safe_scan(), "student_paper", None).unwrap();
        outbox::ensure_pending_for_consumer(&fixture.conn, CONSUMER, EVENT_TYPE, 1).unwrap();
        let (_, consumption) = outbox::claim_next(
            &fixture.conn,
            CONSUMER,
            "2026-07-13T08:00:00.000Z",
            "interrupted-lease",
            "2026-07-13T08:05:00.000Z",
        )
        .unwrap()
        .unwrap();
        assert_eq!(consumption.event_id, job.active_event_id.unwrap());
        assert_eq!(
            recover_interrupted_question_ingest(&fixture.conn).unwrap(),
            1
        );
        assert_eq!(
            get_job(&fixture.conn, job.id).unwrap().unwrap().state,
            "failed"
        );
        retry_question_ingest(&fixture.conn, job.id).unwrap();
        assert_eq!(
            process_next_question_ingest(&fixture.conn)
                .unwrap()
                .unwrap()
                .candidate_status,
            "candidate_created"
        );
    }
}
