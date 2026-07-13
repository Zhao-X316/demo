//! K1 题目、答案、rubric 与 link set 的不可变版本仓储。

use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionIdentity {
    pub id: i64,
    pub public_id: String,
    pub owner_scope: String,
    pub owner_id: String,
    pub question_family_id: Option<i64>,
    pub rights_status: String,
    pub sharing_allowed: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionVersion {
    pub id: i64,
    pub public_id: String,
    pub question_id: i64,
    pub revision: i64,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub max_score: f64,
    pub content_hash: String,
    pub source_artifact_id: Option<i64>,
    pub source_anchor_json: Option<String>,
    pub supersedes_version_id: Option<i64>,
    pub quality_level: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestionOption {
    pub id: i64,
    pub public_id: String,
    pub question_version_id: i64,
    pub label: String,
    pub content: String,
    pub order_index: i64,
    pub created_at: String,
}

pub struct NewQuestion<'a> {
    pub owner_scope: &'a str,
    pub owner_id: &'a str,
    pub question_family_id: Option<i64>,
    pub rights_status: &'a str,
    pub sharing_allowed: bool,
}

pub struct NewQuestionOption<'a> {
    pub label: &'a str,
    pub content: &'a str,
    pub order_index: i64,
}

pub struct NewQuestionVersion<'a> {
    pub question_id: i64,
    pub revision: i64,
    pub question_type: &'a str,
    pub stem: &'a str,
    pub material_text: Option<&'a str>,
    pub max_score: f64,
    pub source_artifact_id: Option<i64>,
    pub source_anchor_json: Option<&'a str>,
    pub supersedes_version_id: Option<i64>,
    pub quality_level: &'a str,
    pub state: &'a str,
    pub options: &'a [NewQuestionOption<'a>],
}

#[derive(Serialize)]
struct ContentHashInput<'a> {
    schema_version: i64,
    question_type: &'a str,
    stem: &'a str,
    material_text: Option<&'a str>,
    max_score_millis: i64,
    options: Vec<(String, String, i64)>,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Invalid(format!("{label}不能为空")));
    }
    Ok(())
}

fn validate_question_type(value: &str) -> CoreResult<()> {
    if !matches!(
        value,
        "single" | "multiple" | "true_false" | "fill_blank" | "short_answer"
    ) {
        return Err(CoreError::Invalid("K1 题型非法".into()));
    }
    Ok(())
}

pub fn content_hash(input: &NewQuestionVersion<'_>) -> CoreResult<String> {
    let stem = input.stem.trim();
    let material = input.material_text.map(str::trim).filter(|v| !v.is_empty());
    let mut options: Vec<_> = input
        .options
        .iter()
        .map(|option| {
            (
                option.label.trim().to_ascii_uppercase(),
                option.content.trim().to_owned(),
                option.order_index,
            )
        })
        .collect();
    options.sort_by(|left, right| {
        left.2
            .cmp(&right.2)
            .then_with(|| left.0.cmp(&right.0))
            .then_with(|| left.1.cmp(&right.1))
    });
    let canonical = ContentHashInput {
        schema_version: 1,
        question_type: input.question_type,
        stem,
        material_text: material,
        max_score_millis: (input.max_score * 1000.0).round() as i64,
        options,
    };
    let bytes = serde_json::to_vec(&canonical)
        .map_err(|error| CoreError::Parse(format!("K1 内容 hash 序列化失败：{error}")))?;
    Ok(hashing::sha256_hex(&bytes))
}

pub fn create_question(conn: &Connection, input: &NewQuestion<'_>) -> CoreResult<QuestionIdentity> {
    if !matches!(input.owner_scope, "personal" | "school" | "official") {
        return Err(CoreError::Invalid("题库空间非法".into()));
    }
    required(input.owner_id, "题目所有者")?;
    if !matches!(input.rights_status, "unknown" | "cleared" | "restricted") {
        return Err(CoreError::Invalid("题目权利状态非法".into()));
    }
    if input.owner_scope == "official" && !input.sharing_allowed {
        return Err(CoreError::Invalid("官方题目必须明确允许共享".into()));
    }
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    conn.execute(
        "INSERT INTO k1_questions
         (public_id, owner_scope, owner_id, question_family_id, rights_status, sharing_allowed, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7)",
        (
            &public_id,
            input.owner_scope,
            input.owner_id.trim(),
            input.question_family_id,
            input.rights_status,
            input.sharing_allowed,
            &created_at,
        ),
    )?;
    get_question(conn, conn.last_insert_rowid())?
        .ok_or_else(|| CoreError::NotFound("刚创建的题目身份".into()))
}

