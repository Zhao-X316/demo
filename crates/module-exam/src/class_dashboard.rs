//! M6.1-1 班级运行仪表盘只读事实层。
//!
//! 本模块只汇总已经存在的任务、提交、终审、作业 attempt 与流水线异常，
//! 不计算知识掌握度、不写业务表，也不代替背诵/作业工作台执行终审或发布。

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use suite_core::domain::time;
use suite_core::error::{CoreError, CoreResult};

pub const CLASS_OPERATIONS_DASHBOARD_SCHEMA_VERSION: i64 = 2;
pub const CLASS_OPERATIONS_DASHBOARD_RULE_VERSION: &str = "m6.1-operations-v2";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardClass {
    pub id: i64,
    pub name: String,
    pub term: Option<String>,
    pub textbook: Option<String>,
    pub enabled_student_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardSourceMeta {
    pub schema_version: i64,
    pub rule_version: String,
    pub calculated_at: String,
    pub as_of_date: String,
    pub recitation_watermark: Option<String>,
    pub exam_watermark: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecitationOperations {
    pub expected_student_count: i64,
    pub completed_student_count: i64,
    pub expected_task_count: i64,
    pub confirmed_task_count: i64,
    pub submitted_task_count: i64,
    pub not_submitted_student_count: i64,
    pub pending_teacher_review_count: i64,
    pub overdue_pending_review_count: i64,
    pub recognition_failure_count: i64,
    pub recognition_processing_count: i64,
    pub denominator_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExamOperations {
    pub active_assessment_count: i64,
    pub expected_submission_count: i64,
    pub submitted_submission_count: i64,
    pub missing_submission_count: i64,
    pub ingesting_attempt_count: i64,
    pub grading_attempt_count: i64,
    pub ready_to_publish_attempt_count: i64,
    pub published_submission_count: i64,
    pub open_pipeline_issue_count: i64,
    pub denominator_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StudentOperationsRow {
    pub student_id: i64,
    pub student_no: String,
    pub student_name: String,
    pub recitation_status: String,
    pub recitation_due_task_count: i64,
    pub recitation_confirmed_task_count: i64,
    pub exam_status: String,
    pub exam_expected_submission_count: i64,
    pub exam_submitted_submission_count: i64,
    pub exam_published_submission_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DashboardAction {
    pub kind: String,
    pub title: String,
    pub detail: String,
    pub count: i64,
    pub severity: String,
    pub target_module: String,
    pub target_view: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassOperationsDashboard {
    pub meta: DashboardSourceMeta,
    pub class: DashboardClass,
    pub recitation: RecitationOperations,
    pub exam: ExamOperations,
    pub students: Vec<StudentOperationsRow>,
    pub actions: Vec<DashboardAction>,
}

fn require_date(value: &str) -> CoreResult<()> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d")
        .map(|_| ())
        .map_err(|_| CoreError::Invalid("仪表盘日期必须是 YYYY-MM-DD".into()))
}

fn load_class(conn: &Connection, class_id: i64) -> CoreResult<DashboardClass> {
    let class = conn
        .query_row(
            "SELECT c.id,c.name,c.term,c.textbook,
                    (SELECT COUNT(*) FROM students s
                     WHERE s.class_id=c.id AND s.enabled=1)
             FROM classes c WHERE c.id=?1",
            [class_id],
            |row| {
                Ok(DashboardClass {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    term: row.get(2)?,
                    textbook: row.get(3)?,
                    enabled_student_count: row.get(4)?,
                })
            },
        )
        .optional()?;
    class.ok_or_else(|| CoreError::NotFound(format!("class#{class_id}")))
}

fn recitation_operations(
    conn: &Connection,
    class_id: i64,
    as_of_date: &str,
) -> CoreResult<RecitationOperations> {
    let (
        expected_task_count,
        confirmed_task_count,
        submitted_task_count,
        expected_student_count,
        completed_student_count,
        not_submitted_student_count,
    ): (i64, i64, i64, i64, i64, i64) = conn.query_row(
        "WITH due AS (
           SELECT t.id,t.student_id,t.status
           FROM tasks t
           JOIN students s ON s.id=t.student_id
           WHERE t.module='recitation' AND t.due_date=?2
             AND t.status NOT IN ('closed','expired')
             AND s.class_id=?1 AND s.enabled=1
         ),
         per_student AS (
           SELECT d.student_id,
                  COUNT(*) AS due_count,
                  SUM(CASE WHEN d.status IN ('passed','failed') THEN 1 ELSE 0 END)
                    AS confirmed_count,
                  SUM(CASE WHEN EXISTS(
                    SELECT 1 FROM submissions sub
                    WHERE sub.task_id=d.id AND sub.module='recitation'
                      AND sub.status<>'voided'
                  ) THEN 1 ELSE 0 END) AS submitted_count
           FROM due d GROUP BY d.student_id
         )
         SELECT
           (SELECT COUNT(*) FROM due),
           (SELECT COUNT(*) FROM due WHERE status IN ('passed','failed')),
           (SELECT COUNT(*) FROM due d WHERE EXISTS(
             SELECT 1 FROM submissions sub
             WHERE sub.task_id=d.id AND sub.module='recitation'
               AND sub.status<>'voided'
           )),
           (SELECT COUNT(*) FROM per_student),
           (SELECT COUNT(*) FROM per_student WHERE confirmed_count=due_count),
           (SELECT COUNT(*) FROM per_student WHERE submitted_count<due_count)",
        (class_id, as_of_date),
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
    )?;

    let (
        pending_teacher_review_count,
        overdue_pending_review_count,
        recognition_failure_count,
        recognition_processing_count,
    ): (i64, i64, i64, i64) = conn.query_row(
        "WITH scoped_submissions AS (
           SELECT sub.id,sub.recognize_status,t.due_date
           FROM submissions sub
           JOIN tasks t ON t.id=sub.task_id AND t.module='recitation'
           JOIN students s ON s.id=t.student_id
           WHERE sub.module='recitation' AND sub.status<>'voided'
             AND t.status NOT IN ('closed','expired')
             AND t.due_date<=?2 AND s.class_id=?1 AND s.enabled=1
         ),
         pending AS (
           SELECT scoped.id,scoped.due_date
           FROM scoped_submissions scoped
           JOIN verdicts v ON v.id=(
             SELECT MAX(v2.id) FROM verdicts v2
             WHERE v2.submission_id=scoped.id AND v2.module='recitation'
           )
           WHERE v.human_result IS NULL
         )
         SELECT
           (SELECT COUNT(*) FROM pending),
           (SELECT COUNT(*) FROM pending WHERE due_date<?2),
           (SELECT COUNT(*) FROM scoped_submissions WHERE recognize_status='failed'),
           (SELECT COUNT(*) FROM scoped_submissions
            WHERE recognize_status IN ('pending','processing'))",
        (class_id, as_of_date),
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
    )?;

    Ok(RecitationOperations {
        expected_student_count,
        completed_student_count,
        expected_task_count,
        confirmed_task_count,
        submitted_task_count,
        not_submitted_student_count,
        pending_teacher_review_count,
        overdue_pending_review_count,
        recognition_failure_count,
        recognition_processing_count,
        denominator_note:
            "分母为所选日期有至少一项有效背诵任务的启用学生；该生当日全部任务终审后才计为完成。"
                .into(),
    })
}

fn exam_operations(conn: &Connection, class_id: i64) -> CoreResult<ExamOperations> {
    let (active_assessment_count, expected_submission_count): (i64, i64) = conn.query_row(
        "WITH active_assessments AS (
           SELECT assessment.id,assessment.audience_kind
           FROM exam_assessments_v2 assessment
           WHERE assessment.class_id=?1 AND assessment.state='active'
             AND EXISTS(
               SELECT 1 FROM exam_assessment_versions_v2 version
               WHERE version.assessment_id=assessment.id AND version.state='confirmed'
             )
         ),
         expected_pairs AS (
           SELECT assessment.id AS assessment_id,student.id AS student_id
           FROM active_assessments assessment
           JOIN students student ON student.class_id=?1 AND student.enabled=1
           WHERE assessment.audience_kind='class'
           UNION
           SELECT assessment.id,target.student_id
           FROM active_assessments assessment
           JOIN exam_assessment_versions_v2 version
             ON version.assessment_id=assessment.id AND version.state='confirmed'
           JOIN exam_assessment_targets_v2 target
             ON target.assessment_version_id=version.id
           JOIN students student
             ON student.id=target.student_id
            AND student.class_id=?1 AND student.enabled=1
           WHERE assessment.audience_kind='explicit'
         )
         SELECT
           (SELECT COUNT(*) FROM active_assessments),
           (SELECT COUNT(*) FROM expected_pairs)",
        [class_id],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;

    let (
        submitted_submission_count,
        ingesting_attempt_count,
        grading_attempt_count,
        ready_to_publish_attempt_count,
        published_submission_count,
    ): (i64, i64, i64, i64, i64) = conn.query_row(
        "WITH active_assessments AS (
           SELECT assessment.id,assessment.audience_kind
           FROM exam_assessments_v2 assessment
           WHERE assessment.class_id=?1 AND assessment.state='active'
             AND EXISTS(
               SELECT 1 FROM exam_assessment_versions_v2 version
               WHERE version.assessment_id=assessment.id AND version.state='confirmed'
             )
         ),
         expected_pairs AS (
           SELECT assessment.id AS assessment_id,student.id AS student_id
           FROM active_assessments assessment
           JOIN students student ON student.class_id=?1 AND student.enabled=1
           WHERE assessment.audience_kind='class'
           UNION
           SELECT assessment.id,target.student_id
           FROM active_assessments assessment
           JOIN exam_assessment_versions_v2 version
             ON version.assessment_id=assessment.id AND version.state='confirmed'
           JOIN exam_assessment_targets_v2 target
             ON target.assessment_version_id=version.id
           JOIN students student
             ON student.id=target.student_id
            AND student.class_id=?1 AND student.enabled=1
           WHERE assessment.audience_kind='explicit'
         ),
         active_attempts AS (
           SELECT assessment.id AS assessment_id,attempt.id AS attempt_id,
                  attempt.student_id,attempt.state
           FROM active_assessments assessment
           JOIN exam_assessment_versions_v2 version
             ON version.assessment_id=assessment.id AND version.state='confirmed'
           JOIN exam_attempts_v2 attempt
             ON attempt.assessment_version_id=version.id AND attempt.state<>'voided'
           JOIN expected_pairs expected
             ON expected.assessment_id=assessment.id
            AND expected.student_id=attempt.student_id
         )
         SELECT
           (SELECT COUNT(*) FROM (
             SELECT assessment_id,student_id FROM active_attempts
             GROUP BY assessment_id,student_id
           )),
           (SELECT COUNT(*) FROM active_attempts WHERE state='ingesting'),
           (SELECT COUNT(*) FROM active_attempts WHERE state='grading'),
           (SELECT COUNT(*) FROM active_attempts WHERE state='ready_to_publish'),
           (SELECT COUNT(*) FROM (
             SELECT assessment_id,student_id FROM active_attempts
             WHERE state='published'
             GROUP BY assessment_id,student_id
           ))",
        [class_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )?;

    let open_pipeline_issue_count: i64 = conn.query_row(
        "WITH scoped_batches AS (
           SELECT b.id,b.public_id
           FROM exam_ingest_batches_v2 b
           JOIN exam_assessment_versions_v2 v ON v.id=b.assessment_version_id
           JOIN exam_assessments_v2 a ON a.id=v.assessment_id
           WHERE a.class_id=?1 AND a.state<>'archived'
         ),
         scoped_pages AS (
           SELECT p.id,p.public_id
           FROM exam_ingest_pages_v2 p
           JOIN scoped_batches b ON b.id=p.batch_id
         ),
         targets(target_type,target_public_id) AS (
           SELECT 'batch',public_id FROM scoped_batches
           UNION ALL SELECT 'page',public_id FROM scoped_pages
           UNION ALL
             SELECT 'quality',q.public_id
             FROM exam_page_quality_revisions_v2 q
             JOIN scoped_pages p ON p.id=q.page_id
           UNION ALL
             SELECT 'match',m.public_id
             FROM exam_page_match_revisions_v2 m
             JOIN scoped_pages p ON p.id=m.page_id
           UNION ALL
             SELECT 'alignment',al.public_id
             FROM exam_page_alignment_revisions_v2 al
             JOIN scoped_pages p ON p.id=al.page_id
           UNION ALL
             SELECT 'region',r.public_id
             FROM exam_answer_region_revisions_v2 r
             JOIN scoped_pages p ON p.id=r.page_id
         )
         SELECT COUNT(DISTINCT i.id)
         FROM exam_pipeline_issues_v2 i
         JOIN targets t
           ON t.target_type=i.target_type AND t.target_public_id=i.target_public_id
         WHERE i.state='open'",
        [class_id],
        |row| row.get(0),
    )?;

    Ok(ExamOperations {
        active_assessment_count,
        expected_submission_count,
        submitted_submission_count,
        missing_submission_count: (expected_submission_count - submitted_submission_count).max(0),
        ingesting_attempt_count,
        grading_attempt_count,
        ready_to_publish_attempt_count,
        published_submission_count,
        open_pipeline_issue_count,
        denominator_note:
            "普通作业分母为班级启用学生，定向订正只计被冻结的目标学生；M2 尚无截止日期，因此这里不称为“今日作业”。"
                .into(),
    })
}

#[derive(Debug)]
struct RecitationStudentFacts {
    due_count: i64,
    confirmed_count: i64,
    submitted_count: i64,
    pending_count: i64,
    failed_recognition_count: i64,
}

fn recitation_student_facts(
    conn: &Connection,
    student_id: i64,
    as_of_date: &str,
) -> CoreResult<RecitationStudentFacts> {
    conn.query_row(
        "WITH due AS (
           SELECT id,status FROM tasks
           WHERE module='recitation' AND student_id=?1 AND due_date=?2
             AND status NOT IN ('closed','expired')
         ),
         scoped_submissions AS (
           SELECT sub.id,sub.recognize_status
           FROM submissions sub JOIN due d ON d.id=sub.task_id
           WHERE sub.module='recitation' AND sub.status<>'voided'
         )
         SELECT
           (SELECT COUNT(*) FROM due),
           (SELECT COUNT(*) FROM due WHERE status IN ('passed','failed')),
           (SELECT COUNT(*) FROM due d WHERE EXISTS(
             SELECT 1 FROM scoped_submissions sub
             JOIN submissions raw ON raw.id=sub.id
             WHERE raw.task_id=d.id
           )),
           (SELECT COUNT(*) FROM scoped_submissions sub
            JOIN verdicts v ON v.id=(
              SELECT MAX(v2.id) FROM verdicts v2
              WHERE v2.submission_id=sub.id AND v2.module='recitation'
            )
            WHERE v.human_result IS NULL),
           (SELECT COUNT(*) FROM scoped_submissions WHERE recognize_status='failed')",
        (student_id, as_of_date),
        |row| {
            Ok(RecitationStudentFacts {
                due_count: row.get(0)?,
                confirmed_count: row.get(1)?,
                submitted_count: row.get(2)?,
                pending_count: row.get(3)?,
                failed_recognition_count: row.get(4)?,
            })
        },
    )
    .map_err(Into::into)
}

#[derive(Debug)]
struct ExamStudentFacts {
    expected_count: i64,
    submitted_count: i64,
    published_count: i64,
    ingesting_count: i64,
    grading_count: i64,
    ready_count: i64,
}

fn exam_student_facts(
    conn: &Connection,
    class_id: i64,
    student_id: i64,
) -> CoreResult<ExamStudentFacts> {
    conn.query_row(
        "WITH active_assessments AS (
           SELECT assessment.id,assessment.audience_kind
           FROM exam_assessments_v2 assessment
           WHERE assessment.class_id=?1 AND assessment.state='active'
             AND EXISTS(
               SELECT 1 FROM exam_assessment_versions_v2 version
               WHERE version.assessment_id=assessment.id AND version.state='confirmed'
             )
         ),
         expected_assessments AS (
           SELECT assessment.id
           FROM active_assessments assessment
           WHERE assessment.audience_kind='class'
              OR EXISTS(
                SELECT 1
                FROM exam_assessment_versions_v2 version
                JOIN exam_assessment_targets_v2 target
                  ON target.assessment_version_id=version.id
                 AND target.student_id=?2
                WHERE version.assessment_id=assessment.id
                  AND version.state='confirmed'
              )
         ),
         active_attempts AS (
           SELECT expected.id AS assessment_id,attempt.state
           FROM expected_assessments expected
           JOIN exam_assessment_versions_v2 version
             ON version.assessment_id=expected.id AND version.state='confirmed'
           JOIN exam_attempts_v2 attempt
             ON attempt.assessment_version_id=version.id
            AND attempt.student_id=?2 AND attempt.state<>'voided'
         )
         SELECT
           (SELECT COUNT(*) FROM expected_assessments),
           (SELECT COUNT(*) FROM (
             SELECT assessment_id FROM active_attempts GROUP BY assessment_id
           )),
           (SELECT COUNT(*) FROM (
             SELECT assessment_id FROM active_attempts
             WHERE state='published' GROUP BY assessment_id
           )),
           (SELECT COUNT(*) FROM active_attempts WHERE state='ingesting'),
           (SELECT COUNT(*) FROM active_attempts WHERE state='grading'),
           (SELECT COUNT(*) FROM active_attempts WHERE state='ready_to_publish')",
        (class_id, student_id),
        |row| {
            Ok(ExamStudentFacts {
                expected_count: row.get(0)?,
                submitted_count: row.get(1)?,
                published_count: row.get(2)?,
                ingesting_count: row.get(3)?,
                grading_count: row.get(4)?,
                ready_count: row.get(5)?,
            })
        },
    )
    .map_err(Into::into)
}

