//! M1.1 结构化背诵工作台的隔离真机验收夹具。
//!
//! 夹具只允许写入 `jiaofu-recitation-fixture-*` 根下、且 bundle id 为
//! `com.jiaofu.suite.recitationfixture` 的全新数据目录。学生、内容、任务、
//! 有效 WAV 录音、ASR run、机器总判和逐点评分均通过正式迁移与业务服务形成；
//! 评分点设置、老师逐点终审和快捷键操作留给真 `.app`，`verify` 再检查持久化。

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use suite_core::db::repo::classes;
use suite_core::db::repo::students::{self, StudentInput};
use suite_core::db::repo::submissions;
use suite_core::domain::{hashing, time};
use suite_core::ports::RecognizedWord;

use module_recitation::db::contents::{self, ContentInput};
use module_recitation::db::structured::{self, CreateRubricDraftInput, RubricPointDraftInput};
use module_recitation::service::ai_pipeline::{
    begin_asr_run, finish_asr_success, AsrRunDescriptor, BeginAsrRun, FinishAsrSuccessInput,
};
use module_recitation::service::import::import_resolved;
use module_recitation::service::scoring::{score_submission, ScoreCfg};
use module_recitation::service::structured_scoring::record_score_if_ready;
use module_recitation::service::tasks::generate_normal;

const FIXTURE_BUNDLE_ID: &str = "com.jiaofu.suite.recitationfixture";
const FIXTURE_ROOT_PREFIX: &str = "jiaofu-recitation-fixture-";
const FIXTURE_MARKER: &str = "recitation-acceptance-fixture.json";
const FIXTURE_SCHEMA: i64 = 1;
const TEACHER: &str = "recitation-fixture-teacher";
const SCORED_CONTENT_NO: &str = "M1-REVIEW-001";
const SETUP_CONTENT_NO: &str = "M1-RUBRIC-SETUP";
const ANSWER_TEXT: &str =
    "洋务运动前期以自强为口号，创办军事工业；后期以求富为口号，创办民用企业。";
const EMPTY_ITEMS: &str = r#"{"schema_version":1,"items":[]}"#;

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy)]
struct Scenario {
    student_no: &'static str,
    student_name: &'static str,
    transcript: &'static str,
}

const SCENARIOS: [Scenario; 3] = [
    Scenario {
        student_no: "1001",
        student_name: "矛盾优先",
        transcript: "洋务运动前期以求富为口号，创办军事工业；后期以自强为口号，创办民用企业。",
    },
    Scenario {
        student_no: "1002",
        student_name: "遗漏待判",
        transcript: "洋务运动前期以自强为口号，创办军事工业。",
    },
    Scenario {
        student_no: "1003",
        student_name: "完整通过",
        transcript: ANSWER_TEXT,
    },
];

#[derive(Debug, Deserialize, Serialize)]
struct FixtureMarker {
    schema_version: i64,
    bundle_id: String,
    business_date: String,
    scored_content_no: String,
    setup_content_no: String,
    expected_review_order: Vec<String>,
}

#[derive(Debug, Serialize)]
struct FixtureReport {
    schema_version: i64,
    phase: String,
    data_dir: String,
    business_date: String,
    migration_count: i64,
    integrity_check: String,
    foreign_key_violations: i64,
    students: i64,
    contents: i64,
    tasks: i64,
    submissions: i64,
    pending_reviews: i64,
    confirmed_submissions: i64,
    decision_effects: i64,
    structured_scores: i64,
    point_states: BTreeMap<String, i64>,
    active_point_reviews: i64,
    point_review_items: i64,
    scored_content_confirmed_rubrics: i64,
    setup_content_confirmed_rubrics: i64,
    expected_review_order: Vec<String>,
    archived_audio_files: i64,
    missing_audio_files: i64,
}

fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

fn validate_fixture_data_dir(data_dir: &Path) -> AppResult<()> {
    if !data_dir.is_absolute() {
        return Err(invalid("--data-dir 必须是绝对路径"));
    }
    if data_dir.file_name().and_then(|value| value.to_str()) != Some(FIXTURE_BUNDLE_ID) {
        return Err(invalid(format!("隔离目录末级必须是 {FIXTURE_BUNDLE_ID}")));
    }
    let has_isolated_root = data_dir.components().any(|component| match component {
        Component::Normal(value) => value
            .to_str()
            .is_some_and(|value| value.starts_with(FIXTURE_ROOT_PREFIX)),
        _ => false,
    });
    if !has_isolated_root {
        return Err(invalid(format!(
            "隔离目录必须位于名称以 {FIXTURE_ROOT_PREFIX} 开头的根目录下"
        )));
    }
    Ok(())
}

