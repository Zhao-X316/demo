//! M2-B3a0 连续拍摄资料的顺序证据、材料类型和按学号分组预览。
//!
//! 这里保存的是可纠正的推断/确认 revision。它不会把系统建议伪装成老师确认的页面身份，
//! 也不会创建评分、发布成绩或学习证据。

use std::cmp::Ordering;
use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use suite_core::db::repo::audit;
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

const MATERIAL_AUTO_ACCEPT_CONFIDENCE: f64 = 0.85;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportOrderEntry {
    pub source_artifact_id: i64,
    pub original_name: String,
    pub original_request_index: i64,
    pub sorted_index: i64,
    pub page_count: i64,
    pub capture_time: Option<String>,
    pub file_created_ms: Option<i64>,
    pub file_modified_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImportOrderRevision {
    pub id: i64,
    pub public_id: String,
    pub ingest_batch_id: i64,
    pub revision: i64,
    pub snapshot_hash: String,
    pub sort_policy: String,
    pub order_confidence: f64,
    pub ordered_sources_json: String,
    pub conflict_codes_json: String,
    pub state: String,
    pub created_by_type: String,
    pub created_by: Option<String>,
}

pub struct NewImportOrderRevision<'a> {
    pub ingest_batch_id: i64,
    pub sort_policy: &'a str,
    pub order_confidence: f64,
    pub entries: &'a [ImportOrderEntry],
    pub conflict_codes: &'a [String],
    pub created_by_type: &'a str,
    pub created_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialTypeRevision {
    pub id: i64,
    pub public_id: String,
    pub ingest_batch_id: i64,
    pub revision: i64,
    pub material_type: String,
    pub confidence: f64,
    pub evidence_json: String,
    pub decision: String,
    pub state: String,
    pub created_by_type: String,
    pub created_by: Option<String>,
    pub confirmed_by: Option<String>,
}

pub struct NewMaterialTypeRevision<'a> {
    pub ingest_batch_id: i64,
    pub material_type: &'a str,
    pub confidence: f64,
    pub evidence_json: &'a str,
    pub decision: &'a str,
    pub created_by_type: &'a str,
    pub created_by: Option<&'a str>,
    pub confirmed_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageTypeRevision {
    pub id: i64,
    pub public_id: String,
    pub page_id: i64,
    pub revision: i64,
    pub page_type_key: String,
    pub confidence: f64,
    pub evidence_json: String,
    pub decision: String,
    pub state: String,
}

pub struct NewPageTypeRevision<'a> {
    pub page_id: i64,
    pub page_type_key: &'a str,
    pub confidence: f64,
    pub evidence_json: &'a str,
    pub decision: &'a str,
    pub created_by_type: &'a str,
    pub created_by: Option<&'a str>,
    pub confirmed_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderedGroupingRevision {
    pub id: i64,
    pub public_id: String,
    pub ingest_batch_id: i64,
    pub revision: i64,
    pub snapshot_hash: String,
    pub import_order_revision_id: i64,
    pub material_type_revision_id: Option<i64>,
    pub expected_pages_per_attempt: Option<i64>,
    pub route: String,
    pub student_group_count: i64,
    pub issue_codes_json: String,
    pub grouping_json: String,
    pub state: String,
}

pub struct OrderedGroupingInput<'a> {
    pub ingest_batch_id: i64,
    pub expected_pages_per_attempt: i64,
    pub created_by_type: &'a str,
    pub created_by: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MaterialTypeSuggestion {
    pub material_type: &'static str,
    pub confidence: f64,
    pub rule: &'static str,
}

fn actor(actor_type: &str, actor_id: Option<&str>) -> CoreResult<AuditActorType> {
    match (actor_type, actor_id.map(str::trim)) {
        ("system", None) => Ok(AuditActorType::System),
        ("teacher", Some(value)) if !value.is_empty() => Ok(AuditActorType::Teacher),
        ("teacher", _) => Err(CoreError::Invalid("老师操作必须记录操作人".into())),
        ("system", Some(_)) => Err(CoreError::Invalid("系统操作不能记录老师身份".into())),
        _ => Err(CoreError::Invalid("操作人类型非法".into())),
    }
}

fn validate_schema_object(json: &str, field: &str) -> CoreResult<String> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| CoreError::Invalid(format!("{field} JSON 无效：{error}")))?;
    if !value.is_object()
        || value
            .get("schema_version")
            .and_then(Value::as_i64)
            .is_none()
    {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是带整数 schema_version 的对象"
        )));
    }
    serde_json::to_string(&value)
        .map_err(|error| CoreError::Parse(format!("{field} JSON 规范化失败：{error}")))
}

struct AuditInput<'a> {
    idempotency_key: &'a str,
    actor_type: AuditActorType,
    actor_id: Option<&'a str>,
    action: &'a str,
    object_type: &'a str,
    object_id: &'a str,
    revision: i64,
    now: &'a str,
}

