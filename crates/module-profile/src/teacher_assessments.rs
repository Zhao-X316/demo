//! M6-3 个人节点的老师补充判断。
//!
//! 补充判断与系统快照并列，不修改节点掌握分、学习证据或上游成绩。首次保存、
//! 修正和清除都追加不可变 revision；并发编辑通过 expected revision 拒绝覆盖。

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::audit::{self, NewAuditEvent};
use suite_core::db::repo::outbox::{self, NewOutboxEvent};
use suite_core::domain::{hashing, ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::AuditActorType;

const ASSESSMENTS: &[&str] = &[
    "not_taught",
    "needs_support",
    "developing",
    "stable",
    "observe",
];

#[derive(Debug, Clone)]
pub struct SaveProfileTeacherAssessmentInput<'a> {
    pub snapshot_public_id: &'a str,
    pub node_metric_public_id: &'a str,
    pub expected_revision: i64,
    pub assessment: Option<&'a str>,
    pub note: Option<&'a str>,
    pub actor_id: &'a str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileTeacherAssessment {
    pub public_id: String,
    pub snapshot_public_id: String,
    pub node_metric_public_id: String,
    pub target_type: String,
    pub target_public_id: String,
    pub target_title: String,
    pub revision: i64,
    pub assessment: Option<String>,
    pub note: Option<String>,
    pub state: String,
    pub supersedes_public_id: Option<String>,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug)]
struct Target {
    snapshot_id: i64,
    metric_id: i64,
}

fn required(value: &str, label: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Invalid(format!("{label}不能为空")));
    }
    Ok(())
}