pub fn get_question(conn: &Connection, id: i64) -> CoreResult<Option<QuestionIdentity>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, owner_scope, owner_id, question_family_id, rights_status,
                    sharing_allowed, created_at FROM k1_questions WHERE id=?1",
            [id],
            |row| {
                Ok(QuestionIdentity {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    owner_scope: row.get(2)?,
                    owner_id: row.get(3)?,
                    question_family_id: row.get(4)?,
                    rights_status: row.get(5)?,
                    sharing_allowed: row.get(6)?,
                    created_at: row.get(7)?,
                })
            },
        )
        .optional()?)
}

pub fn create_question_version(
    conn: &Connection,
    input: &NewQuestionVersion<'_>,
) -> CoreResult<QuestionVersion> {
    validate_question_type(input.question_type)?;
    required(input.stem, "题干")?;
    if !input.max_score.is_finite() || input.max_score <= 0.0 {
        return Err(CoreError::Invalid("题目分值必须大于 0".into()));
    }
    if input.revision <= 0 {
        return Err(CoreError::Invalid("题目 revision 必须大于 0".into()));
    }
    if !matches!(
        (input.quality_level, input.state),
        ("C0", "candidate") | ("L0", "draft") | ("L0", "review_pending")
    ) {
        return Err(CoreError::Invalid(
            "新题目只能以 C0/candidate 或 L0/draft|review_pending 创建；更高等级必须经过质量闸门"
                .into(),
        ));
    }
    if matches!(input.question_type, "single" | "multiple") && input.options.len() < 2 {
        return Err(CoreError::Invalid("选择题至少需要两个选项".into()));
    }
    let mut labels = std::collections::HashSet::new();
    for option in input.options {
        required(option.label, "选项标签")?;
        required(option.content, "选项内容")?;
        let label = option.label.trim().to_ascii_uppercase();
        if !labels.insert(label) {
            return Err(CoreError::Invalid("同一题选项标签不能重复".into()));
        }
    }
    if let Some(json) = input.source_anchor_json {
        validate_schema_object(json, "来源锚点")?;
    }

    let hash = content_hash(input)?;
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO k1_question_versions
         (public_id, question_id, revision, question_type, stem, material_text, max_score,
          content_hash, source_artifact_id, source_anchor_json, supersedes_version_id,
          quality_level, state, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        (
            &public_id,
            input.question_id,
            input.revision,
            input.question_type,
            input.stem.trim(),
            input.material_text.map(str::trim).filter(|v| !v.is_empty()),
            input.max_score,
            &hash,
            input.source_artifact_id,
            input.source_anchor_json,
            input.supersedes_version_id,
            input.quality_level,
            input.state,
            &created_at,
        ),
    )?;
    let version_id = tx.last_insert_rowid();
    for option in input.options {
        tx.execute(
            "INSERT INTO k1_question_options
             (public_id, question_version_id, label, content, order_index, created_at)
             VALUES (?1,?2,?3,?4,?5,?6)",
            (
                ids::new_public_id(),
                version_id,
                option.label.trim().to_ascii_uppercase(),
                option.content.trim(),
                option.order_index,
                &created_at,
            ),
        )?;
    }
    tx.commit()?;
    get_question_version(conn, version_id)?
        .ok_or_else(|| CoreError::NotFound("刚创建的题目版本".into()))
}

pub fn get_question_version(conn: &Connection, id: i64) -> CoreResult<Option<QuestionVersion>> {
    Ok(conn
        .query_row(
            "SELECT id, public_id, question_id, revision, question_type, stem, material_text,
                    max_score, content_hash, source_artifact_id, source_anchor_json,
                    supersedes_version_id, quality_level, state, created_at
             FROM k1_question_versions WHERE id=?1",
            [id],
            |row| {
                Ok(QuestionVersion {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    question_id: row.get(2)?,
                    revision: row.get(3)?,
                    question_type: row.get(4)?,
                    stem: row.get(5)?,
                    material_text: row.get(6)?,
                    max_score: row.get(7)?,
                    content_hash: row.get(8)?,
                    source_artifact_id: row.get(9)?,
                    source_anchor_json: row.get(10)?,
                    supersedes_version_id: row.get(11)?,
                    quality_level: row.get(12)?,
                    state: row.get(13)?,
                    created_at: row.get(14)?,
                })
            },
        )
        .optional()?)
}