fn append_audit(conn: &Connection, input: &AuditInput<'_>) -> CoreResult<()> {
    audit::append(
        conn,
        &audit::NewAuditEvent {
            idempotency_key: input.idempotency_key,
            actor_type: input.actor_type,
            actor_id: input.actor_id,
            action: input.action,
            object_type: input.object_type,
            object_id: input.object_id,
            object_revision: Some(input.revision),
            note: None,
            meta_json: Some(r#"{"schema_version":1}"#),
            occurred_at: input.now,
        },
    )?;
    Ok(())
}

fn compare_digit_runs(left: &[u8], right: &[u8]) -> Ordering {
    let left_trim = left
        .iter()
        .position(|byte| *byte != b'0')
        .unwrap_or(left.len());
    let right_trim = right
        .iter()
        .position(|byte| *byte != b'0')
        .unwrap_or(right.len());
    let left_number = &left[left_trim..];
    let right_number = &right[right_trim..];
    left_number
        .len()
        .cmp(&right_number.len())
        .then_with(|| left_number.cmp(right_number))
        .then_with(|| left.len().cmp(&right.len()))
}

/// 文件名/学号自然排序：`2` 排在 `10` 前，不把长数字解析进固定整数类型。
pub fn natural_name_cmp(left: &str, right: &str) -> Ordering {
    let left = left.to_lowercase();
    let right = right.to_lowercase();
    let left = left.as_bytes();
    let right = right.as_bytes();
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if left[i].is_ascii_digit() && right[j].is_ascii_digit() {
            let left_end = (i..left.len())
                .find(|index| !left[*index].is_ascii_digit())
                .unwrap_or(left.len());
            let right_end = (j..right.len())
                .find(|index| !right[*index].is_ascii_digit())
                .unwrap_or(right.len());
            let ordering = compare_digit_runs(&left[i..left_end], &right[j..right_end]);
            if ordering != Ordering::Equal {
                return ordering;
            }
            i = left_end;
            j = right_end;
        } else {
            let ordering = left[i].cmp(&right[j]);
            if ordering != Ordering::Equal {
                return ordering;
            }
            i += 1;
            j += 1;
        }
    }
    left.len().cmp(&right.len())
}

pub fn suggest_material_type(names: &[String]) -> MaterialTypeSuggestion {
    let lowered = names
        .iter()
        .map(|name| name.to_lowercase())
        .collect::<Vec<_>>();
    if lowered.iter().any(|name| {
        name.contains("答题卡") || name.contains("answer_sheet") || name.contains("answersheet")
    }) {
        MaterialTypeSuggestion {
            material_type: "answer_sheet",
            confidence: 0.98,
            rule: "filename_answer_sheet_keyword_v1",
        }
    } else if lowered
        .iter()
        .any(|name| name.contains("默写") || name.contains("听写") || name.contains("dictation"))
    {
        MaterialTypeSuggestion {
            material_type: "dictation",
            confidence: 0.98,
            rule: "filename_dictation_keyword_v1",
        }
    } else {
        MaterialTypeSuggestion {
            material_type: "ordinary_paper",
            confidence: 0.45,
            rule: "ordinary_fallback_needs_confirmation_v1",
        }
    }
}

fn import_order_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ImportOrderRevision> {
    Ok(ImportOrderRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        ingest_batch_id: row.get(2)?,
        revision: row.get(3)?,
        snapshot_hash: row.get(4)?,
        sort_policy: row.get(5)?,
        order_confidence: row.get(6)?,
        ordered_sources_json: row.get(7)?,
        conflict_codes_json: row.get(8)?,
        state: row.get(9)?,
        created_by_type: row.get(10)?,
        created_by: row.get(11)?,
    })
}

fn material_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MaterialTypeRevision> {
    Ok(MaterialTypeRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        ingest_batch_id: row.get(2)?,
        revision: row.get(3)?,
        material_type: row.get(4)?,
        confidence: row.get(5)?,
        evidence_json: row.get(6)?,
        decision: row.get(7)?,
        state: row.get(8)?,
        created_by_type: row.get(9)?,
        created_by: row.get(10)?,
        confirmed_by: row.get(11)?,
    })
}

fn page_type_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PageTypeRevision> {
    Ok(PageTypeRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        page_id: row.get(2)?,
        revision: row.get(3)?,
        page_type_key: row.get(4)?,
        confidence: row.get(5)?,
        evidence_json: row.get(6)?,
        decision: row.get(7)?,
        state: row.get(8)?,
    })
}

fn grouping_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<OrderedGroupingRevision> {
    Ok(OrderedGroupingRevision {
        id: row.get(0)?,
        public_id: row.get(1)?,
        ingest_batch_id: row.get(2)?,
        revision: row.get(3)?,
        snapshot_hash: row.get(4)?,
        import_order_revision_id: row.get(5)?,
        material_type_revision_id: row.get(6)?,
        expected_pages_per_attempt: row.get(7)?,
        route: row.get(8)?,
        student_group_count: row.get(9)?,
        issue_codes_json: row.get(10)?,
        grouping_json: row.get(11)?,
        state: row.get(12)?,
    })
}

