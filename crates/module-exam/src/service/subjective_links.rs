//! 答题卡主观题的知识/能力链接复核。
//!
//! 老师确认后追加新的 K1 link set 与未来 assessment version。当前 attempt、评分、
//! 发布及已激活学习证据不重绑；正式图谱仍只消费老师确认链接与老师终审结果。

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit;
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveKnowledgeOption {
    pub id: i64,
    pub public_id: String,
    pub code: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveAbilityOption {
    pub id: i64,
    pub public_id: String,
    pub code: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveKnowledgeLinkView {
    pub knowledge_node_id: i64,
    pub knowledge_node_public_id: String,
    pub knowledge_title: String,
    pub relation_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveAbilityLinkView {
    pub ability_dimension_id: i64,
    pub ability_dimension_public_id: String,
    pub ability_title: String,
    pub evidence_strength: f64,
    pub response_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveLinkSourceView {
    pub source_type: String,
    pub source_public_id: String,
    pub stable_id: String,
    pub order_index: i64,
    pub label: String,
    pub max_score: f64,
    pub knowledge_links: Vec<SubjectiveKnowledgeLinkView>,
    pub ability_links: Vec<SubjectiveAbilityLinkView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveLinkEditor {
    pub source_assessment_item_id: i64,
    pub assessment_id: i64,
    pub base_assessment_version_id: i64,
    pub base_assessment_revision: i64,
    pub base_assessment_item_id: i64,
    pub question_version_id: i64,
    pub question_type: String,
    pub question_no: String,
    pub question_stem: String,
    pub link_set_id: i64,
    pub link_set_revision: i64,
    pub sources: Vec<SubjectiveLinkSourceView>,
    pub knowledge_options: Vec<SubjectiveKnowledgeOption>,
    pub ability_options: Vec<SubjectiveAbilityOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveKnowledgeLinkInput {
    pub knowledge_node_id: i64,
    pub relation_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveAbilityLinkInput {
    pub ability_dimension_id: i64,
    pub evidence_strength: f64,
    pub response_mode: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveSourceLinkInput {
    pub source_type: String,
    pub source_public_id: String,
    pub knowledge_links: Vec<SubjectiveKnowledgeLinkInput>,
    pub ability_links: Vec<SubjectiveAbilityLinkInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubjectiveLinkEditResult {
    pub outcome: String,
    pub edit_id: Option<i64>,
    pub adopted_assessment_version_id: i64,
    pub adopted_assessment_revision: i64,
    pub adopted_link_set_id: i64,
    pub adopted_link_set_revision: i64,
    pub knowledge_link_count: i64,
    pub ability_link_count: i64,
    pub current_attempts_unchanged: bool,
    pub current_publications_unchanged: bool,
}

#[derive(Debug)]
struct AssessmentItemClone {
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    order_index: i64,
    score: f64,
    option_order_json: Option<String>,
    presentation_snapshot_json: String,
}

#[derive(Debug)]
struct SubjectiveBaseScope {
    assessment_id: i64,
    version_id: i64,
    version_revision: i64,
    item_id: i64,
    question_version_id: i64,
    question_type: String,
    question_stem: String,
    question_no: String,
    link_set_id: i64,
}

#[derive(Serialize)]
struct AssessmentHashItem {
    item_id: i64,
    question_version_id: i64,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
    order_index: i64,
    score_millis: i64,
}

#[derive(Serialize)]
struct CanonicalSource<'a> {
    source_type: &'a str,
    source_public_id: &'a str,
    knowledge_links: Vec<(i64, &'a str)>,
    ability_links: Vec<(i64, i64, &'a str)>,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("{label}不能为空")))
    } else {
        Ok(())
    }
}

fn base_scope(
    conn: &Connection,
    source_assessment_item_id: i64,
) -> CoreResult<SubjectiveBaseScope> {
    let source: Option<(i64, i64)> = conn
        .query_row(
            "SELECT v.assessment_id,i.question_version_id
             FROM exam_assessment_items_v2 i
             JOIN exam_assessment_versions_v2 v ON v.id=i.assessment_version_id
             WHERE i.id=?1 AND i.state='active'",
            [source_assessment_item_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let Some((assessment_id, question_version_id)) = source else {
        return Err(CoreError::NotFound(format!(
            "assessment_item#{source_assessment_item_id}"
        )));
    };
    let scope: Option<(i64, i64, i64, i64, String, String, String)> = conn
        .query_row(
            "SELECT version.id,version.revision,item.id,item.link_set_id,
                    question.question_type,question.stem,
                    COALESCE(CAST(json_extract(item.presentation_snapshot_json,'$.question_no') AS TEXT),
                             CAST(item.order_index+1 AS TEXT))
             FROM exam_assessment_versions_v2 version
             JOIN exam_assessment_items_v2 item ON item.assessment_version_id=version.id
             JOIN k1_question_versions question ON question.id=item.question_version_id
             WHERE version.assessment_id=?1 AND version.state='confirmed'
               AND item.question_version_id=?2 AND item.state='active'
             ORDER BY version.revision DESC LIMIT 1",
            (assessment_id, question_version_id),
            |row| {
                Ok((
                    row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?,
                    row.get(5)?, row.get(6)?,
                ))
            },
        )
        .optional()?;
    let Some((
        version_id,
        version_revision,
        item_id,
        link_set_id,
        question_type,
        stem,
        question_no,
    )) = scope
    else {
        return Err(CoreError::Invalid(
            "最新确认作业版本已不包含该题，请先在题库中处理".into(),
        ));
    };
    if !matches!(question_type.as_str(), "fill_blank" | "short_answer") {
        return Err(CoreError::Invalid(
            "只有填空和简答题可使用本链接编辑器".into(),
        ));
    }
    Ok(SubjectiveBaseScope {
        assessment_id,
        version_id,
        version_revision,
        item_id,
        question_version_id,
        question_type,
        question_stem: stem,
        question_no,
        link_set_id,
    })
}

fn link_sources(
    conn: &Connection,
    question_type: &str,
    answer_key_version_id: i64,
    rubric_version_id: i64,
    link_set_id: i64,
) -> CoreResult<Vec<SubjectiveLinkSourceView>> {
    let source_type = if question_type == "fill_blank" {
        "answer_slot"
    } else {
        "rubric_point"
    };
    let sql = if question_type == "fill_blank" {
        "SELECT public_id,stable_id,order_index,
                '第'||(order_index+1)||'空',max_score
         FROM k1_answer_slots WHERE answer_key_version_id=?1 ORDER BY order_index,id"
    } else {
        "SELECT public_id,stable_id,order_index,canonical_text,max_score
         FROM k1_rubric_points WHERE rubric_version_id=?1 ORDER BY order_index,id"
    };
    let version_id = if question_type == "fill_blank" {
        answer_key_version_id
    } else {
        rubric_version_id
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([version_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, f64>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    if rows.is_empty() {
        return Err(CoreError::Invalid(
            "题目尚未建立可链接的答案槽位或评分点".into(),
        ));
    }
    let mut result = Vec::with_capacity(rows.len());
    for (public_id, stable_id, order_index, label, max_score) in rows {
        let mut knowledge_stmt = conn.prepare(
            "SELECT link.knowledge_node_id,node.public_id,node.title,link.relation_type
             FROM k1_knowledge_links link
             JOIN k1_knowledge_nodes node ON node.id=link.knowledge_node_id
             WHERE link.link_set_id=?1 AND link.source_type=?2
               AND link.source_public_id=?3 AND link.confirmation_level='teacher_confirmed'
             ORDER BY node.order_index,node.id,link.id",
        )?;
        let knowledge_links = knowledge_stmt
            .query_map((link_set_id, source_type, &public_id), |row| {
                Ok(SubjectiveKnowledgeLinkView {
                    knowledge_node_id: row.get(0)?,
                    knowledge_node_public_id: row.get(1)?,
                    knowledge_title: row.get(2)?,
                    relation_type: row.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(knowledge_stmt);
        let mut ability_stmt = conn.prepare(
            "SELECT link.ability_dimension_id,dimension.public_id,dimension.title,
                    link.evidence_strength,link.response_mode
             FROM k1_ability_links link
             JOIN k1_ability_dimensions dimension ON dimension.id=link.ability_dimension_id
             WHERE link.link_set_id=?1 AND link.source_type=?2
               AND link.source_public_id=?3 AND link.confirmation_level='teacher_confirmed'
             ORDER BY dimension.code,dimension.id,link.id",
        )?;
        let ability_links = ability_stmt
            .query_map((link_set_id, source_type, &public_id), |row| {
                Ok(SubjectiveAbilityLinkView {
                    ability_dimension_id: row.get(0)?,
                    ability_dimension_public_id: row.get(1)?,
                    ability_title: row.get(2)?,
                    evidence_strength: row.get(3)?,
                    response_mode: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        result.push(SubjectiveLinkSourceView {
            source_type: source_type.into(),
            source_public_id: public_id,
            stable_id,
            order_index,
            label,
            max_score,
            knowledge_links,
            ability_links,
        });
    }
    Ok(result)
}

pub fn get_editor(
    conn: &Connection,
    source_assessment_item_id: i64,
) -> CoreResult<SubjectiveLinkEditor> {
    let scope = base_scope(conn, source_assessment_item_id)?;
    let (link_set_revision, knowledge_map_id, answer_key_version_id, rubric_version_id, subject_id):
        (i64, i64, i64, i64, i64) = conn.query_row(
        "SELECT links.revision,links.knowledge_map_id,item.answer_key_version_id,
                item.rubric_version_id,edition.subject_id
         FROM exam_assessment_items_v2 item
         JOIN k1_link_sets links ON links.id=item.link_set_id
         JOIN k1_knowledge_maps map ON map.id=links.knowledge_map_id
         JOIN k1_textbook_editions edition ON edition.id=map.textbook_edition_id
         WHERE item.id=?1 AND links.state='confirmed'",
        [scope.item_id],
        |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?)),
    )?;
    let sources = link_sources(
        conn,
        &scope.question_type,
        answer_key_version_id,
        rubric_version_id,
        scope.link_set_id,
    )?;
    let mut knowledge_stmt = conn.prepare(
        "SELECT id,public_id,code,title FROM k1_knowledge_nodes
         WHERE knowledge_map_id=?1 AND state='active' ORDER BY order_index,id",
    )?;
    let knowledge_options = knowledge_stmt
        .query_map([knowledge_map_id], |row| {
            Ok(SubjectiveKnowledgeOption {
                id: row.get(0)?,
                public_id: row.get(1)?,
                code: row.get(2)?,
                title: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(knowledge_stmt);
    let mut ability_stmt = conn.prepare(
        "SELECT dimension.id,dimension.public_id,dimension.code,dimension.title
         FROM k1_ability_dimensions dimension
         WHERE dimension.subject_id=?1 AND dimension.state='active'
           AND dimension.revision=(SELECT MAX(latest.revision)
                                   FROM k1_ability_dimensions latest
                                   WHERE latest.stable_id=dimension.stable_id
                                     AND latest.state='active')
         ORDER BY dimension.code,dimension.id",
    )?;
    let ability_options = ability_stmt
        .query_map([subject_id], |row| {
            Ok(SubjectiveAbilityOption {
                id: row.get(0)?,
                public_id: row.get(1)?,
                code: row.get(2)?,
                title: row.get(3)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(SubjectiveLinkEditor {
        source_assessment_item_id,
        assessment_id: scope.assessment_id,
        base_assessment_version_id: scope.version_id,
        base_assessment_revision: scope.version_revision,
        base_assessment_item_id: scope.item_id,
        question_version_id: scope.question_version_id,
        question_type: scope.question_type,
        question_no: scope.question_no,
        question_stem: scope.question_stem,
        link_set_id: scope.link_set_id,
        link_set_revision,
        sources,
        knowledge_options,
        ability_options,
    })
}

fn validate_inputs<'a>(
    editor: &'a SubjectiveLinkEditor,
    inputs: &'a [SubjectiveSourceLinkInput],
) -> CoreResult<(String, i64, i64)> {
    if inputs.len() != editor.sources.len() {
        return Err(CoreError::Invalid(
            "必须一次提交本题全部答案槽位或评分点，允许显式留空".into(),
        ));
    }
    let expected = editor
        .sources
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
    let knowledge_ids = editor
        .knowledge_options
        .iter()
        .map(|item| item.id)
        .collect::<BTreeSet<_>>();
    let ability_ids = editor
        .ability_options
        .iter()
        .map(|item| item.id)
        .collect::<BTreeSet<_>>();
    let mut seen_sources = BTreeSet::new();
    let mut canonical = Vec::with_capacity(inputs.len());
    let mut knowledge_count = 0_i64;
    let mut ability_count = 0_i64;
    for input in inputs {
        let source_type = input.source_type.trim();
        let source_public_id = input.source_public_id.trim();
        if !expected.contains_key(&(source_type, source_public_id))
            || !seen_sources.insert((source_type, source_public_id))
        {
            return Err(CoreError::Invalid(
                "链接来源重复或不属于最新题目版本".into(),
            ));
        }
        let mut knowledge_seen = BTreeSet::new();
        let mut knowledge = Vec::new();
        for link in &input.knowledge_links {
            if !knowledge_ids.contains(&link.knowledge_node_id)
                || !matches!(
                    link.relation_type.as_str(),
                    "direct_assessment"
                        | "answer_basis"
                        | "context"
                        | "prerequisite"
                        | "distractor"
                        | "misconception"
                        | "rubric_basis"
                )
                || !knowledge_seen.insert((link.knowledge_node_id, link.relation_type.as_str()))
            {
                return Err(CoreError::Invalid(
                    "知识点链接非法、重复或跨知识地图".into(),
                ));
            }
            knowledge.push((link.knowledge_node_id, link.relation_type.as_str()));
            knowledge_count += 1;
        }
        knowledge.sort();
        let mut ability_seen = BTreeSet::new();
        let mut abilities = Vec::new();
        for link in &input.ability_links {
            if !ability_ids.contains(&link.ability_dimension_id)
                || !link.evidence_strength.is_finite()
                || !(0.0..=1.0).contains(&link.evidence_strength)
                || !matches!(
                    link.response_mode.as_str(),
                    "recognition"
                        | "recall"
                        | "structured_response"
                        | "source_analysis"
                        | "argumentation"
                )
                || !ability_seen.insert((link.ability_dimension_id, link.response_mode.as_str()))
            {
                return Err(CoreError::Invalid(
                    "能力链接非法、重复或不属于当前学科".into(),
                ));
            }
            abilities.push((
                link.ability_dimension_id,
                (link.evidence_strength * 1_000_000.0).round() as i64,
                link.response_mode.as_str(),
            ));
            ability_count += 1;
        }
        abilities.sort();
        canonical.push(CanonicalSource {
            source_type,
            source_public_id,
            knowledge_links: knowledge,
            ability_links: abilities,
        });
    }
    canonical.sort_by(|left, right| {
        (left.source_type, left.source_public_id).cmp(&(right.source_type, right.source_public_id))
    });
    let input_hash = hashing::sha256_hex(
        &serde_json::to_vec(&canonical)
            .map_err(|error| CoreError::Parse(format!("链接输入 hash 失败：{error}")))?,
    );
    Ok((input_hash, knowledge_count, ability_count))
}

fn current_matches(editor: &SubjectiveLinkEditor, inputs: &[SubjectiveSourceLinkInput]) -> bool {
    let current = editor
        .sources
        .iter()
        .map(|source| {
            let knowledge = source
                .knowledge_links
                .iter()
                .map(|link| (link.knowledge_node_id, link.relation_type.as_str()))
                .collect::<BTreeSet<_>>();
            let abilities = source
                .ability_links
                .iter()
                .map(|link| {
                    (
                        link.ability_dimension_id,
                        (link.evidence_strength * 1_000_000.0).round() as i64,
                        link.response_mode.as_str(),
                    )
                })
                .collect::<BTreeSet<_>>();
            (
                (
                    source.source_type.as_str(),
                    source.source_public_id.as_str(),
                ),
                (knowledge, abilities),
            )
        })
        .collect::<BTreeMap<_, _>>();
    inputs.iter().all(|input| {
        let knowledge = input
            .knowledge_links
            .iter()
            .map(|link| (link.knowledge_node_id, link.relation_type.as_str()))
            .collect::<BTreeSet<_>>();
        let abilities = input
            .ability_links
            .iter()
            .map(|link| {
                (
                    link.ability_dimension_id,
                    (link.evidence_strength * 1_000_000.0).round() as i64,
                    link.response_mode.as_str(),
                )
            })
            .collect::<BTreeSet<_>>();
        current.get(&(input.source_type.as_str(), input.source_public_id.as_str()))
            == Some(&(knowledge, abilities))
    })
}

pub fn save_editor(
    conn: &mut Connection,
    source_assessment_item_id: i64,
    inputs: &[SubjectiveSourceLinkInput],
    confirmed_by: &str,
) -> CoreResult<SubjectiveLinkEditResult> {
    required(confirmed_by, "确认人")?;
    let editor = get_editor(conn, source_assessment_item_id)?;
    let (input_hash, knowledge_count, ability_count) = validate_inputs(&editor, inputs)?;
    if current_matches(&editor, inputs) {
        return Ok(SubjectiveLinkEditResult {
            outcome: "already_current".into(),
            edit_id: None,
            adopted_assessment_version_id: editor.base_assessment_version_id,
            adopted_assessment_revision: editor.base_assessment_revision,
            adopted_link_set_id: editor.link_set_id,
            adopted_link_set_revision: editor.link_set_revision,
            knowledge_link_count: knowledge_count,
            ability_link_count: ability_count,
            current_attempts_unchanged: true,
            current_publications_unchanged: true,
        });
    }
    let existing: Option<(i64, i64, i64, i64, i64)> = conn
        .query_row(
            "SELECT edit.id,edit.adopted_assessment_version_id,version.revision,
                edit.adopted_link_set_id,links.revision
         FROM exam_subjective_link_edits_v2 edit
         JOIN exam_assessment_versions_v2 version ON version.id=edit.adopted_assessment_version_id
         JOIN k1_link_sets links ON links.id=edit.adopted_link_set_id
         WHERE edit.base_link_set_id=?1 AND edit.input_hash=?2",
            (&editor.link_set_id, &input_hash),
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
        .optional()?;
    if let Some((edit_id, version_id, version_revision, link_set_id, link_revision)) = existing {
        return Ok(SubjectiveLinkEditResult {
            outcome: "already_saved".into(),
            edit_id: Some(edit_id),
            adopted_assessment_version_id: version_id,
            adopted_assessment_revision: version_revision,
            adopted_link_set_id: link_set_id,
            adopted_link_set_revision: link_revision,
            knowledge_link_count: knowledge_count,
            ability_link_count: ability_count,
            current_attempts_unchanged: true,
            current_publications_unchanged: true,
        });
    }

    let now = time::utc_now_rfc3339();
    let tx = conn.transaction()?;
    let (knowledge_map_id, question_version_id): (i64, i64) = tx.query_row(
        "SELECT knowledge_map_id,question_version_id FROM k1_link_sets WHERE id=?1",
        [editor.link_set_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let link_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM k1_link_sets WHERE question_version_id=?1",
        [question_version_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO k1_link_sets
         (public_id,question_version_id,knowledge_map_id,revision,state,supersedes_link_set_id,
          created_at,confirmed_by,confirmed_at)
         VALUES (?1,?2,?3,?4,'confirmed',?5,?6,?7,?6)",
        params![
            ids::new_public_id(),
            question_version_id,
            knowledge_map_id,
            link_revision,
            editor.link_set_id,
            &now,
            confirmed_by.trim()
        ],
    )?;
    let adopted_link_set_id = tx.last_insert_rowid();
    let retained_knowledge_links = {
        let mut stmt = tx.prepare(
            "SELECT source_type,source_public_id,knowledge_node_id,relation_type,
                    confirmation_level,verified_by,verified_at
             FROM k1_knowledge_links WHERE link_set_id=?1
               AND source_type NOT IN ('answer_slot','rubric_point')
             ORDER BY id",
        )?;
        let rows = stmt.query_map([editor.link_set_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (
        source_type,
        source_public_id,
        node_id,
        relation_type,
        confirmation,
        verified_by,
        verified_at,
    ) in retained_knowledge_links
    {
        tx.execute(
            "INSERT INTO k1_knowledge_links
             (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,relation_type,
              confirmation_level,verified_by,verified_at,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                ids::new_public_id(),
                adopted_link_set_id,
                source_type,
                source_public_id,
                node_id,
                relation_type,
                confirmation,
                verified_by,
                verified_at,
                &now
            ],
        )?;
    }
    let retained_ability_links = {
        let mut stmt = tx.prepare(
            "SELECT source_type,source_public_id,ability_dimension_id,evidence_strength,
                    response_mode,confirmation_level,verified_by,verified_at
             FROM k1_ability_links WHERE link_set_id=?1
               AND source_type NOT IN ('answer_slot','rubric_point')
             ORDER BY id",
        )?;
        let rows = stmt.query_map([editor.link_set_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, f64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    for (
        source_type,
        source_public_id,
        ability_id,
        evidence_strength,
        response_mode,
        confirmation,
        verified_by,
        verified_at,
    ) in retained_ability_links
    {
        tx.execute(
            "INSERT INTO k1_ability_links
             (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,evidence_strength,
              response_mode,confirmation_level,verified_by,verified_at,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                ids::new_public_id(),
                adopted_link_set_id,
                source_type,
                source_public_id,
                ability_id,
                evidence_strength,
                response_mode,
                confirmation,
                verified_by,
                verified_at,
                &now
            ],
        )?;
    }
    for input in inputs {
        for link in &input.knowledge_links {
            tx.execute(
                "INSERT INTO k1_knowledge_links
                 (public_id,link_set_id,source_type,source_public_id,knowledge_node_id,relation_type,
                  confirmation_level,verified_by,verified_at,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,'teacher_confirmed',?7,?8,?8)",
                params![ids::new_public_id(),adopted_link_set_id,&input.source_type,
                        &input.source_public_id,link.knowledge_node_id,&link.relation_type,
                        confirmed_by.trim(),&now],
            )?;
        }
        for link in &input.ability_links {
            tx.execute(
                "INSERT INTO k1_ability_links
                 (public_id,link_set_id,source_type,source_public_id,ability_dimension_id,
                  evidence_strength,response_mode,confirmation_level,verified_by,verified_at,created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,'teacher_confirmed',?8,?9,?9)",
                params![ids::new_public_id(),adopted_link_set_id,&input.source_type,
                        &input.source_public_id,link.ability_dimension_id,link.evidence_strength,
                        &link.response_mode,confirmed_by.trim(),&now],
            )?;
        }
    }

    let assessment_revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_assessment_versions_v2 WHERE assessment_id=?1",
        [editor.assessment_id], |row| row.get(0),
    )?;
    let template_version: Option<String> = tx.query_row(
        "SELECT template_version FROM exam_assessment_versions_v2 WHERE id=?1",
        [editor.base_assessment_version_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "INSERT INTO exam_assessment_versions_v2
         (public_id,assessment_id,revision,template_version,state,supersedes_version_id,created_at)
         VALUES (?1,?2,?3,?4,'draft',?5,?6)",
        params![
            ids::new_public_id(),
            editor.assessment_id,
            assessment_revision,
            template_version.as_deref(),
            editor.base_assessment_version_id,
            &now
        ],
    )?;
    let adopted_assessment_version_id = tx.last_insert_rowid();
    let mut stmt = tx.prepare(
        "SELECT question_version_id,answer_key_version_id,rubric_version_id,link_set_id,
                order_index,score,option_order_json,presentation_snapshot_json
         FROM exam_assessment_items_v2
         WHERE assessment_version_id=?1 AND state='active' ORDER BY order_index,id",
    )?;
    let items = stmt
        .query_map([editor.base_assessment_version_id], |row| {
            Ok(AssessmentItemClone {
                question_version_id: row.get(0)?,
                answer_key_version_id: row.get(1)?,
                rubric_version_id: row.get(2)?,
                link_set_id: row.get(3)?,
                order_index: row.get(4)?,
                score: row.get(5)?,
                option_order_json: row.get(6)?,
                presentation_snapshot_json: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);
    let mut hash_items = Vec::with_capacity(items.len());
    let mut adopted_assessment_item_id = None;
    for item in items {
        let link_set_id = if item.question_version_id == editor.question_version_id {
            adopted_link_set_id
        } else {
            item.link_set_id
        };
        tx.execute(
            "INSERT INTO exam_assessment_items_v2
             (public_id,assessment_version_id,question_version_id,answer_key_version_id,
              rubric_version_id,link_set_id,order_index,score,option_order_json,
              presentation_snapshot_json,state,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'active',?11)",
            params![
                ids::new_public_id(),
                adopted_assessment_version_id,
                item.question_version_id,
                item.answer_key_version_id,
                item.rubric_version_id,
                link_set_id,
                item.order_index,
                item.score,
                item.option_order_json.as_deref(),
                &item.presentation_snapshot_json,
                &now
            ],
        )?;
        let item_id = tx.last_insert_rowid();
        if item.question_version_id == editor.question_version_id {
            adopted_assessment_item_id = Some(item_id);
        }
        hash_items.push(AssessmentHashItem {
            item_id,
            question_version_id: item.question_version_id,
            answer_key_version_id: item.answer_key_version_id,
            rubric_version_id: item.rubric_version_id,
            link_set_id,
            order_index: item.order_index,
            score_millis: (item.score * 1000.0).round() as i64,
        });
    }
    let adopted_assessment_item_id = adopted_assessment_item_id
        .ok_or_else(|| CoreError::Invalid("新作业版本未复制目标主观题，已回滚".into()))?;
    let item_set_hash = hashing::sha256_hex(
        &serde_json::to_vec(&hash_items)
            .map_err(|error| CoreError::Parse(format!("新作业版本 hash 失败：{error}")))?,
    );
    tx.execute(
        "UPDATE exam_assessment_versions_v2
         SET item_set_hash=?1,state='confirmed',confirmed_by=?2,confirmed_at=?3
         WHERE id=?4 AND state='draft'",
        params![
            item_set_hash,
            confirmed_by.trim(),
            &now,
            adopted_assessment_version_id
        ],
    )?;
    tx.execute(
        "UPDATE exam_assessments_v2 SET updated_at=?1 WHERE id=?2",
        params![&now, editor.assessment_id],
    )?;
    let edit_public_id = ids::new_public_id();
    tx.execute(
        "INSERT INTO exam_subjective_link_edits_v2
         (public_id,source_assessment_item_id,assessment_id,question_version_id,question_type,
          base_assessment_version_id,base_assessment_item_id,base_link_set_id,
          adopted_assessment_version_id,adopted_assessment_item_id,adopted_link_set_id,
          input_hash,knowledge_link_count,ability_link_count,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        params![
            &edit_public_id,
            source_assessment_item_id,
            editor.assessment_id,
            editor.question_version_id,
            &editor.question_type,
            editor.base_assessment_version_id,
            editor.base_assessment_item_id,
            editor.link_set_id,
            adopted_assessment_version_id,
            adopted_assessment_item_id,
            adopted_link_set_id,
            &input_hash,
            knowledge_count,
            ability_count,
            confirmed_by.trim(),
            &now
        ],
    )?;
    let edit_id = tx.last_insert_rowid();
    let meta = serde_json::json!({
        "schema_version": 1,
        "base_assessment_version_id": editor.base_assessment_version_id,
        "adopted_assessment_version_id": adopted_assessment_version_id,
        "base_link_set_id": editor.link_set_id,
        "adopted_link_set_id": adopted_link_set_id,
        "current_attempts_unchanged": true,
        "current_publications_unchanged": true
    })
    .to_string();
    audit::append(
        &tx,
        &audit::NewAuditEvent {
            idempotency_key: &format!("exam:subjective-link-edit:{edit_public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(confirmed_by.trim()),
            action: "exam.subjective_links.confirmed",
            object_type: "exam_subjective_link_edit",
            object_id: &edit_public_id,
            object_revision: Some(assessment_revision),
            note: None,
            meta_json: Some(&meta),
            occurred_at: &now,
        },
    )?;
    tx.commit()?;
    Ok(SubjectiveLinkEditResult {
        outcome: "created_new_version".into(),
        edit_id: Some(edit_id),
        adopted_assessment_version_id,
        adopted_assessment_revision: assessment_revision,
        adopted_link_set_id,
        adopted_link_set_revision: link_revision,
        knowledge_link_count: knowledge_count,
        ability_link_count: ability_count,
        current_attempts_unchanged: true,
        current_publications_unchanged: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use module_knowledge::db::content::{
        create_answer_key_version, create_link_set, create_question, create_question_version,
        create_rubric_version, promote_question_version, NewAnswerKeyVersion, NewAnswerSlot,
        NewQuestion, NewQuestionVersion, NewRubricPoint, NewRubricVersion,
    };
    use module_knowledge::db::taxonomy::{
        create_ability_dimension, create_knowledge_map, create_knowledge_node,
        create_textbook_edition, NewAbilityDimension, NewKnowledgeMap, NewKnowledgeNode,
        NewTextbookEdition,
    };
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    use crate::service::assessment::{
        add_assessment_item, confirm_assessment_version, create_assessment_draft,
        NewAssessmentDraft, NewAssessmentItem,
    };

    struct Fixture {
        conn: Connection,
        item_id: i64,
        version_id: i64,
        link_set_id: i64,
        knowledge_id: i64,
        ability_id: i64,
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
        let edition = create_textbook_edition(
            &conn,
            &NewTextbookEdition {
                subject_id: 1,
                publisher_code: "PEP",
                edition_code: "2024",
                title: "中国历史八上",
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
        let knowledge = create_knowledge_node(
            &conn,
            &NewKnowledgeNode {
                stable_id: Some("nanjing-treaty-year"),
                knowledge_map_id: map.id,
                curriculum_node_id: None,
                parent_id: None,
                code: Some("K1"),
                title: "南京条约签订时间",
                description: None,
                order_index: 1,
            },
        )
        .unwrap();
        let ability = create_ability_dimension(
            &conn,
            &NewAbilityDimension {
                stable_id: Some("fact-recall"),
                subject_id: 1,
                revision: 1,
                code: "fact_recall",
                title: "事实识记与提取",
                description: None,
                supersedes_dimension_id: None,
            },
        )
        .unwrap();
        let question = create_question(
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
        let question_version = create_question_version(
            &conn,
            &NewQuestionVersion {
                question_id: question.id,
                revision: 1,
                question_type: "fill_blank",
                stem: "《南京条约》签订于____年。",
                material_text: None,
                max_score: 1.0,
                source_artifact_id: None,
                source_anchor_json: None,
                supersedes_version_id: None,
                quality_level: "L0",
                state: "draft",
                options: &[],
            },
        )
        .unwrap();
        let answer = create_answer_key_version(
            &conn,
            &NewAnswerKeyVersion {
                question_version_id: question_version.id,
                revision: 1,
                answer_json: r#"{"schema_version":1,"answers":["1842"]}"#,
                state: "confirmed",
                confirmed_by: Some("teacher"),
                supersedes_answer_key_id: None,
                slots: &[NewAnswerSlot {
                    stable_id: Some("year"),
                    order_index: 0,
                    canonical_answers_json: r#"{"schema_version":1,"answers":["1842"]}"#,
                    normalization_rules_json: None,
                    max_score: 1.0,
                }],
            },
        )
        .unwrap();
        let rubric = create_rubric_version(
            &conn,
            &NewRubricVersion {
                question_version_id: question_version.id,
                revision: 1,
                max_score: 1.0,
                state: "confirmed",
                confirmed_by: Some("teacher"),
                supersedes_rubric_id: None,
                points: &[NewRubricPoint {
                    stable_id: Some("year"),
                    order_index: 0,
                    canonical_text: "写出 1842 年",
                    allowed_paraphrases_json: None,
                    required_concepts_json: None,
                    max_score: 1.0,
                }],
            },
        )
        .unwrap();
        let link_set = create_link_set(
            &conn,
            question_version.id,
            map.id,
            1,
            "confirmed",
            Some("teacher"),
            None,
        )
        .unwrap();
        promote_question_version(&conn, question_version.id, "L2", "teacher", None).unwrap();
        let assessment = create_assessment_draft(
            &conn,
            &NewAssessmentDraft {
                title: "第一课填空",
                class_id: 1,
                assessment_context: "quiz",
                evidence_policy: "include",
                created_by: "teacher",
                template_version: None,
            },
        )
        .unwrap();
        let item = add_assessment_item(
            &conn,
            assessment.assessment_version_id,
            &NewAssessmentItem {
                question_version_id: question_version.id,
                answer_key_version_id: answer.id,
                rubric_version_id: rubric.id,
                link_set_id: link_set.id,
                order_index: 0,
                score: 1.0,
                option_order_json: None,
                presentation_snapshot_json: r#"{"schema_version":1,"question_no":"1"}"#,
            },
        )
        .unwrap();
        confirm_assessment_version(&conn, assessment.assessment_version_id, "teacher").unwrap();
        Fixture {
            conn,
            item_id: item.id,
            version_id: assessment.assessment_version_id,
            link_set_id: link_set.id,
            knowledge_id: knowledge.id,
            ability_id: ability.id,
        }
    }

    #[test]
    fn confirmed_links_create_future_versions_without_rebinding_history() {
        let mut fixture = setup();
        let editor = get_editor(&fixture.conn, fixture.item_id).unwrap();
        assert_eq!(editor.sources.len(), 1);
        assert_eq!(editor.sources[0].source_type, "answer_slot");
        let input = vec![SubjectiveSourceLinkInput {
            source_type: "answer_slot".into(),
            source_public_id: editor.sources[0].source_public_id.clone(),
            knowledge_links: vec![SubjectiveKnowledgeLinkInput {
                knowledge_node_id: fixture.knowledge_id,
                relation_type: "answer_basis".into(),
            }],
            ability_links: vec![SubjectiveAbilityLinkInput {
                ability_dimension_id: fixture.ability_id,
                evidence_strength: 0.6,
                response_mode: "recall".into(),
            }],
        }];
        let result = save_editor(&mut fixture.conn, fixture.item_id, &input, "teacher").unwrap();
        assert_eq!(result.outcome, "created_new_version");
        assert_ne!(result.adopted_assessment_version_id, fixture.version_id);
        assert_ne!(result.adopted_link_set_id, fixture.link_set_id);
        let old_item_link_set: i64 = fixture
            .conn
            .query_row(
                "SELECT link_set_id FROM exam_assessment_items_v2 WHERE id=?1",
                [fixture.item_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(old_item_link_set, fixture.link_set_id);
        let refreshed = get_editor(&fixture.conn, fixture.item_id).unwrap();
        assert_eq!(refreshed.sources[0].knowledge_links.len(), 1);
        assert_eq!(refreshed.sources[0].ability_links.len(), 1);
        let repeated = save_editor(&mut fixture.conn, fixture.item_id, &input, "teacher").unwrap();
        assert_eq!(repeated.outcome, "already_current");
    }

    #[test]
    fn editor_rejects_partial_duplicate_and_cross_map_inputs() {
        let mut fixture = setup();
        let editor = get_editor(&fixture.conn, fixture.item_id).unwrap();
        assert!(save_editor(&mut fixture.conn, fixture.item_id, &[], "teacher").is_err());
        let invalid = vec![SubjectiveSourceLinkInput {
            source_type: "answer_slot".into(),
            source_public_id: editor.sources[0].source_public_id.clone(),
            knowledge_links: vec![SubjectiveKnowledgeLinkInput {
                knowledge_node_id: 999_999,
                relation_type: "answer_basis".into(),
            }],
            ability_links: vec![],
        }];
        assert!(save_editor(&mut fixture.conn, fixture.item_id, &invalid, "teacher").is_err());
    }

    #[test]
    fn link_edit_ledger_is_immutable() {
        let mut fixture = setup();
        let editor = get_editor(&fixture.conn, fixture.item_id).unwrap();
        let input = vec![SubjectiveSourceLinkInput {
            source_type: "answer_slot".into(),
            source_public_id: editor.sources[0].source_public_id.clone(),
            knowledge_links: vec![],
            ability_links: vec![SubjectiveAbilityLinkInput {
                ability_dimension_id: fixture.ability_id,
                evidence_strength: 0.5,
                response_mode: "recall".into(),
            }],
        }];
        let result = save_editor(&mut fixture.conn, fixture.item_id, &input, "teacher").unwrap();
        assert!(fixture
            .conn
            .execute(
                "UPDATE exam_subjective_link_edits_v2 SET knowledge_link_count=3 WHERE id=?1",
                [result.edit_id.unwrap()],
            )
            .is_err());
        assert!(fixture
            .conn
            .execute(
                "DELETE FROM exam_subjective_link_edits_v2 WHERE id=?1",
                [result.edit_id.unwrap()],
            )
            .is_err());
    }
}