fn student_rows(
    conn: &Connection,
    class_id: i64,
    as_of_date: &str,
) -> CoreResult<Vec<StudentOperationsRow>> {
    let mut stmt = conn.prepare(
        "SELECT id,student_no,name FROM students
         WHERE class_id=?1 AND enabled=1
         ORDER BY student_no,id",
    )?;
    let roster = stmt
        .query_map([class_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    roster
        .into_iter()
        .map(|(student_id, student_no, student_name)| {
            let rec = recitation_student_facts(conn, student_id, as_of_date)?;
            let recitation_status = if rec.due_count == 0 {
                "not_scheduled"
            } else if rec.failed_recognition_count > 0 {
                "recognition_failed"
            } else if rec.pending_count > 0 {
                "pending_review"
            } else if rec.confirmed_count == rec.due_count {
                "completed"
            } else if rec.submitted_count == 0 {
                "not_submitted"
            } else if rec.confirmed_count > 0 {
                "partial"
            } else {
                "submitted"
            };

            let exam = exam_student_facts(conn, class_id, student_id)?;
            let exam_status = if exam.expected_count == 0 {
                "not_assigned"
            } else if exam.submitted_count == 0 {
                "not_submitted"
            } else if exam.ingesting_count > 0 {
                "ingesting"
            } else if exam.grading_count > 0 {
                "grading"
            } else if exam.ready_count > 0 {
                "ready_to_publish"
            } else if exam.published_count == exam.expected_count {
                "published"
            } else {
                "partial"
            };

            Ok(StudentOperationsRow {
                student_id,
                student_no,
                student_name,
                recitation_status: recitation_status.into(),
                recitation_due_task_count: rec.due_count,
                recitation_confirmed_task_count: rec.confirmed_count,
                exam_status: exam_status.into(),
                exam_expected_submission_count: exam.expected_count,
                exam_submitted_submission_count: exam.submitted_count,
                exam_published_submission_count: exam.published_count,
            })
        })
        .collect()
}

fn source_watermarks(
    conn: &Connection,
    class_id: i64,
) -> CoreResult<(Option<String>, Option<String>)> {
    let recitation = conn.query_row(
        "SELECT MAX(mark) FROM (
           SELECT MAX(t.updated_at) AS mark
           FROM tasks t JOIN students s ON s.id=t.student_id
           WHERE t.module='recitation' AND s.class_id=?1
           UNION ALL
           SELECT MAX(sub.updated_at)
           FROM submissions sub JOIN students s ON s.id=sub.student_id
           WHERE sub.module='recitation' AND s.class_id=?1
           UNION ALL
           SELECT MAX(v.updated_at)
           FROM verdicts v
           JOIN submissions sub ON sub.id=v.submission_id
           JOIN students s ON s.id=sub.student_id
           WHERE v.module='recitation' AND s.class_id=?1
         )",
        [class_id],
        |row| row.get(0),
    )?;
    let exam = conn.query_row(
        "SELECT MAX(mark) FROM (
           SELECT MAX(a.updated_at) AS mark
           FROM exam_assessments_v2 a WHERE a.class_id=?1
           UNION ALL
           SELECT MAX(at.updated_at)
           FROM exam_attempts_v2 at
           JOIN exam_assessment_versions_v2 v ON v.id=at.assessment_version_id
           JOIN exam_assessments_v2 a ON a.id=v.assessment_id
           WHERE a.class_id=?1
           UNION ALL
           SELECT MAX(b.updated_at)
           FROM exam_ingest_batches_v2 b
           JOIN exam_assessment_versions_v2 v ON v.id=b.assessment_version_id
           JOIN exam_assessments_v2 a ON a.id=v.assessment_id
           WHERE a.class_id=?1
         )",
        [class_id],
        |row| row.get(0),
    )?;
    Ok((recitation, exam))
}

