//! M2.5-2 私有候选整理。
//!
//! 只消费 `question_ingest` 已创建且通过隐私门禁的个人 C0 候选。老师核对后：
//! - 以同一题目身份创建新的不可变 L0 版本；
//! - 创建老师确认的答案版本；
//! - 经质量事件提升为 L1；
//! - 记录 M2 assessment item 与新 K1 版本的候选晋级关系。
//!
//! 原候选、原 C0 版本和当前作业固定版本均不覆盖。L2/L3 仍需答案槽位/rubric、
//! 知识与能力链接等后续老师确认。

use std::collections::HashSet;

use module_knowledge::db::content::{self, NewQuestionOption, NewQuestionVersion};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

pub const CANDIDATE_REVIEW_SCHEMA_VERSION: i64 = 1;
pub const CANDIDATE_REVIEW_RULE_VERSION: &str = "m2.5-candidate-review-v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateOption {
    pub label: String,
    pub content: String,
    pub order_index: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateReviewItem {
    pub candidate_public_id: String,
    pub source_type: String,
    pub privacy_status: String,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub max_score: f64,
    pub options: Vec<CandidateOption>,
    pub content_hash: String,
    pub quality_issues: Vec<String>,
    pub source_question_version_public_id: String,
    pub source_quality_level: String,
    pub created_at: String,
    pub review_action: Option<String>,
    pub result_question_version_public_id: Option<String>,
    pub reviewed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateReviewInbox {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub pending_count: i64,
    pub items: Vec<CandidateReviewItem>,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateReviewOptionInput {
    pub label: String,
    pub content: String,
    pub order_index: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteCandidateRequest {
    pub request_key: String,
    pub candidate_public_id: String,
    pub expected_content_hash: String,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub max_score: f64,
    pub options: Vec<CandidateReviewOptionInput>,
    pub answer_text: String,
    pub note: Option<String>,
    pub reviewed_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardCandidateRequest {
    pub request_key: String,
    pub candidate_public_id: String,
    pub expected_content_hash: String,
    pub note: Option<String>,
    pub reviewed_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateReviewDecision {
    pub public_id: String,
    pub candidate_public_id: String,
    pub action: String,
    pub source_question_version_public_id: String,
    pub result_question_version_public_id: Option<String>,
    pub result_answer_key_version_public_id: Option<String>,
    pub result_quality_level: Option<String>,
    pub reviewed_by: String,
    pub reviewed_at: String,
    pub current_assessment_rebound: bool,
    pub boundary_note: String,
}

#[derive(Debug)]
struct CandidateRow {
    id: i64,
    public_id: String,
    _owner_id: String,
    assessment_item_id: i64,
    source_type: String,
    question_type: String,
    stem: String,
    material_text: Option<String>,
    max_score: f64,
    options_json: String,
    content_hash: String,
    privacy_status: String,
    quality_issues_json: String,
    created_version_id: i64,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct StoredOptions {
    schema_version: i64,
    options: Vec<CandidateOption>,
}

#[derive(Debug, Deserialize)]
struct StoredIssues {
    schema_version: i64,
    issues: Vec<String>,
}

#[derive(Serialize)]
struct PromoteHashInput<'a> {
    schema_version: i64,
    candidate_public_id: &'a str,
    expected_content_hash: &'a str,
    question_type: &'a str,
    stem: &'a str,
    material_text: Option<&'a str>,
    max_score_millis: i64,
    options: &'a [CandidateReviewOptionInput],
    answer_text: &'a str,
    note: Option<&'a str>,
    reviewed_by: &'a str,
}

#[derive(Serialize)]
struct DiscardHashInput<'a> {
    schema_version: i64,
    candidate_public_id: &'a str,
    expected_content_hash: &'a str,
    note: Option<&'a str>,
    reviewed_by: &'a str,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn parse_options(value: &str) -> CoreResult<Vec<CandidateOption>> {
    let parsed: StoredOptions = serde_json::from_str(value)
        .map_err(|error| CoreError::Parse(format!("候选选项 JSON 无效：{error}")))?;
    if parsed.schema_version != 1 {
        return Err(CoreError::Invalid("候选选项 schema_version 不支持".into()));
    }
    Ok(parsed.options)
}

fn parse_issues(value: &str) -> CoreResult<Vec<String>> {
    let parsed: StoredIssues = serde_json::from_str(value)
        .map_err(|error| CoreError::Parse(format!("候选问题 JSON 无效：{error}")))?;
    if parsed.schema_version != 1 {
        return Err(CoreError::Invalid("候选问题 schema_version 不支持".into()));
    }
    Ok(parsed.issues)
}

fn candidate_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CandidateRow> {
    Ok(CandidateRow {
        id: row.get(0)?,
        public_id: row.get(1)?,
        _owner_id: row.get(2)?,
        assessment_item_id: row.get(3)?,
        source_type: row.get(4)?,
        question_type: row.get(5)?,
        stem: row.get(6)?,
        material_text: row.get(7)?,
        max_score: row.get(8)?,
        options_json: row.get(9)?,
        content_hash: row.get(10)?,
        privacy_status: row.get(11)?,
        quality_issues_json: row.get(12)?,
        created_version_id: row.get(13)?,
        created_at: row.get(14)?,
    })
}

fn load_candidate(conn: &Connection, owner_id: &str, public_id: &str) -> CoreResult<CandidateRow> {
    conn.query_row(
        "SELECT id,public_id,owner_id,assessment_item_id,source_type,question_type,
                normalized_stem,material_text,max_score,options_json,content_hash,
                privacy_status,quality_issues_json,created_question_version_id,created_at
         FROM exam_question_candidates_v2
         WHERE public_id=?1 AND owner_id=?2 AND status='candidate_created'
           AND privacy_status IN ('text_only','reusable_asset')
           AND created_question_version_id IS NOT NULL",
        (public_id, owner_id.trim()),
        candidate_row,
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound("待整理私有候选".into()))
}

fn decision_from_row(conn: &Connection, id: i64) -> CoreResult<CandidateReviewDecision> {
    conn.query_row(
        "SELECT review.public_id,candidate.public_id,review.action,
                source.public_id,result.public_id,answer.public_id,
                result.quality_level,review.reviewed_by,review.reviewed_at
         FROM exam_question_candidate_reviews_v2 review
         JOIN exam_question_candidates_v2 candidate ON candidate.id=review.candidate_id
         JOIN k1_question_versions source ON source.id=review.source_question_version_id
         LEFT JOIN k1_question_versions result ON result.id=review.result_question_version_id
         LEFT JOIN k1_answer_key_versions answer ON answer.id=review.result_answer_key_version_id
         WHERE review.id=?1",
        [id],
        |row| {
            Ok(CandidateReviewDecision {
                public_id: row.get(0)?,
                candidate_public_id: row.get(1)?,
                action: row.get(2)?,
                source_question_version_public_id: row.get(3)?,
                result_question_version_public_id: row.get(4)?,
                result_answer_key_version_public_id: row.get(5)?,
                result_quality_level: row.get(6)?,
                reviewed_by: row.get(7)?,
                reviewed_at: row.get(8)?,
                current_assessment_rebound: false,
                boundary_note: "只更新个人题库候选；当前作业固定题目、成绩和学习证据均未切换。"
                    .into(),
            })
        },
    )
    .map_err(Into::into)
}

fn existing_by_request(conn: &Connection, request_key: &str) -> CoreResult<Option<(i64, String)>> {
    Ok(conn
        .query_row(
            "SELECT id,request_hash FROM exam_question_candidate_reviews_v2
             WHERE request_key=?1",
            [request_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

fn ensure_not_reviewed(conn: &Connection, candidate_id: i64) -> CoreResult<()> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_question_candidate_reviews_v2 WHERE candidate_id=?1)",
        [candidate_id],
        |row| row.get(0),
    )?;
    if exists {
        return Err(CoreError::Invalid(
            "该候选已完成老师整理；如需纠正请基于已发布版本创建新的编辑 revision".into(),
        ));
    }
    Ok(())
}

fn ensure_personal_c0_source(
    conn: &Connection,
    owner_id: &str,
    source: &content::QuestionVersion,
) -> CoreResult<()> {
    if source.quality_level != "C0" || source.state != "candidate" {
        return Err(CoreError::Invalid(
            "只有未整理的 C0 候选可以进入本入口".into(),
        ));
    }
    let owned: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM k1_questions
           WHERE id=?1 AND owner_scope='personal' AND owner_id=?2
         )",
        params![source.question_id, owner_id.trim()],
        |row| row.get(0),
    )?;
    if !owned {
        return Err(CoreError::Invalid("只能整理当前老师的个人题库候选".into()));
    }
    Ok(())
}

fn ensure_current_source(conn: &Connection, source: &content::QuestionVersion) -> CoreResult<()> {
    let current_version_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM k1_question_versions
             WHERE question_id=?1 ORDER BY revision DESC,id DESC LIMIT 1",
            [source.question_id],
            |row| row.get(0),
        )
        .optional()?;
    if current_version_id != Some(source.id) {
        return Err(CoreError::Invalid(
            "候选来源已被新的题目版本取代，请刷新后按当前版本处理".into(),
        ));
    }
    Ok(())
}

fn normalize_type_and_answer(
    question_type: &str,
    answer_text: &str,
    options: &[CandidateReviewOptionInput],
) -> CoreResult<String> {
    required(answer_text, "标准答案")?;
    match question_type {
        "single" | "multiple" => {
            let labels: Vec<String> = answer_text
                .split([',', '，', '、', ' '])
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_ascii_uppercase)
                .collect();
            if labels.is_empty() || (question_type == "single" && labels.len() != 1) {
                return Err(CoreError::Invalid(
                    "单选题填写一个选项字母，多选题填写一个或多个选项字母".into(),
                ));
            }
            let unique_labels: HashSet<&String> = labels.iter().collect();
            if unique_labels.len() != labels.len() {
                return Err(CoreError::Invalid("标准答案选项不能重复".into()));
            }
            let allowed: HashSet<String> = options
                .iter()
                .map(|option| option.label.trim().to_ascii_uppercase())
                .collect();
            if labels.iter().any(|label| !allowed.contains(label)) {
                return Err(CoreError::Invalid("标准答案包含不存在的选项".into()));
            }
            Ok(serde_json::json!({
                "schema_version": 1,
                "answer_kind": "option_labels",
                "correct_labels": labels
            })
            .to_string())
        }
        "true_false" => {
            let normalized = answer_text.trim().to_ascii_lowercase();
            let value = match normalized.as_str() {
                "对" | "正确" | "true" | "t" | "√" => true,
                "错" | "错误" | "false" | "f" | "×" | "x" => false,
                _ => return Err(CoreError::Invalid("判断题答案请填写“对”或“错”".into())),
            };
            Ok(serde_json::json!({
                "schema_version": 1,
                "answer_kind": "true_false",
                "correct": value
            })
            .to_string())
        }
        "fill_blank" | "short_answer" => Ok(serde_json::json!({
            "schema_version": 1,
            "answer_kind": "teacher_reference_text",
            "text": answer_text.trim()
        })
        .to_string()),
        _ => Err(CoreError::Invalid("候选题型非法".into())),
    }
}

fn validate_review_content<'a>(
    request: &'a PromoteCandidateRequest,
) -> CoreResult<Vec<NewQuestionOption<'a>>> {
    required(&request.request_key, "请求键")?;
    required(&request.candidate_public_id, "候选 ID")?;
    required(&request.reviewed_by, "确认老师")?;
    required(&request.stem, "题干")?;
    if !request.max_score.is_finite() || request.max_score <= 0.0 {
        return Err(CoreError::Invalid("题目分值必须大于 0".into()));
    }
    if !matches!(
        request.question_type.as_str(),
        "single" | "multiple" | "true_false" | "fill_blank" | "short_answer"
    ) {
        return Err(CoreError::Invalid("候选题型非法".into()));
    }
    if matches!(request.question_type.as_str(), "single" | "multiple") && request.options.len() < 2
    {
        return Err(CoreError::Invalid("选择题至少需要两个选项".into()));
    }
    if !matches!(request.question_type.as_str(), "single" | "multiple")
        && !request.options.is_empty()
    {
        return Err(CoreError::Invalid("非选择题不能保留选择题选项".into()));
    }
    let mut labels = HashSet::new();
    for option in &request.options {
        required(&option.label, "选项标签")?;
        required(&option.content, "选项内容")?;
        if !labels.insert(option.label.trim().to_ascii_uppercase()) {
            return Err(CoreError::Invalid("选项标签不能重复".into()));
        }
    }
    Ok(request
        .options
        .iter()
        .map(|option| NewQuestionOption {
            label: &option.label,
            content: &option.content,
            order_index: option.order_index,
        })
        .collect())
}