fn run_all_migrations(conn: &Connection) -> AppResult<()> {
    suite_core::db::run_migrations(conn, suite_core::db::CORE_MIGRATIONS)?;
    suite_core::db::run_migrations(conn, module_knowledge::knowledge_migrations())?;
    suite_core::db::run_migrations(conn, module_recitation::recitation_migrations())?;
    suite_core::db::run_migrations(conn, module_exam::exam_migrations())?;
    suite_core::db::run_migrations(conn, module_wrongbook::wrongbook_migrations())?;
    suite_core::db::run_migrations(conn, module_profile::profile_migrations())?;
    Ok(())
}

fn business_date() -> String {
    time::shanghai_business_date_at(Utc::now())
        .format("%Y-%m-%d")
        .to_string()
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_fixture_wav(path: &Path, frequency_hz: f32) -> AppResult<()> {
    const SAMPLE_RATE: u32 = 16_000;
    const DURATION_SECONDS: u32 = 6;
    const CHANNELS: u16 = 1;
    const BITS_PER_SAMPLE: u16 = 16;
    let sample_count = SAMPLE_RATE * DURATION_SECONDS;
    let data_len = sample_count * u32::from(BITS_PER_SAMPLE / 8);
    let mut bytes = Vec::with_capacity((44 + data_len) as usize);
    bytes.extend_from_slice(b"RIFF");
    push_u32(&mut bytes, 36 + data_len);
    bytes.extend_from_slice(b"WAVEfmt ");
    push_u32(&mut bytes, 16);
    push_u16(&mut bytes, 1);
    push_u16(&mut bytes, CHANNELS);
    push_u32(&mut bytes, SAMPLE_RATE);
    push_u32(
        &mut bytes,
        SAMPLE_RATE * u32::from(CHANNELS) * u32::from(BITS_PER_SAMPLE / 8),
    );
    push_u16(&mut bytes, CHANNELS * (BITS_PER_SAMPLE / 8));
    push_u16(&mut bytes, BITS_PER_SAMPLE);
    bytes.extend_from_slice(b"data");
    push_u32(&mut bytes, data_len);
    for index in 0..sample_count {
        let time = index as f32 / SAMPLE_RATE as f32;
        let envelope = if index < SAMPLE_RATE / 20 {
            index as f32 / (SAMPLE_RATE / 20) as f32
        } else {
            1.0
        };
        let sample =
            (time * frequency_hz * std::f32::consts::TAU).sin() * 0.12 * envelope * i16::MAX as f32;
        bytes.extend_from_slice(&(sample as i16).to_le_bytes());
    }
    fs::write(path, bytes)?;
    Ok(())
}

fn seed_confirmed_rubric(conn: &Connection, content_id: i64) -> AppResult<()> {
    let answer = structured::current_answer_version(conn, content_id)?
        .ok_or_else(|| invalid("评分内容缺少当前答案版本"))?;
    let points = [
        RubricPointDraftInput {
            stable_key: "early_slogan",
            canonical_text: "前期以自强为口号",
            required_entities_json: r#"{"schema_version":1,"items":["前期","自强"]}"#,
            allowed_paraphrases_json: EMPTY_ITEMS,
            contradiction_rules_json: r#"{"schema_version":1,"items":["前期以求富为口号"]}"#,
            required: true,
            weight: 1.0,
            order_index: 0,
            knowledge_node_id: None,
            knowledge_link_state: "none",
            verified_by: None,
            verified_at: None,
        },
        RubricPointDraftInput {
            stable_key: "military_industry",
            canonical_text: "创办军事工业",
            required_entities_json: r#"{"schema_version":1,"items":["军事工业"]}"#,
            allowed_paraphrases_json: EMPTY_ITEMS,
            contradiction_rules_json: EMPTY_ITEMS,
            required: true,
            weight: 1.0,
            order_index: 1,
            knowledge_node_id: None,
            knowledge_link_state: "none",
            verified_by: None,
            verified_at: None,
        },
        RubricPointDraftInput {
            stable_key: "late_slogan",
            canonical_text: "后期以求富为口号",
            required_entities_json: r#"{"schema_version":1,"items":["后期","求富"]}"#,
            allowed_paraphrases_json: EMPTY_ITEMS,
            contradiction_rules_json: r#"{"schema_version":1,"items":["后期以自强为口号"]}"#,
            required: true,
            weight: 1.0,
            order_index: 2,
            knowledge_node_id: None,
            knowledge_link_state: "none",
            verified_by: None,
            verified_at: None,
        },
        RubricPointDraftInput {
            stable_key: "civilian_industry",
            canonical_text: "创办民用企业",
            required_entities_json: r#"{"schema_version":1,"items":["民用企业"]}"#,
            allowed_paraphrases_json: EMPTY_ITEMS,
            contradiction_rules_json: EMPTY_ITEMS,
            required: true,
            weight: 1.0,
            order_index: 3,
            knowledge_node_id: None,
            knowledge_link_state: "none",
            verified_by: None,
            verified_at: None,
        },
    ];
    let rubric = structured::create_rubric_draft(
        conn,
        &CreateRubricDraftInput {
            answer_version_id: answer.id,
            generated_by_ai_run_id: None,
            created_by: TEACHER,
            points: &points,
        },
    )?;
    structured::confirm_rubric(conn, rubric.id, TEACHER)?;
    Ok(())
}

fn transcript_words(transcript: &str) -> Vec<RecognizedWord> {
    let chunks = transcript
        .split_inclusive(['，', '；', '。'])
        .filter(|chunk| !chunk.trim().is_empty())
        .collect::<Vec<_>>();
    let span = 5_000_u64 / chunks.len().max(1) as u64;
    chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| RecognizedWord {
            text: chunk.to_string(),
            start_ms: 500 + index as u64 * span,
            end_ms: 500 + (index as u64 + 1) * span,
        })
        .collect()
}

