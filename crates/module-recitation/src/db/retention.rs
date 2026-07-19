//! M1.2-2 背诵终审日期上下文与跨日期保持窗口。
//!
//! 本层不自行开启事务；调用方必须把 effect、学习证据、窗口状态和排程放在
//! 同一个 SQLite transaction 中提交。

use chrono::NaiveDate;
use rusqlite::{params, Connection, OptionalExtension};
use suite_core::db::repo::decision_effects::DecisionEffect;
use suite_core::db::repo::learning_evidence;
use suite_core::domain::{ids, time};
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::{EvidenceSourceModule, Task, TaskKind};

const CONTEXT_COLS: &str = "id,public_id,decision_effect_id,decision_date,task_id,task_kind,\
                           task_due_date,state,created_at,reverted_at";
const WINDOW_COLS: &str = "id,public_id,student_id,content_id,window_date,revision,\
                          current_effect_id,previous_effect_id,current_context_id,\
                          previous_context_id,task_id,task_kind,task_due_date,\
                          planned_interval_days,actual_interval_days,result,\
                          recovered_after_lapse,state,created_at,superseded_at,reverted_at";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecDecisionContext {
    pub id: i64,
    pub public_id: String,
    pub decision_effect_id: i64,
    pub decision_date: String,
    pub task_id: Option<i64>,
    pub task_kind: Option<String>,
    pub task_due_date: Option<String>,
    pub state: String,
    pub created_at: String,
    pub reverted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriorDecisionContext {
    pub context: RecDecisionContext,
    pub result: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecRetentionWindow {
    pub id: i64,
    pub public_id: String,
    pub student_id: i64,
    pub content_id: i64,
    pub window_date: String,
    pub revision: i64,
    pub current_effect_id: i64,
    pub previous_effect_id: i64,
    pub current_context_id: i64,
    pub previous_context_id: i64,
    pub task_id: Option<i64>,
    pub task_kind: Option<String>,
    pub task_due_date: Option<String>,
    pub planned_interval_days: Option<i64>,
    pub actual_interval_days: i64,
    pub result: String,
    pub recovered_after_lapse: bool,
    pub state: String,
    pub created_at: String,
    pub superseded_at: Option<String>,
    pub reverted_at: Option<String>,
}

fn task_kind(kind: TaskKind) -> &'static str {
    match kind {
        TaskKind::Normal => "normal",
        TaskKind::Makeup => "makeup",
        TaskKind::Review => "review",
    }
}

fn context_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecDecisionContext> {
    Ok(RecDecisionContext {
        id: row.get(0)?,
        public_id: row.get(1)?,
        decision_effect_id: row.get(2)?,
        decision_date: row.get(3)?,
        task_id: row.get(4)?,
        task_kind: row.get(5)?,
        task_due_date: row.get(6)?,
        state: row.get(7)?,
        created_at: row.get(8)?,
        reverted_at: row.get(9)?,
    })
}

fn prior_context_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PriorDecisionContext> {
    Ok(PriorDecisionContext {
        context: context_row(row)?,
        result: row.get(10)?,
    })
}

fn window_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<RecRetentionWindow> {
    Ok(RecRetentionWindow {
        id: row.get(0)?,
        public_id: row.get(1)?,
        student_id: row.get(2)?,
        content_id: row.get(3)?,
        window_date: row.get(4)?,
        revision: row.get(5)?,
        current_effect_id: row.get(6)?,
        previous_effect_id: row.get(7)?,
        current_context_id: row.get(8)?,
        previous_context_id: row.get(9)?,
        task_id: row.get(10)?,
        task_kind: row.get(11)?,
        task_due_date: row.get(12)?,
        planned_interval_days: row.get(13)?,
        actual_interval_days: row.get(14)?,
        result: row.get(15)?,
        recovered_after_lapse: row.get::<_, i64>(16)? == 1,
        state: row.get(17)?,
        created_at: row.get(18)?,
        superseded_at: row.get(19)?,
        reverted_at: row.get(20)?,
    })
}

fn get_context(
    conn: &Connection,
    decision_effect_id: i64,
) -> CoreResult<Option<RecDecisionContext>> {
    let sql =
        format!("SELECT {CONTEXT_COLS} FROM rec_decision_contexts WHERE decision_effect_id=?1");
    Ok(conn
        .query_row(&sql, [decision_effect_id], context_row)
        .optional()?)
}

pub(crate) fn record_context(
    conn: &Connection,
    effect: &DecisionEffect,
    decision_date: NaiveDate,
    task: Option<&Task>,
) -> CoreResult<RecDecisionContext> {
    let decision_date = decision_date.format("%Y-%m-%d").to_string();
    let created_at = time::utc_now_rfc3339();
    let kind = task.map(|value| task_kind(value.kind));
    let due_date = task.map(|value| value.due_date.as_str());
    conn.execute(
        "INSERT INTO rec_decision_contexts
          (public_id,decision_effect_id,decision_date,task_id,task_kind,task_due_date,
           state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,'active',?7)
         ON CONFLICT(decision_effect_id) DO NOTHING",
        params![
            ids::new_public_id(),
            effect.id,
            decision_date,
            task.map(|value| value.id),
            kind,
            due_date,
            created_at
        ],
    )?;
    let context = get_context(conn, effect.id)?
        .ok_or_else(|| CoreError::Db("背诵终审日期上下文写入后无法读取".into()))?;
    if context.decision_date != decision_date
        || context.task_id != task.map(|value| value.id)
        || context.task_kind.as_deref() != kind
        || context.task_due_date.as_deref() != due_date
        || context.state != "active"
    {
        return Err(CoreError::Invalid(
            "同一终审效果已经绑定不同的日期或任务上下文".into(),
        ));
    }
    Ok(context)
}

fn latest_prior_distinct_date(
    conn: &Connection,
    effect: &DecisionEffect,
    decision_date: &str,
) -> CoreResult<Option<PriorDecisionContext>> {
    let sql = "SELECT context.id,context.public_id,context.decision_effect_id,\
                context.decision_date,context.task_id,context.task_kind,\
                context.task_due_date,context.state,context.created_at,\
                context.reverted_at,prior_effect.result
         FROM rec_decision_contexts context
         JOIN decision_effects prior_effect
           ON prior_effect.id=context.decision_effect_id
         WHERE prior_effect.module=?1
           AND prior_effect.student_id=?2
           AND prior_effect.ref_type=?3
           AND prior_effect.ref_id=?4
           AND prior_effect.state='active'
           AND context.state='active'
           AND context.decision_date<?5
         ORDER BY context.decision_date DESC,prior_effect.id DESC
         LIMIT 1";
    Ok(conn
        .query_row(
            sql,
            params![
                effect.module.as_str(),
                effect.student_id,
                effect.ref_type,
                effect.ref_id,
                decision_date
            ],
            prior_context_row,
        )
        .optional()?)
}

fn active_window_for_date(
    conn: &Connection,
    student_id: i64,
    content_id: i64,
    window_date: &str,
) -> CoreResult<Option<RecRetentionWindow>> {
    let sql = format!(
        "SELECT {WINDOW_COLS} FROM rec_retention_windows
         WHERE student_id=?1 AND content_id=?2 AND window_date=?3 AND state='active'"
    );
    Ok(conn
        .query_row(
            &sql,
            params![student_id, content_id, window_date],
            window_row,
        )
        .optional()?)
}

fn planned_interval_days(task: Option<&Task>, prior_date: NaiveDate) -> CoreResult<Option<i64>> {
    let Some(task) = task else {
        return Ok(None);
    };
    let due_date = NaiveDate::parse_from_str(&task.due_date, "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid("背诵任务到期日格式无效".into()))?;
    Ok(Some((due_date - prior_date).num_days().max(0)))
}

fn had_active_lapse_on_date(
    conn: &Connection,
    effect: &DecisionEffect,
    decision_date: &str,
) -> CoreResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT count(*)
         FROM rec_decision_contexts context
         JOIN decision_effects prior_effect
           ON prior_effect.id=context.decision_effect_id
         WHERE prior_effect.module=?1
           AND prior_effect.student_id=?2
           AND prior_effect.ref_type=?3
           AND prior_effect.ref_id=?4
           AND prior_effect.id<>?5
           AND prior_effect.result='fail'
           AND prior_effect.state='active'
           AND context.state='active'
           AND context.decision_date=?6",
        params![
            effect.module.as_str(),
            effect.student_id,
            effect.ref_type,
            effect.ref_id,
            effect.id,
            decision_date
        ],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

pub(crate) fn record_window(
    conn: &Connection,
    effect: &DecisionEffect,
    context: &RecDecisionContext,
    task: Option<&Task>,
) -> CoreResult<Option<RecRetentionWindow>> {
    let Some(prior) = latest_prior_distinct_date(conn, effect, &context.decision_date)? else {
        return Ok(None);
    };
    let current_date = NaiveDate::parse_from_str(&context.decision_date, "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid("背诵终审业务日期格式无效".into()))?;
    let prior_date = NaiveDate::parse_from_str(&prior.context.decision_date, "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid("前次背诵终审业务日期格式无效".into()))?;
    let actual_interval_days = (current_date - prior_date).num_days();
    if actual_interval_days < 1 {
        return Err(CoreError::Invalid(
            "保持证据必须比较不同业务日期的老师终审".into(),
        ));
    }

    let prior_window = active_window_for_date(
        conn,
        effect.student_id,
        effect.ref_id,
        &context.decision_date,
    )?;
    let revision = if let Some(window) = prior_window.as_ref() {
        window.revision + 1
    } else {
        conn.query_row(
            "SELECT COALESCE(max(revision),0)+1
             FROM rec_retention_windows
             WHERE student_id=?1 AND content_id=?2 AND window_date=?3",
            params![effect.student_id, effect.ref_id, context.decision_date],
            |row| row.get(0),
        )?
    };
    if let Some(window) = prior_window.as_ref() {
        learning_evidence::supersede_for_source(
            conn,
            EvidenceSourceModule::Recitation,
            "recitation_retention_window",
            &window.public_id,
            window.revision,
        )?;
        conn.execute(
            "UPDATE rec_retention_windows
             SET state='superseded',superseded_at=?2
             WHERE id=?1 AND state='active'",
            params![window.id, time::utc_now_rfc3339()],
        )?;
    }

    let planned_interval_days = planned_interval_days(task, prior_date)?;
    let recovered_after_lapse = effect.result == "pass"
        && (task.is_some_and(|value| value.kind == TaskKind::Makeup)
            || prior.result == "fail"
            || had_active_lapse_on_date(conn, effect, &context.decision_date)?);
    let created_at = time::utc_now_rfc3339();
    let public_id = ids::new_public_id();
    conn.execute(
        "INSERT INTO rec_retention_windows
          (public_id,student_id,content_id,window_date,revision,current_effect_id,
           previous_effect_id,current_context_id,previous_context_id,task_id,task_kind,
           task_due_date,planned_interval_days,actual_interval_days,result,
           recovered_after_lapse,state,created_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,'active',?17)",
        params![
            public_id,
            effect.student_id,
            effect.ref_id,
            context.decision_date,
            revision,
            effect.id,
            prior.context.decision_effect_id,
            context.id,
            prior.context.id,
            task.map(|value| value.id),
            task.map(|value| task_kind(value.kind)),
            task.map(|value| value.due_date.as_str()),
            planned_interval_days,
            actual_interval_days,
            effect.result,
            i64::from(recovered_after_lapse),
            created_at
        ],
    )?;
    let id = conn.last_insert_rowid();
    let sql = format!("SELECT {WINDOW_COLS} FROM rec_retention_windows WHERE id=?1");
    let window = conn.query_row(&sql, [id], window_row)?;
    Ok(Some(window))
}

pub(crate) fn revert_for_effect(conn: &Connection, effect: &DecisionEffect) -> CoreResult<usize> {
    let at = time::utc_now_rfc3339();
    let windows = conn.execute(
        "UPDATE rec_retention_windows
         SET state='reverted',reverted_at=?2
         WHERE current_effect_id=?1 AND state='active'",
        params![effect.id, at],
    )?;
    let contexts = conn.execute(
        "UPDATE rec_decision_contexts
         SET state='reverted',reverted_at=?2
         WHERE decision_effect_id=?1 AND state='active'",
        params![effect.id, at],
    )?;
    Ok(windows + contexts)
}