pub fn list_candidate_review_inbox(
    conn: &Connection,
    owner_id: &str,
    include_reviewed: bool,
    limit: i64,
) -> CoreResult<CandidateReviewInbox> {
    required(owner_id, "题库所有者")?;
    if !(1..=200).contains(&limit) {
        return Err(CoreError::Invalid("候选列表 limit 必须在 1～200".into()));
    }
    let mut stmt = conn.prepare(
        "SELECT candidate.id,candidate.public_id,candidate.owner_id,
                candidate.assessment_item_id,candidate.source_type,candidate.question_type,
                candidate.normalized_stem,candidate.material_text,candidate.max_score,
                candidate.options_json,candidate.content_hash,candidate.privacy_status,
                candidate.quality_issues_json,candidate.created_question_version_id,
                candidate.created_at,source.public_id,source.quality_level,
                review.action,result.public_id,review.reviewed_at
         FROM exam_question_candidates_v2 candidate
         JOIN k1_question_versions source ON source.id=candidate.created_question_version_id
         JOIN k1_questions question ON question.id=source.question_id
         LEFT JOIN exam_question_candidate_reviews_v2 review ON review.candidate_id=candidate.id
         LEFT JOIN k1_question_versions result ON result.id=review.result_question_version_id
         WHERE candidate.owner_id=?1 AND candidate.status='candidate_created'
           AND candidate.created_question_version_id IS NOT NULL
           AND candidate.privacy_status IN ('text_only','reusable_asset')
           AND question.owner_scope='personal' AND question.owner_id=?1
           AND source.quality_level='C0' AND source.state='candidate'
           AND (?2=1 OR review.id IS NULL)
         ORDER BY CASE WHEN review.id IS NULL THEN 0 ELSE 1 END,candidate.id DESC
         LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![owner_id.trim(), include_reviewed, limit], |row| {
        Ok((
            candidate_row(row)?,
            row.get::<_, String>(15)?,
            row.get::<_, String>(16)?,
            row.get::<_, Option<String>>(17)?,
            row.get::<_, Option<String>>(18)?,
            row.get::<_, Option<String>>(19)?,
        ))
    })?;
    let mut items = Vec::new();
    for row in rows {
        let (candidate, source_public_id, source_quality, action, result_public_id, reviewed_at) =
            row?;
        items.push(CandidateReviewItem {
            candidate_public_id: candidate.public_id,
            source_type: candidate.source_type,
            privacy_status: candidate.privacy_status,
            question_type: candidate.question_type,
            stem: candidate.stem,
            material_text: candidate.material_text,
            max_score: candidate.max_score,
            options: parse_options(&candidate.options_json)?,
            content_hash: candidate.content_hash,
            quality_issues: parse_issues(&candidate.quality_issues_json)?,
            source_question_version_public_id: source_public_id,
            source_quality_level: source_quality,
            created_at: candidate.created_at,
            review_action: action,
            result_question_version_public_id: result_public_id,
            reviewed_at,
        });
    }
    let pending_count: i64 = conn.query_row(
        "SELECT COUNT(*)
         FROM exam_question_candidates_v2 candidate
         JOIN k1_question_versions source ON source.id=candidate.created_question_version_id
         JOIN k1_questions question ON question.id=source.question_id
         LEFT JOIN exam_question_candidate_reviews_v2 review ON review.candidate_id=candidate.id
         WHERE candidate.owner_id=?1 AND candidate.status='candidate_created'
           AND candidate.created_question_version_id IS NOT NULL
           AND candidate.privacy_status IN ('text_only','reusable_asset')
           AND question.owner_scope='personal' AND question.owner_id=?1
           AND source.quality_level='C0' AND source.state='candidate'
           AND review.id IS NULL",
        [owner_id.trim()],
        |row| row.get(0),
    )?;
    Ok(CandidateReviewInbox {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        rule_version: CANDIDATE_REVIEW_RULE_VERSION.into(),
        calculated_at: time::utc_now_rfc3339(),
        pending_count,
        items,
        boundary_note: "这里只整理已通过隐私门禁的老师私有 C0 候选；确认到 L1 不会切换当前作业、重算成绩或进入学习图谱。".into(),
    })
}