pub fn record_import_order(
    conn: &Connection,
    input: &NewImportOrderRevision<'_>,
) -> CoreResult<ImportOrderRevision> {
    if !matches!(
        input.sort_policy,
        "filename_natural_exif_filetime_crosscheck_v1" | "teacher_corrected_v1"
    ) || !(0.0..=1.0).contains(&input.order_confidence)
    {
        return Err(CoreError::Invalid("导入顺序策略或置信度非法".into()));
    }
    let actor_type = actor(input.created_by_type, input.created_by)?;
    if input.entries.is_empty() {
        return Err(CoreError::Invalid("导入顺序至少需要一个学生资料".into()));
    }
    let mut source_ids = BTreeSet::new();
    for (position, entry) in input.entries.iter().enumerate() {
        if entry.sorted_index != position as i64 || entry.page_count < 1 {
            return Err(CoreError::Invalid("导入顺序位置必须连续且页数有效".into()));
        }
        if !source_ids.insert(entry.source_artifact_id) {
            return Err(CoreError::Invalid("导入顺序包含重复原始资料".into()));
        }
        let registered: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM exam_fixed_input_documents_v2
             WHERE ingest_batch_id=?1 AND source_artifact_id=?2
               AND document_role='student_work' AND state<>'voided' AND page_count=?3)",
            (
                input.ingest_batch_id,
                entry.source_artifact_id,
                entry.page_count,
            ),
            |row| row.get(0),
        )?;
        if !registered {
            return Err(CoreError::Invalid(
                "导入顺序引用了未登记或页数漂移的资料".into(),
            ));
        }
    }
    let ordered_sources = serde_json::json!({
        "schema_version": 1,
        "entries": input.entries
    });
    let conflicts = serde_json::json!({
        "schema_version": 1,
        "codes": input.conflict_codes
    });
    let snapshot = serde_json::json!({
        "schema_version": 1,
        "sort_policy": input.sort_policy,
        "order_confidence": input.order_confidence,
        "ordered_sources": ordered_sources,
        "conflicts": conflicts
    });
    let ordered_sources_json = ordered_sources.to_string();
    let conflict_codes_json = conflicts.to_string();
    let snapshot_hash = hashing::sha256_hex(
        &serde_json::to_vec(&snapshot)
            .map_err(|error| CoreError::Parse(format!("导入顺序快照失败：{error}")))?,
    );
    let existing = conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,revision,snapshot_hash,sort_policy,
                    order_confidence,ordered_sources_json,conflict_codes_json,state,
                    created_by_type,created_by
             FROM exam_import_order_revisions_v2
             WHERE ingest_batch_id=?1 AND snapshot_hash=?2 AND state='active'",
            (input.ingest_batch_id, &snapshot_hash),
            import_order_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.unchecked_transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_import_order_revisions_v2
         WHERE ingest_batch_id=?1",
        [input.ingest_batch_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_import_order_revisions_v2 SET state='superseded'
         WHERE ingest_batch_id=?1 AND state='active'",
        [input.ingest_batch_id],
    )?;
    tx.execute(
        "INSERT INTO exam_import_order_revisions_v2
         (public_id,ingest_batch_id,revision,snapshot_hash,sort_policy,order_confidence,
          ordered_sources_json,conflict_codes_json,state,created_by_type,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'active',?9,?10,?11)",
        params![
            &public_id,
            input.ingest_batch_id,
            revision,
            &snapshot_hash,
            input.sort_policy,
            input.order_confidence,
            &ordered_sources_json,
            &conflict_codes_json,
            input.created_by_type,
            input.created_by.map(str::trim),
            &now,
        ],
    )?;
    let id = tx.last_insert_rowid();
    append_audit(
        &tx,
        &AuditInput {
            idempotency_key: &format!("exam:import-order:{public_id}:active"),
            actor_type,
            actor_id: input.created_by.map(str::trim),
            action: "exam.import_order.activated",
            object_type: "exam_import_order",
            object_id: &public_id,
            revision,
            now: &now,
        },
    )?;
    tx.commit()?;
    conn.query_row(
        "SELECT id,public_id,ingest_batch_id,revision,snapshot_hash,sort_policy,
                order_confidence,ordered_sources_json,conflict_codes_json,state,
                created_by_type,created_by
         FROM exam_import_order_revisions_v2 WHERE id=?1",
        [id],
        import_order_row,
    )
    .map_err(Into::into)
}

pub fn record_material_type(
    conn: &Connection,
    input: &NewMaterialTypeRevision<'_>,
) -> CoreResult<MaterialTypeRevision> {
    if !matches!(
        input.material_type,
        "ordinary_paper" | "answer_sheet" | "dictation" | "unknown"
    ) || !matches!(
        input.decision,
        "suggested" | "teacher_confirmed" | "rejected"
    ) || !(0.0..=1.0).contains(&input.confidence)
    {
        return Err(CoreError::Invalid("材料类型结论非法".into()));
    }
    let actor_type = actor(input.created_by_type, input.created_by)?;
    if input.decision == "teacher_confirmed"
        && (input.created_by_type != "teacher"
            || input
                .confirmed_by
                .map(str::trim)
                .unwrap_or_default()
                .is_empty())
    {
        return Err(CoreError::Invalid("材料类型确认必须记录老师".into()));
    }
    let evidence_json = validate_schema_object(input.evidence_json, "材料类型证据")?;
    let existing = conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,revision,material_type,confidence,
                    evidence_json,decision,state,created_by_type,created_by,confirmed_by
             FROM exam_material_type_revisions_v2
             WHERE ingest_batch_id=?1 AND material_type=?2 AND confidence=?3
               AND evidence_json=?4 AND decision=?5 AND state='active'",
            params![
                input.ingest_batch_id,
                input.material_type,
                input.confidence,
                &evidence_json,
                input.decision
            ],
            material_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.unchecked_transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_material_type_revisions_v2
         WHERE ingest_batch_id=?1",
        [input.ingest_batch_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_material_type_revisions_v2 SET state='superseded'
         WHERE ingest_batch_id=?1 AND state='active'",
        [input.ingest_batch_id],
    )?;
    tx.execute(
        "INSERT INTO exam_material_type_revisions_v2
         (public_id,ingest_batch_id,revision,material_type,confidence,evidence_json,
          decision,state,created_by_type,created_by,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'active',?8,?9,?10,?11)",
        params![
            &public_id,
            input.ingest_batch_id,
            revision,
            input.material_type,
            input.confidence,
            &evidence_json,
            input.decision,
            input.created_by_type,
            input.created_by.map(str::trim),
            input.confirmed_by.map(str::trim),
            &now,
        ],
    )?;
    let id = tx.last_insert_rowid();
    append_audit(
        &tx,
        &AuditInput {
            idempotency_key: &format!("exam:material-type:{public_id}:active"),
            actor_type,
            actor_id: input.created_by.map(str::trim),
            action: if input.decision == "teacher_confirmed" {
                "exam.material_type.teacher_confirmed"
            } else {
                "exam.material_type.suggested"
            },
            object_type: "exam_material_type",
            object_id: &public_id,
            revision,
            now: &now,
        },
    )?;
    tx.commit()?;
    conn.query_row(
        "SELECT id,public_id,ingest_batch_id,revision,material_type,confidence,
                evidence_json,decision,state,created_by_type,created_by,confirmed_by
         FROM exam_material_type_revisions_v2 WHERE id=?1",
        [id],
        material_row,
    )
    .map_err(Into::into)
}