fn seed_submission(
    conn: &Connection,
    archive_dir: &Path,
    scenario: &Scenario,
    student_id: i64,
    content_id: i64,
    index: usize,
) -> AppResult<()> {
    let audio_path = archive_dir.join(format!("{}-review.wav", scenario.student_no));
    write_fixture_wav(&audio_path, 220.0 + index as f32 * 55.0)?;
    let audio_hash = hashing::sha256_file(&audio_path)?;
    let audio_path_text = audio_path
        .to_str()
        .ok_or_else(|| invalid("录音归档路径不是有效 UTF-8"))?;
    let (submission_id, task_id) = import_resolved(
        conn,
        audio_path_text,
        &audio_hash,
        student_id,
        content_id,
        Some(6_000),
    )?;
    if task_id.is_none() {
        return Err(invalid("隔离夹具提交没有匹配到正式任务"));
    }
    submissions::set_archived_path(conn, submission_id, audio_path_text)?;
    let descriptor = AsrRunDescriptor {
        provider: "fixture",
        model_name: "timestamped-transcript",
        model_version: "1",
        config_version: "recitation-fixture-v1",
        prompt_or_rule_version: "fixture-transcript-v1",
    };
    let ai_run_id = match begin_asr_run(conn, submission_id, &audio_hash, &descriptor, false)? {
        BeginAsrRun::Execute { ai_run_id, .. } => ai_run_id,
        BeginAsrRun::Cached { .. } => return Err(invalid("全新夹具不应命中 ASR 缓存")),
    };
    let words = transcript_words(scenario.transcript);
    finish_asr_success(
        conn,
        ai_run_id,
        &FinishAsrSuccessInput {
            submission_id,
            raw_transcript: scenario.transcript,
            normalized_transcript: scenario.transcript,
            normalization_version: "fixture-normalize-v1",
            words: &words,
            duration_ms: 6_000,
        },
    )?;
    submissions::set_recognition(
        conn,
        submission_id,
        Some(scenario.transcript),
        "ok",
        Some(r#"{"schema_version":1,"fixture":true}"#),
        Some(6_000),
    )?;
    let today = time::shanghai_business_date_at(Utc::now());
    let cfg = ScoreCfg::default();
    score_submission(conn, submission_id, &words, today, &cfg)?;
    if record_score_if_ready(conn, submission_id, &cfg)?.is_none() {
        return Err(invalid("已确认 rubric 未生成结构化评分"));
    }
    Ok(())
}

fn seed_fixture(data_dir: &Path) -> AppResult<FixtureReport> {
    validate_fixture_data_dir(data_dir)?;
    let db_path = data_dir.join("data.db");
    if db_path.exists() {
        return Err(invalid(format!(
            "拒绝覆盖已有数据库：{}",
            db_path.display()
        )));
    }
    if data_dir.exists() && fs::read_dir(data_dir)?.next().is_some() {
        return Err(invalid(format!(
            "seed 只接受全新空目录：{}",
            data_dir.display()
        )));
    }
    let archive_dir = data_dir.join("archive/recitation-acceptance-fixture");
    fs::create_dir_all(&archive_dir)?;
    let conn = suite_core::db::open(&db_path)?;
    run_all_migrations(&conn)?;
    conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])?;
    let subject_id = conn.last_insert_rowid();
    let class = classes::create(
        &conn,
        "M1.1 隔离验收班",
        Some("人教版八年级上册"),
        Some("2026秋"),
    )?;
    let scored_content = contents::upsert(
        &conn,
        &ContentInput {
            content_no: SCORED_CONTENT_NO,
            title: "洋务运动口号与企业",
            answer_text: ANSWER_TEXT,
            subject_id: Some(subject_id),
            enabled: true,
        },
    )?;
    contents::upsert(
        &conn,
        &ContentInput {
            content_no: SETUP_CONTENT_NO,
            title: "评分点一键设置验收",
            answer_text: "鸦片战争爆发于1840年，清政府战败后签订《南京条约》。",
            subject_id: Some(subject_id),
            enabled: true,
        },
    )?;
    seed_confirmed_rubric(&conn, scored_content.id)?;
    let students = SCENARIOS
        .iter()
        .map(|scenario| {
            students::upsert(
                &conn,
                &StudentInput {
                    student_no: scenario.student_no,
                    name: scenario.student_name,
                    class_id: Some(class.id),
                    enabled: true,
                },
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let due_date = business_date();
    let pairs = students
        .iter()
        .map(|student| (student.id, scored_content.id))
        .collect::<Vec<_>>();
    let task_report = generate_normal(&conn, &due_date, &pairs)?;
    if task_report.created != SCENARIOS.len() {
        return Err(invalid("隔离夹具未创建完整背诵任务"));
    }
    for (index, (scenario, student)) in SCENARIOS.iter().zip(students.iter()).enumerate() {
        seed_submission(
            &conn,
            &archive_dir,
            scenario,
            student.id,
            scored_content.id,
            index,
        )?;
    }
    let marker = FixtureMarker {
        schema_version: FIXTURE_SCHEMA,
        bundle_id: FIXTURE_BUNDLE_ID.to_owned(),
        business_date: due_date,
        scored_content_no: SCORED_CONTENT_NO.to_owned(),
        setup_content_no: SETUP_CONTENT_NO.to_owned(),
        expected_review_order: SCENARIOS
            .iter()
            .map(|scenario| scenario.student_no.to_owned())
            .collect(),
    };
    fs::write(
        data_dir.join(FIXTURE_MARKER),
        serde_json::to_vec_pretty(&marker)?,
    )?;
    drop(conn);
    verify_fixture(data_dir, "seeded")
}

fn count(conn: &Connection, sql: &str) -> AppResult<i64> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn grouped_counts(conn: &Connection, sql: &str) -> AppResult<BTreeMap<String, i64>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
    })?;
    Ok(rows.collect::<Result<BTreeMap<_, _>, _>>()?)
}