fn normalized_note(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn assessment_row(row: &Row<'_>) -> rusqlite::Result<ProfileTeacherAssessment> {
    Ok(ProfileTeacherAssessment {
        public_id: row.get(0)?,
        snapshot_public_id: row.get(1)?,
        node_metric_public_id: row.get(2)?,
        target_type: row.get(3)?,
        target_public_id: row.get(4)?,
        target_title: row.get(5)?,
        revision: row.get(6)?,
        assessment: row.get(7)?,
        note: row.get(8)?,
        state: row.get(9)?,
        supersedes_public_id: row.get(10)?,
        created_by: row.get(11)?,
        created_at: row.get(12)?,
    })
}

const ASSESSMENT_SELECT: &str = "SELECT revision.public_id,snapshot.public_id,metric.public_id,
            metric.target_type,metric.target_public_id,metric.target_title,
            revision.revision,revision.assessment,revision.note,revision.state,
            revision.supersedes_public_id,revision.created_by,revision.created_at
     FROM profile_teacher_assessment_revisions revision
     JOIN profile_snapshots snapshot ON snapshot.id=revision.snapshot_id
     JOIN profile_node_metrics metric ON metric.id=revision.node_metric_id";

fn load_target(
    conn: &Connection,
    snapshot_public_id: &str,
    node_metric_public_id: &str,
) -> CoreResult<Target> {
    conn.query_row(
        "SELECT snapshot.id,metric.id
         FROM profile_snapshots snapshot
         JOIN profile_node_metrics metric ON metric.snapshot_id=snapshot.id
         WHERE snapshot.public_id=?1 AND metric.public_id=?2",
        (snapshot_public_id, node_metric_public_id),
        |row| {
            Ok(Target {
                snapshot_id: row.get(0)?,
                metric_id: row.get(1)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| CoreError::Invalid("节点不属于指定个人快照".into()))
}

fn latest_for_target(
    conn: &Connection,
    target: &Target,
) -> CoreResult<Option<ProfileTeacherAssessment>> {
    let sql = format!(
        "{ASSESSMENT_SELECT}
         WHERE revision.snapshot_id=?1 AND revision.node_metric_id=?2
         ORDER BY revision.revision DESC LIMIT 1"
    );
    Ok(conn
        .query_row(&sql, (target.snapshot_id, target.metric_id), assessment_row)
        .optional()?)
}

fn load_by_public_id(
    conn: &Connection,
    public_id: &str,
) -> CoreResult<Option<ProfileTeacherAssessment>> {
    let sql = format!("{ASSESSMENT_SELECT} WHERE revision.public_id=?1");
    Ok(conn
        .query_row(&sql, [public_id], assessment_row)
        .optional()?)
}

pub fn list_profile_teacher_assessments(
    conn: &Connection,
    snapshot_public_id: &str,
) -> CoreResult<Vec<ProfileTeacherAssessment>> {
    required(snapshot_public_id, "快照 ID")?;
    let snapshot_exists = conn
        .query_row(
            "SELECT 1 FROM profile_snapshots WHERE public_id=?1",
            [snapshot_public_id],
            |row| row.get::<_, i64>(0),
        )
        .optional()?
        .is_some();
    if !snapshot_exists {
        return Err(CoreError::Invalid("个人快照不存在".into()));
    }
    let sql = format!(
        "{ASSESSMENT_SELECT}
         WHERE snapshot.public_id=?1
           AND revision.revision=(
             SELECT MAX(latest.revision)
             FROM profile_teacher_assessment_revisions latest
             WHERE latest.snapshot_id=revision.snapshot_id
               AND latest.node_metric_id=revision.node_metric_id
           )
         ORDER BY metric.target_type,metric.target_title,metric.public_id"
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map([snapshot_public_id], assessment_row)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn save_profile_teacher_assessment(
    conn: &mut Connection,
    input: &SaveProfileTeacherAssessmentInput<'_>,
) -> CoreResult<ProfileTeacherAssessment> {
    required(input.snapshot_public_id, "快照 ID")?;
    required(input.node_metric_public_id, "节点 ID")?;
    required(input.actor_id, "确认人")?;
    if input.expected_revision < 0 {
        return Err(CoreError::Invalid("预期 revision 不能小于 0".into()));
    }
    let assessment = input
        .assessment
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if assessment.is_some_and(|value| !ASSESSMENTS.contains(&value)) {
        return Err(CoreError::Invalid("不支持的老师补充判断".into()));
    }
    let note = normalized_note(input.note);
    if note
        .as_ref()
        .is_some_and(|value| value.chars().count() > 500)
    {
        return Err(CoreError::Invalid("补充说明不能超过 500 个字符".into()));
    }
    let tx = conn.transaction()?;
    let target = load_target(&tx, input.snapshot_public_id, input.node_metric_public_id)?;
    let latest = latest_for_target(&tx, &target)?;
    let requested_state = if assessment.is_some() {
        "active"
    } else {
        "voided"
    };
    if let Some(current) = latest.as_ref() {
        if current.state == requested_state
            && current.assessment.as_deref() == assessment
            && current.note == note
        {
            tx.commit()?;
            return Ok(current.clone());
        }
    } else if assessment.is_none() {
        return Err(CoreError::Invalid("当前节点还没有可清除的老师判断".into()));
    }
    let current_revision = latest.as_ref().map_or(0, |value| value.revision);
    if current_revision != input.expected_revision {
        return Err(CoreError::Invalid(
            "老师补充判断已被更新，请刷新后再保存".into(),
        ));
    }
    let revision = current_revision + 1;
    let supersedes_public_id = latest.as_ref().map(|value| value.public_id.as_str());
    let public_id = ids::new_public_id();
    let created_at = time::utc_now_rfc3339();
    let payload = serde_json::json!({
        "schema_version": 1,
        "snapshot_public_id": input.snapshot_public_id,
        "node_metric_public_id": input.node_metric_public_id,
        "revision": revision,
        "assessment": assessment,
        "note": note,
        "state": requested_state,
        "affects_system_metric": false
    });
    let payload_sha256 = hashing::sha256_hex(
        serde_json::to_vec(&payload)
            .map_err(|error| CoreError::Invalid(error.to_string()))?
            .as_slice(),
    );
    tx.execute(
        "INSERT INTO profile_teacher_assessment_revisions
          (public_id,snapshot_id,node_metric_id,revision,assessment,note,state,
           supersedes_public_id,payload_sha256,created_by,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![
            public_id,
            target.snapshot_id,
            target.metric_id,
            revision,
            assessment,
            note,
            requested_state,
            supersedes_public_id,
            payload_sha256,
            input.actor_id.trim(),
            created_at
        ],
    )?;
    let event_payload = payload.to_string();
    outbox::create_event(
        &tx,
        &NewOutboxEvent {
            idempotency_key: &format!("profile:outbox:teacher-assessment:{public_id}"),
            event_type: "profile_teacher_assessment_changed",
            event_version: 1,
            aggregate_type: "profile_teacher_assessment",
            aggregate_id: &public_id,
            aggregate_revision: revision,
            payload_json: &event_payload,
            occurred_at: &created_at,
        },
    )?;
    audit::append(
        &tx,
        &NewAuditEvent {
            idempotency_key: &format!("profile:audit:teacher-assessment:{public_id}"),
            actor_type: AuditActorType::Teacher,
            actor_id: Some(input.actor_id.trim()),
            action: if requested_state == "active" {
                "profile.teacher_assessment.saved"
            } else {
                "profile.teacher_assessment.voided"
            },
            object_type: "profile_teacher_assessment",
            object_id: &public_id,
            object_revision: Some(revision),
            note: Some("老师补充判断与系统结论并列保存，不修改原始证据或掌握分"),
            meta_json: Some(&event_payload),
            occurred_at: &created_at,
        },
    )?;
    tx.commit()?;
    load_by_public_id(conn, &public_id)?
        .ok_or_else(|| CoreError::Db("老师补充判断写入后无法读取".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    fn setup() -> (Connection, String, String) {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, module_exam::exam_migrations()).unwrap();
        run_migrations(&conn, module_wrongbook::wrongbook_migrations()).unwrap();
        run_migrations(&conn, crate::profile_migrations()).unwrap();
        conn.execute("INSERT INTO classes(name) VALUES ('八年级一班')", [])
            .unwrap();
        let class_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO students(student_no,name,class_id,enabled)
             VALUES ('01','张三',?1,1)",
            [class_id],
        )
        .unwrap();
        let student_id = conn.last_insert_rowid();
        let policy_id: i64 = conn
            .query_row(
                "SELECT id FROM profile_policy_versions WHERE state='active'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO profile_snapshots
              (public_id,student_id,class_id,revision,range_start,range_end,scope_kind,
               scope_json,source_config_json,evidence_cutoff_at,policy_id,policy_revision,
               source_watermark,evidence_count,knowledge_node_total,knowledge_node_assessed,
               knowledge_node_eligible,ability_node_total,ability_node_assessed,
               ability_node_eligible,state,payload_sha256,generated_by,generated_at,
               confirmed_by,confirmed_at)
             VALUES
              ('snapshot-1',?1,?2,1,'2026-07-01','2026-07-31','confirmed_evidence_maps',
               '{}','{}','2026-07-19T00:00:00.000Z',?3,1,
               ?4,0,1,0,0,0,0,0,'teacher_confirmed',?4,
               'teacher','2026-07-19T00:00:00.000Z',
               'teacher','2026-07-19T00:00:00.000Z')",
            params![student_id, class_id, policy_id, "0".repeat(64)],
        )
        .unwrap();
        let snapshot_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO profile_node_metrics
              (public_id,snapshot_id,target_type,target_public_id,target_title,mastery_score,
               status,confidence_level,freshness,evidence_count,independent_group_count,
               distinct_date_count,distinct_source_count,last_evidence_at,
               source_breakdown_json,explanation)
             VALUES
              ('metric-1',?1,'knowledge_node','knowledge-1','洋务运动',NULL,
               'unassessed','none','none',0,0,0,0,NULL,'{}','尚无证据')",
            [snapshot_id],
        )
        .unwrap();
        (conn, "snapshot-1".into(), "metric-1".into())
    }

    #[test]
    fn teacher_assessment_is_append_only_and_does_not_change_metric() {
        let (mut conn, snapshot, metric) = setup();
        let first = save_profile_teacher_assessment(
            &mut conn,
            &SaveProfileTeacherAssessmentInput {
                snapshot_public_id: &snapshot,
                node_metric_public_id: &metric,
                expected_revision: 0,
                assessment: Some("not_taught"),
                note: Some("下周讲授"),
                actor_id: "teacher-1",
            },
        )
        .unwrap();
        assert_eq!(first.revision, 1);
        let second = save_profile_teacher_assessment(
            &mut conn,
            &SaveProfileTeacherAssessmentInput {
                snapshot_public_id: &snapshot,
                node_metric_public_id: &metric,
                expected_revision: 1,
                assessment: Some("observe"),
                note: Some("课堂回答较好，继续观察"),
                actor_id: "teacher-1",
            },
        )
        .unwrap();
        assert_eq!(second.revision, 2);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM profile_teacher_assessment_revisions",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        assert!(conn
            .execute(
                "UPDATE profile_teacher_assessment_revisions SET note='tampered'
                 WHERE public_id=?1",
                [&second.public_id]
            )
            .is_err());
        let mastery: Option<f64> = conn
            .query_row(
                "SELECT mastery_score FROM profile_node_metrics WHERE public_id=?1",
                [&metric],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mastery, None);
    }

    #[test]
    fn stale_expected_revision_is_rejected_and_clear_adds_revision() {
        let (mut conn, snapshot, metric) = setup();
        save_profile_teacher_assessment(
            &mut conn,
            &SaveProfileTeacherAssessmentInput {
                snapshot_public_id: &snapshot,
                node_metric_public_id: &metric,
                expected_revision: 0,
                assessment: Some("needs_support"),
                note: None,
                actor_id: "teacher-1",
            },
        )
        .unwrap();
        let stale = save_profile_teacher_assessment(
            &mut conn,
            &SaveProfileTeacherAssessmentInput {
                snapshot_public_id: &snapshot,
                node_metric_public_id: &metric,
                expected_revision: 0,
                assessment: Some("stable"),
                note: None,
                actor_id: "teacher-1",
            },
        )
        .unwrap_err();
        assert!(stale.to_string().contains("刷新"));
        let cleared = save_profile_teacher_assessment(
            &mut conn,
            &SaveProfileTeacherAssessmentInput {
                snapshot_public_id: &snapshot,
                node_metric_public_id: &metric,
                expected_revision: 1,
                assessment: None,
                note: None,
                actor_id: "teacher-1",
            },
        )
        .unwrap();
        assert_eq!(cleared.revision, 2);
        assert_eq!(cleared.state, "voided");
        let listed = list_profile_teacher_assessments(&conn, &snapshot).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].state, "voided");
    }
}
