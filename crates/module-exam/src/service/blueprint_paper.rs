//! K1-4b 老师换题、排序与可打印试卷。
//!
//! 只允许在已确认蓝图的固定槽位中选择同题型、同分值的当前合格题目。
//! 每次老师确认都会追加 assessment version 和不可变 paper edition；不会
//! 自动选择未来上传默认版本，也不会改写历史 attempt、成绩或学习证据。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

use super::assessment::{add_assessment_item, NewAssessmentItem};
use super::blueprint_assembly::{
    load_candidates, load_scope, BlueprintCandidate, BlueprintPreviewRequest,
    BlueprintQuestionTypeTarget, InternalCandidate,
};

pub const PAPER_SCHEMA_VERSION: i64 = 1;
pub const PAPER_RULE_VERSION: &str = "k1-blueprint-paper-v1";
pub const PAPER_TEMPLATE_VERSION: &str = "k1-blueprint-paper-assessment-v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintPaperEditorItem {
    pub source_slot_order_index: i64,
    pub order_index: i64,
    pub question_version_public_id: String,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub score: f64,
    pub page_break_before: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintPaperEditor {
    pub schema_version: i64,
    pub rule_version: String,
    pub assembly_public_id: String,
    pub title: String,
    pub class_name: String,
    pub knowledge_map_title: String,
    pub curriculum_node_title: Option<String>,
    pub current_edition_public_id: Option<String>,
    pub current_revision: i64,
    pub source_assessment_version_public_id: String,
    pub required_knowledge_node_public_ids: Vec<String>,
    pub items: Vec<BlueprintPaperEditorItem>,
    pub candidates: Vec<BlueprintCandidate>,
    pub boundary_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlueprintPaperItemInput {
    pub source_slot_order_index: i64,
    pub question_version_public_id: String,
    pub page_break_before: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmBlueprintPaperRequest {
    pub request_key: String,
    pub assembly_public_id: String,
    pub expected_source_assessment_version_public_id: String,
    pub title: String,
    pub items: Vec<BlueprintPaperItemInput>,
    pub confirmed_by: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintPaperEditionItem {
    pub source_slot_order_index: i64,
    pub order_index: i64,
    pub question_version_public_id: String,
    pub question_type: String,
    pub stem: String,
    pub material_text: Option<String>,
    pub score: f64,
    pub page_break_before: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlueprintPaperEdition {
    pub public_id: String,
    pub assembly_public_id: String,
    pub revision: i64,
    pub title: String,
    pub assessment_public_id: String,
    pub source_assessment_version_public_id: String,
    pub assessment_version_public_id: String,
    pub supersedes_edition_public_id: Option<String>,
    pub item_set_hash: String,
    pub question_html_sha256: String,
    pub answer_html_sha256: String,
    pub suggested_question_file_name: String,
    pub suggested_answer_file_name: String,
    pub state: String,
    pub confirmed_by: String,
    pub confirmed_at: String,
    pub items: Vec<BlueprintPaperEditionItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WrittenBlueprintPaper {
    pub edition_public_id: String,
    pub export_kind: String,
    pub file_name: String,
    pub byte_size: i64,
    pub sha256: String,
}

#[derive(Debug)]
struct AssemblySource {
    id: i64,
    public_id: String,
    assessment_id: i64,
    assessment_public_id: String,
    base_assessment_version_id: i64,
    base_assessment_version_public_id: String,
    class_id: i64,
    class_name: String,
    knowledge_map_public_id: String,
    knowledge_map_title: String,
    curriculum_node_public_id: Option<String>,
    curriculum_node_title: Option<String>,
    title: String,
    total_score: f64,
    question_type_targets: Vec<BlueprintQuestionTypeTarget>,
    required_knowledge_node_public_ids: Vec<String>,
}

#[derive(Debug)]
struct CurrentEdition {
    id: i64,
    public_id: String,
    revision: i64,
    assessment_version_id: i64,
    assessment_version_public_id: String,
    title: String,
}

#[derive(Debug)]
struct PrintableQuestion {
    order_index: i64,
    question_type: String,
    stem: String,
    material_text: Option<String>,
    score: f64,
    page_break_before: bool,
    options: Vec<(String, String)>,
    answer_json: String,
    slots: Vec<String>,
    rubric_points: Vec<(String, f64)>,
}

#[derive(Debug)]
struct SourceSlot {
    output: BlueprintPaperEditorItem,
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    knowledge_node_public_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AssessmentItemHash {
    item_id: i64,
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    order_index: i64,
    score_millis: i64,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Invalid(format!("{label}不能为空")));
    }
    Ok(())
}

fn load_assembly_source(conn: &Connection, assembly_public_id: &str) -> CoreResult<AssemblySource> {
    conn.query_row(
        "SELECT assembly.id,assembly.public_id,assembly.assessment_id,assessment.public_id,
                assembly.assessment_version_id,base_version.public_id,
                assembly.class_id,class.name,map.public_id,edition.title,
                curriculum.public_id,curriculum.title,assembly.title,assembly.total_score,
                assembly.question_type_targets_json,assembly.required_knowledge_node_ids_json
         FROM exam_blueprint_assemblies_v2 assembly
         JOIN exam_assessments_v2 assessment ON assessment.id=assembly.assessment_id
         JOIN exam_assessment_versions_v2 base_version
           ON base_version.id=assembly.assessment_version_id
         JOIN classes class ON class.id=assembly.class_id
         JOIN k1_knowledge_maps map ON map.id=assembly.knowledge_map_id
         JOIN k1_textbook_editions edition ON edition.id=map.textbook_edition_id
         LEFT JOIN k1_curriculum_nodes curriculum ON curriculum.id=assembly.curriculum_node_id
         WHERE assembly.public_id=?1 AND assembly.state='confirmed'",
        [assembly_public_id.trim()],
        |row| {
            let targets_json: String = row.get(14)?;
            let required_json: String = row.get(15)?;
            Ok(AssemblySource {
                id: row.get(0)?,
                public_id: row.get(1)?,
                assessment_id: row.get(2)?,
                assessment_public_id: row.get(3)?,
                base_assessment_version_id: row.get(4)?,
                base_assessment_version_public_id: row.get(5)?,
                class_id: row.get(6)?,
                class_name: row.get(7)?,
                knowledge_map_public_id: row.get(8)?,
                knowledge_map_title: row.get(9)?,
                curriculum_node_public_id: row.get(10)?,
                curriculum_node_title: row.get(11)?,
                title: row.get(12)?,
                total_score: row.get(13)?,
                question_type_targets: serde_json::from_str(&targets_json).map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        targets_json.len(),
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
                required_knowledge_node_public_ids: serde_json::from_str(&required_json).map_err(
                    |error| {
                        rusqlite::Error::FromSqlConversionFailure(
                            required_json.len(),
                            rusqlite::types::Type::Text,
                            Box::new(error),
                        )
                    },
                )?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::NotFound("已确认组卷蓝图".into()))
}

fn load_current_edition(conn: &Connection, assembly_id: i64) -> CoreResult<Option<CurrentEdition>> {
    Ok(conn
        .query_row(
            "SELECT edition.id,edition.public_id,edition.revision,
                    edition.assessment_version_id,version.public_id,edition.title
             FROM exam_blueprint_paper_editions_v2 edition
             JOIN exam_assessment_versions_v2 version
               ON version.id=edition.assessment_version_id
             WHERE edition.assembly_id=?1
             ORDER BY edition.revision DESC,edition.id DESC LIMIT 1",
            [assembly_id],
            |row| {
                Ok(CurrentEdition {
                    id: row.get(0)?,
                    public_id: row.get(1)?,
                    revision: row.get(2)?,
                    assessment_version_id: row.get(3)?,
                    assessment_version_public_id: row.get(4)?,
                    title: row.get(5)?,
                })
            },
        )
        .optional()?)
}

fn source_scope(
    conn: &Connection,
    source: &AssemblySource,
) -> CoreResult<super::blueprint_assembly::Scope> {
    load_scope(
        conn,
        &BlueprintPreviewRequest {
            class_id: source.class_id,
            knowledge_map_public_id: source.knowledge_map_public_id.clone(),
            curriculum_node_public_id: source.curriculum_node_public_id.clone(),
            total_score: source.total_score,
            question_type_targets: source.question_type_targets.clone(),
            required_knowledge_node_public_ids: source.required_knowledge_node_public_ids.clone(),
        },
        &source.required_knowledge_node_public_ids,
    )
}

fn load_source_slots(
    conn: &Connection,
    source: &AssemblySource,
    current: Option<&CurrentEdition>,
) -> CoreResult<Vec<SourceSlot>> {
    let (sql, id) = if let Some(current) = current {
        (
            "SELECT item.source_slot_order_index,item.order_index,version.public_id,
                    version.question_type,version.stem,version.material_text,item.score,
                    item.page_break_before,item.question_version_id,item.answer_key_version_id,
                    item.rubric_version_id,item.link_set_id
             FROM exam_blueprint_paper_items_v2 item
             JOIN k1_question_versions version ON version.id=item.question_version_id
             WHERE item.edition_id=?1 ORDER BY item.order_index,item.id",
            current.id,
        )
    } else {
        (
            "SELECT item.order_index,item.order_index,version.public_id,
                    version.question_type,version.stem,version.material_text,item.score,0,
                    item.question_version_id,item.answer_key_version_id,
                    item.rubric_version_id,item.link_set_id
             FROM exam_blueprint_items_v2 item
             JOIN k1_question_versions version ON version.id=item.question_version_id
             WHERE item.assembly_id=?1 ORDER BY item.order_index,item.id",
            source.id,
        )
    };
    let mut statement = conn.prepare(sql)?;
    let base_rows = statement
        .query_map([id], |row| {
            Ok((
                BlueprintPaperEditorItem {
                    source_slot_order_index: row.get(0)?,
                    order_index: row.get(1)?,
                    question_version_public_id: row.get(2)?,
                    question_type: row.get(3)?,
                    stem: row.get(4)?,
                    material_text: row.get(5)?,
                    score: row.get(6)?,
                    page_break_before: row.get::<_, i64>(7)? == 1,
                },
                row.get::<_, i64>(8)?,
                row.get::<_, i64>(9)?,
                row.get::<_, i64>(10)?,
                row.get::<_, i64>(11)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    let mut result = Vec::with_capacity(base_rows.len());
    for (output, question_version_id, answer_key_version_id, rubric_version_id, link_set_id) in
        base_rows
    {
        let knowledge_node_public_ids = {
            let mut statement = conn.prepare(
                "SELECT knowledge.public_id
                 FROM k1_knowledge_links link
                 JOIN k1_knowledge_nodes knowledge ON knowledge.id=link.knowledge_node_id
                 WHERE link.link_set_id=?1
                   AND link.confirmation_level='teacher_confirmed'
                   AND link.relation_type IN ('direct_assessment','rubric_basis')
                 ORDER BY knowledge.order_index,knowledge.id",
            )?;
            let rows = statement
                .query_map([link_set_id], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        result.push(SourceSlot {
            output,
            question_version_id,
            answer_key_version_id,
            rubric_version_id,
            link_set_id,
            knowledge_node_public_ids,
        });
    }
    Ok(result)
}

pub fn load_blueprint_paper_editor(
    conn: &Connection,
    assembly_public_id: &str,
) -> CoreResult<BlueprintPaperEditor> {
    required(assembly_public_id, "蓝图")?;
    let source = load_assembly_source(conn, assembly_public_id)?;
    let current = load_current_edition(conn, source.id)?;
    let scope = source_scope(conn, &source)?;
    let candidates = load_candidates(conn, &scope)?
        .into_iter()
        .map(|candidate| candidate.output)
        .collect();
    let source_slots = load_source_slots(conn, &source, current.as_ref())?;
    if source_slots.is_empty() {
        return Err(CoreError::Invalid("组卷蓝图没有可编辑题目".into()));
    }
    let items = source_slots.into_iter().map(|slot| slot.output).collect();
    let (current_edition_public_id, current_revision, source_version, title) =
        current.as_ref().map_or_else(
            || {
                (
                    None,
                    0,
                    source.base_assessment_version_public_id.clone(),
                    source.title.clone(),
                )
            },
            |edition| {
                (
                    Some(edition.public_id.clone()),
                    edition.revision,
                    edition.assessment_version_public_id.clone(),
                    edition.title.clone(),
                )
            },
        );
    Ok(BlueprintPaperEditor {
        schema_version: PAPER_SCHEMA_VERSION,
        rule_version: PAPER_RULE_VERSION.into(),
        assembly_public_id: source.public_id,
        title,
        class_name: source.class_name,
        knowledge_map_title: source.knowledge_map_title,
        curriculum_node_title: source.curriculum_node_title,
        current_edition_public_id,
        current_revision,
        source_assessment_version_public_id: source_version,
        required_knowledge_node_public_ids: source.required_knowledge_node_public_ids,
        items,
        candidates,
        boundary_note:
            "换题只允许同题型、同分值的当前 L3/L4 题；确认会追加新作业版本和打印快照，不改变以后上传默认版，也不触碰历史成绩。"
                .into(),
    })
}

fn request_hash(input: &ConfirmBlueprintPaperRequest) -> CoreResult<String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "schema_version": PAPER_SCHEMA_VERSION,
        "rule_version": PAPER_RULE_VERSION,
        "assembly_public_id": input.assembly_public_id.trim(),
        "expected_source_assessment_version_public_id":
            input.expected_source_assessment_version_public_id.trim(),
        "title": input.title.trim(),
        "items": input.items
    }))
    .map_err(|error| CoreError::Parse(format!("打印版请求 hash 失败：{error}")))?;
    Ok(hashing::sha256_hex(&bytes))
}

fn file_safe_title(title: &str) -> String {
    let value = title
        .chars()
        .map(|character| {
            if matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            ) {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let trimmed = value.trim().trim_matches('.').trim();
    if trimmed.is_empty() {
        "课堂练习".into()
    } else {
        trimmed.chars().take(60).collect()
    }
}

fn load_edition_by_id(conn: &Connection, edition_id: i64) -> CoreResult<BlueprintPaperEdition> {
    let mut edition = conn
        .query_row(
            "SELECT edition.public_id,assembly.public_id,edition.revision,edition.title,
                    assessment.public_id,source.public_id,target.public_id,
                    previous.public_id,edition.item_set_hash,edition.question_html_sha256,
                    edition.answer_html_sha256,edition.state,edition.confirmed_by,
                    edition.confirmed_at
             FROM exam_blueprint_paper_editions_v2 edition
             JOIN exam_blueprint_assemblies_v2 assembly ON assembly.id=edition.assembly_id
             JOIN exam_assessments_v2 assessment ON assessment.id=assembly.assessment_id
             JOIN exam_assessment_versions_v2 source
               ON source.id=edition.source_assessment_version_id
             JOIN exam_assessment_versions_v2 target
               ON target.id=edition.assessment_version_id
             LEFT JOIN exam_blueprint_paper_editions_v2 previous
               ON previous.id=edition.supersedes_edition_id
             WHERE edition.id=?1",
            [edition_id],
            |row| {
                let title: String = row.get(3)?;
                let safe_title = file_safe_title(&title);
                Ok(BlueprintPaperEdition {
                    public_id: row.get(0)?,
                    assembly_public_id: row.get(1)?,
                    revision: row.get(2)?,
                    title,
                    assessment_public_id: row.get(4)?,
                    source_assessment_version_public_id: row.get(5)?,
                    assessment_version_public_id: row.get(6)?,
                    supersedes_edition_public_id: row.get(7)?,
                    item_set_hash: row.get(8)?,
                    question_html_sha256: row.get(9)?,
                    answer_html_sha256: row.get(10)?,
                    suggested_question_file_name: format!(
                        "{safe_title}-题卷-第{}版.html",
                        row.get::<_, i64>(2)?
                    ),
                    suggested_answer_file_name: format!(
                        "{safe_title}-答案-第{}版.html",
                        row.get::<_, i64>(2)?
                    ),
                    state: row.get(11)?,
                    confirmed_by: row.get(12)?,
                    confirmed_at: row.get(13)?,
                    items: Vec::new(),
                })
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("试卷打印版".into()))?;
    let mut statement = conn.prepare(
        "SELECT item.source_slot_order_index,item.order_index,version.public_id,
                version.question_type,version.stem,version.material_text,item.score,
                item.page_break_before
         FROM exam_blueprint_paper_items_v2 item
         JOIN k1_question_versions version ON version.id=item.question_version_id
         WHERE item.edition_id=?1 ORDER BY item.order_index,item.id",
    )?;
    edition.items = statement
        .query_map([edition_id], |row| {
            Ok(BlueprintPaperEditionItem {
                source_slot_order_index: row.get(0)?,
                order_index: row.get(1)?,
                question_version_public_id: row.get(2)?,
                question_type: row.get(3)?,
                stem: row.get(4)?,
                material_text: row.get(5)?,
                score: row.get(6)?,
                page_break_before: row.get::<_, i64>(7)? == 1,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(edition)
}

fn load_edition_by_request_key(
    conn: &Connection,
    request_key: &str,
    expected_hash: &str,
) -> CoreResult<Option<BlueprintPaperEdition>> {
    let existing = conn
        .query_row(
            "SELECT id,request_hash FROM exam_blueprint_paper_editions_v2
             WHERE request_key=?1",
            [request_key],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    match existing {
        Some((id, stored_hash)) if stored_hash == expected_hash => {
            Ok(Some(load_edition_by_id(conn, id)?))
        }
        Some(_) => Err(CoreError::Invalid(
            "同一请求标识已用于不同的试卷打印版".into(),
        )),
        None => Ok(None),
    }
}

fn html_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn value_strings(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::String(value)) => vec![value.clone()],
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

fn answer_summary(question: &PrintableQuestion) -> String {
    let value = serde_json::from_str::<Value>(&question.answer_json).unwrap_or(Value::Null);
    if let Some(labels) = value.get("correct_labels") {
        let values = value_strings(Some(labels));
        if !values.is_empty() {
            return values.join("、");
        }
    }
    if let Some(correct) = value.get("correct").and_then(Value::as_bool) {
        return if correct { "正确" } else { "错误" }.into();
    }
    for key in ["reference_answer", "answer"] {
        if let Some(answer) = value.get(key).and_then(Value::as_str) {
            return answer.to_string();
        }
    }
    for key in ["answers", "canonical_answers"] {
        let values = value_strings(value.get(key));
        if !values.is_empty() {
            return values.join("、");
        }
    }
    if !question.slots.is_empty() {
        return question.slots.join("；");
    }
    question.answer_json.clone()
}

fn load_printable_question(
    conn: &Connection,
    candidate: &super::blueprint_assembly::InternalCandidate,
    order_index: i64,
    score: f64,
    page_break_before: bool,
) -> CoreResult<PrintableQuestion> {
    let answer_json: String = conn.query_row(
        "SELECT answer_json FROM k1_answer_key_versions WHERE id=?1",
        [candidate.answer_key_version_id],
        |row| row.get(0),
    )?;
    let options = {
        let mut statement = conn.prepare(
            "SELECT label,content FROM k1_question_options
             WHERE question_version_id=?1 ORDER BY order_index,id",
        )?;
        let rows = statement
            .query_map([candidate.question_version_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    let slots = {
        let mut statement = conn.prepare(
            "SELECT canonical_answers_json FROM k1_answer_slots
             WHERE answer_key_version_id=?1 ORDER BY order_index,id",
        )?;
        let rows = statement
            .query_map([candidate.answer_key_version_id], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows.into_iter()
            .map(|json| {
                let value = serde_json::from_str::<Value>(&json).unwrap_or(Value::Null);
                let mut values = value_strings(value.get("answers"));
                if values.is_empty() {
                    values = value_strings(value.get("canonical_answers"));
                }
                if values.is_empty() {
                    json
                } else {
                    values.join(" / ")
                }
            })
            .collect()
    };
    let rubric_points = {
        let mut statement = conn.prepare(
            "SELECT canonical_text,max_score FROM k1_rubric_points
             WHERE rubric_version_id=?1 ORDER BY order_index,id",
        )?;
        let rows = statement
            .query_map([candidate.rubric_version_id], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        rows
    };
    Ok(PrintableQuestion {
        order_index,
        question_type: candidate.output.question_type.clone(),
        stem: candidate.output.stem.clone(),
        material_text: candidate.output.material_text.clone(),
        score,
        page_break_before,
        options,
        answer_json,
        slots,
        rubric_points,
    })
}

fn render_paper_html(
    title: &str,
    class_name: &str,
    revision: i64,
    confirmed_at: &str,
    questions: &[PrintableQuestion],
    include_answers: bool,
) -> String {
    let document_kind = if include_answers {
        "答案与评分参考"
    } else {
        "题卷"
    };
    let total_score = questions.iter().map(|item| item.score).sum::<f64>();
    let mut html = String::from(
        "<!doctype html><html lang=\"zh-CN\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>",
    );
    html.push_str(&html_escape(&format!("{title}-{document_kind}")));
    html.push_str(
        "</title><style>
        :root{font-family:\"Songti SC\",\"SimSun\",\"Noto Serif CJK SC\",serif;color:#111;background:#fff}
        *{box-sizing:border-box}body{margin:0;background:#eee}.paper{width:210mm;min-height:297mm;margin:10mm auto;padding:16mm 18mm;background:#fff}
        h1{margin:0;text-align:center;font-size:22px}.meta{display:flex;justify-content:space-between;margin:8px 0 18px;padding-bottom:8px;border-bottom:1px solid #222;font-size:12px}
        .student{display:flex;gap:28px;margin:12px 0 20px;font-size:14px}.line{display:inline-block;min-width:110px;border-bottom:1px solid #222}
        .question{margin:0 0 18px;break-inside:avoid}.question.page-break{break-before:page}.head{display:flex;gap:8px;align-items:flex-start}.no{font-weight:700}.stem{flex:1;white-space:pre-wrap}.score{white-space:nowrap}
        .material{margin:9px 0;padding:9px 12px;border-left:3px solid #888;background:#f6f6f6;white-space:pre-wrap;font-size:13px}
        ol.options{margin:8px 0 0;padding-left:28px}.options li{margin:4px 0}.answer-space{margin:10px 0;border-bottom:1px solid #aaa;height:22px}.answer-space.long{height:96px;background:repeating-linear-gradient(to bottom,transparent 0,transparent 23px,#bbb 24px)}
        .answer{margin:10px 0;padding:10px 12px;border:1px solid #999;background:#fafafa;font-size:13px}.rubric{margin:8px 0 0;padding-left:22px}.rubric li{margin:4px 0}.foot{margin-top:28px;padding-top:8px;border-top:1px solid #bbb;color:#666;font-size:10px}
        @page{size:A4;margin:14mm}@media print{body{background:#fff}.paper{width:auto;min-height:0;margin:0;padding:0}.student{display:flex}}
        </style></head><body><main class=\"paper\">",
    );
    write!(
        html,
        "<h1>{}</h1><div class=\"meta\"><span>{}</span><span>第 {} 版 · 总分 {:.1}</span></div>",
        html_escape(title),
        html_escape(class_name),
        revision,
        total_score
    )
    .expect("writing to String cannot fail");
    if !include_answers {
        html.push_str("<div class=\"student\"><span>姓名：<i class=\"line\"></i></span><span>学号：<i class=\"line\"></i></span><span>得分：<i class=\"line\"></i></span></div>");
    } else {
        html.push_str("<div class=\"student\"><b>教师核对用</b><span>请以本版冻结答案和评分点为准</span></div>");
    }
    for question in questions {
        let page_break = if question.page_break_before {
            " page-break"
        } else {
            ""
        };
        write!(
            html,
            "<section class=\"question{}\"><div class=\"head\"><span class=\"no\">{}.</span><div class=\"stem\">{}</div><span class=\"score\">（{:.1} 分）</span></div>",
            page_break,
            question.order_index + 1,
            html_escape(&question.stem),
            question.score
        )
        .expect("writing to String cannot fail");
        if let Some(material) = question.material_text.as_deref() {
            write!(
                html,
                "<div class=\"material\">{}</div>",
                html_escape(material)
            )
            .expect("writing to String cannot fail");
        }
        if !question.options.is_empty() {
            html.push_str("<ol class=\"options\">");
            for (label, content) in &question.options {
                write!(
                    html,
                    "<li><b>{}.</b> {}</li>",
                    html_escape(label),
                    html_escape(content)
                )
                .expect("writing to String cannot fail");
            }
            html.push_str("</ol>");
        }
        if include_answers {
            write!(
                html,
                "<div class=\"answer\"><b>答案：</b>{}</div>",
                html_escape(&answer_summary(question))
            )
            .expect("writing to String cannot fail");
            if !question.rubric_points.is_empty() {
                html.push_str("<ol class=\"rubric\">");
                for (point, score) in &question.rubric_points {
                    write!(html, "<li>{}（{:.1} 分）</li>", html_escape(point), score)
                        .expect("writing to String cannot fail");
                }
                html.push_str("</ol>");
            }
        } else {
            let long = question.question_type == "short_answer";
            html.push_str(if long {
                "<div class=\"answer-space long\"></div>"
            } else {
                "<div class=\"answer-space\"></div>"
            });
        }
        html.push_str("</section>");
    }
    write!(
        html,
        "<div class=\"foot\">本文件由老师于 {} 明确冻结生成；不含学生作答，不会自动布置、上传或发布成绩。打印时请选择 A4、100% 比例。</div></main></body></html>",
        html_escape(confirmed_at)
    )
    .expect("writing to String cannot fail");
    html
}

pub fn confirm_blueprint_paper(
    conn: &mut Connection,
    input: &ConfirmBlueprintPaperRequest,
) -> CoreResult<BlueprintPaperEdition> {
    required(&input.request_key, "请求标识")?;
    required(&input.assembly_public_id, "蓝图")?;
    required(
        &input.expected_source_assessment_version_public_id,
        "来源作业版本",
    )?;
    required(&input.title, "试卷名称")?;
    required(&input.confirmed_by, "确认人")?;
    if input.title.trim().chars().count() > 100 {
        return Err(CoreError::Invalid("试卷名称不能超过 100 个字符".into()));
    }
    let hash = request_hash(input)?;
    if let Some(existing) = load_edition_by_request_key(conn, input.request_key.trim(), &hash)? {
        return Ok(existing);
    }

    let source = load_assembly_source(conn, &input.assembly_public_id)?;
    let current = load_current_edition(conn, source.id)?;
    let current_source_version_id = current
        .as_ref()
        .map(|edition| edition.assessment_version_id)
        .unwrap_or(source.base_assessment_version_id);
    let current_source_version_public_id = current
        .as_ref()
        .map(|edition| edition.assessment_version_public_id.as_str())
        .unwrap_or(source.base_assessment_version_public_id.as_str());
    if current_source_version_public_id != input.expected_source_assessment_version_public_id.trim()
    {
        return Err(CoreError::Invalid(
            "试卷已有更新版本，请重新打开后再确认".into(),
        ));
    }
    let source_slots = load_source_slots(conn, &source, current.as_ref())?;
    if input.items.len() != source_slots.len() {
        return Err(CoreError::Invalid("必须保留蓝图中的全部题目槽位".into()));
    }
    let source_by_slot = source_slots
        .iter()
        .map(|slot| (slot.output.source_slot_order_index, slot))
        .collect::<BTreeMap<_, _>>();
    let supplied_slots = input
        .items
        .iter()
        .map(|item| item.source_slot_order_index)
        .collect::<BTreeSet<_>>();
    if supplied_slots.len() != input.items.len()
        || supplied_slots != source_by_slot.keys().copied().collect()
    {
        return Err(CoreError::Invalid(
            "题目槽位缺失、重复或不属于当前蓝图".into(),
        ));
    }
    if input
        .items
        .first()
        .is_some_and(|item| item.page_break_before)
    {
        return Err(CoreError::Invalid("第一题不能从新页开始".into()));
    }
    let scope = source_scope(conn, &source)?;
    let candidates = load_candidates(conn, &scope)?;
    let candidate_by_public_id = candidates
        .iter()
        .map(|candidate| {
            (
                candidate.output.question_version_public_id.as_str(),
                candidate,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let selected_ids = input
        .items
        .iter()
        .map(|item| item.question_version_public_id.trim())
        .collect::<BTreeSet<_>>();
    if selected_ids.len() != input.items.len() {
        return Err(CoreError::Invalid("同一试卷不能重复使用同一题目".into()));
    }
    let mut selected = Vec::with_capacity(input.items.len());
    let mut covered = BTreeSet::new();
    for (order_index, item) in input.items.iter().enumerate() {
        let slot = source_by_slot[&item.source_slot_order_index];
        let requested_public_id = item.question_version_public_id.trim();
        let candidate = if requested_public_id == slot.output.question_version_public_id {
            let mut candidate = candidate_by_public_id
                .get(requested_public_id)
                .copied()
                .cloned()
                .unwrap_or_else(|| InternalCandidate {
                    output: BlueprintCandidate {
                        question_version_public_id: slot.output.question_version_public_id.clone(),
                        question_type: slot.output.question_type.clone(),
                        stem: slot.output.stem.clone(),
                        material_text: slot.output.material_text.clone(),
                        score: slot.output.score,
                        quality_level: "frozen".into(),
                        knowledge_nodes: Vec::new(),
                        ability_dimensions: Vec::new(),
                        explanation: "沿用上一版冻结题目与评价版本".into(),
                    },
                    question_version_id: slot.question_version_id,
                    answer_key_version_id: slot.answer_key_version_id,
                    rubric_version_id: slot.rubric_version_id,
                    link_set_id: slot.link_set_id,
                });
            candidate.question_version_id = slot.question_version_id;
            candidate.answer_key_version_id = slot.answer_key_version_id;
            candidate.rubric_version_id = slot.rubric_version_id;
            candidate.link_set_id = slot.link_set_id;
            candidate.output.question_type = slot.output.question_type.clone();
            candidate.output.stem = slot.output.stem.clone();
            candidate.output.material_text = slot.output.material_text.clone();
            candidate.output.score = slot.output.score;
            for public_id in &slot.knowledge_node_public_ids {
                covered.insert(public_id.clone());
            }
            candidate
        } else {
            let candidate = candidate_by_public_id
                .get(requested_public_id)
                .copied()
                .cloned()
                .ok_or_else(|| CoreError::Invalid("所选替换题已不再满足当前蓝图质量闸门".into()))?;
            for knowledge in &candidate.output.knowledge_nodes {
                covered.insert(knowledge.public_id.clone());
            }
            candidate
        };
        if candidate.output.question_type != slot.output.question_type
            || (candidate.output.score - slot.output.score).abs() > 0.000_001
        {
            return Err(CoreError::Invalid(format!(
                "第 {} 个槽位只能换成同题型、同分值题目",
                item.source_slot_order_index + 1
            )));
        }
        selected.push((
            item.source_slot_order_index,
            order_index as i64,
            item.page_break_before,
            slot.output.score,
            candidate,
        ));
    }
    for required_knowledge in &source.required_knowledge_node_public_ids {
        if !covered.contains(required_knowledge.as_str()) {
            return Err(CoreError::Invalid(format!(
                "换题后未覆盖蓝图必选知识点：{required_knowledge}"
            )));
        }
    }

    let now = time::utc_now_rfc3339();
    let next_edition_revision = current.as_ref().map_or(1, |value| value.revision + 1);
    let printable = selected
        .iter()
        .map(|(_, order_index, page_break_before, score, candidate)| {
            load_printable_question(conn, candidate, *order_index, *score, *page_break_before)
        })
        .collect::<CoreResult<Vec<_>>>()?;
    let question_html = render_paper_html(
        input.title.trim(),
        &source.class_name,
        next_edition_revision,
        &now,
        &printable,
        false,
    );
    let answer_html = render_paper_html(
        input.title.trim(),
        &source.class_name,
        next_edition_revision,
        &now,
        &printable,
        true,
    );
    let question_html_sha256 = hashing::sha256_hex(question_html.as_bytes());
    let answer_html_sha256 = hashing::sha256_hex(answer_html.as_bytes());
    let layout_json = serde_json::json!({
        "schema_version": PAPER_SCHEMA_VERSION,
        "paper_size": "A4",
        "orientation": "portrait",
        "items": input.items
    })
    .to_string();
    let edition_item_set_hash = hashing::sha256_hex(
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": PAPER_SCHEMA_VERSION,
            "assembly_public_id": source.public_id,
            "items": input.items
        }))
        .map_err(|error| CoreError::Parse(format!("试卷题目 hash 失败：{error}")))?,
    );
    let template_version: Option<String> = conn.query_row(
        "SELECT template_version FROM exam_assessment_versions_v2 WHERE id=?1",
        [current_source_version_id],
        |row| row.get(0),
    )?;
    let edition_public_id = ids::new_public_id();
    let assessment_version_public_id = ids::new_public_id();
    let tx = conn.transaction()?;
    let assessment_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_assessment_versions_v2
         WHERE assessment_id=?1",
        [source.assessment_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO exam_assessment_versions_v2
         (public_id,assessment_id,revision,template_version,state,
          supersedes_version_id,created_at)
         VALUES (?1,?2,?3,?4,'draft',?5,?6)",
        params![
            assessment_version_public_id,
            source.assessment_id,
            assessment_revision,
            template_version
                .as_deref()
                .unwrap_or(PAPER_TEMPLATE_VERSION),
            current_source_version_id,
            now
        ],
    )?;
    let assessment_version_id = tx.last_insert_rowid();
    let mut assessment_hash_items = Vec::with_capacity(selected.len());
    let mut created_items = Vec::with_capacity(selected.len());
    for (source_slot, order_index, page_break_before, score, candidate) in &selected {
        let presentation = serde_json::json!({
            "schema_version": PAPER_SCHEMA_VERSION,
            "source": "k1_blueprint_paper",
            "paper_edition_revision": next_edition_revision,
            "question_no": order_index + 1,
            "source_slot_order_index": source_slot,
            "question_version_public_id": candidate.output.question_version_public_id
        })
        .to_string();
        let item = add_assessment_item(
            &tx,
            assessment_version_id,
            &NewAssessmentItem {
                question_version_id: candidate.question_version_id,
                answer_key_version_id: candidate.answer_key_version_id,
                rubric_version_id: candidate.rubric_version_id,
                link_set_id: candidate.link_set_id,
                order_index: *order_index,
                score: *score,
                option_order_json: None,
                presentation_snapshot_json: &presentation,
            },
        )?;
        assessment_hash_items.push(AssessmentItemHash {
            item_id: item.id,
            question_version_id: candidate.question_version_id,
            answer_key_version_id: candidate.answer_key_version_id,
            rubric_version_id: candidate.rubric_version_id,
            link_set_id: candidate.link_set_id,
            order_index: *order_index,
            score_millis: (*score * 1000.0).round() as i64,
        });
        created_items.push((
            item.id,
            *source_slot,
            *order_index,
            *page_break_before,
            *score,
            candidate.clone(),
        ));
    }
    let assessment_item_set_hash = hashing::sha256_hex(
        &serde_json::to_vec(&assessment_hash_items)
            .map_err(|error| CoreError::Parse(format!("作业题目 hash 失败：{error}")))?,
    );
    tx.execute(
        "UPDATE exam_assessment_versions_v2
         SET item_set_hash=?1,state='confirmed',confirmed_by=?2,confirmed_at=?3
         WHERE id=?4 AND state='draft'",
        params![
            assessment_item_set_hash,
            input.confirmed_by.trim(),
            now,
            assessment_version_id
        ],
    )?;
    tx.execute(
        "INSERT INTO exam_blueprint_paper_editions_v2
         (public_id,request_key,request_hash,assembly_id,revision,
          source_assessment_version_id,assessment_version_id,supersedes_edition_id,
          title,layout_json,item_set_hash,question_html_sha256,answer_html_sha256,
          question_html,answer_html,state,confirmed_by,confirmed_at,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,
                 'confirmed',?16,?17,?17)",
        params![
            edition_public_id,
            input.request_key.trim(),
            hash,
            source.id,
            next_edition_revision,
            current_source_version_id,
            assessment_version_id,
            current.as_ref().map(|value| value.id),
            input.title.trim(),
            layout_json,
            edition_item_set_hash,
            question_html_sha256,
            answer_html_sha256,
            question_html,
            answer_html,
            input.confirmed_by.trim(),
            now
        ],
    )?;
    let edition_id = tx.last_insert_rowid();
    for (assessment_item_id, source_slot, order_index, page_break, score, candidate) in
        created_items
    {
        tx.execute(
            "INSERT INTO exam_blueprint_paper_items_v2
             (edition_id,source_slot_order_index,assessment_item_id,question_version_id,
              answer_key_version_id,rubric_version_id,link_set_id,order_index,score,
              page_break_before,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                edition_id,
                source_slot,
                assessment_item_id,
                candidate.question_version_id,
                candidate.answer_key_version_id,
                candidate.rubric_version_id,
                candidate.link_set_id,
                order_index,
                score,
                i64::from(page_break),
                now
            ],
        )?;
    }
    tx.execute(
        "UPDATE exam_assessments_v2 SET updated_at=?1 WHERE id=?2",
        params![now, source.assessment_id],
    )?;
    let event_payload = serde_json::json!({
        "schema_version": PAPER_SCHEMA_VERSION,
        "rule_version": PAPER_RULE_VERSION,
        "paper_edition_public_id": edition_public_id,
        "assembly_public_id": source.public_id,
        "assessment_public_id": source.assessment_public_id,
        "source_assessment_version_public_id": current_source_version_public_id,
        "assessment_version_public_id": assessment_version_public_id,
        "revision": next_edition_revision,
        "item_count": input.items.len(),
        "default_for_future_intake": false,
        "changes_historical_attempts": false,
        "changes_grades": false,
        "changes_learning_evidence": false
    })
    .to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("exam:outbox:blueprint-paper:{edition_public_id}"),
            event_type: "k1_blueprint_paper_edition_confirmed",
            event_version: 1,
            aggregate_type: "exam_blueprint_paper_edition",
            aggregate_id: &edition_public_id,
            aggregate_revision: next_edition_revision,
            payload_json: &event_payload,
            occurred_at: &now,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("exam:audit:blueprint-paper:{edition_public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.confirmed_by.trim()),
            action: "exam.blueprint_paper_edition.confirmed",
            object_type: "exam_blueprint_paper_edition",
            object_id: &edition_public_id,
            object_revision: Some(next_edition_revision),
            note: Some("老师明确确认换题/排序并冻结本地题卷与答案卷；未切换未来上传默认版"),
            meta_json: Some(&event_payload),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    load_edition_by_id(conn, edition_id)
}

fn load_edition_html(
    conn: &Connection,
    edition_public_id: &str,
    export_kind: &str,
) -> CoreResult<(i64, String, String, String)> {
    let (id, question_html, question_hash, answer_html, answer_hash) = conn
        .query_row(
            "SELECT id,question_html,question_html_sha256,answer_html,answer_html_sha256
             FROM exam_blueprint_paper_editions_v2 WHERE public_id=?1",
            [edition_public_id.trim()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("试卷打印版".into()))?;
    match export_kind {
        "question" => Ok((id, question_html, question_hash, "question".into())),
        "answer" => Ok((id, answer_html, answer_hash, "answer".into())),
        _ => Err(CoreError::Invalid("导出类型必须是题卷或答案卷".into())),
    }
}

fn write_private_html(path: &Path, bytes: &[u8], edition_public_id: &str) -> CoreResult<()> {
    if !path.is_absolute() {
        return Err(CoreError::Invalid("导出位置必须是绝对路径".into()));
    }
    if path
        .extension()
        .and_then(|value| value.to_str())
        .is_none_or(|value| !value.eq_ignore_ascii_case("html"))
    {
        return Err(CoreError::Invalid("试卷必须使用 .html 扩展名".into()));
    }
    if path.exists() {
        return Err(CoreError::Invalid(
            "所选文件已存在，请在保存窗口中换一个名称".into(),
        ));
    }
    let parent = path
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| CoreError::Invalid("导出目录不存在".into()))?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CoreError::Invalid("导出文件名无效".into()))?;
    let temporary = parent.join(format!(
        ".{file_name}.{edition_public_id}.{}.tmp",
        ids::new_public_id()
    ));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| -> CoreResult<()> {
        let mut file = options
            .open(&temporary)
            .map_err(|error| CoreError::Io(format!("创建试卷临时文件失败：{error}")))?;
        file.write_all(bytes)
            .map_err(|error| CoreError::Io(format!("写入试卷文件失败：{error}")))?;
        file.sync_all()
            .map_err(|error| CoreError::Io(format!("刷新试卷文件失败：{error}")))?;
        std::fs::hard_link(&temporary, path)
            .map_err(|error| CoreError::Io(format!("保存试卷文件失败：{error}")))?;
        std::fs::remove_file(&temporary)
            .map_err(|error| CoreError::Io(format!("清理试卷临时文件失败：{error}")))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub fn write_blueprint_paper_html(
    conn: &Connection,
    edition_public_id: &str,
    export_kind: &str,
    output_path: &str,
) -> CoreResult<WrittenBlueprintPaper> {
    required(edition_public_id, "试卷打印版")?;
    let (_, html, stored_hash, normalized_kind) =
        load_edition_html(conn, edition_public_id, export_kind)?;
    let actual_hash = hashing::sha256_hex(html.as_bytes());
    if actual_hash != stored_hash {
        return Err(CoreError::Invalid("试卷打印快照内容校验失败".into()));
    }
    let path = Path::new(output_path);
    write_private_html(path, html.as_bytes(), edition_public_id)?;
    Ok(WrittenBlueprintPaper {
        edition_public_id: edition_public_id.trim().into(),
        export_kind: normalized_kind,
        file_name: path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_string(),
        byte_size: html.len() as i64,
        sha256: stored_hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::blueprint_assembly::{
        confirm_blueprint, preview_blueprint, ConfirmBlueprintRequest,
    };
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
        assembly_public_id: String,
        base_assessment_version_public_id: String,
        original_question_public_id: String,
        replacement_question_public_id: String,
        map_id: i64,
        knowledge_id: i64,
        ability_id: i64,
    }

    fn create_question_fixture(
        conn: &Connection,
        map_id: i64,
        knowledge_id: i64,
        ability_id: i64,
        question_type: &str,
        stem: &str,
        score: f64,
    ) -> String {
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
                question_type,
                stem,
                material_text: None,
                max_score: score,
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
                answer_json: r#"{"schema_version":1,"correct":true}"#,
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
                max_score: score,
                state: "confirmed",
                confirmed_by: Some("local_teacher"),
                supersedes_rubric_id: None,
                points: &[NewRubricPoint {
                    stable_id: None,
                    order_index: 0,
                    canonical_text: "符合标准答案",
                    allowed_paraphrases_json: None,
                    required_concepts_json: None,
                    max_score: score,
                }],
            },
        )
        .unwrap();
        let links = create_link_set(
            conn,
            version.id,
            map_id,
            1,
            "confirmed",
            Some("local_teacher"),
            None,
        )
        .unwrap();
        add_knowledge_link(
            conn,
            &NewKnowledgeLink {
                link_set_id: links.id,
                source_type: "question",
                source_public_id: &version.public_id,
                knowledge_node_id: knowledge_id,
                relation_type: "direct_assessment",
                confirmation_level: "teacher_confirmed",
                verified_by: Some("local_teacher"),
            },
        )
        .unwrap();
        add_ability_link(
            conn,
            &NewAbilityLink {
                link_set_id: links.id,
                source_type: "question",
                source_public_id: &version.public_id,
                ability_dimension_id: ability_id,
                evidence_strength: 0.5,
                response_mode: "recognition",
                confirmation_level: "teacher_confirmed",
                verified_by: Some("local_teacher"),
            },
        )
        .unwrap();
        promote_question_version(conn, version.id, "L3", "local_teacher", None).unwrap();
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
        let original_question_public_id = create_question_fixture(
            &conn,
            map.id,
            knowledge.id,
            ability.id,
            "true_false",
            "鸦片战争爆发于 1840 年。",
            1.0,
        );
        let replacement_question_public_id = create_question_fixture(
            &conn,
            map.id,
            knowledge.id,
            ability.id,
            "true_false",
            "鸦片战争发生于十九世纪。",
            1.0,
        );
        let preview_request = BlueprintPreviewRequest {
            class_id,
            knowledge_map_public_id: map.public_id.clone(),
            curriculum_node_public_id: Some(curriculum.public_id),
            total_score: 1.0,
            question_type_targets: vec![BlueprintQuestionTypeTarget {
                question_type: "true_false".into(),
                count: 1,
            }],
            required_knowledge_node_public_ids: vec![knowledge.public_id],
        };
        let preview = preview_blueprint(&conn, &preview_request).unwrap();
        let mut conn = conn;
        let assembly = confirm_blueprint(
            &mut conn,
            &ConfirmBlueprintRequest {
                request_key: "paper-fixture-blueprint".into(),
                title: "鸦片战争练习".into(),
                preview_request,
                expected_preview_hash: preview.preview_hash,
                selected_question_version_public_ids: vec![original_question_public_id.clone()],
                confirmed_by: "local_teacher".into(),
            },
        )
        .unwrap();
        Fixture {
            conn,
            assembly_public_id: assembly.public_id,
            base_assessment_version_public_id: assembly.assessment_version_public_id,
            original_question_public_id,
            replacement_question_public_id,
            map_id: map.id,
            knowledge_id: knowledge.id,
            ability_id: ability.id,
        }
    }

    fn confirm_request(
        fixture: &Fixture,
        request_key: &str,
        source_version: &str,
        question_public_id: &str,
    ) -> ConfirmBlueprintPaperRequest {
        ConfirmBlueprintPaperRequest {
            request_key: request_key.into(),
            assembly_public_id: fixture.assembly_public_id.clone(),
            expected_source_assessment_version_public_id: source_version.into(),
            title: "鸦片战争课堂练习".into(),
            items: vec![BlueprintPaperItemInput {
                source_slot_order_index: 0,
                question_version_public_id: question_public_id.into(),
                page_break_before: false,
            }],
            confirmed_by: "local_teacher".into(),
        }
    }

    #[test]
    fn teacher_replacement_freezes_new_version_without_changing_history_or_default() {
        let mut fixture = setup();
        let editor =
            load_blueprint_paper_editor(&fixture.conn, &fixture.assembly_public_id).unwrap();
        assert_eq!(editor.current_revision, 0);
        assert_eq!(
            editor.source_assessment_version_public_id,
            fixture.base_assessment_version_public_id
        );
        assert_eq!(editor.candidates.len(), 2);

        let request = confirm_request(
            &fixture,
            "paper-edition-1",
            &fixture.base_assessment_version_public_id,
            &fixture.replacement_question_public_id,
        );
        let edition = confirm_blueprint_paper(&mut fixture.conn, &request).unwrap();
        let repeated = confirm_blueprint_paper(&mut fixture.conn, &request).unwrap();
        assert_eq!(edition.public_id, repeated.public_id);
        assert_eq!(edition.revision, 1);
        assert_eq!(
            edition.items[0].question_version_public_id,
            fixture.replacement_question_public_id
        );
        assert_ne!(
            edition.assessment_version_public_id,
            fixture.base_assessment_version_public_id
        );

        let counts: (i64, i64, i64, i64, i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT
                   (SELECT COUNT(*) FROM exam_assessment_versions_v2),
                   (SELECT COUNT(*) FROM exam_blueprint_paper_editions_v2),
                   (SELECT COUNT(*) FROM exam_blueprint_paper_items_v2),
                   (SELECT COUNT(*) FROM exam_attempts_v2),
                   (SELECT COUNT(*) FROM exam_assessment_default_version_selections_v2),
                   (SELECT COUNT(*) FROM audit_events
                    WHERE action='exam.blueprint_paper_edition.confirmed'),
                   (SELECT COUNT(*) FROM outbox_events
                    WHERE event_type='k1_blueprint_paper_edition_confirmed')",
                [],
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
            )
            .unwrap();
        assert_eq!(counts, (2, 1, 1, 0, 0, 1, 1));
        let old_question: String = fixture
            .conn
            .query_row(
                "SELECT question.public_id
                 FROM exam_blueprint_items_v2 item
                 JOIN k1_question_versions question ON question.id=item.question_version_id",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_question, fixture.original_question_public_id);
        assert!(fixture
            .conn
            .execute(
                "UPDATE exam_blueprint_paper_editions_v2 SET title='篡改'",
                [],
            )
            .is_err());
        assert!(fixture
            .conn
            .execute("DELETE FROM exam_blueprint_paper_items_v2", [])
            .is_err());

        let refreshed =
            load_blueprint_paper_editor(&fixture.conn, &fixture.assembly_public_id).unwrap();
        assert_eq!(refreshed.current_revision, 1);
        assert_eq!(
            refreshed.items[0].question_version_public_id,
            fixture.replacement_question_public_id
        );
    }

    #[test]
    fn unchanged_question_keeps_frozen_answer_rubric_and_links() {
        let mut fixture = setup();
        let (question_version_id, frozen_answer_id, frozen_rubric_id, frozen_link_set_id): (
            i64,
            i64,
            i64,
            i64,
        ) = fixture
            .conn
            .query_row(
                "SELECT item.question_version_id,item.answer_key_version_id,
                        item.rubric_version_id,item.link_set_id
                 FROM exam_blueprint_items_v2 item",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        create_answer_key_version(
            &fixture.conn,
            &NewAnswerKeyVersion {
                question_version_id,
                revision: 2,
                answer_json: r#"{"schema_version":1,"correct":false}"#,
                state: "confirmed",
                confirmed_by: Some("local_teacher"),
                supersedes_answer_key_id: Some(frozen_answer_id),
                slots: &[],
            },
        )
        .unwrap();
        let request = confirm_request(
            &fixture,
            "paper-keep-frozen-refs",
            &fixture.base_assessment_version_public_id,
            &fixture.original_question_public_id,
        );
        let edition = confirm_blueprint_paper(&mut fixture.conn, &request).unwrap();
        let refs: (i64, i64, i64) = fixture
            .conn
            .query_row(
                "SELECT item.answer_key_version_id,item.rubric_version_id,item.link_set_id
                 FROM exam_blueprint_paper_items_v2 paper
                 JOIN exam_assessment_items_v2 item ON item.id=paper.assessment_item_id
                 JOIN exam_blueprint_paper_editions_v2 edition ON edition.id=paper.edition_id
                 WHERE edition.public_id=?1",
                [&edition.public_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            refs,
            (frozen_answer_id, frozen_rubric_id, frozen_link_set_id)
        );
        let answer_html: String = fixture
            .conn
            .query_row(
                "SELECT answer_html FROM exam_blueprint_paper_editions_v2
                 WHERE public_id=?1",
                [&edition.public_id],
                |row| row.get(0),
            )
            .unwrap();
        assert!(answer_html.contains("<b>答案：</b>正确"));
        assert!(!answer_html.contains("<b>答案：</b>错误"));
    }

    #[test]
    fn stale_source_and_incompatible_slot_are_rejected_without_half_version() {
        let mut fixture = setup();
        let incompatible = create_question_fixture(
            &fixture.conn,
            fixture.map_id,
            fixture.knowledge_id,
            fixture.ability_id,
            "true_false",
            "这是一道两分题。",
            2.0,
        );
        let wrong_score_request = confirm_request(
            &fixture,
            "paper-wrong-score",
            &fixture.base_assessment_version_public_id,
            &incompatible,
        );
        let error = confirm_blueprint_paper(&mut fixture.conn, &wrong_score_request).unwrap_err();
        assert!(error.to_string().contains("同题型、同分值"));
        let versions_after_reject: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_assessment_versions_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(versions_after_reject, 1);

        let valid_request = confirm_request(
            &fixture,
            "paper-valid",
            &fixture.base_assessment_version_public_id,
            &fixture.replacement_question_public_id,
        );
        let first = confirm_blueprint_paper(&mut fixture.conn, &valid_request).unwrap();
        let stale_request = confirm_request(
            &fixture,
            "paper-stale",
            &fixture.base_assessment_version_public_id,
            &fixture.original_question_public_id,
        );
        let stale = confirm_blueprint_paper(&mut fixture.conn, &stale_request).unwrap_err();
        assert!(stale.to_string().contains("已有更新版本"));
        let editor =
            load_blueprint_paper_editor(&fixture.conn, &fixture.assembly_public_id).unwrap();
        let second_request = confirm_request(
            &fixture,
            "paper-second",
            &editor.source_assessment_version_public_id,
            &fixture.original_question_public_id,
        );
        let second = confirm_blueprint_paper(&mut fixture.conn, &second_request).unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(
            second.supersedes_edition_public_id.as_deref(),
            Some(first.public_id.as_str())
        );
        let version_count: i64 = fixture
            .conn
            .query_row(
                "SELECT COUNT(*) FROM exam_assessment_versions_v2",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(version_count, 3);
    }

    #[test]
    fn frozen_question_and_answer_html_export_safely() {
        let mut fixture = setup();
        let export_request = confirm_request(
            &fixture,
            "paper-export",
            &fixture.base_assessment_version_public_id,
            &fixture.replacement_question_public_id,
        );
        let edition = confirm_blueprint_paper(&mut fixture.conn, &export_request).unwrap();
        let (question_html, answer_html): (String, String) = fixture
            .conn
            .query_row(
                "SELECT question_html,answer_html
                 FROM exam_blueprint_paper_editions_v2 WHERE public_id=?1",
                [&edition.public_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert!(question_html.contains("鸦片战争发生于十九世纪"));
        assert!(!question_html.contains("<script"));
        assert!(!question_html.contains("<b>答案：</b>"));
        assert!(answer_html.contains("<b>答案：</b>正确"));
        assert!(answer_html.contains("符合标准答案"));

        let root = std::env::temp_dir().join(format!("jiaofu-paper-{}", ids::new_public_id()));
        std::fs::create_dir_all(&root).unwrap();
        let output = root.join(&edition.suggested_question_file_name);
        let written = write_blueprint_paper_html(
            &fixture.conn,
            &edition.public_id,
            "question",
            output.to_str().unwrap(),
        )
        .unwrap();
        assert_eq!(written.sha256, edition.question_html_sha256);
        assert_eq!(std::fs::read_to_string(&output).unwrap(), question_html);
        assert!(write_blueprint_paper_html(
            &fixture.conn,
            &edition.public_id,
            "question",
            output.to_str().unwrap()
        )
        .unwrap_err()
        .to_string()
        .contains("已存在"));
        assert!(write_blueprint_paper_html(
            &fixture.conn,
            &edition.public_id,
            "answer",
            "relative.html"
        )
        .unwrap_err()
        .to_string()
        .contains("绝对路径"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&output).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
