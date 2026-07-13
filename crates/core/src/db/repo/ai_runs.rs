//! AI 运行账本仓储。结果只追加、状态转换受约束，重试不覆盖旧行。

use std::io;

use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::ids;
use crate::error::{CoreError, CoreResult};
use crate::models::{AiRun, AiRunStatus};

const COLS: &str = "id, public_id, idempotency_key, run_type, source_module, \
                    business_ref_type, business_ref_id, input_artifact_id, provider, \
                    model_name, model_version, config_version, prompt_or_rule_version, \
                    input_hash, output_hash, status, retry_of_ai_run_id, remote_run_id, \
                    confidence, output_json, error_meta_json, started_at, finished_at, created_at";

#[derive(Clone, Copy)]
pub struct NewAiRun<'a> {
    pub idempotency_key: &'a str,
    pub run_type: &'a str,
    pub source_module: &'a str,
    pub business_ref_type: &'a str,
    pub business_ref_id: &'a str,
    pub input_artifact_id: Option<i64>,
    pub provider: &'a str,
    pub model_name: &'a str,
    pub model_version: &'a str,
    pub config_version: &'a str,
    pub prompt_or_rule_version: &'a str,
    pub input_hash: &'a str,
    pub retry_of_ai_run_id: Option<i64>,
}

fn invalid_status(value: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        15,
        Type::Text,
        Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid ai run status: {value}"),
        )),
    )
}

fn row_to_ai_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<AiRun> {
    let status_raw: String = row.get(15)?;
    Ok(AiRun {
        id: row.get(0)?,
        public_id: row.get(1)?,
        idempotency_key: row.get(2)?,
        run_type: row.get(3)?,
        source_module: row.get(4)?,
        business_ref_type: row.get(5)?,
        business_ref_id: row.get(6)?,
        input_artifact_id: row.get(7)?,
        provider: row.get(8)?,
        model_name: row.get(9)?,
        model_version: row.get(10)?,
        config_version: row.get(11)?,
        prompt_or_rule_version: row.get(12)?,
        input_hash: row.get(13)?,
        output_hash: row.get(14)?,
        status: AiRunStatus::from_db(&status_raw).ok_or_else(|| invalid_status(status_raw))?,
        retry_of_ai_run_id: row.get(16)?,
        remote_run_id: row.get(17)?,
        confidence: row.get(18)?,
        output_json: row.get(19)?,
        error_meta_json: row.get(20)?,
        started_at: row.get(21)?,
        finished_at: row.get(22)?,
        created_at: row.get(23)?,
    })
}

fn required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        Err(CoreError::Invalid(format!("ai run {field} is required")))
    } else {
        Ok(())
    }
}

fn normalized_hash(value: &str, field: &str) -> CoreResult<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!(
            "ai run {field} must be 64 hexadecimal characters"
        )));
    }
    Ok(value)
}

fn validate_new(input: &NewAiRun<'_>) -> CoreResult<String> {
    for (value, field) in [
        (input.idempotency_key, "idempotency_key"),
        (input.run_type, "run_type"),
        (input.source_module, "source_module"),
        (input.business_ref_type, "business_ref_type"),
        (input.business_ref_id, "business_ref_id"),
        (input.provider, "provider"),
        (input.model_name, "model_name"),
        (input.model_version, "model_version"),
        (input.config_version, "config_version"),
        (input.prompt_or_rule_version, "prompt_or_rule_version"),
    ] {
        required(value, field)?;
    }
    normalized_hash(input.input_hash, "input_hash")
}

fn same_identity(existing: &AiRun, input: &NewAiRun<'_>, input_hash: &str) -> bool {
    existing.run_type == input.run_type.trim()
        && existing.source_module == input.source_module.trim()
        && existing.business_ref_type == input.business_ref_type.trim()
        && existing.business_ref_id == input.business_ref_id.trim()
        && existing.input_artifact_id == input.input_artifact_id
        && existing.provider == input.provider.trim()
        && existing.model_name == input.model_name.trim()
        && existing.model_version == input.model_version.trim()
        && existing.config_version == input.config_version.trim()
        && existing.prompt_or_rule_version == input.prompt_or_rule_version.trim()
        && existing.input_hash == input_hash
        && existing.retry_of_ai_run_id == input.retry_of_ai_run_id
}