fn actions(recitation: &RecitationOperations, exam: &ExamOperations) -> Vec<DashboardAction> {
    let mut result = Vec::new();
    let mut push = |kind: &str,
                    title: &str,
                    detail: &str,
                    count: i64,
                    severity: &str,
                    target_module: &str,
                    target_view: &str| {
        if count > 0 {
            result.push(DashboardAction {
                kind: kind.into(),
                title: title.into(),
                detail: detail.into(),
                count,
                severity: severity.into(),
                target_module: target_module.into(),
                target_view: target_view.into(),
            });
        }
    };
    push(
        "recitation_recognition_failed",
        "处理背诵识别失败",
        "进入背诵批改台重试、重新定位或作废失败录音。",
        recitation.recognition_failure_count,
        "blocking",
        "recitation",
        "desk",
    );
    push(
        "exam_pipeline_issue",
        "处理作业导入异常",
        "进入题目批改，核对图片质量、学生匹配、页码或题区。",
        exam.open_pipeline_issue_count,
        "blocking",
        "exam",
        "exam",
    );
    push(
        "recitation_pending_review",
        "终审背诵证据",
        "机器结果不会自动生效，请回到今日页查看录音和文本证据。",
        recitation.pending_teacher_review_count,
        "review",
        "recitation",
        "today",
    );
    push(
        "exam_grading",
        "继续作业终审",
        "这些作业正在批改中，需在作业工作台处理分歧和模糊项。",
        exam.grading_attempt_count,
        "review",
        "exam",
        "exam",
    );
    push(
        "exam_ready_to_publish",
        "发布已完成作业",
        "终审已完成，但成绩仍需老师显式发布。",
        exam.ready_to_publish_attempt_count,
        "review",
        "exam",
        "exam",
    );
    push(
        "recitation_not_submitted",
        "跟进今日未交背诵",
        "这些学生至少有一项今日任务尚无有效提交。",
        recitation.not_submitted_student_count,
        "info",
        "recitation",
        "today",
    );
    push(
        "exam_not_submitted",
        "跟进当前作业未上传",
        "按当前 active 作业与启用学生计算，尚未发现有效 attempt。",
        exam.missing_submission_count,
        "info",
        "exam",
        "exam",
    );
    result
}