pub fn list_options(conn: &Connection, version_id: i64) -> CoreResult<Vec<QuestionOption>> {
    let mut stmt = conn.prepare(
        "SELECT id, public_id, question_version_id, label, content, order_index, created_at
         FROM k1_question_options WHERE question_version_id=?1 ORDER BY order_index, id",
    )?;
    let rows = stmt.query_map([version_id], |row| {
        Ok(QuestionOption {
            id: row.get(0)?,
            public_id: row.get(1)?,
            question_version_id: row.get(2)?,
            label: row.get(3)?,
            content: row.get(4)?,
            order_index: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}

pub fn find_exact_versions(
    conn: &Connection,
    hash: &str,
    question_type: &str,
) -> CoreResult<Vec<QuestionVersion>> {
    let mut stmt = conn.prepare(
        "SELECT id FROM k1_question_versions
         WHERE content_hash=?1 AND question_type=?2 AND state NOT IN ('deprecated','archived')
         ORDER BY id",
    )?;
    let ids = stmt.query_map((hash, question_type), |row| row.get::<_, i64>(0))?;
    let mut out = Vec::new();
    for id in ids {
        if let Some(version) = get_question_version(conn, id?)? {
            out.push(version);
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VersionRef {
    pub id: i64,
    pub public_id: String,
    pub question_version_id: i64,
    pub revision: i64,
    pub state: String,
}

pub struct NewAnswerSlot<'a> {
    pub stable_id: Option<&'a str>,
    pub order_index: i64,
    pub canonical_answers_json: &'a str,
    pub normalization_rules_json: Option<&'a str>,
    pub max_score: f64,
}

pub struct NewAnswerKeyVersion<'a> {
    pub question_version_id: i64,
    pub revision: i64,
    pub answer_json: &'a str,
    pub state: &'a str,
    pub confirmed_by: Option<&'a str>,
    pub supersedes_answer_key_id: Option<i64>,
    pub slots: &'a [NewAnswerSlot<'a>],
}

fn validate_schema_object(json: &str, label: &str) -> CoreResult<Value> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| CoreError::Invalid(format!("{label} JSON 无效：{error}")))?;
    if !value.is_object()
        || value
            .get("schema_version")
            .and_then(Value::as_i64)
            .is_none()
    {
        return Err(CoreError::Invalid(format!(
            "{label} 必须是带整数 schema_version 的对象"
        )));
    }
    Ok(value)
}

pub fn create_answer_key_version(
    conn: &Connection,
    input: &NewAnswerKeyVersion<'_>,
) -> CoreResult<VersionRef> {
    validate_schema_object(input.answer_json, "标准答案")?;
    if input.revision <= 0 || !matches!(input.state, "draft" | "confirmed" | "retired") {
        return Err(CoreError::Invalid("答案 revision/state 非法".into()));
    }
    if input.state == "confirmed" {
        required(input.confirmed_by.unwrap_or_default(), "答案确认人")?;
    }
    for slot in input.slots {
        validate_schema_object(slot.canonical_answers_json, "答案槽位")?;
        if let Some(json) = slot.normalization_rules_json {
            validate_schema_object(json, "答案标准化规则")?;
        }
        if !slot.max_score.is_finite() || slot.max_score <= 0.0 {
            return Err(CoreError::Invalid("答案槽位分值必须大于 0".into()));
        }
    }
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    let confirmed_at = (input.state == "confirmed").then(|| created_at.clone());
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO k1_answer_key_versions
         (public_id, question_version_id, revision, answer_json, state,
          supersedes_answer_key_id, created_at, confirmed_by, confirmed_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        (
            &public_id,
            input.question_version_id,
            input.revision,
            input.answer_json,
            input.state,
            input.supersedes_answer_key_id,
            &created_at,
            input.confirmed_by.map(str::trim),
            confirmed_at.as_deref(),
        ),
    )?;
    let answer_key_id = tx.last_insert_rowid();
    for slot in input.slots {
        tx.execute(
            "INSERT INTO k1_answer_slots
             (public_id, stable_id, answer_key_version_id, order_index,
              canonical_answers_json, normalization_rules_json, max_score, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            (
                ids::new_public_id(),
                slot.stable_id
                    .map(str::to_owned)
                    .unwrap_or_else(ids::new_public_id),
                answer_key_id,
                slot.order_index,
                slot.canonical_answers_json,
                slot.normalization_rules_json,
                slot.max_score,
                &created_at,
            ),
        )?;
    }
    tx.commit()?;
    Ok(VersionRef {
        id: answer_key_id,
        public_id,
        question_version_id: input.question_version_id,
        revision: input.revision,
        state: input.state.into(),
    })
}

pub struct NewRubricPoint<'a> {
    pub stable_id: Option<&'a str>,
    pub order_index: i64,
    pub canonical_text: &'a str,
    pub allowed_paraphrases_json: Option<&'a str>,
    pub required_concepts_json: Option<&'a str>,
    pub max_score: f64,
}

pub struct NewRubricVersion<'a> {
    pub question_version_id: i64,
    pub revision: i64,
    pub max_score: f64,
    pub state: &'a str,
    pub confirmed_by: Option<&'a str>,
    pub supersedes_rubric_id: Option<i64>,
    pub points: &'a [NewRubricPoint<'a>],
}