pub fn promote_candidate_to_l1(
    conn: &mut Connection,
    owner_id: &str,
    request: &PromoteCandidateRequest,
) -> CoreResult<CandidateReviewDecision> {
    let options = validate_review_content(request)?;
    if request.reviewed_by.trim() != owner_id.trim() {
        return Err(CoreError::Invalid("只能由当前题库老师确认候选".into()));
    }
    let answer_json = normalize_type_and_answer(
        &request.question_type,
        &request.answer_text,
        &request.options,
    )?;
    let request_bytes = serde_json::to_vec(&PromoteHashInput {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        candidate_public_id: request.candidate_public_id.trim(),
        expected_content_hash: request.expected_content_hash.trim(),
        question_type: request.question_type.trim(),
        stem: request.stem.trim(),
        material_text: request
            .material_text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        max_score_millis: (request.max_score * 1000.0).round() as i64,
        options: &request.options,
        answer_text: request.answer_text.trim(),
        note: request
            .note
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        reviewed_by: request.reviewed_by.trim(),
    })
    .map_err(|error| CoreError::Parse(format!("候选确认请求序列化失败：{error}")))?;
    let request_hash = hashing::sha256_hex(&request_bytes);
    if let Some((id, existing_hash)) = existing_by_request(conn, request.request_key.trim())? {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid("同一请求键对应不同候选确认内容".into()));
        }
        return decision_from_row(conn, id);
    }

    let candidate = load_candidate(conn, owner_id, request.candidate_public_id.trim())?;
    if candidate.content_hash != request.expected_content_hash {
        return Err(CoreError::Invalid("候选内容已变化，请刷新后重试".into()));
    }
    ensure_not_reviewed(conn, candidate.id)?;
    let source = content::get_question_version(conn, candidate.created_version_id)?
        .ok_or_else(|| CoreError::NotFound("候选来源题目版本".into()))?;
    ensure_personal_c0_source(conn, owner_id, &source)?;
    ensure_current_source(conn, &source)?;
    let next_revision: i64 = conn.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_question_versions WHERE question_id=?1",
        [source.question_id],
        |row| row.get(0),
    )?;
    let source_anchor = serde_json::json!({
        "schema_version": 1,
        "source": "m2_question_candidate_teacher_review",
        "candidate_public_id": &candidate.public_id,
        "source_question_version_public_id": &source.public_id
    })
    .to_string();
    let version_input = NewQuestionVersion {
        question_id: source.question_id,
        revision: next_revision,
        question_type: request.question_type.trim(),
        stem: request.stem.trim(),
        material_text: request
            .material_text
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        max_score: request.max_score,
        source_artifact_id: source.source_artifact_id,
        source_anchor_json: Some(&source_anchor),
        supersedes_version_id: Some(source.id),
        quality_level: "L0",
        state: "draft",
        options: &options,
    };
    let new_hash = content::content_hash(&version_input)?;
    let exact = content::find_exact_versions_for_owner(
        conn,
        &new_hash,
        request.question_type.trim(),
        "personal",
        owner_id,
    )?;
    if exact
        .iter()
        .any(|version| version.question_id != source.question_id)
    {
        return Err(CoreError::Invalid(
            "老师修正后的内容与另一道个人题完全一致，请先在“找题与查重”中确认复用关系".into(),
        ));
    }

    let now = time::utc_now_rfc3339();
    let review_public_id = ids::new_public_id();
    let tx = conn.transaction()?;
    let version = content::create_question_version_in_transaction(&tx, &version_input)?;
    let answer_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO k1_answer_key_versions
         (public_id,question_version_id,revision,answer_json,state,created_at,
          confirmed_by,confirmed_at)
         VALUES (?1,?2,1,?3,'confirmed',?4,?5,?4)",
        params![
            &answer_public_id,
            version.id,
            &answer_json,
            &now,
            request.reviewed_by.trim()
        ],
    )?;
    let answer_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO k1_question_quality_events
         (public_id,question_version_id,revision,from_quality,to_quality,
          from_state,to_state,reason,verified_by,verified_at,created_at)
         VALUES (?1,?2,1,'L0','L1','draft','published',?3,?4,?5,?5)",
        params![
            ids::new_public_id(),
            version.id,
            request
                .note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .unwrap_or("老师核对题干与标准答案"),
            request.reviewed_by.trim(),
            &now
        ],
    )?;
    tx.execute(
        "UPDATE k1_question_versions SET quality_level='L1',state='published' WHERE id=?1",
        [version.id],
    )?;
    let active_link_exists: bool = tx.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM exam_assessment_item_question_links_v2
           WHERE assessment_item_id=?1 AND state='active'
         )",
        [candidate.assessment_item_id],
        |row| row.get(0),
    )?;
    if active_link_exists {
        return Err(CoreError::Invalid(
            "当前作业题目已有题库绑定；本入口不会静默覆盖，请先处理绑定冲突".into(),
        ));
    }
    let link_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1
         FROM exam_assessment_item_question_links_v2 WHERE assessment_item_id=?1",
        [candidate.assessment_item_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO exam_assessment_item_question_links_v2
         (public_id,assessment_item_id,revision,question_version_id,candidate_id,
          link_source,state,linked_by,created_at)
         VALUES (?1,?2,?3,?4,?5,'candidate_promoted','active',?6,?7)",
        params![
            ids::new_public_id(),
            candidate.assessment_item_id,
            link_revision,
            version.id,
            candidate.id,
            request.reviewed_by.trim(),
            &now
        ],
    )?;
    tx.execute(
        "INSERT INTO exam_question_candidate_reviews_v2
         (public_id,request_key,request_hash,candidate_id,action,
          source_question_version_id,result_question_version_id,
          result_answer_key_version_id,reviewed_by,note,reviewed_at)
         VALUES (?1,?2,?3,?4,'promote_l1',?5,?6,?7,?8,?9,?10)",
        params![
            &review_public_id,
            request.request_key.trim(),
            &request_hash,
            candidate.id,
            source.id,
            version.id,
            answer_id,
            request.reviewed_by.trim(),
            request
                .note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            &now
        ],
    )?;
    let review_id = tx.last_insert_rowid();
    let event_payload = serde_json::json!({
        "schema_version": 1,
        "candidate_public_id": &candidate.public_id,
        "action": "promote_l1",
        "result_question_version_public_id": &version.public_id
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "exam.question_candidate.reviewed",
            event_version: 1,
            aggregate_type: "exam_question_candidate",
            aggregate_id: &candidate.public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("{}:audit", request.request_key.trim()),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(request.reviewed_by.trim()),
            action: "k1.question_candidate.promoted_l1",
            object_type: "exam_question_candidate",
            object_id: &candidate.public_id,
            object_revision: Some(1),
            note: request
                .note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            meta_json: Some(&event_payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    decision_from_row(conn, review_id)
}

pub fn discard_candidate(
    conn: &mut Connection,
    owner_id: &str,
    request: &DiscardCandidateRequest,
) -> CoreResult<CandidateReviewDecision> {
    required(&request.request_key, "请求键")?;
    required(&request.candidate_public_id, "候选 ID")?;
    required(&request.reviewed_by, "确认老师")?;
    if request.reviewed_by.trim() != owner_id.trim() {
        return Err(CoreError::Invalid("只能由当前题库老师处理候选".into()));
    }
    let request_bytes = serde_json::to_vec(&DiscardHashInput {
        schema_version: CANDIDATE_REVIEW_SCHEMA_VERSION,
        candidate_public_id: request.candidate_public_id.trim(),
        expected_content_hash: request.expected_content_hash.trim(),
        note: request
            .note
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty()),
        reviewed_by: request.reviewed_by.trim(),
    })
    .map_err(|error| CoreError::Parse(format!("候选丢弃请求序列化失败：{error}")))?;
    let request_hash = hashing::sha256_hex(&request_bytes);
    if let Some((id, existing_hash)) = existing_by_request(conn, request.request_key.trim())? {
        if existing_hash != request_hash {
            return Err(CoreError::Invalid("同一请求键对应不同候选丢弃内容".into()));
        }
        return decision_from_row(conn, id);
    }
    let candidate = load_candidate(conn, owner_id, request.candidate_public_id.trim())?;
    if candidate.content_hash != request.expected_content_hash {
        return Err(CoreError::Invalid("候选内容已变化，请刷新后重试".into()));
    }
    ensure_not_reviewed(conn, candidate.id)?;
    let source = content::get_question_version(conn, candidate.created_version_id)?
        .ok_or_else(|| CoreError::NotFound("候选来源题目版本".into()))?;
    ensure_personal_c0_source(conn, owner_id, &source)?;
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.transaction()?;
    tx.execute(
        "INSERT INTO exam_question_candidate_reviews_v2
         (public_id,request_key,request_hash,candidate_id,action,
          source_question_version_id,reviewed_by,note,reviewed_at)
         VALUES (?1,?2,?3,?4,'discard',?5,?6,?7,?8)",
        params![
            &public_id,
            request.request_key.trim(),
            &request_hash,
            candidate.id,
            source.id,
            request.reviewed_by.trim(),
            request
                .note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            &now
        ],
    )?;
    let review_id = tx.last_insert_rowid();
    let event_payload = serde_json::json!({
        "schema_version": 1,
        "candidate_public_id": &candidate.public_id,
        "action": "discard"
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("{}:outbox", request.request_key.trim()),
            event_type: "exam.question_candidate.reviewed",
            event_version: 1,
            aggregate_type: "exam_question_candidate",
            aggregate_id: &candidate.public_id,
            aggregate_revision: 1,
            payload_json: &event_payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("{}:audit", request.request_key.trim()),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(request.reviewed_by.trim()),
            action: "k1.question_candidate.discarded",
            object_type: "exam_question_candidate",
            object_id: &candidate.public_id,
            object_revision: Some(1),
            note: request
                .note
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty()),
            meta_json: Some(&event_payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    decision_from_row(conn, review_id)
}