pub fn record_page_type(
    conn: &Connection,
    input: &NewPageTypeRevision<'_>,
) -> CoreResult<PageTypeRevision> {
    if input.page_type_key.trim().is_empty()
        || !(0.0..=1.0).contains(&input.confidence)
        || !matches!(
            input.decision,
            "suggested" | "teacher_confirmed" | "unknown" | "rejected"
        )
    {
        return Err(CoreError::Invalid("页型结论非法".into()));
    }
    actor(input.created_by_type, input.created_by)?;
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM exam_ingest_pages_v2 WHERE id=?1 AND state<>'voided')",
        [input.page_id],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(CoreError::NotFound("待记录页型的页面".into()));
    }
    let evidence_json = validate_schema_object(input.evidence_json, "页型证据")?;
    let existing = conn
        .query_row(
            "SELECT id,public_id,page_id,revision,page_type_key,confidence,evidence_json,
                    decision,state FROM exam_page_type_revisions_v2
             WHERE page_id=?1 AND page_type_key=?2 AND confidence=?3
               AND evidence_json=?4 AND decision=?5 AND state='active'",
            params![
                input.page_id,
                input.page_type_key.trim(),
                input.confidence,
                &evidence_json,
                input.decision
            ],
            page_type_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let tx = conn.unchecked_transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_page_type_revisions_v2 WHERE page_id=?1",
        [input.page_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_page_type_revisions_v2 SET state='superseded'
         WHERE page_id=?1 AND state='active'",
        [input.page_id],
    )?;
    tx.execute(
        "INSERT INTO exam_page_type_revisions_v2
         (public_id,page_id,revision,page_type_key,confidence,evidence_json,decision,state,
          created_by_type,created_by,confirmed_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'active',?8,?9,?10,?11)",
        params![
            &public_id,
            input.page_id,
            revision,
            input.page_type_key.trim(),
            input.confidence,
            &evidence_json,
            input.decision,
            input.created_by_type,
            input.created_by.map(str::trim),
            input.confirmed_by.map(str::trim),
            &now,
        ],
    )?;
    let id = tx.last_insert_rowid();
    tx.commit()?;
    conn.query_row(
        "SELECT id,public_id,page_id,revision,page_type_key,confidence,evidence_json,
                decision,state FROM exam_page_type_revisions_v2 WHERE id=?1",
        [id],
        page_type_row,
    )
    .map_err(Into::into)
}

#[derive(Debug)]
struct PageForGrouping {
    id: i64,
    import_index: i64,
    source_artifact_id: i64,
    page_type_key: Option<String>,
    page_type_decision: Option<String>,
}

fn current_import_order(conn: &Connection, batch_id: i64) -> CoreResult<ImportOrderRevision> {
    conn.query_row(
        "SELECT id,public_id,ingest_batch_id,revision,snapshot_hash,sort_policy,
                order_confidence,ordered_sources_json,conflict_codes_json,state,
                created_by_type,created_by
         FROM exam_import_order_revisions_v2 WHERE ingest_batch_id=?1 AND state='active'",
        [batch_id],
        import_order_row,
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("缺少有效导入顺序快照".into()))
}

fn current_material(conn: &Connection, batch_id: i64) -> CoreResult<Option<MaterialTypeRevision>> {
    Ok(conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,revision,material_type,confidence,
                    evidence_json,decision,state,created_by_type,created_by,confirmed_by
             FROM exam_material_type_revisions_v2 WHERE ingest_batch_id=?1 AND state='active'",
            [batch_id],
            material_row,
        )
        .optional()?)
}