/// 幂等创建 pending run。同一 key 如果指向不同输入，明确拒绝而不是静默复用。
pub fn create_or_get(conn: &Connection, input: &NewAiRun<'_>) -> CoreResult<AiRun> {
    let input_hash = validate_new(input)?;
    if let Some(parent_id) = input.retry_of_ai_run_id {
        let parent = get_by_id(conn, parent_id)?
            .ok_or_else(|| CoreError::NotFound(format!("retry ai run {parent_id}")))?;
        if parent.status != AiRunStatus::Failed
            || parent.run_type != input.run_type.trim()
            || parent.source_module != input.source_module.trim()
            || parent.business_ref_type != input.business_ref_type.trim()
            || parent.business_ref_id != input.business_ref_id.trim()
        {
            return Err(CoreError::Invalid(
                "retry ai run must reference a failed run in the same business scope".into(),
            ));
        }
    }

    conn.execute(
        "INSERT INTO ai_runs
            (public_id, idempotency_key, run_type, source_module, business_ref_type,
             business_ref_id, input_artifact_id, provider, model_name, model_version,
             config_version, prompt_or_rule_version, input_hash, status, retry_of_ai_run_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'pending', ?14)
         ON CONFLICT(idempotency_key) DO NOTHING",
        params![
            ids::new_public_id(),
            input.idempotency_key.trim(),
            input.run_type.trim(),
            input.source_module.trim(),
            input.business_ref_type.trim(),
            input.business_ref_id.trim(),
            input.input_artifact_id,
            input.provider.trim(),
            input.model_name.trim(),
            input.model_version.trim(),
            input.config_version.trim(),
            input.prompt_or_rule_version.trim(),
            input_hash,
            input.retry_of_ai_run_id,
        ],
    )?;

    let run = get_by_idempotency_key(conn, input.idempotency_key.trim())?
        .ok_or_else(|| CoreError::Db("ai run insert did not produce a row".into()))?;
    if !same_identity(&run, input, &input_hash) {
        return Err(CoreError::Invalid(
            "ai run idempotency_key already belongs to different input".into(),
        ));
    }
    Ok(run)
}

pub fn get_by_id(conn: &Connection, id: i64) -> CoreResult<Option<AiRun>> {
    let sql = format!("SELECT {COLS} FROM ai_runs WHERE id=?1");
    Ok(conn.query_row(&sql, [id], row_to_ai_run).optional()?)
}

pub fn get_by_idempotency_key(conn: &Connection, key: &str) -> CoreResult<Option<AiRun>> {
    let sql = format!("SELECT {COLS} FROM ai_runs WHERE idempotency_key=?1");
    Ok(conn.query_row(&sql, [key], row_to_ai_run).optional()?)
}

#[allow(clippy::too_many_arguments)]
pub fn find_succeeded_cache(
    conn: &Connection,
    run_type: &str,
    input_hash: &str,
    provider: &str,
    model_name: &str,
    model_version: &str,
    config_version: &str,
    prompt_or_rule_version: &str,
) -> CoreResult<Option<AiRun>> {
    let sql = format!(
        "SELECT {COLS} FROM ai_runs
         WHERE run_type=?1 AND input_hash=?2 AND provider=?3 AND model_name=?4
           AND model_version=?5 AND config_version=?6 AND prompt_or_rule_version=?7
           AND status='succeeded'
         ORDER BY id DESC LIMIT 1"
    );
    Ok(conn
        .query_row(
            &sql,
            params![
                run_type,
                input_hash.trim().to_ascii_lowercase(),
                provider,
                model_name,
                model_version,
                config_version,
                prompt_or_rule_version,
            ],
            row_to_ai_run,
        )
        .optional()?)
}

fn transition_error(conn: &Connection, id: i64, expected: &str) -> CoreError {
    match get_by_id(conn, id) {
        Ok(None) => CoreError::NotFound(format!("ai run {id}")),
        Ok(Some(run)) => CoreError::Invalid(format!(
            "ai run {id} is {}, expected {expected}",
            run.status.as_str()
        )),
        Err(err) => err,
    }
}

pub fn start(
    conn: &Connection,
    id: i64,
    started_at: &str,
    remote_run_id: Option<&str>,
) -> CoreResult<AiRun> {
    let changed = conn.execute(
        "UPDATE ai_runs
         SET status='processing', started_at=?2, remote_run_id=?3
         WHERE id=?1 AND status='pending'",
        params![id, started_at, remote_run_id],
    )?;
    if changed != 1 {
        return Err(transition_error(conn, id, "pending"));
    }
    get_by_id(conn, id)?.ok_or_else(|| CoreError::NotFound(format!("ai run {id}")))
}