pub fn create_rubric_version(
    conn: &Connection,
    input: &NewRubricVersion<'_>,
) -> CoreResult<VersionRef> {
    if input.revision <= 0 || !input.max_score.is_finite() || input.max_score <= 0.0 {
        return Err(CoreError::Invalid("rubric revision/分值非法".into()));
    }
    if !matches!(input.state, "draft" | "confirmed" | "retired") {
        return Err(CoreError::Invalid("rubric 状态非法".into()));
    }
    if input.state == "confirmed" {
        required(input.confirmed_by.unwrap_or_default(), "rubric 确认人")?;
    }
    let point_total: f64 = input.points.iter().map(|point| point.max_score).sum();
    if input.points.is_empty() || (point_total - input.max_score).abs() > 0.000_001 {
        return Err(CoreError::Invalid(
            "rubric point 分值之和必须等于总分".into(),
        ));
    }
    for point in input.points {
        required(point.canonical_text, "评分点")?;
        if !point.max_score.is_finite() || point.max_score <= 0.0 {
            return Err(CoreError::Invalid("评分点分值必须大于 0".into()));
        }
        for json in [point.allowed_paraphrases_json, point.required_concepts_json]
            .into_iter()
            .flatten()
        {
            serde_json::from_str::<Value>(json)
                .map_err(|error| CoreError::Invalid(format!("评分点 JSON 无效：{error}")))?;
        }
    }
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    let confirmed_at = (input.state == "confirmed").then(|| created_at.clone());
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO k1_rubric_versions
         (public_id, question_version_id, revision, max_score, state, supersedes_rubric_id,
          created_at, confirmed_by, confirmed_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        (
            &public_id,
            input.question_version_id,
            input.revision,
            input.max_score,
            input.state,
            input.supersedes_rubric_id,
            &created_at,
            input.confirmed_by.map(str::trim),
            confirmed_at.as_deref(),
        ),
    )?;
    let rubric_id = tx.last_insert_rowid();
    for point in input.points {
        tx.execute(
            "INSERT INTO k1_rubric_points
             (public_id, stable_id, rubric_version_id, order_index, canonical_text,
              allowed_paraphrases_json, required_concepts_json, max_score, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            (
                ids::new_public_id(),
                point
                    .stable_id
                    .map(str::to_owned)
                    .unwrap_or_else(ids::new_public_id),
                rubric_id,
                point.order_index,
                point.canonical_text.trim(),
                point.allowed_paraphrases_json,
                point.required_concepts_json,
                point.max_score,
                &created_at,
            ),
        )?;
    }
    tx.commit()?;
    Ok(VersionRef {
        id: rubric_id,
        public_id,
        question_version_id: input.question_version_id,
        revision: input.revision,
        state: input.state.into(),
    })
}

pub fn create_link_set(
    conn: &Connection,
    question_version_id: i64,
    knowledge_map_id: i64,
    revision: i64,
    state: &str,
    confirmed_by: Option<&str>,
    supersedes_link_set_id: Option<i64>,
) -> CoreResult<VersionRef> {
    if revision <= 0 || !matches!(state, "draft" | "confirmed" | "retired") {
        return Err(CoreError::Invalid("link set revision/state 非法".into()));
    }
    if state == "confirmed" {
        required(confirmed_by.unwrap_or_default(), "link set 确认人")?;
    }
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    let confirmed_at = (state == "confirmed").then(|| created_at.clone());
    conn.execute(
        "INSERT INTO k1_link_sets
         (public_id, question_version_id, knowledge_map_id, revision, state,
          supersedes_link_set_id, created_at, confirmed_by, confirmed_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        (
            &public_id,
            question_version_id,
            knowledge_map_id,
            revision,
            state,
            supersedes_link_set_id,
            &created_at,
            confirmed_by.map(str::trim),
            confirmed_at.as_deref(),
        ),
    )?;
    Ok(VersionRef {
        id: conn.last_insert_rowid(),
        public_id,
        question_version_id,
        revision,
        state: state.into(),
    })
}

fn source_belongs_to_question(
    conn: &Connection,
    question_version_id: i64,
    source_type: &str,
    source_public_id: &str,
) -> CoreResult<bool> {
    let sql = match source_type {
        "question" => "SELECT EXISTS(SELECT 1 FROM k1_question_versions WHERE id=?1 AND public_id=?2)",
        "option" => "SELECT EXISTS(SELECT 1 FROM k1_question_options WHERE question_version_id=?1 AND public_id=?2)",
        "answer_slot" => "SELECT EXISTS(SELECT 1 FROM k1_answer_slots s JOIN k1_answer_key_versions a ON a.id=s.answer_key_version_id WHERE a.question_version_id=?1 AND s.public_id=?2)",
        "rubric_point" => "SELECT EXISTS(SELECT 1 FROM k1_rubric_points p JOIN k1_rubric_versions r ON r.id=p.rubric_version_id WHERE r.question_version_id=?1 AND p.public_id=?2)",
        _ => return Err(CoreError::Invalid("link source_type 非法".into())),
    };
    Ok(
        conn.query_row(sql, (question_version_id, source_public_id), |row| {
            row.get(0)
        })?,
    )
}

fn link_set_question(conn: &Connection, link_set_id: i64) -> CoreResult<i64> {
    conn.query_row(
        "SELECT question_version_id FROM k1_link_sets WHERE id=?1",
        [link_set_id],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub struct NewKnowledgeLink<'a> {
    pub link_set_id: i64,
    pub source_type: &'a str,
    pub source_public_id: &'a str,
    pub knowledge_node_id: i64,
    pub relation_type: &'a str,
    pub confirmation_level: &'a str,
    pub verified_by: Option<&'a str>,
}

pub fn add_knowledge_link(conn: &Connection, input: &NewKnowledgeLink<'_>) -> CoreResult<i64> {
    let question_version_id = link_set_question(conn, input.link_set_id)?;
    if !source_belongs_to_question(
        conn,
        question_version_id,
        input.source_type,
        input.source_public_id,
    )? {
        return Err(CoreError::Invalid("link source 不属于当前题目版本".into()));
    }
    let verified_at = (input.confirmation_level == "teacher_confirmed").then(time::utc_now_rfc3339);
    conn.execute(
        "INSERT INTO k1_knowledge_links
         (public_id, link_set_id, source_type, source_public_id, knowledge_node_id,
          relation_type, confirmation_level, verified_by, verified_at, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        (
            ids::new_public_id(),
            input.link_set_id,
            input.source_type,
            input.source_public_id,
            input.knowledge_node_id,
            input.relation_type,
            input.confirmation_level,
            input.verified_by,
            verified_at.as_deref(),
            time::utc_now_rfc3339(),
        ),
    )?;
    Ok(conn.last_insert_rowid())
}

pub struct NewAbilityLink<'a> {
    pub link_set_id: i64,
    pub source_type: &'a str,
    pub source_public_id: &'a str,
    pub ability_dimension_id: i64,
    pub evidence_strength: f64,
    pub response_mode: &'a str,
    pub confirmation_level: &'a str,
    pub verified_by: Option<&'a str>,
}

pub fn add_ability_link(conn: &Connection, input: &NewAbilityLink<'_>) -> CoreResult<i64> {
    let question_version_id = link_set_question(conn, input.link_set_id)?;
    if !source_belongs_to_question(
        conn,
        question_version_id,
        input.source_type,
        input.source_public_id,
    )? {
        return Err(CoreError::Invalid(
            "ability link source 不属于当前题目版本".into(),
        ));
    }
    let verified_at = (input.confirmation_level == "teacher_confirmed").then(time::utc_now_rfc3339);
    conn.execute(
        "INSERT INTO k1_ability_links
         (public_id, link_set_id, source_type, source_public_id, ability_dimension_id,
          evidence_strength, response_mode, confirmation_level, verified_by, verified_at, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        (
            ids::new_public_id(),
            input.link_set_id,
            input.source_type,
            input.source_public_id,
            input.ability_dimension_id,
            input.evidence_strength,
            input.response_mode,
            input.confirmation_level,
            input.verified_by,
            verified_at.as_deref(),
            time::utc_now_rfc3339(),
        ),
    )?;
    Ok(conn.last_insert_rowid())
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

fn confirmed_answer_exists(conn: &Connection, question_version_id: i64) -> CoreResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM k1_answer_key_versions
         WHERE question_version_id=?1 AND state='confirmed')",
        [question_version_id],
        |row| row.get(0),
    )?)
}

fn l2_ready(conn: &Connection, version: &QuestionVersion) -> CoreResult<bool> {
    let sql = match version.question_type.as_str() {
        "single" | "multiple" | "true_false" => {
            "SELECT EXISTS(SELECT 1 FROM k1_answer_key_versions
             WHERE question_version_id=?1 AND state='confirmed')"
        }
        "fill_blank" => {
            "SELECT EXISTS(
               SELECT 1 FROM k1_answer_key_versions a
               JOIN k1_answer_slots s ON s.answer_key_version_id=a.id
               WHERE a.question_version_id=?1 AND a.state='confirmed'
             )"
        }
        "short_answer" => {
            "SELECT EXISTS(
               SELECT 1 FROM k1_rubric_versions r
               JOIN k1_rubric_points p ON p.rubric_version_id=r.id
               WHERE r.question_version_id=?1 AND r.state='confirmed'
             )"
        }
        _ => return Err(CoreError::Invalid("题型非法".into())),
    };
    Ok(conn.query_row(sql, [version.id], |row| row.get(0))?)
}