fn rubric_count(conn: &Connection, content_no: &str) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*)
         FROM rec_rubric_versions rubric
         JOIN rec_answer_versions answer ON answer.id=rubric.answer_version_id
         JOIN rec_contents content ON content.id=answer.content_id
         WHERE content.content_no=?1
           AND content.answer_version=answer.answer_version
           AND rubric.status='confirmed'",
        [content_no],
        |row| row.get(0),
    )?)
}

fn inspect_fixture(data_dir: &Path, phase: &str) -> AppResult<FixtureReport> {
    validate_fixture_data_dir(data_dir)?;
    if !data_dir.join(FIXTURE_MARKER).is_file() {
        return Err(invalid("缺少隔离夹具标记文件"));
    }
    let marker: FixtureMarker = serde_json::from_slice(&fs::read(data_dir.join(FIXTURE_MARKER))?)?;
    if marker.schema_version != FIXTURE_SCHEMA || marker.bundle_id != FIXTURE_BUNDLE_ID {
        return Err(invalid("隔离夹具标记版本或 bundle id 不匹配"));
    }
    let conn = Connection::open_with_flags(
        data_dir.join("data.db"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.pragma_update(None, "foreign_keys", true)?;
    let integrity_check =
        conn.query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))?;
    let foreign_key_violations = {
        let mut statement = conn.prepare("PRAGMA foreign_key_check")?;
        let mut rows = statement.query([])?;
        let mut total = 0;
        while rows.next()?.is_some() {
            total += 1;
        }
        total
    };
    let mut audio_paths = Vec::new();
    {
        let mut statement = conn.prepare(
            "SELECT COALESCE(archived_path,file_path)
             FROM submissions WHERE module='recitation' ORDER BY id",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        for row in rows {
            audio_paths.push(row?);
        }
    }
    let missing_audio_files = audio_paths
        .iter()
        .filter(|path| !Path::new(path).is_file())
        .count() as i64;
    Ok(FixtureReport {
        schema_version: FIXTURE_SCHEMA,
        phase: phase.to_owned(),
        data_dir: data_dir.display().to_string(),
        business_date: marker.business_date,
        migration_count: count(&conn, "SELECT COUNT(*) FROM schema_migrations")?,
        integrity_check,
        foreign_key_violations,
        students: count(&conn, "SELECT COUNT(*) FROM students")?,
        contents: count(&conn, "SELECT COUNT(*) FROM rec_contents")?,
        tasks: count(
            &conn,
            "SELECT COUNT(*) FROM tasks WHERE module='recitation'",
        )?,
        submissions: count(
            &conn,
            "SELECT COUNT(*) FROM submissions WHERE module='recitation'",
        )?,
        pending_reviews: count(
            &conn,
            "SELECT COUNT(*)
             FROM verdicts verdict
             JOIN submissions submission ON submission.id=verdict.submission_id
             WHERE submission.module='recitation' AND verdict.human_result IS NULL",
        )?,
        confirmed_submissions: count(
            &conn,
            "SELECT COUNT(*) FROM submissions
             WHERE module='recitation' AND status='confirmed'",
        )?,
        decision_effects: count(
            &conn,
            "SELECT COUNT(*) FROM decision_effects
             WHERE module='recitation' AND state='active'",
        )?,
        structured_scores: count(
            &conn,
            "SELECT COUNT(*) FROM rec_score_runs WHERE state='active'",
        )?,
        point_states: grouped_counts(
            &conn,
            "SELECT machine_state,COUNT(*) FROM rec_point_results
             GROUP BY machine_state ORDER BY machine_state",
        )?,
        active_point_reviews: count(
            &conn,
            "SELECT COUNT(*) FROM rec_point_review_revisions WHERE state='active'",
        )?,
        point_review_items: count(&conn, "SELECT COUNT(*) FROM rec_point_review_items")?,
        scored_content_confirmed_rubrics: rubric_count(&conn, SCORED_CONTENT_NO)?,
        setup_content_confirmed_rubrics: rubric_count(&conn, SETUP_CONTENT_NO)?,
        expected_review_order: marker.expected_review_order,
        archived_audio_files: audio_paths.len() as i64,
        missing_audio_files,
    })
}