fn ordered_pages(conn: &Connection, batch_id: i64) -> CoreResult<Vec<PageForGrouping>> {
    let mut stmt = conn.prepare(
        "SELECT p.id,p.import_index,p.source_artifact_id,t.page_type_key,t.decision
         FROM exam_ingest_pages_v2 p
         LEFT JOIN exam_page_type_revisions_v2 t ON t.page_id=p.id AND t.state='active'
         WHERE p.batch_id=?1 AND p.state<>'voided'
         ORDER BY p.import_index,p.id",
    )?;
    let rows = stmt.query_map([batch_id], |row| {
        Ok(PageForGrouping {
            id: row.get(0)?,
            import_index: row.get(1)?,
            source_artifact_id: row.get(2)?,
            page_type_key: row.get(3)?,
            page_type_decision: row.get(4)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

#[derive(Debug)]
struct RosterStudent {
    id: i64,
    student_no: String,
    name: String,
}

fn roster(conn: &Connection, batch_id: i64) -> CoreResult<Vec<RosterStudent>> {
    let mut stmt = conn.prepare(
        "SELECT s.id,s.student_no,s.name
         FROM exam_ingest_batches_v2 b
         JOIN exam_assessment_versions_v2 v ON v.id=b.assessment_version_id
         JOIN exam_assessments_v2 a ON a.id=v.assessment_id
         JOIN students s ON s.class_id=a.class_id AND s.enabled=1
         WHERE b.id=?1",
    )?;
    let rows = stmt.query_map([batch_id], |row| {
        Ok(RosterStudent {
            id: row.get(0)?,
            student_no: row.get(1)?,
            name: row.get(2)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    result.sort_by(|left, right| {
        natural_name_cmp(&left.student_no, &right.student_no).then_with(|| left.id.cmp(&right.id))
    });
    Ok(result)
}

pub fn preview_ordered_grouping(
    conn: &Connection,
    input: &OrderedGroupingInput<'_>,
) -> CoreResult<OrderedGroupingRevision> {
    if input.expected_pages_per_attempt < 1 {
        return Err(CoreError::Invalid("每名学生页数必须大于 0".into()));
    }
    let actor_type = actor(input.created_by_type, input.created_by)?;
    let order = current_import_order(conn, input.ingest_batch_id)?;
    let material = current_material(conn, input.ingest_batch_id)?;
    let pages = ordered_pages(conn, input.ingest_batch_id)?;
    let roster = roster(conn, input.ingest_batch_id)?;
    let mut issues = BTreeSet::<String>::new();
    if pages.is_empty() {
        issues.insert("BATCH_EMPTY".into());
    }
    for (position, page) in pages.iter().enumerate() {
        if page.import_index != position as i64 {
            issues.insert("IMPORT_SEQUENCE_GAP".into());
        }
    }
    let unique_artifacts = pages
        .iter()
        .map(|page| page.source_artifact_id)
        .collect::<BTreeSet<_>>();
    if unique_artifacts.len() != pages.len() {
        issues.insert("DUPLICATE_PAGE_ARTIFACT".into());
    }
    let page_count = pages.len() as i64;
    let expected_pages = input.expected_pages_per_attempt;
    if page_count % expected_pages != 0 {
        issues.insert("GROUP_PAGE_COUNT_MISMATCH".into());
    }
    let group_count = page_count / expected_pages;
    if group_count > roster.len() as i64 {
        issues.insert("STUDENT_GROUP_EXCEEDS_ROSTER".into());
    } else if group_count != roster.len() as i64 {
        issues.insert("STUDENT_RANGE_OR_ABSENCE_CONFIRMATION_REQUIRED".into());
    }
    match material.as_ref() {
        Some(value)
            if value.decision == "teacher_confirmed"
                || (value.decision == "suggested"
                    && value.confidence >= MATERIAL_AUTO_ACCEPT_CONFIDENCE
                    && value.material_type != "unknown") => {}
        _ => {
            issues.insert("MATERIAL_TYPE_CONFIRMATION_REQUIRED".into());
        }
    }

    if expected_pages > 1 {
        let all_page_types_known = pages.iter().all(|page| {
            page.page_type_key.is_some()
                && matches!(
                    page.page_type_decision.as_deref(),
                    Some("suggested" | "teacher_confirmed")
                )
        });
        if !all_page_types_known {
            issues.insert("PAGE_TYPE_CYCLE_UNVERIFIED".into());
        } else if pages.len() >= expected_pages as usize {
            let cycle = pages
                .iter()
                .take(expected_pages as usize)
                .map(|page| page.page_type_key.as_deref().unwrap_or_default())
                .collect::<Vec<_>>();
            if pages.iter().enumerate().any(|(index, page)| {
                page.page_type_key.as_deref().unwrap_or_default()
                    != cycle[index % expected_pages as usize]
            }) {
                issues.insert("PAGE_TYPE_CYCLE_MISMATCH".into());
            }
        }
    }

    let order_conflicts: Value = serde_json::from_str(&order.conflict_codes_json)
        .map_err(|error| CoreError::Parse(format!("顺序冲突证据读取失败：{error}")))?;
    if order_conflicts
        .get("codes")
        .and_then(Value::as_array)
        .is_some_and(|codes| !codes.is_empty())
    {
        issues.insert("ORDER_EVIDENCE_CONFLICT".into());
    }

    let safe_to_assign_roster = group_count == roster.len() as i64
        && !issues.contains("GROUP_PAGE_COUNT_MISMATCH")
        && !issues.contains("PAGE_TYPE_CYCLE_MISMATCH")
        && !issues.contains("DUPLICATE_PAGE_ARTIFACT")
        && !issues.contains("IMPORT_SEQUENCE_GAP");
    let mut groups = Vec::new();
    for group_index in 0..group_count.max(0) as usize {
        let start = group_index * expected_pages as usize;
        let end = start + expected_pages as usize;
        if end > pages.len() {
            break;
        }
        let student = safe_to_assign_roster
            .then(|| roster.get(group_index))
            .flatten();
        groups.push(serde_json::json!({
            "group_index": group_index,
            "student_id": student.map(|value| value.id),
            "student_no": student.map(|value| value.student_no.as_str()),
            "student_name": student.map(|value| value.name.as_str()),
            "start_import_index": pages[start].import_index,
            "end_import_index": pages[end - 1].import_index,
            "page_ids": pages[start..end].iter().map(|page| page.id).collect::<Vec<_>>(),
            "page_type_keys": pages[start..end]
                .iter()
                .map(|page| page.page_type_key.as_deref())
                .collect::<Vec<_>>()
        }));
    }
    let blocking = [
        "BATCH_EMPTY",
        "IMPORT_SEQUENCE_GAP",
        "DUPLICATE_PAGE_ARTIFACT",
        "GROUP_PAGE_COUNT_MISMATCH",
        "STUDENT_GROUP_EXCEEDS_ROSTER",
        "PAGE_TYPE_CYCLE_MISMATCH",
    ]
    .iter()
    .any(|code| issues.contains(*code));
    let route = if blocking {
        "blocked"
    } else if issues.is_empty() {
        "preview_ready"
    } else {
        "review_required"
    };
    let issue_codes_json = serde_json::json!({
        "schema_version": 1,
        "codes": issues.iter().collect::<Vec<_>>()
    });
    let grouping_json = serde_json::json!({
        "schema_version": 1,
        "policy": "natural_order_fixed_cycle_student_no_ascending_v1",
        "page_count": pages.len(),
        "expected_pages_per_attempt": expected_pages,
        "class_roster_count": roster.len(),
        "student_group_count": groups.len(),
        "student_assignment_state": if safe_to_assign_roster { "suggested" } else { "withheld" },
        "groups": groups
    });
    let snapshot = serde_json::json!({
        "schema_version": 1,
        "ingest_batch_id": input.ingest_batch_id,
        "import_order_revision_id": order.id,
        "material_type_revision_id": material.as_ref().map(|value| value.id),
        "expected_pages_per_attempt": expected_pages,
        "route": route,
        "issues": issue_codes_json,
        "grouping": grouping_json
    });
    let snapshot_hash = hashing::sha256_hex(
        &serde_json::to_vec(&snapshot)
            .map_err(|error| CoreError::Parse(format!("连续拍摄分组快照失败：{error}")))?,
    );
    let existing = conn
        .query_row(
            "SELECT id,public_id,ingest_batch_id,revision,snapshot_hash,
                    import_order_revision_id,material_type_revision_id,
                    expected_pages_per_attempt,route,student_group_count,
                    issue_codes_json,grouping_json,state
             FROM exam_ordered_grouping_revisions_v2
             WHERE ingest_batch_id=?1 AND snapshot_hash=?2 AND state='active'",
            (input.ingest_batch_id, &snapshot_hash),
            grouping_row,
        )
        .optional()?;
    if let Some(existing) = existing {
        return Ok(existing);
    }
    let now = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    let issue_codes_json = issue_codes_json.to_string();
    let grouping_json = grouping_json.to_string();
    let tx = conn.unchecked_transaction()?;
    let revision: i64 = tx.query_row(
        "SELECT COALESCE(MAX(revision),0)+1 FROM exam_ordered_grouping_revisions_v2
         WHERE ingest_batch_id=?1",
        [input.ingest_batch_id],
        |row| row.get(0),
    )?;
    tx.execute(
        "UPDATE exam_ordered_grouping_revisions_v2 SET state='superseded'
         WHERE ingest_batch_id=?1 AND state='active'",
        [input.ingest_batch_id],
    )?;
    tx.execute(
        "INSERT INTO exam_ordered_grouping_revisions_v2
         (public_id,ingest_batch_id,revision,snapshot_hash,import_order_revision_id,
          material_type_revision_id,expected_pages_per_attempt,route,student_group_count,
          issue_codes_json,grouping_json,state,created_by_type,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,'active',?12,?13,?14)",
        params![
            &public_id,
            input.ingest_batch_id,
            revision,
            &snapshot_hash,
            order.id,
            material.as_ref().map(|value| value.id),
            expected_pages,
            route,
            groups.len() as i64,
            &issue_codes_json,
            &grouping_json,
            input.created_by_type,
            input.created_by.map(str::trim),
            &now,
        ],
    )?;
    let id = tx.last_insert_rowid();
    append_audit(
        &tx,
        &AuditInput {
            idempotency_key: &format!("exam:ordered-grouping:{public_id}:active"),
            actor_type,
            actor_id: input.created_by.map(str::trim),
            action: "exam.ordered_grouping.activated",
            object_type: "exam_ordered_grouping",
            object_id: &public_id,
            revision,
            now: &now,
        },
    )?;
    tx.commit()?;
    conn.query_row(
        "SELECT id,public_id,ingest_batch_id,revision,snapshot_hash,
                import_order_revision_id,material_type_revision_id,
                expected_pages_per_attempt,route,student_group_count,
                issue_codes_json,grouping_json,state
         FROM exam_ordered_grouping_revisions_v2 WHERE id=?1",
        [id],
        grouping_row,
    )
    .map_err(Into::into)
}

pub fn confirm_material_type(
    conn: &Connection,
    batch_id: i64,
    material_type: &str,
    teacher: &str,
) -> CoreResult<MaterialTypeRevision> {
    record_material_type(
        conn,
        &NewMaterialTypeRevision {
            ingest_batch_id: batch_id,
            material_type,
            confidence: 1.0,
            evidence_json: r#"{"schema_version":1,"source":"teacher_one_time_confirmation"}"#,
            decision: "teacher_confirmed",
            created_by_type: "teacher",
            created_by: Some(teacher),
            confirmed_by: Some(teacher),
        },
    )
}

pub fn parse_codes(json: &str, field: &str) -> CoreResult<Vec<String>> {
    let value: Value = serde_json::from_str(json)
        .map_err(|error| CoreError::Parse(format!("{field}读取失败：{error}")))?;
    Ok(value
        .get("codes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::papers::{
        create_or_get_ingest_batch, register_ingest_page, NewIngestBatch, NewIngestPage,
    };
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn seed() -> Connection {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        conn.execute_batch(
            r#"INSERT INTO subjects(name) VALUES ('历史');
               INSERT INTO classes(name,term) VALUES ('八年级一班','2026秋');
               INSERT INTO students(student_no,name,class_id,enabled)
                 VALUES ('1','小一',1,1),('2','小二',1,1),('10','小十',1,1);
               INSERT INTO k1_textbook_editions
                 (public_id,subject_id,publisher_code,edition_code,title,grade,volume,state,created_at)
                 VALUES ('edition',1,'PEP','2024','中国历史八上','8','upper','active','2026-07-15T08:00:00.000Z');
               INSERT INTO k1_knowledge_maps
                 (public_id,textbook_edition_id,revision,state,created_at,confirmed_at)
                 VALUES ('map',1,1,'confirmed','2026-07-15T08:00:00.000Z','2026-07-15T08:00:00.000Z');
               INSERT INTO k1_questions
                 (public_id,owner_scope,owner_id,rights_status,sharing_allowed,created_at)
                 VALUES ('q','personal','teacher','unknown',0,'2026-07-15T08:00:00.000Z');
               INSERT INTO k1_question_versions
                 (public_id,question_id,revision,question_type,stem,max_score,content_hash,quality_level,state,created_at)
                 VALUES ('qv',1,1,'true_false','测试题',1,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','L3','published','2026-07-15T08:00:00.000Z');
               INSERT INTO k1_answer_key_versions
                 (public_id,question_version_id,revision,answer_json,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('ak',1,1,'{"schema_version":1,"correct":true}','confirmed','2026-07-15T08:00:00.000Z','teacher','2026-07-15T08:00:00.000Z');
               INSERT INTO k1_rubric_versions
                 (public_id,question_version_id,revision,max_score,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('rv',1,1,1,'confirmed','2026-07-15T08:00:00.000Z','teacher','2026-07-15T08:00:00.000Z');
               INSERT INTO k1_link_sets
                 (public_id,question_version_id,knowledge_map_id,revision,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('ls',1,1,1,'confirmed','2026-07-15T08:00:00.000Z','teacher','2026-07-15T08:00:00.000Z');
               INSERT INTO exam_assessments_v2
                 (public_id,title,class_id,assessment_context,evidence_policy,state,created_by,created_at,updated_at)
                 VALUES ('a','连续拍摄测试',1,'homework','include','active','teacher','2026-07-15T08:00:00.000Z','2026-07-15T08:00:00.000Z');
               INSERT INTO exam_assessment_versions_v2
                 (public_id,assessment_id,revision,item_set_hash,template_version,state,created_at,confirmed_by,confirmed_at)
                 VALUES ('av',1,1,'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa','fixed-v1','confirmed','2026-07-15T08:00:00.000Z','teacher','2026-07-15T08:00:00.000Z');
               INSERT INTO exam_assessment_items_v2
                 (public_id,assessment_version_id,question_version_id,answer_key_version_id,rubric_version_id,link_set_id,order_index,score,presentation_snapshot_json,state,created_at)
                 VALUES ('ai',1,1,1,1,1,0,1,'{"schema_version":1}','active','2026-07-15T08:00:00.000Z');"#,
        )
        .unwrap();
        conn
    }

    fn register_artifact(conn: &Connection, index: i64) -> i64 {
        conn.execute(
            "INSERT INTO artifacts
             (public_id,kind,sha256,mime_type,byte_size,original_name,archived_path,
              processing_version,privacy_class,archive_status,created_at)
             VALUES (?1,'image',?2,'image/jpeg',10,?3,?4,'test-v1','student_sensitive','ready',?5)",
            params![
                ids::new_public_id(),
                format!("{index:064}"),
                format!("IMG_{index}.jpg"),
                format!("/tmp/IMG_{index}.jpg"),
                "2026-07-15T08:00:00.000Z"
            ],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn seed_batch(conn: &Connection, pages: i64) -> (i64, Vec<ImportOrderEntry>) {
        let batch = create_or_get_ingest_batch(
            conn,
            &NewIngestBatch {
                assessment_version_id: 1,
                source_kind: "image_folder",
                idempotency_key: "ordered-test",
                created_by: "teacher",
            },
        )
        .unwrap();
        let mut entries = Vec::new();
        for index in 0..pages {
            let artifact_id = register_artifact(conn, index + 1);
            conn.execute(
                "INSERT INTO exam_fixed_input_documents_v2
                 (public_id,ingest_batch_id,source_artifact_id,document_role,source_format,
                  import_index,page_count,idempotency_key,state,created_by,created_at,updated_at)
                 VALUES (?1,?2,?3,'student_work','jpeg',?4,1,?5,'registered','teacher',?6,?6)",
                params![
                    format!("document-{index}"),
                    batch.id,
                    artifact_id,
                    index,
                    format!("document-key-{index}"),
                    "2026-07-15T08:00:00.000Z"
                ],
            )
            .unwrap();
            register_ingest_page(
                conn,
                &NewIngestPage {
                    batch_id: batch.id,
                    source_artifact_id: artifact_id,
                    import_index: index,
                    expected_page_no: Some(1),
                },
            )
            .unwrap();
            entries.push(ImportOrderEntry {
                source_artifact_id: artifact_id,
                original_name: format!("IMG_{}.jpg", index + 1),
                original_request_index: index,
                sorted_index: index,
                page_count: 1,
                capture_time: None,
                file_created_ms: None,
                file_modified_ms: None,
            });
        }
        (batch.id, entries)
    }

    #[test]
    fn natural_sort_handles_numeric_runs_without_integer_overflow() {
        let mut names = vec!["IMG_10.jpg", "IMG_2.jpg", "IMG_0002.jpg", "IMG_1.jpg"];
        names.sort_by(|left, right| natural_name_cmp(left, right));
        assert_eq!(
            names,
            vec!["IMG_1.jpg", "IMG_2.jpg", "IMG_0002.jpg", "IMG_10.jpg"]
        );
    }

    #[test]
    fn low_confidence_fallback_requires_one_material_confirmation() {
        let suggestion = suggest_material_type(&["IMG_0001.jpg".into()]);
        assert_eq!(suggestion.material_type, "ordinary_paper");
        assert!(suggestion.confidence < MATERIAL_AUTO_ACCEPT_CONFIDENCE);
        let answer_sheet = suggest_material_type(&["八上答题卡01.jpg".into()]);
        assert_eq!(answer_sheet.material_type, "answer_sheet");
        assert!(answer_sheet.confidence >= MATERIAL_AUTO_ACCEPT_CONFIDENCE);
    }

    #[test]
    fn exact_roster_count_produces_student_number_order_preview() {
        let conn = seed();
        let (batch_id, entries) = seed_batch(&conn, 3);
        record_import_order(
            &conn,
            &NewImportOrderRevision {
                ingest_batch_id: batch_id,
                sort_policy: "filename_natural_exif_filetime_crosscheck_v1",
                order_confidence: 0.8,
                entries: &entries,
                conflict_codes: &[],
                created_by_type: "system",
                created_by: None,
            },
        )
        .unwrap();
        confirm_material_type(&conn, batch_id, "ordinary_paper", "teacher").unwrap();
        let grouping = preview_ordered_grouping(
            &conn,
            &OrderedGroupingInput {
                ingest_batch_id: batch_id,
                expected_pages_per_attempt: 1,
                created_by_type: "system",
                created_by: None,
            },
        )
        .unwrap();
        assert_eq!(grouping.route, "preview_ready");
        let value: Value = serde_json::from_str(&grouping.grouping_json).unwrap();
        let numbers = value["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|group| group["student_no"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(numbers, vec!["1", "2", "10"]);
    }

    #[test]
    fn missing_student_scope_withholds_assignment_instead_of_shifting_roster() {
        let conn = seed();
        let (batch_id, entries) = seed_batch(&conn, 2);
        record_import_order(
            &conn,
            &NewImportOrderRevision {
                ingest_batch_id: batch_id,
                sort_policy: "filename_natural_exif_filetime_crosscheck_v1",
                order_confidence: 0.8,
                entries: &entries,
                conflict_codes: &[],
                created_by_type: "system",
                created_by: None,
            },
        )
        .unwrap();
        confirm_material_type(&conn, batch_id, "dictation", "teacher").unwrap();
        let grouping = preview_ordered_grouping(
            &conn,
            &OrderedGroupingInput {
                ingest_batch_id: batch_id,
                expected_pages_per_attempt: 1,
                created_by_type: "system",
                created_by: None,
            },
        )
        .unwrap();
        assert_eq!(grouping.route, "review_required");
        let codes = parse_codes(&grouping.issue_codes_json, "分组原因").unwrap();
        assert!(codes
            .iter()
            .any(|code| code == "STUDENT_RANGE_OR_ABSENCE_CONFIRMATION_REQUIRED"));
        let value: Value = serde_json::from_str(&grouping.grouping_json).unwrap();
        assert_eq!(value["student_assignment_state"], "withheld");
    }

    #[test]
    fn material_correction_appends_revision_and_preserves_history() {
        let conn = seed();
        let (batch_id, entries) = seed_batch(&conn, 1);
        record_import_order(
            &conn,
            &NewImportOrderRevision {
                ingest_batch_id: batch_id,
                sort_policy: "filename_natural_exif_filetime_crosscheck_v1",
                order_confidence: 0.8,
                entries: &entries,
                conflict_codes: &[],
                created_by_type: "system",
                created_by: None,
            },
        )
        .unwrap();
        let suggestion = record_material_type(
            &conn,
            &NewMaterialTypeRevision {
                ingest_batch_id: batch_id,
                material_type: "ordinary_paper",
                confidence: 0.45,
                evidence_json: r#"{"schema_version":1,"rule":"fallback"}"#,
                decision: "suggested",
                created_by_type: "system",
                created_by: None,
                confirmed_by: None,
            },
        )
        .unwrap();
        let confirmed = confirm_material_type(&conn, batch_id, "answer_sheet", "teacher").unwrap();
        assert_eq!((suggestion.revision, confirmed.revision), (1, 2));
        assert_eq!(confirmed.material_type, "answer_sheet");
        let states: Vec<String> = conn
            .prepare("SELECT state FROM exam_material_type_revisions_v2 ORDER BY revision")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(states, vec!["superseded", "active"]);
    }
}