/// 读取一个班级在给定日期的运行事实。该函数只执行 SELECT。
pub fn class_operations_dashboard(
    conn: &Connection,
    class_id: i64,
    as_of_date: &str,
) -> CoreResult<ClassOperationsDashboard> {
    require_date(as_of_date)?;
    let class = load_class(conn, class_id)?;
    let recitation = recitation_operations(conn, class_id, as_of_date)?;
    let exam = exam_operations(conn, class_id)?;
    let students = student_rows(conn, class_id, as_of_date)?;
    let (recitation_watermark, exam_watermark) = source_watermarks(conn, class_id)?;
    let actions = actions(&recitation, &exam);
    Ok(ClassOperationsDashboard {
        meta: DashboardSourceMeta {
            schema_version: CLASS_OPERATIONS_DASHBOARD_SCHEMA_VERSION,
            rule_version: CLASS_OPERATIONS_DASHBOARD_RULE_VERSION.into(),
            calculated_at: time::utc_now_rfc3339(),
            as_of_date: as_of_date.into(),
            recitation_watermark,
            exam_watermark,
        },
        class,
        recitation,
        exam,
        students,
        actions,
    })
}

#[cfg(test)]
mod tests {
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    use super::*;

    fn setup() -> Connection {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::exam_migrations()).unwrap();
        let hash = "a".repeat(64);
        conn.execute_batch(&format!(
            r#"INSERT INTO classes(id,name,term,textbook)
                 VALUES (1,'八年级一班','2026秋','中国历史八上'),
                        (2,'八年级二班','2026秋','中国历史八上');
               INSERT INTO students(id,student_no,name,class_id,enabled) VALUES
                 (1,'01','小林',1,1),
                 (2,'02','小周',1,1),
                 (3,'03','已停用',1,0),
                 (4,'04','他班学生',2,1);

               INSERT INTO tasks(id,module,student_id,ref_type,ref_id,kind,due_date,status,updated_at)
                 VALUES
                 (1,'recitation',1,'content',1,'normal','2026-07-16','passed','2026-07-16T08:00:00Z'),
                 (2,'recitation',1,'content',2,'normal','2026-07-16','failed','2026-07-16T08:10:00Z'),
                 (3,'recitation',2,'content',1,'normal','2026-07-16','open','2026-07-16T08:20:00Z'),
                 (4,'recitation',2,'content',2,'normal','2026-07-15','submitted','2026-07-16T08:30:00Z'),
                 (5,'recitation',2,'content',3,'normal','2026-07-15','submitted','2026-07-16T08:40:00Z'),
                 (6,'recitation',3,'content',1,'normal','2026-07-16','open','2026-07-16T08:50:00Z'),
                 (7,'recitation',4,'content',1,'normal','2026-07-16','passed','2026-07-16T09:00:00Z');

               INSERT INTO submissions
                 (id,module,task_id,student_id,media_type,file_path,file_hash,
                  recognize_status,status,updated_at)
                 VALUES
                 (1,'recitation',1,1,'audio','/tmp/1.wav','hash-1','ok','confirmed','2026-07-16T09:00:00Z'),
                 (2,'recitation',2,1,'audio','/tmp/2.wav','hash-2','ok','confirmed','2026-07-16T09:01:00Z'),
                 (3,'recitation',4,2,'audio','/tmp/3.wav','hash-3','ok','scored','2026-07-16T09:02:00Z'),
                 (4,'recitation',5,2,'audio','/tmp/4.wav','hash-4','failed','pending','2026-07-16T09:03:00Z');
               INSERT INTO verdicts
                 (submission_id,module,scorer,answer_version,human_result,updated_at)
                 VALUES
                 (1,'recitation','fixture',1,'pass','2026-07-16T09:00:00Z'),
                 (2,'recitation','fixture',1,'fail','2026-07-16T09:01:00Z'),
                 (3,'recitation','fixture',1,NULL,'2026-07-16T09:02:00Z');

               INSERT INTO exam_assessments_v2
                 (id,public_id,title,class_id,assessment_context,evidence_policy,state,
                  created_by,created_at,updated_at)
                 VALUES
                 (1,'assessment-1','近代史周练',1,'quiz','include','active','teacher',
                  '2026-07-16T08:00:00Z','2026-07-16T10:00:00Z'),
                 (2,'assessment-2','已归档练习',1,'homework','include','archived','teacher',
                  '2026-07-15T08:00:00Z','2026-07-15T10:00:00Z'),
                 (3,'assessment-3','他班周练',2,'quiz','include','active','teacher',
                  '2026-07-16T08:00:00Z','2026-07-16T10:00:00Z');
               INSERT INTO exam_assessment_versions_v2
                 (id,public_id,assessment_id,revision,item_set_hash,state,created_at,confirmed_by,confirmed_at)
                 VALUES
                 (1,'version-1',1,1,'{hash}','confirmed','2026-07-16T08:00:00Z','teacher','2026-07-16T08:01:00Z'),
                 (2,'version-2',2,1,'{hash}','confirmed','2026-07-15T08:00:00Z','teacher','2026-07-15T08:01:00Z'),
                 (3,'version-3',3,1,'{hash}','confirmed','2026-07-16T08:00:00Z','teacher','2026-07-16T08:01:00Z');
               INSERT INTO exam_attempts_v2
                 (id,public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES
                 (1,'attempt-1',1,1,1,'image','first','ready_to_publish',
                  '2026-07-16T10:00:00Z','2026-07-16T10:10:00Z'),
                 (2,'attempt-2',2,1,1,'image','first','published',
                  '2026-07-15T10:00:00Z','2026-07-15T10:10:00Z'),
                 (3,'attempt-3',3,4,1,'image','first','published',
                  '2026-07-16T10:00:00Z','2026-07-16T10:10:00Z');
               INSERT INTO exam_ingest_batches_v2
                 (id,public_id,assessment_version_id,source_kind,idempotency_key,state,
                  created_by,created_at,updated_at)
                 VALUES
                 (1,'batch-1',1,'camera','batch-key-1','needs_review','teacher',
                  '2026-07-16T10:00:00Z','2026-07-16T10:20:00Z');
               INSERT INTO exam_pipeline_issues_v2
                 (public_id,target_type,target_public_id,issue_code,severity,details_json,
                  state,created_at,updated_at)
                 VALUES
                 ('issue-1','batch','batch-1','PAGE_ORDER_UNCERTAIN','blocking',
                  '{{"schema_version":1}}','open','2026-07-16T10:21:00Z','2026-07-16T10:21:00Z');"#
        ))
        .unwrap();
        conn
    }