fn expect(condition: bool, message: impl Into<String>) -> AppResult<()> {
    if condition {
        Ok(())
    } else {
        Err(invalid(message))
    }
}

fn verify_fixture(data_dir: &Path, phase: &str) -> AppResult<FixtureReport> {
    let report = inspect_fixture(data_dir, phase)?;
    let expected_migrations = (suite_core::db::CORE_MIGRATIONS.len()
        + module_knowledge::knowledge_migrations().len()
        + module_recitation::recitation_migrations().len()
        + module_exam::exam_migrations().len()
        + module_wrongbook::wrongbook_migrations().len()
        + module_profile::profile_migrations().len()) as i64;
    expect(
        report.migration_count == expected_migrations,
        format!("夹具必须包含全部 {expected_migrations} 个迁移"),
    )?;
    expect(report.integrity_check == "ok", "integrity_check 必须为 ok")?;
    expect(report.foreign_key_violations == 0, "夹具不能包含外键违规")?;
    expect(
        report.students == 3
            && report.contents == 2
            && report.tasks >= 3
            && report.submissions == 3,
        "夹具必须包含 3 名学生、2 篇内容和 3 条录音提交",
    )?;
    expect(
        report.structured_scores == 3,
        "夹具必须包含 3 条当前结构化评分",
    )?;
    expect(
        report.point_states
            == BTreeMap::from([
                ("contradiction".to_owned(), 2),
                ("covered".to_owned(), 8),
                ("omitted".to_owned(), 2),
            ]),
        "逐点机器状态分布不符合固定夹具",
    )?;
    expect(
        report.scored_content_confirmed_rubrics == 1,
        "待终审内容必须有且只有一个已确认 rubric",
    )?;
    expect(
        report.expected_review_order == ["1001", "1002", "1003"],
        "夹具预期风险顺序必须是矛盾、遗漏、完整通过",
    )?;
    expect(
        report.archived_audio_files == 3 && report.missing_audio_files == 0,
        "三段隔离录音必须真实存在于归档目录",
    )?;
    match phase {
        "seeded" => {
            expect(report.pending_reviews == 3, "seeded 阶段必须有 3 条待终审")?;
            expect(
                report.confirmed_submissions == 0
                    && report.decision_effects == 0
                    && report.active_point_reviews == 0
                    && report.point_review_items == 0,
                "seeded 阶段不能有老师终审、效果或逐点复核",
            )?;
            expect(
                report.setup_content_confirmed_rubrics == 0,
                "seeded 阶段评分点设置内容必须保持未配置",
            )?;
        }
        "reviewed" | "restarted" => {
            expect(report.pending_reviews == 0, "终审后不能残留待确认记录")?;
            expect(
                report.confirmed_submissions == 3 && report.decision_effects == 3,
                "终审后必须有 3 条 confirmed 提交和 3 条 active 效果",
            )?;
            expect(
                report.active_point_reviews >= 1 && report.point_review_items >= 4,
                "真机验收至少要保存一条完整逐点评审",
            )?;
            expect(
                report.setup_content_confirmed_rubrics == 1,
                "真机验收必须完成评分点一键设置",
            )?;
        }
        _ => return Err(invalid("--phase 只能是 seeded、reviewed 或 restarted")),
    }
    Ok(report)
}

