use rusqlite::{params, Connection, OptionalExtension};
use suite_core::domain::{hashing, time};
use suite_core::error::{CoreError, CoreResult};

use super::{
    accessible_question_clause, canonical_rate, load_target_components, required,
    ImpactTargetVersion, PerformanceContextBreakdown, QuestionImpactReviewCase,
    QuestionImpactReviewCaseCatalog, QuestionImpactRow, QuestionPerformanceCatalog,
    QuestionPerformanceItem, QuestionVersionImpactPreview, TargetIds,
    QUESTION_IMPACT_REVIEW_RULE_VERSION, QUESTION_IMPACT_RULE_VERSION,
    QUESTION_PERFORMANCE_RULE_VERSION, QUESTION_PERFORMANCE_SCHEMA_VERSION,
};

pub fn list_question_performance(
    conn: &Connection,
    owner_id: &str,
    limit: i64,
) -> CoreResult<QuestionPerformanceCatalog> {
    required(owner_id, "题库老师")?;
    if !(1..=500).contains(&limit) {
        return Err(CoreError::Invalid(
            "题目表现列表数量必须在 1 到 500 之间".into(),
        ));
    }
    let sql = format!(
        "WITH published_response AS (
           SELECT item.question_version_id,assessment.id AS assessment_id,
                  assessment.assessment_context,attempt.id AS attempt_id,
                  attempt.attempt_kind,decision.teacher_score,item.score,
                  publication.published_at
           FROM exam_attempts_v2 attempt
           JOIN exam_grade_publications_v2 publication
             ON publication.id=attempt.active_publication_id
            AND publication.state='published'
           JOIN exam_grade_publication_items_v2 publication_item
             ON publication_item.publication_id=publication.id
            AND publication_item.attempt_id=attempt.id
           JOIN exam_grade_publication_decisions_v2 publication_decision
             ON publication_decision.publication_item_id=publication_item.id
            AND publication_decision.attempt_id=attempt.id
           JOIN exam_grade_decisions_v2 decision
             ON decision.id=publication_decision.grade_decision_id
            AND decision.attempt_id=attempt.id
            AND decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
           JOIN exam_assessment_items_v2 item
             ON item.id=decision.assessment_item_id
            AND item.assessment_version_id=attempt.assessment_version_id
            AND item.state='active'
           JOIN exam_assessment_versions_v2 assessment_version
             ON assessment_version.id=attempt.assessment_version_id
           JOIN exam_assessments_v2 assessment
             ON assessment.id=assessment_version.assessment_id
         )
         SELECT version.id,version.public_id,version.revision,question.owner_scope,
                version.question_type,version.stem,version.max_score,
                version.quality_level,version.state,
                (SELECT COUNT(DISTINCT assessment_version.assessment_id)
                 FROM exam_assessment_items_v2 item
                 JOIN exam_assessment_versions_v2 assessment_version
                   ON assessment_version.id=item.assessment_version_id
                 WHERE item.question_version_id=version.id),
                COUNT(response.attempt_id),
                COALESCE(SUM(CASE WHEN response.teacher_score>=response.score-0.000001
                                  THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN response.teacher_score>0.000001
                                    AND response.teacher_score<response.score-0.000001
                                  THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN response.teacher_score<=0.000001
                                  THEN 1 ELSE 0 END),0),
                AVG(CASE WHEN response.score>0
                         THEN response.teacher_score/response.score END),
                AVG(CASE WHEN response.attempt_id IS NULL THEN NULL
                         WHEN response.teacher_score>=response.score-0.000001 THEN 1.0
                         ELSE 0.0 END),
                COALESCE(SUM(CASE WHEN response.attempt_kind='first' THEN 1 ELSE 0 END),0),
                COALESCE(SUM(CASE WHEN response.attempt_kind='correction' THEN 1 ELSE 0 END),0),
                MAX(response.published_at),
                EXISTS(
                  SELECT 1 FROM exam_assessment_items_v2 item
                  WHERE item.question_version_id=version.id
                    AND (
                      item.answer_key_version_id<>(
                        SELECT target.id FROM k1_answer_key_versions target
                        WHERE target.question_version_id=version.id AND target.state='confirmed'
                        ORDER BY target.revision DESC,target.id DESC LIMIT 1
                      )
                      OR item.rubric_version_id<>(
                        SELECT target.id FROM k1_rubric_versions target
                        WHERE target.question_version_id=version.id AND target.state='confirmed'
                        ORDER BY target.revision DESC,target.id DESC LIMIT 1
                      )
                      OR item.link_set_id<>(
                        SELECT target.id FROM k1_link_sets target
                        WHERE target.question_version_id=version.id AND target.state='confirmed'
                        ORDER BY target.revision DESC,target.id DESC LIMIT 1
                      )
                    )
                )
         FROM k1_question_versions version
         JOIN k1_questions question ON question.id=version.question_id
         LEFT JOIN published_response response ON response.question_version_id=version.id
         WHERE ({})
           AND (
             response.attempt_id IS NOT NULL
             OR EXISTS (
               SELECT 1 FROM exam_assessment_items_v2 used_item
               WHERE used_item.question_version_id=version.id
             )
           )
         GROUP BY version.id
         ORDER BY COUNT(response.attempt_id) DESC,version.created_at DESC,version.id DESC
         LIMIT ?2",
        accessible_question_clause()
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(params![owner_id.trim(), limit], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            QuestionPerformanceItem {
                question_version_public_id: row.get(1)?,
                revision: row.get(2)?,
                owner_scope: row.get(3)?,
                question_type: row.get(4)?,
                stem: row.get(5)?,
                max_score: row.get(6)?,
                quality_level: row.get(7)?,
                state: row.get(8)?,
                assessment_usage_count: row.get(9)?,
                published_response_count: row.get(10)?,
                full_credit_count: row.get(11)?,
                partial_credit_count: row.get(12)?,
                zero_score_count: row.get(13)?,
                average_score_rate: canonical_rate(row.get(14)?),
                full_credit_rate: canonical_rate(row.get(15)?),
                first_attempt_count: row.get(16)?,
                correction_attempt_count: row.get(17)?,
                latest_published_at: row.get(18)?,
                context_breakdown: Vec::new(),
                has_version_update_impact: row.get(19)?,
            },
        ))
    })?;
    let mut items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut context_statement = conn.prepare(
        "SELECT assessment.assessment_context,COUNT(*),
                AVG(decision.teacher_score/item.score),
                AVG(CASE WHEN decision.teacher_score>=item.score-0.000001
                         THEN 1.0 ELSE 0.0 END)
         FROM exam_attempts_v2 attempt
         JOIN exam_grade_publications_v2 publication
           ON publication.id=attempt.active_publication_id AND publication.state='published'
         JOIN exam_grade_publication_items_v2 publication_item
           ON publication_item.publication_id=publication.id
          AND publication_item.attempt_id=attempt.id
         JOIN exam_grade_publication_decisions_v2 publication_decision
           ON publication_decision.publication_item_id=publication_item.id
          AND publication_decision.attempt_id=attempt.id
         JOIN exam_grade_decisions_v2 decision
           ON decision.id=publication_decision.grade_decision_id
          AND decision.confirmation_level IN ('teacher_accepted','teacher_corrected')
         JOIN exam_assessment_items_v2 item
           ON item.id=decision.assessment_item_id
          AND item.assessment_version_id=attempt.assessment_version_id
          AND item.question_version_id=?1
          AND item.state='active'
         JOIN exam_assessment_versions_v2 assessment_version
           ON assessment_version.id=attempt.assessment_version_id
         JOIN exam_assessments_v2 assessment ON assessment.id=assessment_version.assessment_id
         GROUP BY assessment.assessment_context
         ORDER BY COUNT(*) DESC,assessment.assessment_context",
    )?;
    for (question_version_id, item) in &mut items {
        let context_rows = context_statement.query_map([*question_version_id], |row| {
            Ok(PerformanceContextBreakdown {
                assessment_context: row.get(0)?,
                published_response_count: row.get(1)?,
                average_score_rate: canonical_rate(row.get(2)?),
                full_credit_rate: canonical_rate(row.get(3)?),
            })
        })?;
        item.context_breakdown = context_rows.collect::<rusqlite::Result<Vec<_>>>()?;
    }
    Ok(QuestionPerformanceCatalog {
        schema_version: QUESTION_PERFORMANCE_SCHEMA_VERSION,
        rule_version: QUESTION_PERFORMANCE_RULE_VERSION.into(),
        calculated_at: time::utc_now_rfc3339(),
        items: items.into_iter().map(|(_, item)| item).collect(),
        boundary_note:
            "只统计每份答卷当前有效发布快照中的老师确认得分；满分率不是永久难度，班级和作业场景不会被混写回题目。"
                .into(),
    })
}