    #[test]
    fn dashboard_separates_operational_states_and_scopes_the_class() {
        let conn = setup();
        let before: i64 = conn
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap();
        let dashboard = class_operations_dashboard(&conn, 1, "2026-07-16").unwrap();
        let after: i64 = conn
            .query_row("SELECT total_changes()", [], |row| row.get(0))
            .unwrap();
        assert_eq!(after, before, "仪表盘读取不得写业务表");

        assert_eq!(dashboard.class.enabled_student_count, 2);
        assert_eq!(dashboard.recitation.expected_student_count, 2);
        assert_eq!(dashboard.recitation.completed_student_count, 1);
        assert_eq!(dashboard.recitation.expected_task_count, 3);
        assert_eq!(dashboard.recitation.confirmed_task_count, 2);
        assert_eq!(dashboard.recitation.submitted_task_count, 2);
        assert_eq!(dashboard.recitation.not_submitted_student_count, 1);
        assert_eq!(dashboard.recitation.pending_teacher_review_count, 1);
        assert_eq!(dashboard.recitation.overdue_pending_review_count, 1);
        assert_eq!(dashboard.recitation.recognition_failure_count, 1);

        assert_eq!(dashboard.exam.active_assessment_count, 1);
        assert_eq!(dashboard.exam.expected_submission_count, 2);
        assert_eq!(dashboard.exam.submitted_submission_count, 1);
        assert_eq!(dashboard.exam.missing_submission_count, 1);
        assert_eq!(dashboard.exam.ready_to_publish_attempt_count, 1);
        assert_eq!(dashboard.exam.published_submission_count, 0);
        assert_eq!(dashboard.exam.open_pipeline_issue_count, 1);

        assert_eq!(dashboard.students.len(), 2);
        assert_eq!(dashboard.students[0].recitation_status, "completed");
        assert_eq!(dashboard.students[0].exam_status, "ready_to_publish");
        assert_eq!(dashboard.students[1].recitation_status, "not_submitted");
        assert_eq!(dashboard.students[1].exam_status, "not_submitted");
        assert!(dashboard
            .actions
            .iter()
            .any(|action| action.kind == "recitation_recognition_failed"));
        assert!(dashboard
            .actions
            .iter()
            .any(|action| action.kind == "exam_pipeline_issue"));
        assert_eq!(
            dashboard.meta.rule_version,
            CLASS_OPERATIONS_DASHBOARD_RULE_VERSION
        );
    }