fn parse_args() -> AppResult<(String, PathBuf, String)> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or_else(|| {
        invalid(
            "用法：m1_structured_fixture <seed|verify> --data-dir <绝对路径> [--phase seeded|reviewed|restarted]",
        )
    })?;
    let mut data_dir = None;
    let mut phase = "seeded".to_owned();
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--data-dir" => data_dir = args.next().map(PathBuf::from),
            "--phase" => phase = args.next().ok_or_else(|| invalid("--phase 缺少值"))?,
            _ => return Err(invalid(format!("未知参数：{argument}"))),
        }
    }
    Ok((
        command,
        data_dir.ok_or_else(|| invalid("缺少 --data-dir"))?,
        phase,
    ))
}

fn main() -> AppResult<()> {
    let (command, data_dir, phase) = parse_args()?;
    let report = match command.as_str() {
        "seed" => seed_fixture(&data_dir)?,
        "verify" => verify_fixture(&data_dir, &phase)?,
        _ => return Err(invalid("首个参数只能是 seed 或 verify")),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_data_dir(label: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "{FIXTURE_ROOT_PREFIX}{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let data_dir = root
            .join("Library/Application Support")
            .join(FIXTURE_BUNDLE_ID);
        (root, data_dir)
    }

    #[test]
    fn guard_rejects_production_data_dir() {
        assert!(validate_fixture_data_dir(Path::new(
            "/Users/teacher/Library/Application Support/com.jiaofu.suite"
        ))
        .is_err());
    }

    #[test]
    fn seeded_fixture_has_real_audio_and_no_teacher_effects() {
        let (root, data_dir) = test_data_dir("seeded");
        let report = seed_fixture(&data_dir).unwrap();
        assert_eq!(report.pending_reviews, 3);
        assert_eq!(report.structured_scores, 3);
        assert_eq!(report.archived_audio_files, 3);
        assert_eq!(report.missing_audio_files, 0);
        assert_eq!(report.decision_effects, 0);
        fs::remove_dir_all(root).unwrap();
    }
}
