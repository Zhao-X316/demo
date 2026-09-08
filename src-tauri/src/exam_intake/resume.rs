//! Restore upload context from domain records without preparing files or issuing commands.
use super::*;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IntakeResume {
    pub class_id: i64,
    pub assessment_version_id: i64,
    pub result: FixedIntakeResult,
    pub processed_page_ids: Vec<i64>,
    pub processing_history: Vec<ProcessingHistory>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessingHistory {
    pub id: i64,
    pub stage: String,
    pub status: String,
    pub message: Option<String>,
    pub updated_at: String,
}

pub(crate) fn read(conn: &Connection, batch_id: i64) -> CoreResult<IntakeResume> {
    let (public_id, version_id, class_id): (String, i64, i64) = conn
        .query_row(
            "SELECT b.public_id,b.assessment_version_id,a.class_id FROM exam_ingest_batches_v2 b
         JOIN exam_assessment_versions_v2 v ON v.id=b.assessment_version_id
         JOIN exam_assessments_v2 a ON a.id=v.assessment_id WHERE b.id=?1 AND b.state<>'voided'",
            [batch_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("本次上传任务已不可用".into()))?;
    let order = ordered_intake::current_import_order(conn, batch_id)?;
    let material = ordered_intake::current_material(conn, batch_id)?
        .ok_or_else(|| CoreError::Invalid("材料登记尚未完成，请检查原上传结果".into()))?;
    let grouping = ordered_intake::current_grouping(conn, batch_id)?;
    let pages = grouping
        .expected_pages_per_attempt
        .ok_or_else(|| CoreError::Invalid("本批尚未确认每份页数".into()))?;
    let decision = ordered_intake::current_grouping_decision(conn, batch_id)?;
    let (confirmed, first, last) = grouping_scope_summary(decision.as_ref())?;
    let activation = ordered_activation::current_activation(conn, batch_id)?;
    let preflight = fixed_paper::inspect_fixed_paper_batch(
        conn,
        &FixedPaperPreflightInput {
            ingest_batch_id: batch_id,
            expected_pages_per_attempt: pages,
            created_by_type: "teacher",
            created_by: Some(ACTOR),
        },
    )?;
    let mut stmt = conn.prepare(
        "SELECT d.document_role,d.source_format,a.original_name,d.page_count
        FROM exam_fixed_input_documents_v2 d JOIN artifacts a ON a.id=d.source_artifact_id
        WHERE d.ingest_batch_id=?1 AND d.state<>'voided' ORDER BY d.import_index,d.id",
    )?;
    let documents = stmt
        .query_map([batch_id], |row| {
            Ok(FixedIntakeDocumentSummary {
                role: row.get(0)?,
                format: row.get(1)?,
                original_name: row
                    .get::<_, Option<String>>(2)?
                    .unwrap_or_else(|| "已归档材料".into()),
                page_count: row.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let student_document_count = documents
        .iter()
        .filter(|d| d.role == "student_work")
        .count() as i64;
    let student_page_count = documents
        .iter()
        .filter(|d| d.role == "student_work")
        .map(|d| d.page_count)
        .sum();
    let answer_document_count = documents
        .iter()
        .filter(|d| d.role == "answer_source")
        .count() as i64;
    let needs_type = material.decision != "teacher_confirmed" && material.confidence < 0.85;
    let grouping_issue_codes = ordered_intake::parse_codes(&grouping.issue_codes_json, "页面分组")?;
    let next = if needs_type {
        "确认本批材料类型"
    } else if grouping.route == "blocked" {
        "检查缺页或页面顺序"
    } else if !confirmed {
        "确认本批学生范围"
    } else if activation.is_none() {
        "检查页面清晰度与学生归属"
    } else {
        "继续处理材料，或核对已有识别结果"
    };
    // Segmentation is a persisted structure fact, not proof that OCR or grading succeeded.
    let mut processed = conn.prepare(
        "SELECT id FROM exam_ingest_pages_v2 WHERE batch_id=?1 AND state='segmented' ORDER BY id",
    )?;
    let processed_page_ids = processed
        .query_map([batch_id], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let processing_history = read_history(conn, batch_id, version_id)?;
    Ok(IntakeResume {
        class_id,
        assessment_version_id: version_id,
        processed_page_ids,
        processing_history,
        result: FixedIntakeResult {
            batch_id,
            batch_public_id: public_id,
            documents,
            student_document_count,
            student_page_count,
            answer_document_count,
            route: preflight.route,
            target_count: preflight.target_count,
            ready_count: preflight.ready_count,
            review_count: preflight.review_count,
            blocked_count: preflight.blocked_count,
            completed_count: preflight.completed_count,
            reason_codes: ordered_intake::parse_codes(&preflight.reason_codes_json, "预检")?,
            order_policy: order.sort_policy,
            order_confidence: order.order_confidence,
            order_conflict_codes: ordered_intake::parse_codes(
                &order.conflict_codes_json,
                "导入顺序",
            )?,
            material_type: material.material_type,
            material_type_decision: material.decision,
            material_type_confidence: material.confidence,
            material_type_needs_confirmation: needs_type,
            grouping_route: grouping.route,
            student_group_count: grouping.student_group_count,
            grouping_issue_codes,
            expected_pages_per_attempt: pages,
            page_cycle_source: "saved_grouping".into(),
            page_cycle_confidence: 0.0,
            page_cycle_needs_teacher_input: false,
            grouping_roster: ordered_intake::roster(conn, batch_id)?
                .into_iter()
                .map(|s| GroupingRosterStudent {
                    student_id: s.id,
                    student_no: s.student_no,
                    student_name: s.name,
                })
                .collect(),
            grouping_confirmed: confirmed,
            grouping_first_student_no: first,
            grouping_last_student_no: last,
            quality_review_completed: activation.is_some(),
            mapped_group_count: activation
                .as_ref()
                .map(|a| a.mapped_group_count)
                .unwrap_or(0),
            rejected_group_count: activation
                .as_ref()
                .map(|a| a.rejected_group_count)
                .unwrap_or(0),
            next_action: next.into(),
        },
    })
}

pub(crate) fn read_history(
    conn: &Connection,
    batch_id: i64,
    version_id: i64,
) -> CoreResult<Vec<ProcessingHistory>> {
    // Existing run facts only. Rendering this history never resumes provider calls.
    let mut history = conn.prepare("SELECT r.id,r.run_type,r.status,
        CASE WHEN json_valid(r.error_meta_json) THEN json_extract(r.error_meta_json,'$.safe_message') ELSE NULL END,
        COALESCE(r.finished_at,r.started_at,r.created_at)
        FROM ai_runs r WHERE r.source_module='exam' AND r.status<>'voided' AND (
          (r.business_ref_type IN ('ingest_batch','fixed_answer_source') AND r.business_ref_id=CAST(?1 AS TEXT)) OR
          (r.business_ref_type='ingest_page' AND EXISTS (
            SELECT 1 FROM exam_ingest_pages_v2 p WHERE p.batch_id=?1 AND p.state<>'voided'
              AND r.business_ref_id=CAST(p.id AS TEXT) AND r.input_artifact_id=p.source_artifact_id)) OR
          (r.business_ref_type='answer_region_revision' AND EXISTS (
            SELECT 1 FROM exam_answer_region_revisions_v2 ar JOIN exam_ingest_pages_v2 p ON p.id=ar.page_id
            WHERE ar.state='active' AND p.state<>'voided' AND p.batch_id=?1 AND r.business_ref_id=CAST(ar.id AS TEXT))) OR
          (r.business_ref_type='assessment_page' AND r.business_ref_id LIKE CAST(?2 AS TEXT)||':%'))
        AND NOT EXISTS(SELECT 1 FROM ai_runs newer WHERE newer.id>r.id AND newer.status<>'voided'
          AND newer.run_type=r.run_type AND newer.business_ref_type=r.business_ref_type AND newer.business_ref_id=r.business_ref_id)
        ORDER BY r.id DESC")?;
    let processing_history = history
        .query_map([batch_id, version_id], |row| {
            Ok(ProcessingHistory {
                id: row.get(0)?,
                stage: row.get(1)?,
                status: row.get(2)?,
                message: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(processing_history)
}