    #[test]
    fn explicit_assessment_counts_only_its_frozen_target_student() {
        let conn = setup();
        let hash = "d".repeat(64);
        conn.execute(
            "INSERT INTO exam_assessments_v2
             (id,public_id,title,class_id,assessment_context,evidence_policy,state,
              audience_kind,created_by,created_at,updated_at)
             VALUES (4,'assessment-4','小林单题订正',1,'correction','progress_only','draft',
                     'explicit','teacher','2026-07-16T11:00:00Z','2026-07-16T11:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exam_assessment_versions_v2
             (id,public_id,assessment_id,revision,item_set_hash,state,created_at,
              confirmed_by,confirmed_at)
             VALUES (4,'version-4',4,1,?1,'confirmed','2026-07-16T11:00:00Z',
                     'teacher','2026-07-16T11:01:00Z')",
            [&hash],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO exam_assessment_targets_v2
             (public_id,assessment_version_id,student_id,created_by,created_at)
             VALUES ('target-4',4,1,'teacher','2026-07-16T11:01:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "UPDATE exam_assessments_v2 SET state='active' WHERE id=4",
            [],
        )
        .unwrap();

        let dashboard = class_operations_dashboard(&conn, 1, "2026-07-16").unwrap();
        assert_eq!(dashboard.exam.active_assessment_count, 2);
        assert_eq!(dashboard.exam.expected_submission_count, 3);
        assert_eq!(dashboard.exam.submitted_submission_count, 1);
        assert_eq!(dashboard.exam.missing_submission_count, 2);
        assert_eq!(dashboard.students[0].exam_expected_submission_count, 2);
        assert_eq!(dashboard.students[1].exam_expected_submission_count, 1);
        assert!(dashboard.exam.denominator_note.contains("定向订正"));

        assert!(conn
            .execute(
                "INSERT INTO exam_attempts_v2
                 (public_id,assessment_version_id,student_id,attempt_no,source_kind,
                  attempt_kind,state,created_at,updated_at)
                 VALUES ('wrong-target',4,2,1,'image','correction','ingesting',
                         '2026-07-16T11:02:00Z','2026-07-16T11:02:00Z')",
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "UPDATE exam_assessment_targets_v2 SET student_id=2 WHERE public_id='target-4'",
                [],
            )
            .is_err());
        assert!(conn
            .execute(
                "DELETE FROM exam_assessment_targets_v2 WHERE public_id='target-4'",
                [],
            )
            .is_err());
    }

    #[test]
    fn dashboard_rejects_invalid_scope_and_does_not_import_other_classes() {
        let conn = setup();
        let other = class_operations_dashboard(&conn, 2, "2026-07-16").unwrap();
        assert_eq!(other.class.enabled_student_count, 1);
        assert_eq!(other.recitation.expected_student_count, 1);
        assert_eq!(other.recitation.completed_student_count, 1);
        assert_eq!(other.exam.expected_submission_count, 1);
        assert_eq!(other.exam.published_submission_count, 1);
        assert_eq!(other.exam.open_pipeline_issue_count, 0);

        assert!(class_operations_dashboard(&conn, 999, "2026-07-16").is_err());
        assert!(class_operations_dashboard(&conn, 1, "07/16/2026").is_err());
    }
}