fn l3_ready(conn: &Connection, question_version_id: i64) -> CoreResult<bool> {
    let ready: bool = conn.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM k1_link_sets ls
           WHERE ls.question_version_id=?1 AND ls.state='confirmed'
             AND EXISTS(
               SELECT 1 FROM k1_knowledge_links kl
               WHERE kl.link_set_id=ls.id
                 AND kl.confirmation_level='teacher_confirmed'
                 AND kl.relation_type IN ('direct_assessment','rubric_basis')
             )
             AND EXISTS(
               SELECT 1 FROM k1_ability_links al
               WHERE al.link_set_id=ls.id
                 AND al.confirmation_level='teacher_confirmed'
             )
         )",
        [question_version_id],
        |row| row.get(0),
    )?;
    Ok(ready)
}

/// 经质量闸门晋级。题干等版本内容不可修改，晋级事件永久保留。
pub fn promote_question_version(
    conn: &Connection,
    question_version_id: i64,
    target_quality: &str,
    verified_by: &str,
    reason: Option<&str>,
) -> CoreResult<QuestionVersion> {
    required(verified_by, "质量确认人")?;
    let target_rank = quality_rank(target_quality)
        .ok_or_else(|| CoreError::Invalid("目标质量等级非法".into()))?;
    if target_rank < quality_rank("L1").unwrap() {
        return Err(CoreError::Invalid("质量晋级目标必须是 L1～L4".into()));
    }
    let current = get_question_version(conn, question_version_id)?
        .ok_or_else(|| CoreError::NotFound(format!("question_version#{question_version_id}")))?;
    let current_rank = quality_rank(&current.quality_level)
        .ok_or_else(|| CoreError::Invalid("当前质量等级非法".into()))?;
    if target_rank <= current_rank {
        return Err(CoreError::Invalid("质量等级只能向上晋级".into()));
    }
    if !confirmed_answer_exists(conn, question_version_id)? {
        return Err(CoreError::Invalid("L1 需要老师确认的答案版本".into()));
    }
    if target_rank >= quality_rank("L2").unwrap() && !l2_ready(conn, &current)? {
        return Err(CoreError::Invalid(
            "L2 需要与题型匹配的已确认答案槽位或 rubric".into(),
        ));
    }
    if target_rank >= quality_rank("L3").unwrap() && !l3_ready(conn, question_version_id)? {
        return Err(CoreError::Invalid(
            "L3 需要老师确认的直接知识链接和能力证据强度".into(),
        ));
    }
    if target_rank >= quality_rank("L4").unwrap() {
        let share_ready: bool = conn.query_row(
            "SELECT q.rights_status='cleared' AND q.sharing_allowed=1
             FROM k1_question_versions v JOIN k1_questions q ON q.id=v.question_id
             WHERE v.id=?1",
            [question_version_id],
            |row| row.get(0),
        )?;
        if !share_ready {
            return Err(CoreError::Invalid(
                "L4 需要已核对权利状态并明确允许共享".into(),
            ));
        }
    }

    let verified_at = time::utc_now_rfc3339();
    let tx = conn.unchecked_transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision), 0) + 1 FROM k1_question_quality_events
         WHERE question_version_id=?1",
        [question_version_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO k1_question_quality_events
         (public_id, question_version_id, revision, from_quality, to_quality,
          from_state, to_state, reason, verified_by, verified_at, created_at)
         VALUES (?1,?2,?3,?4,?5,?6,'published',?7,?8,?9,?9)",
        (
            ids::new_public_id(),
            question_version_id,
            revision,
            &current.quality_level,
            target_quality,
            &current.state,
            reason.map(str::trim),
            verified_by.trim(),
            &verified_at,
        ),
    )?;
    tx.execute(
        "UPDATE k1_question_versions SET quality_level=?1, state='published' WHERE id=?2",
        (target_quality, question_version_id),
    )?;
    tx.commit()?;
    get_question_version(conn, question_version_id)?
        .ok_or_else(|| CoreError::NotFound("晋级后的题目版本".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::taxonomy::{
        create_ability_dimension, create_curriculum_node, create_knowledge_map,
        create_knowledge_node, create_textbook_edition, NewAbilityDimension, NewCurriculumNode,
        NewKnowledgeMap, NewKnowledgeNode, NewTextbookEdition,
    };
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> (Connection, i64, i64, i64) {
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
        (conn, map.id, knowledge.id, ability.id)
    }

    #[test]
    fn freezes_question_answer_rubric_and_links() {
        let (conn, map_id, knowledge_id, ability_id) = setup();
        let question = create_question(
            &conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: "local-teacher",
                question_family_id: None,
                rights_status: "unknown",
                sharing_allowed: false,
            },
        )
        .unwrap();
        let version_input = NewQuestionVersion {
            question_id: question.id,
            revision: 1,
            question_type: "single",
            stem: "鸦片战争爆发于哪一年？",
            material_text: None,
            max_score: 2.0,
            source_artifact_id: None,
            source_anchor_json: Some(r#"{"schema_version":1,"page":1}"#),
            supersedes_version_id: None,
            quality_level: "L0",
            state: "draft",
            options: &[
                NewQuestionOption {
                    label: "A",
                    content: "1839",
                    order_index: 0,
                },
                NewQuestionOption {
                    label: "B",
                    content: "1840",
                    order_index: 1,
                },
            ],
        };
        let version = create_question_version(&conn, &version_input).unwrap();
        let answer = create_answer_key_version(
            &conn,
            &NewAnswerKeyVersion {
                question_version_id: version.id,
                revision: 1,
                answer_json: r#"{"schema_version":1,"correct_labels":["B"]}"#,
                state: "confirmed",
                confirmed_by: Some("teacher-local"),
                supersedes_answer_key_id: None,
                slots: &[],
            },
        )
        .unwrap();
        let rubric = create_rubric_version(
            &conn,
            &NewRubricVersion {
                question_version_id: version.id,
                revision: 1,
                max_score: 2.0,
                state: "confirmed",
                confirmed_by: Some("teacher-local"),
                supersedes_rubric_id: None,
                points: &[NewRubricPoint {
                    stable_id: None,
                    order_index: 0,
                    canonical_text: "选择 1840 年",
                    allowed_paraphrases_json: None,
                    required_concepts_json: None,
                    max_score: 2.0,
                }],
            },
        )
        .unwrap();
        assert!(promote_question_version(
            &conn,
            version.id,
            "L3",
            "teacher-local",
            Some("链接尚未确认"),
        )
        .is_err());
        let link_set = create_link_set(
            &conn,
            version.id,
            map_id,
            1,
            "confirmed",
            Some("teacher-local"),
            None,
        )
        .unwrap();
        add_knowledge_link(
            &conn,
            &NewKnowledgeLink {
                link_set_id: link_set.id,
                source_type: "question",
                source_public_id: &version.public_id,
                knowledge_node_id: knowledge_id,
                relation_type: "direct_assessment",
                confirmation_level: "teacher_confirmed",
                verified_by: Some("teacher-local"),
            },
        )
        .unwrap();
        add_ability_link(
            &conn,
            &NewAbilityLink {
                link_set_id: link_set.id,
                source_type: "question",
                source_public_id: &version.public_id,
                ability_dimension_id: ability_id,
                evidence_strength: 0.4,
                response_mode: "recognition",
                confirmation_level: "teacher_confirmed",
                verified_by: Some("teacher-local"),
            },
        )
        .unwrap();

        let promoted = promote_question_version(
            &conn,
            version.id,
            "L3",
            "teacher-local",
            Some("题目、答案、评分点和链接均已核对"),
        )
        .unwrap();

        assert_eq!(list_options(&conn, version.id).unwrap().len(), 2);
        assert_eq!(answer.question_version_id, version.id);
        assert_eq!(rubric.question_version_id, version.id);
        assert_eq!(link_set.question_version_id, version.id);
        assert_eq!(promoted.quality_level, "L3");
        assert_eq!(promoted.state, "published");
        assert!(conn
            .execute(
                "UPDATE k1_question_versions SET stem='覆盖' WHERE id=?1",
                [version.id]
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE k1_question_versions SET quality_level='L4' WHERE id=?1",
                [version.id]
            )
            .is_err());
    }

    #[test]
    fn exact_hash_is_deterministic_and_source_must_belong_to_question() {
        let (conn, map_id, knowledge_id, _) = setup();
        let question = create_question(
            &conn,
            &NewQuestion {
                owner_scope: "personal",
                owner_id: "local",
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
        let input = NewQuestionVersion {
            question_id: question.id,
            revision: 1,
            question_type: "single",
            stem: "  鸦片战争爆发年份？ ",
            material_text: None,
            max_score: 1.0,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "L0",
            state: "draft",
            options: &options,
        };
        let version = create_question_version(&conn, &input).unwrap();
        assert_eq!(content_hash(&input).unwrap(), version.content_hash);
        let reversed_options = [
            NewQuestionOption {
                label: "b",
                content: "1842",
                order_index: 1,
            },
            NewQuestionOption {
                label: "a",
                content: "1840",
                order_index: 0,
            },
        ];
        let reordered = NewQuestionVersion {
            options: &reversed_options,
            ..input
        };
        assert_eq!(content_hash(&reordered).unwrap(), version.content_hash);
        assert_eq!(
            find_exact_versions(&conn, &version.content_hash, "single")
                .unwrap()
                .len(),
            1
        );
        let link_set = create_link_set(&conn, version.id, map_id, 1, "draft", None, None).unwrap();
        let result = add_knowledge_link(
            &conn,
            &NewKnowledgeLink {
                link_set_id: link_set.id,
                source_type: "question",
                source_public_id: "not-this-question",
                knowledge_node_id: knowledge_id,
                relation_type: "direct_assessment",
                confirmation_level: "machine_suggested",
                verified_by: None,
            },
        );
        assert!(result.is_err());
    }
}