pub(super) fn load_target(
    conn: &Connection,
    owner_id: &str,
    question_version_public_id: &str,
) -> CoreResult<(TargetIds, String, String, ImpactTargetVersion)> {
    required(question_version_public_id, "题目版本")?;
    let sql = format!(
        "SELECT version.id,version.question_type,version.stem
         FROM k1_question_versions version
         JOIN k1_questions question ON question.id=version.question_id
         WHERE version.public_id=?2 AND ({})",
        accessible_question_clause()
    );
    let (question_version_id, question_type, stem): (i64, String, String) = conn
        .query_row(
            &sql,
            params![owner_id.trim(), question_version_public_id.trim()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::NotFound("可访问题目版本".into()))?;
    let (answer_id, answer_public_id, answer_revision): (i64, String, i64) = conn
        .query_row(
            "SELECT id,public_id,revision FROM k1_answer_key_versions
             WHERE question_version_id=?1 AND state='confirmed'
             ORDER BY revision DESC,id DESC LIMIT 1",
            [question_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("题目没有已确认答案版本".into()))?;
    let (rubric_id, rubric_public_id, rubric_revision): (i64, String, i64) = conn
        .query_row(
            "SELECT id,public_id,revision FROM k1_rubric_versions
             WHERE question_version_id=?1 AND state='confirmed'
             ORDER BY revision DESC,id DESC LIMIT 1",
            [question_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("题目没有已确认评分规则版本".into()))?;
    let (link_id, link_public_id, link_revision): (i64, String, i64) = conn
        .query_row(
            "SELECT id,public_id,revision FROM k1_link_sets
             WHERE question_version_id=?1 AND state='confirmed'
             ORDER BY revision DESC,id DESC LIMIT 1",
            [question_version_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?
        .ok_or_else(|| CoreError::Invalid("题目没有已确认知识能力链接版本".into()))?;
    Ok((
        TargetIds {
            question_version_id,
            target_answer_key_version_id: answer_id,
            target_rubric_version_id: rubric_id,
            target_link_set_id: link_id,
        },
        question_type,
        stem,
        ImpactTargetVersion {
            answer_key_version_public_id: answer_public_id,
            answer_key_revision: answer_revision,
            rubric_version_public_id: rubric_public_id,
            rubric_revision,
            link_set_public_id: link_public_id,
            link_set_revision: link_revision,
        },
    ))
}

pub fn preview_question_version_impact(
    conn: &Connection,
    owner_id: &str,
    question_version_public_id: &str,
) -> CoreResult<QuestionVersionImpactPreview> {
    required(owner_id, "题库老师")?;
    let (target_ids, question_type, stem, target) =
        load_target(conn, owner_id, question_version_public_id)?;
    let mut statement = conn.prepare(
        "SELECT assessment.public_id,assessment.title,class.name,
                assessment_version.public_id,assessment_version.revision,item.public_id,
                answer.public_id,rubric.public_id,links.public_id,
                item.answer_key_version_id<>?2,
                item.rubric_version_id<>?3,
                item.link_set_id<>?4,
                (SELECT COUNT(*) FROM exam_attempts_v2 attempt
                 WHERE attempt.assessment_version_id=item.assessment_version_id
                   AND attempt.active_publication_id IS NULL AND attempt.state<>'voided'),
                (SELECT COUNT(*) FROM exam_attempts_v2 attempt
                 JOIN exam_grade_publications_v2 publication
                   ON publication.id=attempt.active_publication_id
                  AND publication.state='published'
                 WHERE attempt.assessment_version_id=item.assessment_version_id),
                (SELECT COUNT(DISTINCT evidence.id)
                 FROM exam_grade_decisions_v2 decision
                 JOIN exam_attempts_v2 attempt ON attempt.id=decision.attempt_id
                 JOIN learning_evidence evidence
                   ON evidence.decision_ref_type='grade_decision'
                  AND evidence.decision_ref_id=decision.public_id
                  AND evidence.state='active'
                 WHERE decision.assessment_item_id=item.id
                   AND attempt.assessment_version_id=item.assessment_version_id),
                (SELECT COUNT(DISTINCT profile_link.snapshot_id)
                 FROM exam_grade_decisions_v2 decision
                 JOIN learning_evidence evidence
                   ON evidence.decision_ref_type='grade_decision'
                  AND evidence.decision_ref_id=decision.public_id
                  AND evidence.state='active'
                 JOIN profile_evidence_links profile_link
                   ON profile_link.learning_evidence_id=evidence.id
                 WHERE decision.assessment_item_id=item.id)
                ,
                assessment_version.id=COALESCE(
                  (SELECT default_selection.selected_assessment_version_id
                   FROM exam_assessment_default_version_selections_v2 default_selection
                   WHERE default_selection.assessment_id=assessment.id
                   ORDER BY default_selection.revision DESC,default_selection.id DESC
                   LIMIT 1),
                  (SELECT latest_version.id
                   FROM exam_assessment_versions_v2 latest_version
                   WHERE latest_version.assessment_id=assessment.id
                     AND latest_version.state='confirmed'
                   ORDER BY latest_version.revision DESC,latest_version.id DESC
                   LIMIT 1)
                )
         FROM exam_assessment_items_v2 item
         JOIN exam_assessment_versions_v2 assessment_version
           ON assessment_version.id=item.assessment_version_id
         JOIN exam_assessments_v2 assessment ON assessment.id=assessment_version.assessment_id
         JOIN classes class ON class.id=assessment.class_id
         JOIN k1_answer_key_versions answer ON answer.id=item.answer_key_version_id
         JOIN k1_rubric_versions rubric ON rubric.id=item.rubric_version_id
         JOIN k1_link_sets links ON links.id=item.link_set_id
         WHERE item.question_version_id=?1
           AND item.state='active'
           AND (item.answer_key_version_id<>?2
                OR item.rubric_version_id<>?3
                OR item.link_set_id<>?4)
         ORDER BY assessment.created_at,assessment_version.revision,item.order_index,item.id",
    )?;
    let rows = statement.query_map(
        params![
            target_ids.question_version_id,
            target_ids.target_answer_key_version_id,
            target_ids.target_rubric_version_id,
            target_ids.target_link_set_id
        ],
        |row| {
            Ok(QuestionImpactRow {
                assessment_public_id: row.get(0)?,
                assessment_title: row.get(1)?,
                class_name: row.get(2)?,
                assessment_version_public_id: row.get(3)?,
                assessment_version_revision: row.get(4)?,
                assessment_item_public_id: row.get(5)?,
                source_answer_key_version_public_id: row.get(6)?,
                source_rubric_version_public_id: row.get(7)?,
                source_link_set_public_id: row.get(8)?,
                answer_changed: row.get(9)?,
                rubric_changed: row.get(10)?,
                link_changed: row.get(11)?,
                unpublished_attempt_count: row.get(12)?,
                published_attempt_count: row.get(13)?,
                active_learning_evidence_count: row.get(14)?,
                profile_snapshot_count: row.get(15)?,
                is_current_default: row.get(16)?,
            })
        },
    )?;
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let affected_assessment_count = rows
        .iter()
        .map(|row| row.assessment_public_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len() as i64;
    let affected_assessment_version_count = rows
        .iter()
        .map(|row| row.assessment_version_public_id.as_str())
        .collect::<std::collections::BTreeSet<_>>()
        .len() as i64;
    let unpublished_attempt_count = rows.iter().map(|row| row.unpublished_attempt_count).sum();
    let published_attempt_count = rows.iter().map(|row| row.published_attempt_count).sum();
    let active_learning_evidence_count = rows
        .iter()
        .map(|row| row.active_learning_evidence_count)
        .sum();
    let profile_snapshot_count = rows.iter().map(|row| row.profile_snapshot_count).sum();
    let hash_payload = serde_json::json!({
        "schema_version": QUESTION_PERFORMANCE_SCHEMA_VERSION,
        "rule_version": QUESTION_IMPACT_RULE_VERSION,
        "question_version_public_id": question_version_public_id.trim(),
        "target": &target,
        "rows": &rows
    });
    let preview_hash = hashing::sha256_hex(
        &serde_json::to_vec(&hash_payload)
            .map_err(|error| CoreError::Parse(format!("影响预览 hash 序列化失败：{error}")))?,
    );
    Ok(QuestionVersionImpactPreview {
        schema_version: QUESTION_PERFORMANCE_SCHEMA_VERSION,
        rule_version: QUESTION_IMPACT_RULE_VERSION.into(),
        calculated_at: time::utc_now_rfc3339(),
        preview_hash,
        question_version_public_id: question_version_public_id.trim().into(),
        question_type,
        stem,
        target,
        affected_assessment_count,
        affected_assessment_version_count,
        affected_item_count: rows.len() as i64,
        unpublished_attempt_count,
        published_attempt_count,
        active_learning_evidence_count,
        profile_snapshot_count,
        rows,
        boundary_note:
            "这里只冻结影响范围和老师选择；不会直接换绑作业、重算成绩、重新发布或改写既有学习图谱。"
                .into(),
    })
}

pub fn list_question_impact_review_cases(
    conn: &Connection,
    owner_id: &str,
    plan_public_id: &str,
) -> CoreResult<QuestionImpactReviewCaseCatalog> {
    required(owner_id, "题库老师")?;
    required(plan_public_id, "影响计划")?;
    let exists = conn
        .query_row(
            "SELECT 1 FROM exam_question_version_impact_plans_v2
             WHERE public_id=?1 AND planned_by=?2",
            params![plan_public_id.trim(), owner_id.trim()],
            |_| Ok(()),
        )
        .optional()?;
    if exists.is_none() {
        return Err(CoreError::NotFound("未找到当前老师的版本影响计划".into()));
    }
    let mut statement = conn.prepare(
        "SELECT review_case.public_id,task.public_id,plan.public_id,review_case.case_kind,
                assessment.title,class.name,student.student_no,student.name,
                question.public_id,question.question_type,question.stem,item.order_index+1,item.score,
                attempt.public_id,attempt.state,publication.public_id,decision.public_id,
                review_case.source_grade_decision_revision,review_case.source_teacher_score,
                review_case.source_point_results_json,review_case.source_snapshot_hash,
                COALESCE(
                  (SELECT transcription.result_state
                   FROM exam_subjective_transcription_revisions_v2 transcription
                   WHERE transcription.attempt_id=attempt.id
                     AND transcription.assessment_item_id=item.id
                     AND transcription.state='active'
                   ORDER BY transcription.revision DESC,transcription.id DESC LIMIT 1),
                  (SELECT observation.result_state
                   FROM exam_objective_observation_revisions_v2 observation
                   WHERE observation.attempt_id=attempt.id
                     AND observation.assessment_item_id=item.id
                     AND observation.state='active'
                   ORDER BY observation.revision DESC,observation.id DESC LIMIT 1)
                ),
                COALESCE(
                  (SELECT COALESCE(transcription.teacher_corrected_text,
                                   transcription.normalized_text,
                                   transcription.raw_ocr_text)
                   FROM exam_subjective_transcription_revisions_v2 transcription
                   WHERE transcription.attempt_id=attempt.id
                     AND transcription.assessment_item_id=item.id
                     AND transcription.state='active'
                   ORDER BY transcription.revision DESC,transcription.id DESC LIMIT 1),
                  (SELECT observation.observed_answer_json
                   FROM exam_objective_observation_revisions_v2 observation
                   WHERE observation.attempt_id=attempt.id
                     AND observation.assessment_item_id=item.id
                     AND observation.state='active'
                   ORDER BY observation.revision DESC,observation.id DESC LIMIT 1)
                ),
                COALESCE(
                  (SELECT artifact.archived_path
                   FROM exam_subjective_transcription_revisions_v2 transcription
                   JOIN exam_answer_region_revisions_v2 region
                     ON region.id=transcription.answer_region_revision_id
                   JOIN artifacts artifact ON artifact.id=region.crop_artifact_id
                   WHERE transcription.attempt_id=attempt.id
                     AND transcription.assessment_item_id=item.id
                     AND transcription.state='active'
                     AND artifact.archive_status='ready'
                   ORDER BY transcription.revision DESC,transcription.id DESC LIMIT 1),
                  (SELECT artifact.archived_path
                   FROM exam_objective_observation_revisions_v2 observation
                   JOIN exam_answer_region_revisions_v2 region
                     ON region.id=observation.answer_region_revision_id
                   JOIN artifacts artifact ON artifact.id=region.crop_artifact_id
                   WHERE observation.attempt_id=attempt.id
                     AND observation.assessment_item_id=item.id
                     AND observation.state='active'
                     AND artifact.archive_status='ready'
                   ORDER BY observation.revision DESC,observation.id DESC LIMIT 1)
                ),
                target_answer.public_id,target_answer.revision,target_answer.answer_json,
                target_rubric.public_id,target_rubric.revision,
                target_link.public_id,target_link.revision,
                review_case.prepared_by,review_case.prepared_at,review_case.state,
                resolution.public_id,resolved_decision.public_id,resolved_decision.teacher_score,
                resolution.resolved_at,active_publication.public_id,
                CASE WHEN resolution.id IS NOT NULL AND EXISTS (
                  SELECT 1
                  FROM exam_grade_publication_items_v2 resolved_publication_item
                  JOIN exam_grade_publication_decisions_v2 resolved_publication_decision
                    ON resolved_publication_decision.publication_item_id=resolved_publication_item.id
                   AND resolved_publication_decision.attempt_id=resolved_publication_item.attempt_id
                  WHERE resolved_publication_item.publication_id=attempt.active_publication_id
                    AND resolved_publication_item.attempt_id=attempt.id
                    AND resolved_publication_decision.grade_decision_id=resolution.grade_decision_id
                ) THEN 1 ELSE 0 END
         FROM exam_question_version_review_cases_v2 review_case
         JOIN exam_question_version_impact_tasks_v2 task
           ON task.id=review_case.impact_task_id
         JOIN exam_question_version_impact_plans_v2 plan
           ON plan.id=task.impact_plan_id
         JOIN exam_attempts_v2 attempt ON attempt.id=review_case.attempt_id
         JOIN students student ON student.id=attempt.student_id
         JOIN exam_assessment_items_v2 item ON item.id=review_case.assessment_item_id
         JOIN exam_assessment_versions_v2 assessment_version
           ON assessment_version.id=item.assessment_version_id
         JOIN exam_assessments_v2 assessment
           ON assessment.id=assessment_version.assessment_id
         JOIN classes class ON class.id=assessment.class_id
         JOIN k1_question_versions question ON question.id=item.question_version_id
         JOIN k1_answer_key_versions target_answer
           ON target_answer.id=review_case.target_answer_key_version_id
         JOIN k1_rubric_versions target_rubric
           ON target_rubric.id=review_case.target_rubric_version_id
         JOIN k1_link_sets target_link ON target_link.id=review_case.target_link_set_id
         LEFT JOIN exam_grade_publications_v2 publication
           ON publication.id=review_case.publication_id
         LEFT JOIN exam_grade_decisions_v2 decision
           ON decision.id=review_case.source_grade_decision_id
         LEFT JOIN exam_question_version_review_resolutions_v2 resolution
           ON resolution.review_case_id=review_case.id
         LEFT JOIN exam_grade_decisions_v2 resolved_decision
           ON resolved_decision.id=resolution.grade_decision_id
         LEFT JOIN exam_grade_publications_v2 active_publication
           ON active_publication.id=attempt.active_publication_id
         WHERE plan.public_id=?1 AND plan.planned_by=?2
         ORDER BY assessment.title,student.student_no,student.id,item.order_index,review_case.id",
    )?;
    let rows = statement.query_map(
        params![plan_public_id.trim(), owner_id.trim()],
        |row| {
            let case_kind: String = row.get(3)?;
            let resolved_is_published = row.get::<_, i64>(39)? != 0;
            let resolution_public_id: Option<String> = row.get(34)?;
            let state = if resolved_is_published {
                "republished"
            } else if resolution_public_id.is_some() {
                "grade_confirmed"
            } else {
                "open"
            };
            let next_step_note = match (state, case_kind.as_str()) {
                ("republished", _) => "新评分已由老师再次明确发布；旧发布快照保持审计。",
                ("grade_confirmed", _) => {
                    "新评分 revision 已确认但尚未正式生效；请明确发布整份成绩。"
                }
                (_, "unpublished_recalculation") => {
                    "请对照学生作答与新答案逐条确认；保存后仍须显式发布整份成绩。"
                }
                _ => {
                    "请核对正式发布时的旧评分与学生作答；保存新评分后旧正式成绩仍保持不变，直到明确生成新的发布 revision。"
                }
            };
            Ok(QuestionImpactReviewCase {
                public_id: row.get(0)?,
                impact_task_public_id: row.get(1)?,
                plan_public_id: row.get(2)?,
                case_kind,
                assessment_title: row.get(4)?,
                class_name: row.get(5)?,
                student_no: row.get(6)?,
                student_name: row.get(7)?,
                question_version_public_id: row.get(8)?,
                question_type: row.get(9)?,
                question_stem: row.get(10)?,
                question_no: row.get(11)?,
                max_score: row.get(12)?,
                attempt_public_id: row.get(13)?,
                attempt_state: row.get(14)?,
                publication_public_id: row.get(15)?,
                source_grade_decision_public_id: row.get(16)?,
                source_grade_decision_revision: row.get(17)?,
                source_teacher_score: row.get(18)?,
                source_point_results_json: row.get(19)?,
                source_snapshot_hash: row.get(20)?,
                student_response_state: row.get(21)?,
                student_response_text: row.get(22)?,
                crop_path: row.get(23)?,
                target_answer_key_version_public_id: row.get(24)?,
                target_answer_key_revision: row.get(25)?,
                target_answer_json: row.get(26)?,
                target_components: Vec::new(),
                target_rubric_version_public_id: row.get(27)?,
                target_rubric_revision: row.get(28)?,
                target_link_set_public_id: row.get(29)?,
                target_link_set_revision: row.get(30)?,
                prepared_by: row.get(31)?,
                prepared_at: row.get(32)?,
                state: state.into(),
                resolution_public_id,
                resolved_grade_decision_public_id: row.get(35)?,
                resolved_teacher_score: row.get(36)?,
                resolved_at: row.get(37)?,
                active_publication_public_id: row.get(38)?,
                next_step_note: next_step_note.into(),
                changes_assessment_binding: false,
                changes_grade: state != "open",
                changes_publication: state == "republished",
                changes_learning_evidence: state == "republished",
            })
        },
    )?;
    let mut cases = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);
    for review_case in &mut cases {
        review_case.target_components = load_target_components(
            conn,
            &review_case.question_type,
            &review_case.target_answer_key_version_public_id,
            &review_case.target_rubric_version_public_id,
        )?;
    }
    Ok(QuestionImpactReviewCaseCatalog {
        schema_version: QUESTION_PERFORMANCE_SCHEMA_VERSION,
        rule_version: QUESTION_IMPACT_REVIEW_RULE_VERSION.into(),
        plan_public_id: plan_public_id.trim().into(),
        cases,
        boundary_note:
            "建立 case 不改任何成绩。老师保存新评分后仍不会自动发布；只有再次明确发布，正式成绩和学习证据才在同一事务中切换。"
                .into(),
    })
}