pub fn finalize_succeeded(
    conn: &Connection,
    id: i64,
    output_hash: &str,
    confidence: Option<f64>,
    output_json: &str,
    finished_at: &str,
) -> CoreResult<AiRun> {
    let output_hash = normalized_hash(output_hash, "output_hash")?;
    let changed = conn.execute(
        "UPDATE ai_runs
         SET status='succeeded', output_hash=?2, confidence=?3, output_json=?4,
             error_meta_json=NULL, finished_at=?5
         WHERE id=?1 AND status='processing'",
        params![id, output_hash, confidence, output_json, finished_at],
    )?;
    if changed != 1 {
        return Err(transition_error(conn, id, "processing"));
    }
    get_by_id(conn, id)?.ok_or_else(|| CoreError::NotFound(format!("ai run {id}")))
}

pub fn finalize_failed(
    conn: &Connection,
    id: i64,
    error_meta_json: &str,
    finished_at: &str,
) -> CoreResult<AiRun> {
    let changed = conn.execute(
        "UPDATE ai_runs
         SET status='failed', error_meta_json=?2, finished_at=?3
         WHERE id=?1 AND status='processing'",
        params![id, error_meta_json, finished_at],
    )?;
    if changed != 1 {
        return Err(transition_error(conn, id, "processing"));
    }
    get_by_id(conn, id)?.ok_or_else(|| CoreError::NotFound(format!("ai run {id}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    const INPUT_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OUTPUT_HASH: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const STARTED: &str = "2026-07-13T08:00:00.000Z";
    const FINISHED: &str = "2026-07-13T08:00:01.000Z";

    fn input(key: &'static str) -> NewAiRun<'static> {
        NewAiRun {
            idempotency_key: key,
            run_type: "asr",
            source_module: "recitation",
            business_ref_type: "submission",
            business_ref_id: "42",
            input_artifact_id: None,
            provider: "fixture",
            model_name: "asr-fixture",
            model_version: "1",
            config_version: "1",
            prompt_or_rule_version: "1",
            input_hash: INPUT_HASH,
            retry_of_ai_run_id: None,
        }
    }

    #[test]
    fn idempotency_key_reuses_only_identical_input() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();

        let first = create_or_get(&conn, &input("asr:42:v1")).unwrap();
        let same = create_or_get(&conn, &input("asr:42:v1")).unwrap();
        assert_eq!(first.id, same.id);
        assert!(create_or_get(
            &conn,
            &NewAiRun {
                business_ref_id: "43",
                ..input("asr:42:v1")
            }
        )
        .is_err());
    }

    #[test]
    fn lifecycle_and_cache_are_append_only() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let run = create_or_get(&conn, &input("asr:42:v1")).unwrap();

        start(&conn, run.id, STARTED, Some("remote-1")).unwrap();
        assert!(finalize_succeeded(
            &conn,
            run.id,
            OUTPUT_HASH,
            Some(0.95),
            r#"{"text":"missing schema version"}"#,
            FINISHED,
        )
        .is_err());
        assert_eq!(
            get_by_id(&conn, run.id).unwrap().unwrap().status,
            AiRunStatus::Processing
        );
        let done = finalize_succeeded(
            &conn,
            run.id,
            OUTPUT_HASH,
            Some(0.95),
            r#"{"schema_version":1,"text":"ok"}"#,
            FINISHED,
        )
        .unwrap();
        assert_eq!(done.status, AiRunStatus::Succeeded);
        assert!(start(&conn, run.id, STARTED, None).is_err());
        assert_eq!(
            find_succeeded_cache(
                &conn,
                "asr",
                INPUT_HASH,
                "fixture",
                "asr-fixture",
                "1",
                "1",
                "1"
            )
            .unwrap()
            .unwrap()
            .id,
            run.id
        );
    }

    #[test]
    fn retry_requires_failed_parent_in_same_scope() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let original = create_or_get(&conn, &input("asr:42:v1")).unwrap();
        start(&conn, original.id, STARTED, None).unwrap();
        finalize_failed(
            &conn,
            original.id,
            r#"{"schema_version":1,"code":"timeout"}"#,
            FINISHED,
        )
        .unwrap();

        let retry = create_or_get(
            &conn,
            &NewAiRun {
                idempotency_key: "asr:42:retry:1",
                retry_of_ai_run_id: Some(original.id),
                ..input("unused")
            },
        )
        .unwrap();
        assert_eq!(retry.retry_of_ai_run_id, Some(original.id));
        assert_eq!(original.id + 1, retry.id);
    }
}
