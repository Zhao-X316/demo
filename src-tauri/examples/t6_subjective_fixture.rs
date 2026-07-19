//! 答题卡主观题工作台的隔离真机验收夹具。
//!
//! 夹具只允许写入 `jiaofu-subjective-fixture-*` 根下、且 bundle id 为
//! `com.jiaofu.suite.subjectivefixture` 的全新数据目录。题目、答案、rubric、
//! 页面、题区、OCR、简答分项分析均通过正式迁移与业务服务形成；老师终审和发布
//! 留给真 `.app` 操作，`verify` 再检查持久化结果。

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use image::{DynamicImage, ImageOutputFormat, Rgb, RgbImage};
use module_exam::dictation::DictationRecognitionState;
use module_exam::dictation_recognition::{
    DictationOcrOutput, DictationOcrRequest, DictationRecognizerDescriptor,
};
#[cfg(test)]
use module_exam::service::assessment::publish_attempt;
use module_exam::service::assessment::{
    add_assessment_item, confirm_assessment_version, create_assessment_draft, create_attempt,
    NewAssessmentDraft, NewAssessmentItem,
};
use module_exam::service::papers::{
    create_or_get_ingest_batch, decide_page_match, record_answer_region, record_page_alignment,
    record_page_quality, register_ingest_page, NewAnswerRegionRevision, NewIngestBatch,
    NewIngestPage, NewPageAlignmentRevision, NewPageMatchRevision, NewPageQualityRevision,
};
#[cfg(test)]
use module_exam::service::subjective::{
    accept_subjective_suggestion, correct_subjective_components, SubjectiveComponentGradeInput,
    SubjectiveWorkbenchRow,
};
use module_exam::service::subjective::{
    list_subjective_workbench, load_short_answer_grade_request, record_ocr_ai_run_transcription,
    record_short_answer_grade_ai_run,
};
use module_exam::short_answer_grading::{
    ShortAnswerGradeOutput, ShortAnswerGradeState, ShortAnswerGraderDescriptor,
    ShortAnswerPointResult, ShortAnswerPointStatus, SHORT_ANSWER_GRADE_SCHEMA_VERSION,
};
use module_knowledge::db::content::{
    add_ability_link, add_knowledge_link, create_answer_key_version, create_link_set,
    create_question, create_question_version, create_rubric_version, promote_question_version,
    NewAbilityLink, NewAnswerKeyVersion, NewAnswerSlot, NewKnowledgeLink, NewQuestion,
    NewQuestionVersion, NewRubricPoint, NewRubricVersion,
};
use module_knowledge::db::taxonomy::{
    create_ability_dimension, create_knowledge_map, create_knowledge_node, create_textbook_edition,
    NewAbilityDimension, NewKnowledgeMap, NewKnowledgeNode, NewTextbookEdition,
};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use suite_core::db::repo::ai_runs;
use suite_core::db::repo::artifacts::{create_or_get, NewArtifact};
use suite_core::db::repo::classes;
use suite_core::db::repo::students::{self, StudentInput};
use suite_core::domain::{hashing, ids, time};
use suite_core::models::{ArchiveStatus, ArtifactKind, PrivacyClass};

const FIXTURE_BUNDLE_ID: &str = "com.jiaofu.suite.subjectivefixture";
const FIXTURE_TITLE: &str = "答题卡主观题隔离验收";
const FIXTURE_SCHEMA: i64 = 2;
const TEACHER: &str = "subjective-fixture-teacher";

type AppResult<T> = Result<T, Box<dyn Error>>;

#[derive(Clone, Copy)]
struct Scenario {
    student_no: &'static str,
    student_name: &'static str,
    fill_text: &'static str,
    multi_fill_text: &'static str,
    short_text: &'static str,
    short_mode: ShortMode,
}

#[derive(Clone, Copy)]
enum ShortMode {
    Full,
    Contradicted,
}

const SCENARIOS: [Scenario; 2] = [
    Scenario {
        student_no: "S001",
        student_name: "建议明确",
        fill_text: "1842年",
        multi_fill_text: "1842年 广州",
        short_text: "洋务派只学习西方技术，没有改变封建制度，而且内部管理腐败。",
        short_mode: ShortMode::Full,
    },
    Scenario {
        student_no: "S002",
        student_name: "分歧待判",
        fill_text: "1840年",
        multi_fill_text: "1840年 广州",
        short_text: "洋务运动彻底改变了封建制度，但内部管理腐败。",
        short_mode: ShortMode::Contradicted,
    },
];

#[derive(Debug, Serialize)]
struct FixtureReport {
    schema_version: i64,
    phase: String,
    data_dir: String,
    assessment_version_id: i64,
    migration_count: i64,
    integrity_check: String,
    foreign_key_violations: i64,
    workbench_rows: usize,
    attempts: usize,
    short_answer_analyses: i64,
    subjective_components: i64,
    confirmed_rows: usize,
    confirmation_levels: BTreeMap<String, i64>,
    suggestion_outcomes: BTreeMap<String, i64>,
    attempt_states: BTreeMap<String, i64>,
    published_publications: i64,
    active_learning_evidence: i64,
    missing_artifact_files: i64,
}

#[derive(Clone, Copy)]
struct ContentBundle {
    assessment_version_id: i64,
    fill_item_id: i64,
    multi_fill_item_id: i64,
    short_item_id: i64,
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
            .is_some_and(|value| value.starts_with("jiaofu-subjective-fixture-")),
        _ => false,
    });
    if !has_isolated_root {
        return Err(invalid(
            "隔离目录必须位于名称以 jiaofu-subjective-fixture- 开头的根目录下",
        ));
    }
    Ok(())
}

fn run_all_migrations(conn: &Connection) -> AppResult<()> {
    suite_core::db::run_migrations(conn, suite_core::db::CORE_MIGRATIONS)?;
    suite_core::db::run_migrations(conn, module_knowledge::knowledge_migrations())?;
    suite_core::db::run_migrations(conn, module_recitation::recitation_migrations())?;
    suite_core::db::run_migrations(conn, module_exam::exam_migrations())?;
    Ok(())
}

fn accent_rgb(accent: &str) -> Rgb<u8> {
    let hex = accent.trim().trim_start_matches('#');
    let parse = |range: std::ops::Range<usize>| {
        hex.get(range)
            .and_then(|value| u8::from_str_radix(value, 16).ok())
            .unwrap_or(64)
    };
    Rgb([parse(0..2), parse(2..4), parse(4..6)])
}

fn write_png(path: &Path, title: &str, answer: &str, accent: &str) -> AppResult<Vec<u8>> {
    let mut image = RgbImage::from_pixel(1200, 360, Rgb([251, 250, 246]));
    let accent = accent_rgb(accent);
    for x in 24..38 {
        for y in 24..336 {
            image.put_pixel(x, y, accent);
        }
    }
    for (row, text) in [title, answer, "isolated-fixture"].into_iter().enumerate() {
        let top = 64 + row as u32 * 100;
        for (index, byte) in text.bytes().take(80).enumerate() {
            let left = 70 + index as u32 * 12;
            let height = 8 + u32::from(byte % 34);
            let width = 5 + u32::from(byte % 5);
            let color = if row == 1 { Rgb([38, 52, 44]) } else { accent };
            for x in left..(left + width) {
                for y in top..(top + height) {
                    image.put_pixel(x, y, color);
                }
            }
        }
    }
    let mut bytes = Vec::new();
    DynamicImage::ImageRgb8(image)
        .write_to(&mut io::Cursor::new(&mut bytes), ImageOutputFormat::Png)?;
    fs::write(path, &bytes)?;
    Ok(bytes)
}

struct FixtureArtifact<'a> {
    filename: &'a str,
    title: &'a str,
    answer: &'a str,
    kind: ArtifactKind,
    parent_artifact_id: Option<i64>,
    derivative_type: Option<&'a str>,
    accent: &'a str,
}

fn artifact(
    conn: &Connection,
    archive_dir: &Path,
    fixture: &FixtureArtifact<'_>,
) -> AppResult<(i64, String, Vec<u8>)> {
    let path = archive_dir.join(fixture.filename);
    let bytes = write_png(&path, fixture.title, fixture.answer, fixture.accent)?;
    let sha256 = hashing::sha256_hex(&bytes);
    let archived_path = path
        .to_str()
        .ok_or_else(|| invalid("夹具归档路径不是有效 UTF-8"))?;
    let item = create_or_get(
        conn,
        &NewArtifact {
            kind: fixture.kind,
            sha256: &sha256,
            mime_type: "image/png",
            byte_size: bytes.len() as i64,
            original_name: fixture
                .parent_artifact_id
                .is_none()
                .then_some(fixture.filename),
            original_path: None,
            archived_path,
            parent_artifact_id: fixture.parent_artifact_id,
            derivative_type: fixture.derivative_type,
            processing_version: "subjective-acceptance-fixture-v1",
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        },
    )?;
    Ok((item.id, sha256, bytes))
}

fn add_source_links(
    conn: &Connection,
    link_set_id: i64,
    source_type: &str,
    source_public_id: &str,
    knowledge_node_id: i64,
    ability_dimension_id: i64,
    response_mode: &str,
) -> AppResult<()> {
    add_knowledge_link(
        conn,
        &NewKnowledgeLink {
            link_set_id,
            source_type,
            source_public_id,
            knowledge_node_id,
            relation_type: "direct_assessment",
            confirmation_level: "teacher_confirmed",
            verified_by: Some(TEACHER),
        },
    )?;
    add_ability_link(
        conn,
        &NewAbilityLink {
            link_set_id,
            source_type,
            source_public_id,
            ability_dimension_id,
            evidence_strength: 0.7,
            response_mode,
            confirmation_level: "teacher_confirmed",
            verified_by: Some(TEACHER),
        },
    )?;
    Ok(())
}

fn seed_content(conn: &Connection, class_id: i64, subject_id: i64) -> AppResult<ContentBundle> {
    let edition = create_textbook_edition(
        conn,
        &NewTextbookEdition {
            subject_id,
            publisher_code: "PEP",
            edition_code: "2024",
            title: "中国历史八年级上册",
            grade: "8",
            volume: "upper",
            curriculum_region: Some("CN"),
        },
    )?;
    let map = create_knowledge_map(
        conn,
        &NewKnowledgeMap {
            textbook_edition_id: edition.id,
            revision: 1,
            state: "confirmed",
            supersedes_map_id: None,
        },
    )?;
    let knowledge = create_knowledge_node(
        conn,
        &NewKnowledgeNode {
            stable_id: Some("subjective.fixture.treaty-and-modernization"),
            knowledge_map_id: map.id,
            curriculum_node_id: None,
            parent_id: None,
            code: Some("SUBJECTIVE-FIXTURE-K1"),
            title: "近代史关键史实与洋务运动局限",
            description: Some("主观题隔离验收知识节点"),
            order_index: 1,
        },
    )?;
    let ability = create_ability_dimension(
        conn,
        &NewAbilityDimension {
            stable_id: Some("subjective.fixture.written-reasoning"),
            subject_id,
            revision: 1,
            code: "written_reasoning",
            title: "历史书面表达与原因分析",
            description: Some("主观题隔离验收能力节点"),
            supersedes_dimension_id: None,
        },
    )?;

    let fill_question = create_question(
        conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: TEACHER,
            question_family_id: None,
            rights_status: "unknown",
            sharing_allowed: false,
        },
    )?;
    let fill_version = create_question_version(
        conn,
        &NewQuestionVersion {
            question_id: fill_question.id,
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
    )?;
    let fill_slots = [NewAnswerSlot {
        stable_id: Some("treaty_year"),
        order_index: 0,
        canonical_answers_json: r#"{"schema_version":1,"answers":["1842年","1842"]}"#,
        normalization_rules_json: None,
        max_score: 1.0,
    }];
    let fill_answer = create_answer_key_version(
        conn,
        &NewAnswerKeyVersion {
            question_version_id: fill_version.id,
            revision: 1,
            answer_json: r#"{"schema_version":1,"values":["1842年"],"accepted_variants":["1842"]}"#,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_answer_key_id: None,
            slots: &fill_slots,
        },
    )?;
    let fill_points = [NewRubricPoint {
        stable_id: Some("treaty_year"),
        order_index: 0,
        canonical_text: "准确写出1842年",
        allowed_paraphrases_json: Some(r#"["1842"]"#),
        required_concepts_json: Some(r#"["1842"]"#),
        max_score: 1.0,
    }];
    let fill_rubric = create_rubric_version(
        conn,
        &NewRubricVersion {
            question_version_id: fill_version.id,
            revision: 1,
            max_score: 1.0,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_rubric_id: None,
            points: &fill_points,
        },
    )?;
    let fill_links = create_link_set(
        conn,
        fill_version.id,
        map.id,
        1,
        "confirmed",
        Some(TEACHER),
        None,
    )?;
    let fill_slot_public_id: String = conn.query_row(
        "SELECT public_id FROM k1_answer_slots WHERE answer_key_version_id=?1",
        [fill_answer.id],
        |row| row.get(0),
    )?;
    add_source_links(
        conn,
        fill_links.id,
        "answer_slot",
        &fill_slot_public_id,
        knowledge.id,
        ability.id,
        "recall",
    )?;
    promote_question_version(conn, fill_version.id, "L3", TEACHER, Some("主观题隔离验收"))?;

    let multi_fill_question = create_question(
        conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: TEACHER,
            question_family_id: None,
            rights_status: "unknown",
            sharing_allowed: false,
        },
    )?;
    let multi_fill_version = create_question_version(
        conn,
        &NewQuestionVersion {
            question_id: multi_fill_question.id,
            revision: 1,
            question_type: "fill_blank",
            stem: "《南京条约》签订于____年，开放____等五处通商口岸。",
            material_text: None,
            max_score: 2.0,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "L0",
            state: "draft",
            options: &[],
        },
    )?;
    let multi_fill_slots = [
        NewAnswerSlot {
            stable_id: Some("treaty_year_multi"),
            order_index: 0,
            canonical_answers_json: r#"{"schema_version":1,"answers":["1842年","1842"]}"#,
            normalization_rules_json: None,
            max_score: 1.0,
        },
        NewAnswerSlot {
            stable_id: Some("treaty_port"),
            order_index: 1,
            canonical_answers_json: r#"{"schema_version":1,"answers":["广州"]}"#,
            normalization_rules_json: None,
            max_score: 1.0,
        },
    ];
    let multi_fill_answer = create_answer_key_version(
        conn,
        &NewAnswerKeyVersion {
            question_version_id: multi_fill_version.id,
            revision: 1,
            answer_json: r#"{"schema_version":1,"values":["1842年 广州"],"slots":[{"stable_id":"treaty_year_multi","canonical_answers":["1842年","1842"]},{"stable_id":"treaty_port","canonical_answers":["广州"]}]}"#,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_answer_key_id: None,
            slots: &multi_fill_slots,
        },
    )?;
    let multi_fill_points = [
        NewRubricPoint {
            stable_id: Some("treaty_year_multi"),
            order_index: 0,
            canonical_text: "准确写出1842年",
            allowed_paraphrases_json: Some(r#"["1842"]"#),
            required_concepts_json: Some(r#"["1842"]"#),
            max_score: 1.0,
        },
        NewRubricPoint {
            stable_id: Some("treaty_port"),
            order_index: 1,
            canonical_text: "准确写出广州",
            allowed_paraphrases_json: None,
            required_concepts_json: Some(r#"["广州"]"#),
            max_score: 1.0,
        },
    ];
    let multi_fill_rubric = create_rubric_version(
        conn,
        &NewRubricVersion {
            question_version_id: multi_fill_version.id,
            revision: 1,
            max_score: 2.0,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_rubric_id: None,
            points: &multi_fill_points,
        },
    )?;
    let multi_fill_links = create_link_set(
        conn,
        multi_fill_version.id,
        map.id,
        1,
        "confirmed",
        Some(TEACHER),
        None,
    )?;
    let mut multi_slot_stmt = conn.prepare(
        "SELECT public_id FROM k1_answer_slots
         WHERE answer_key_version_id=?1 ORDER BY order_index,id",
    )?;
    let multi_slot_public_ids = multi_slot_stmt
        .query_map([multi_fill_answer.id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(multi_slot_stmt);
    for slot_public_id in &multi_slot_public_ids {
        add_source_links(
            conn,
            multi_fill_links.id,
            "answer_slot",
            slot_public_id,
            knowledge.id,
            ability.id,
            "recall",
        )?;
    }
    promote_question_version(
        conn,
        multi_fill_version.id,
        "L3",
        TEACHER,
        Some("主观题多槽隔离验收"),
    )?;

    let short_question = create_question(
        conn,
        &NewQuestion {
            owner_scope: "personal",
            owner_id: TEACHER,
            question_family_id: None,
            rights_status: "unknown",
            sharing_allowed: false,
        },
    )?;
    let short_version = create_question_version(
        conn,
        &NewQuestionVersion {
            question_id: short_question.id,
            revision: 1,
            question_type: "short_answer",
            stem: "概括洋务运动失败的主要原因。",
            material_text: None,
            max_score: 4.0,
            source_artifact_id: None,
            source_anchor_json: None,
            supersedes_version_id: None,
            quality_level: "L0",
            state: "draft",
            options: &[],
        },
    )?;
    let short_answer = create_answer_key_version(
        conn,
        &NewAnswerKeyVersion {
            question_version_id: short_version.id,
            revision: 1,
            answer_json: r#"{"schema_version":1,"reference_answer":"只学习西方技术，没有改变封建制度；内部管理腐败。"}"#,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_answer_key_id: None,
            slots: &[],
        },
    )?;
    let short_points = [
        NewRubricPoint {
            stable_id: Some("institution_limit"),
            order_index: 0,
            canonical_text: "只学习技术，没有改变封建制度",
            allowed_paraphrases_json: Some(r#"["没有触动封建制度"]"#),
            required_concepts_json: Some(r#"["技术","封建制度"]"#),
            max_score: 2.0,
        },
        NewRubricPoint {
            stable_id: Some("internal_corruption"),
            order_index: 1,
            canonical_text: "内部管理腐败",
            allowed_paraphrases_json: Some(r#"["洋务派内部腐败"]"#),
            required_concepts_json: Some(r#"["内部","腐败"]"#),
            max_score: 2.0,
        },
    ];
    let short_rubric = create_rubric_version(
        conn,
        &NewRubricVersion {
            question_version_id: short_version.id,
            revision: 1,
            max_score: 4.0,
            state: "confirmed",
            confirmed_by: Some(TEACHER),
            supersedes_rubric_id: None,
            points: &short_points,
        },
    )?;
    let institution_point_id: i64 = conn.query_row(
        "SELECT id FROM k1_rubric_points WHERE rubric_version_id=?1 AND stable_id='institution_limit'",
        [short_rubric.id],
        |row| row.get(0),
    )?;
    conn.execute(
        "INSERT INTO k1_contradiction_rules
         (public_id,rubric_point_id,rule_type,rule_json,created_at)
         VALUES (?1,?2,'fact_conflict',?3,?4)",
        (
            ids::new_public_id(),
            institution_point_id,
            r#"{"schema_version":1,"forbidden_claim":"彻底改变了封建制度"}"#,
            time::utc_now_rfc3339(),
        ),
    )?;
    let short_links = create_link_set(
        conn,
        short_version.id,
        map.id,
        1,
        "confirmed",
        Some(TEACHER),
        None,
    )?;
    let mut point_stmt = conn.prepare(
        "SELECT public_id FROM k1_rubric_points
         WHERE rubric_version_id=?1 ORDER BY order_index,id",
    )?;
    let point_public_ids = point_stmt
        .query_map([short_rubric.id], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(point_stmt);
    for point_public_id in &point_public_ids {
        add_source_links(
            conn,
            short_links.id,
            "rubric_point",
            point_public_id,
            knowledge.id,
            ability.id,
            "structured_response",
        )?;
    }
    promote_question_version(
        conn,
        short_version.id,
        "L3",
        TEACHER,
        Some("主观题隔离验收"),
    )?;

    let assessment = create_assessment_draft(
        conn,
        &NewAssessmentDraft {
            title: FIXTURE_TITLE,
            class_id,
            assessment_context: "quiz",
            evidence_policy: "include",
            created_by: TEACHER,
            template_version: Some("subjective-fixture-sheet-v1"),
        },
    )?;
    let fill_item = add_assessment_item(
        conn,
        assessment.assessment_version_id,
        &NewAssessmentItem {
            question_version_id: fill_version.id,
            answer_key_version_id: fill_answer.id,
            rubric_version_id: fill_rubric.id,
            link_set_id: fill_links.id,
            order_index: 0,
            score: 1.0,
            option_order_json: None,
            presentation_snapshot_json: r#"{"schema_version":1,"question_no":"1","page_no":1}"#,
        },
    )?;
    let short_item = add_assessment_item(
        conn,
        assessment.assessment_version_id,
        &NewAssessmentItem {
            question_version_id: short_version.id,
            answer_key_version_id: short_answer.id,
            rubric_version_id: short_rubric.id,
            link_set_id: short_links.id,
            order_index: 1,
            score: 4.0,
            option_order_json: None,
            presentation_snapshot_json: r#"{"schema_version":1,"question_no":"2","page_no":1}"#,
        },
    )?;
    let multi_fill_item = add_assessment_item(
        conn,
        assessment.assessment_version_id,
        &NewAssessmentItem {
            question_version_id: multi_fill_version.id,
            answer_key_version_id: multi_fill_answer.id,
            rubric_version_id: multi_fill_rubric.id,
            link_set_id: multi_fill_links.id,
            order_index: 2,
            score: 2.0,
            option_order_json: None,
            presentation_snapshot_json: r#"{"schema_version":1,"question_no":"3","page_no":1}"#,
        },
    )?;
    confirm_assessment_version(conn, assessment.assessment_version_id, TEACHER)?;
    Ok(ContentBundle {
        assessment_version_id: assessment.assessment_version_id,
        fill_item_id: fill_item.id,
        multi_fill_item_id: multi_fill_item.id,
        short_item_id: short_item.id,
    })
}

fn ocr_descriptor() -> DictationRecognizerDescriptor {
    DictationRecognizerDescriptor {
        provider: "fixture".into(),
        model_name: "fixture-handwriting".into(),
        model_version: "v1".into(),
        config_version: "subjective-fixture-ocr-v1".into(),
        rule_version: "raw-only-v1".into(),
    }
}

fn seed_ocr(
    conn: &mut Connection,
    region_id: i64,
    crop_id: i64,
    crop_hash: &str,
    crop_bytes: &[u8],
    text: &str,
    key: &str,
) -> AppResult<i64> {
    let request = DictationOcrRequest {
        answer_region_revision_id: region_id,
        crop_artifact_id: crop_id,
        crop_artifact_sha256: crop_hash,
        mime_type: "image/png",
        image_bytes: crop_bytes,
    };
    let input_hash = request.input_hash()?;
    let descriptor = ocr_descriptor();
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: key,
            run_type: "handwriting_ocr",
            source_module: "exam",
            business_ref_type: "answer_region_revision",
            business_ref_id: &region_id.to_string(),
            input_artifact_id: Some(crop_id),
            provider: &descriptor.provider,
            model_name: &descriptor.model_name,
            model_version: &descriptor.model_version,
            config_version: &descriptor.config_version,
            prompt_or_rule_version: &descriptor.rule_version,
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )?;
    ai_runs::start(conn, run.id, &time::utc_now_rfc3339(), None)?;
    let output = DictationOcrOutput {
        schema_version: 1,
        answer_region_revision_id: region_id,
        crop_artifact_id: crop_id,
        crop_artifact_sha256: crop_hash.to_owned(),
        input_hash,
        descriptor,
        state: DictationRecognitionState::Recognized,
        raw_text: Some(text.to_owned()),
        normalized_text: Some(text.to_owned()),
        confidence: Some(0.97),
        issue_codes: Vec::new(),
    };
    let output_json = output.to_json_against(&request)?;
    ai_runs::finalize_succeeded(
        conn,
        run.id,
        &hashing::sha256_hex(output_json.as_bytes()),
        output.confidence,
        &output_json,
        &time::utc_now_rfc3339(),
    )?;
    let transcription = record_ocr_ai_run_transcription(conn, run.id)?;
    Ok(transcription.id)
}

fn short_descriptor() -> ShortAnswerGraderDescriptor {
    ShortAnswerGraderDescriptor {
        provider: "fixture".into(),
        model_name: "fixture-short-answer".into(),
        model_version: "v1".into(),
        config_version: "subjective-fixture-grade-v1".into(),
        rule_version: "rubric-evidence-v1".into(),
    }
}

fn seed_short_grade(
    conn: &mut Connection,
    transcription_id: i64,
    mode: ShortMode,
    key: &str,
) -> AppResult<()> {
    let request = load_short_answer_grade_request(conn, transcription_id)?;
    let descriptor = short_descriptor();
    let point = |stable_id: &str| {
        request
            .rubric_points
            .iter()
            .find(|point| point.stable_id == stable_id)
            .ok_or_else(|| invalid(format!("缺少夹具评分点 {stable_id}")))
    };
    let institution = point("institution_limit")?;
    let corruption = point("internal_corruption")?;
    let (state, suggested_score, confidence, issue_codes, institution_result) = match mode {
        ShortMode::Full => (
            ShortAnswerGradeState::Ready,
            4.0,
            0.96,
            Vec::new(),
            ShortAnswerPointResult {
                rubric_point_id: institution.rubric_point_id,
                stable_id: institution.stable_id.clone(),
                status: ShortAnswerPointStatus::Covered,
                suggested_score: 2.0,
                evidence_snippets: vec!["没有改变封建制度".into()],
                reason: "明确写出未改变封建制度".into(),
                confidence: 0.97,
            },
        ),
        ShortMode::Contradicted => (
            ShortAnswerGradeState::NeedsReview,
            2.0,
            0.76,
            vec!["EXPLICIT_CONTRADICTION".into()],
            ShortAnswerPointResult {
                rubric_point_id: institution.rubric_point_id,
                stable_id: institution.stable_id.clone(),
                status: ShortAnswerPointStatus::Contradicted,
                suggested_score: 0.0,
                evidence_snippets: vec!["彻底改变了封建制度".into()],
                reason: "学生表述与评分点形成直接矛盾，必须老师判定".into(),
                confidence: 0.95,
            },
        ),
    };
    let output = ShortAnswerGradeOutput {
        schema_version: SHORT_ANSWER_GRADE_SCHEMA_VERSION,
        transcription_revision_id: transcription_id,
        assessment_item_id: request.assessment_item_id,
        answer_key_version_id: request.answer_key_version_id,
        rubric_version_id: request.rubric_version_id,
        input_hash: request.input_hash()?,
        descriptor: descriptor.clone(),
        state,
        suggested_score,
        point_results: vec![
            institution_result,
            ShortAnswerPointResult {
                rubric_point_id: corruption.rubric_point_id,
                stable_id: corruption.stable_id.clone(),
                status: ShortAnswerPointStatus::Covered,
                suggested_score: 2.0,
                evidence_snippets: vec!["内部管理腐败".into()],
                reason: "明确写出内部管理腐败".into(),
                confidence: 0.96,
            },
        ],
        confidence,
        issue_codes,
    };
    let input_hash = output.input_hash.clone();
    let run = ai_runs::create_or_get(
        conn,
        &ai_runs::NewAiRun {
            idempotency_key: key,
            run_type: "answer_grade",
            source_module: "exam",
            business_ref_type: "subjective_transcription_revision",
            business_ref_id: &transcription_id.to_string(),
            input_artifact_id: Some(request.crop_artifact_id),
            provider: &descriptor.provider,
            model_name: &descriptor.model_name,
            model_version: &descriptor.model_version,
            config_version: &descriptor.config_version,
            prompt_or_rule_version: &descriptor.rule_version,
            input_hash: &input_hash,
            retry_of_ai_run_id: None,
        },
    )?;
    ai_runs::start(conn, run.id, &time::utc_now_rfc3339(), None)?;
    let output_json = output.to_json_against(&request)?;
    ai_runs::finalize_succeeded(
        conn,
        run.id,
        &hashing::sha256_hex(output_json.as_bytes()),
        Some(confidence),
        &output_json,
        &time::utc_now_rfc3339(),
    )?;
    record_short_answer_grade_ai_run(conn, run.id)?;
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
    let archive_dir = data_dir.join("archive/subjective-acceptance-fixture");
    fs::create_dir_all(&archive_dir)?;
    let mut conn = suite_core::db::open(&db_path)?;
    run_all_migrations(&conn)?;
    conn.execute("INSERT INTO subjects(name) VALUES ('历史')", [])?;
    let subject_id = conn.last_insert_rowid();
    let class = classes::create(
        &conn,
        "主观题隔离验收班",
        Some("人教版八年级上册"),
        Some("2026秋"),
    )?;
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
    let content = seed_content(&conn, class.id, subject_id)?;
    let batch = create_or_get_ingest_batch(
        &conn,
        &NewIngestBatch {
            assessment_version_id: content.assessment_version_id,
            source_kind: "fixed_fixture",
            idempotency_key: "subjective-acceptance-fixture-batch-v1",
            created_by: TEACHER,
        },
    )?;
    conn.execute(
        "INSERT INTO exam_material_type_revisions_v2
         (public_id,ingest_batch_id,revision,material_type,confidence,evidence_json,
          decision,state,created_by_type,created_by,confirmed_by,created_at)
         VALUES (?1,?2,1,'answer_sheet',1.0,'{\"schema_version\":1,\"fixture\":true}',
                 'teacher_confirmed','active','teacher',?3,?3,?4)",
        (
            ids::new_public_id(),
            batch.id,
            TEACHER,
            time::utc_now_rfc3339(),
        ),
    )?;
    let (blank_artifact_id, template_hash, _) = artifact(
        &conn,
        &archive_dir,
        &FixtureArtifact {
            filename: "blank-template.png",
            title: "答题卡主观题空白模板",
            answer: "第1题填空；第2题简答；第3题多槽填空",
            kind: ArtifactKind::Image,
            parent_artifact_id: None,
            derivative_type: None,
            accent: "#465ac8",
        },
    )?;
    let template_json = format!(
        r#"{{"schema_version":2,"items":[{{"assessment_item_id":{},"region_index":0,"question_type":"fill_blank"}},{{"assessment_item_id":{},"region_index":0,"question_type":"short_answer"}},{{"assessment_item_id":{},"region_index":0,"question_type":"fill_blank"}}]}}"#,
        content.fill_item_id, content.short_item_id, content.multi_fill_item_id
    );
    conn.execute(
        "INSERT INTO exam_answer_sheet_template_revisions_v2
         (public_id,assessment_version_id,revision,template_version,page_no,
          blank_artifact_id,template_hash,template_json,confirmed_by,state,created_at)
         VALUES (?1,?2,1,'subjective-fixture-sheet-v1',1,?3,?4,?5,?6,'active',?7)",
        (
            ids::new_public_id(),
            content.assessment_version_id,
            blank_artifact_id,
            &template_hash,
            &template_json,
            TEACHER,
            time::utc_now_rfc3339(),
        ),
    )?;
    let template_revision_id = conn.last_insert_rowid();

    for (index, (scenario, student)) in SCENARIOS.iter().zip(students.iter()).enumerate() {
        let attempt = create_attempt(
            &conn,
            content.assessment_version_id,
            student.id,
            "image",
            "first",
        )?;
        let (page_artifact_id, _, _) = artifact(
            &conn,
            &archive_dir,
            &FixtureArtifact {
                filename: &format!("{}-page.png", scenario.student_no),
                title: &format!("{}号 {}", scenario.student_no, scenario.student_name),
                answer: "答题卡主观题隔离验收页",
                kind: ArtifactKind::Page,
                parent_artifact_id: None,
                derivative_type: None,
                accent: "#465ac8",
            },
        )?;
        let (aligned_artifact_id, _, _) = artifact(
            &conn,
            &archive_dir,
            &FixtureArtifact {
                filename: &format!("{}-aligned.png", scenario.student_no),
                title: &format!("{}号已配准答题卡", scenario.student_no),
                answer: "第1题填空；第2题简答；第3题多槽填空",
                kind: ArtifactKind::Page,
                parent_artifact_id: Some(page_artifact_id),
                derivative_type: Some("answer_sheet_aligned_input"),
                accent: "#465ac8",
            },
        )?;
        let (multi_fill_crop_id, multi_fill_hash, multi_fill_bytes) = artifact(
            &conn,
            &archive_dir,
            &FixtureArtifact {
                filename: &format!("{}-multi-fill.png", scenario.student_no),
                title: "第3题 · 多槽填空",
                answer: scenario.multi_fill_text,
                kind: ArtifactKind::Crop,
                parent_artifact_id: Some(aligned_artifact_id),
                derivative_type: Some("answer_sheet_subjective_region"),
                accent: if scenario.student_no == "S001" {
                    "#228b57"
                } else {
                    "#c07020"
                },
            },
        )?;
        let (fill_crop_id, fill_hash, fill_bytes) = artifact(
            &conn,
            &archive_dir,
            &FixtureArtifact {
                filename: &format!("{}-fill.png", scenario.student_no),
                title: "第1题 · 填空",
                answer: scenario.fill_text,
                kind: ArtifactKind::Crop,
                parent_artifact_id: Some(aligned_artifact_id),
                derivative_type: Some("answer_sheet_subjective_region"),
                accent: if scenario.fill_text == "1842年" {
                    "#228b57"
                } else {
                    "#c07020"
                },
            },
        )?;
        let (short_crop_id, short_hash, short_bytes) = artifact(
            &conn,
            &archive_dir,
            &FixtureArtifact {
                filename: &format!("{}-short.png", scenario.student_no),
                title: "第2题 · 简答",
                answer: scenario.short_text,
                kind: ArtifactKind::Crop,
                parent_artifact_id: Some(aligned_artifact_id),
                derivative_type: Some("answer_sheet_subjective_region"),
                accent: match scenario.short_mode {
                    ShortMode::Full => "#228b57",
                    ShortMode::Contradicted => "#b94b3d",
                },
            },
        )?;
        let page = register_ingest_page(
            &conn,
            &NewIngestPage {
                batch_id: batch.id,
                source_artifact_id: page_artifact_id,
                import_index: index as i64,
                expected_page_no: Some(1),
            },
        )?;
        record_page_quality(
            &conn,
            &NewPageQualityRevision {
                page_id: page.id,
                blur_score: 0.01,
                glare_score: 0.01,
                brightness_score: 0.96,
                perspective_score: 0.99,
                rotation_degrees: 0.0,
                crop_complete: true,
                result: "pass",
                issue_codes_json: r#"{"schema_version":1,"codes":[]}"#,
                checked_by_type: "fixture",
                checked_by: None,
            },
        )?;
        decide_page_match(
            &conn,
            &NewPageMatchRevision {
                page_id: page.id,
                attempt_id: Some(attempt.id),
                page_no: Some(1),
                student_confidence: Some(1.0),
                page_no_confidence: Some(1.0),
                template_confidence: Some(1.0),
                decision: "teacher_confirmed",
                reason_code: Some("SUBJECTIVE_FIXTURE"),
                confirmed_by: Some(TEACHER),
            },
        )?;
        let alignment = record_page_alignment(
            &conn,
            &NewPageAlignmentRevision {
                page_id: page.id,
                template_version: "subjective-fixture-sheet-v1",
                transform_json: r#"{"schema_version":1,"matrix":[1,0,0,0,1,0,0,0,1]}"#,
                confidence: 1.0,
                aligned_artifact_id: Some(aligned_artifact_id),
                decision: "teacher_confirmed",
                reason_code: Some("SUBJECTIVE_FIXTURE"),
                confirmed_by: Some(TEACHER),
            },
        )?;
        let fill_region = record_answer_region(
            &conn,
            &NewAnswerRegionRevision {
                page_id: page.id,
                assessment_item_id: content.fill_item_id,
                region_index: 0,
                bbox_json: r#"{"schema_version":1,"x":0.08,"y":0.12,"width":0.84,"height":0.18}"#,
                crop_artifact_id: Some(fill_crop_id),
                mapping_confidence: Some(1.0),
                decision: "teacher_confirmed",
                reason_code: Some("SUBJECTIVE_FIXTURE"),
                confirmed_by: Some(TEACHER),
            },
        )?;
        let short_region = record_answer_region(
            &conn,
            &NewAnswerRegionRevision {
                page_id: page.id,
                assessment_item_id: content.short_item_id,
                region_index: 0,
                bbox_json: r#"{"schema_version":1,"x":0.08,"y":0.42,"width":0.84,"height":0.42}"#,
                crop_artifact_id: Some(short_crop_id),
                mapping_confidence: Some(1.0),
                decision: "teacher_confirmed",
                reason_code: Some("SUBJECTIVE_FIXTURE"),
                confirmed_by: Some(TEACHER),
            },
        )?;
        let multi_fill_region = record_answer_region(
            &conn,
            &NewAnswerRegionRevision {
                page_id: page.id,
                assessment_item_id: content.multi_fill_item_id,
                region_index: 0,
                bbox_json: r#"{"schema_version":1,"x":0.08,"y":0.84,"width":0.84,"height":0.12}"#,
                crop_artifact_id: Some(multi_fill_crop_id),
                mapping_confidence: Some(1.0),
                decision: "teacher_confirmed",
                reason_code: Some("SUBJECTIVE_FIXTURE"),
                confirmed_by: Some(TEACHER),
            },
        )?;
        let region_ids_json = format!(
            r#"{{"schema_version":1,"region_revision_ids":[{},{},{}]}}"#,
            fill_region.id, short_region.id, multi_fill_region.id
        );
        conn.execute(
            "INSERT INTO exam_answer_sheet_page_materializations_v2
             (public_id,page_id,template_revision_id,alignment_revision_id,
              region_revision_ids_json,confirmed_by,created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            (
                ids::new_public_id(),
                page.id,
                template_revision_id,
                alignment.id,
                &region_ids_json,
                TEACHER,
                time::utc_now_rfc3339(),
            ),
        )?;
        let materialization_id = conn.last_insert_rowid();
        let created_at = time::utc_now_rfc3339();
        conn.execute(
            "INSERT INTO exam_answer_sheet_region_routes_v2
             (materialization_id,answer_region_revision_id,assessment_item_id,region_index,
              recognition_route,question_type,created_at)
             VALUES (?1,?2,?3,0,'handwriting_ocr','fill_blank',?4)",
            (
                materialization_id,
                fill_region.id,
                content.fill_item_id,
                &created_at,
            ),
        )?;
        conn.execute(
            "INSERT INTO exam_answer_sheet_region_routes_v2
             (materialization_id,answer_region_revision_id,assessment_item_id,region_index,
              recognition_route,question_type,created_at)
             VALUES (?1,?2,?3,0,'handwriting_ocr','fill_blank',?4)",
            (
                materialization_id,
                multi_fill_region.id,
                content.multi_fill_item_id,
                &created_at,
            ),
        )?;
        conn.execute(
            "INSERT INTO exam_answer_sheet_region_routes_v2
             (materialization_id,answer_region_revision_id,assessment_item_id,region_index,
              recognition_route,question_type,created_at)
             VALUES (?1,?2,?3,0,'handwriting_ocr','short_answer',?4)",
            (
                materialization_id,
                short_region.id,
                content.short_item_id,
                &created_at,
            ),
        )?;
        seed_ocr(
            &mut conn,
            fill_region.id,
            fill_crop_id,
            &fill_hash,
            &fill_bytes,
            scenario.fill_text,
            &format!("subjective-fixture:{}:fill:ocr", scenario.student_no),
        )?;
        let short_transcription_id = seed_ocr(
            &mut conn,
            short_region.id,
            short_crop_id,
            &short_hash,
            &short_bytes,
            scenario.short_text,
            &format!("subjective-fixture:{}:short:ocr", scenario.student_no),
        )?;
        seed_ocr(
            &mut conn,
            multi_fill_region.id,
            multi_fill_crop_id,
            &multi_fill_hash,
            &multi_fill_bytes,
            scenario.multi_fill_text,
            &format!("subjective-fixture:{}:multi-fill:ocr", scenario.student_no),
        )?;
        seed_short_grade(
            &mut conn,
            short_transcription_id,
            scenario.short_mode,
            &format!("subjective-fixture:{}:short:grade", scenario.student_no),
        )?;
    }
    fs::write(
        data_dir.join("subjective-acceptance-fixture.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": FIXTURE_SCHEMA,
            "bundle_id": FIXTURE_BUNDLE_ID,
            "assessment_title": FIXTURE_TITLE,
            "assessment_version_id": content.assessment_version_id,
            "students": SCENARIOS.len(),
            "items_per_student": 3
        }))?,
    )?;
    drop(conn);
    verify_fixture(data_dir, "seeded")
}

fn count(conn: &Connection, sql: &str) -> AppResult<i64> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn grouped_counts(conn: &Connection, sql: &str) -> AppResult<BTreeMap<String, i64>> {
    let mut statement = conn.prepare(sql)?;
    let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(rows.collect::<Result<BTreeMap<String, i64>, _>>()?)
}

fn marker_assessment_version(data_dir: &Path) -> AppResult<i64> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(
        data_dir.join("subjective-acceptance-fixture.json"),
    )?)?;
    value
        .get("assessment_version_id")
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| invalid("夹具标记缺少 assessment_version_id"))
}

fn inspect_fixture(data_dir: &Path, phase: &str) -> AppResult<FixtureReport> {
    validate_fixture_data_dir(data_dir)?;
    if !data_dir
        .join("subjective-acceptance-fixture.json")
        .is_file()
    {
        return Err(invalid(
            "缺少 subjective-acceptance-fixture.json，拒绝检查未知数据库",
        ));
    }
    let assessment_version_id = marker_assessment_version(data_dir)?;
    // FTS5 的 integrity_check 需要可写句柄；路径已由隔离夹具目录和 marker 守卫，
    // 并且不授予 SQLITE_OPEN_CREATE。
    let conn =
        Connection::open_with_flags(data_dir.join("data.db"), OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let workbench = list_subjective_workbench(&conn, Some(assessment_version_id), 100)?;
    let integrity_check: String = conn.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    let foreign_key_violations = count(&conn, "SELECT COUNT(*) FROM pragma_foreign_key_check")?;
    let missing_artifact_files = {
        let mut statement = conn.prepare(
            "SELECT archived_path FROM artifacts
             WHERE processing_version='subjective-acceptance-fixture-v1'",
        )?;
        let paths = statement.query_map([], |row| row.get::<_, String>(0))?;
        let mut missing = 0;
        for path in paths {
            if !Path::new(&path?).is_file() {
                missing += 1;
            }
        }
        missing
    };
    Ok(FixtureReport {
        schema_version: FIXTURE_SCHEMA,
        phase: phase.to_owned(),
        data_dir: data_dir.display().to_string(),
        assessment_version_id,
        migration_count: count(&conn, "SELECT COUNT(*) FROM schema_migrations")?,
        integrity_check,
        foreign_key_violations,
        workbench_rows: workbench.rows.len(),
        attempts: workbench.attempts.len(),
        short_answer_analyses: count(
            &conn,
            "SELECT COUNT(*) FROM exam_short_answer_grade_analyses_v2 WHERE state='active'",
        )?,
        subjective_components: count(
            &conn,
            "SELECT COUNT(*) FROM exam_grade_decision_subjective_components_v2",
        )?,
        confirmed_rows: workbench
            .rows
            .iter()
            .filter(|row| row.current_suggestion_confirmed)
            .count(),
        confirmation_levels: grouped_counts(
            &conn,
            "SELECT confirmation_level,COUNT(*) FROM exam_grade_decisions_v2
             WHERE state='active' GROUP BY confirmation_level ORDER BY confirmation_level",
        )?,
        suggestion_outcomes: grouped_counts(
            &conn,
            "SELECT COALESCE(a.outcome,s.outcome),COUNT(*)
             FROM exam_subjective_grade_suggestions_v2 s
             LEFT JOIN exam_short_answer_grade_analyses_v2 a
               ON a.suggestion_id=s.id AND a.state='active'
             WHERE s.state='active'
             GROUP BY COALESCE(a.outcome,s.outcome)
             ORDER BY COALESCE(a.outcome,s.outcome)",
        )?,
        attempt_states: grouped_counts(
            &conn,
            "SELECT state,COUNT(*) FROM exam_attempts_v2 GROUP BY state ORDER BY state",
        )?,
        published_publications: count(
            &conn,
            "SELECT COUNT(*) FROM exam_grade_publications_v2 WHERE state='published'",
        )?,
        active_learning_evidence: count(
            &conn,
            "SELECT COUNT(*) FROM learning_evidence
             WHERE state='active' AND source_module='grading'",
        )?,
        missing_artifact_files,
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
        + module_exam::exam_migrations().len()) as i64;
    expect(
        report.migration_count == expected_migrations,
        format!("夹具必须包含全部 {expected_migrations} 个迁移"),
    )?;
    expect(
        report.integrity_check == "ok",
        format!(
            "integrity_check 必须为 ok，实际为：{}",
            report.integrity_check
        ),
    )?;
    expect(report.foreign_key_violations == 0, "夹具不能包含外键违规")?;
    expect(report.workbench_rows == 6, "工作台必须显示 6 条主观题证据")?;
    expect(report.attempts == 2, "工作台必须显示 2 份答题卡")?;
    expect(
        report.short_answer_analyses == 2,
        "必须有 2 条当前简答题分项分析",
    )?;
    expect(
        report.suggestion_outcomes
            == BTreeMap::from([
                ("correct".to_owned(), 3),
                ("incorrect".to_owned(), 2),
                ("partial".to_owned(), 1),
            ]),
        "建议结果分布不符合固定夹具",
    )?;
    expect(report.missing_artifact_files == 0, "夹具证据文件必须可读")?;
    match phase {
        "seeded" => {
            expect(report.confirmed_rows == 0, "seeded 阶段不能有老师终审")?;
            expect(
                report.subjective_components == 0,
                "seeded 阶段不能有逐项人工终审账本",
            )?;
            expect(
                report.confirmation_levels.is_empty(),
                "seeded 阶段不能有 grade decision",
            )?;
            expect(
                report.attempt_states == BTreeMap::from([("ingesting".to_owned(), 2)]),
                "seeded 阶段 attempt 应全部为 ingesting",
            )?;
            expect(
                report.published_publications == 0 && report.active_learning_evidence == 0,
                "seeded 阶段不能发布或生成正式证据",
            )?;
        }
        "reviewed" => {
            expect(report.confirmed_rows == 6, "reviewed 阶段必须完成 6 条终审")?;
            expect(
                report.confirmation_levels
                    == BTreeMap::from([
                        ("teacher_accepted".to_owned(), 2),
                        ("teacher_corrected".to_owned(), 4),
                    ]),
                "reviewed 阶段必须是 2 条接受建议 + 4 条人工修正",
            )?;
            expect(
                report.subjective_components == 7,
                "reviewed 阶段必须保存 7 条逐槽/逐评分点结论",
            )?;
            expect(
                report.attempt_states == BTreeMap::from([("ready_to_publish".to_owned(), 2)]),
                "reviewed 阶段 attempt 应全部待发布",
            )?;
            expect(
                report.published_publications == 0 && report.active_learning_evidence == 0,
                "终审完成仍不能自动发布或生成正式证据",
            )?;
        }
        "published" => {
            expect(
                report.confirmed_rows == 6,
                "published 阶段必须保留 6 条终审",
            )?;
            expect(
                report.confirmation_levels
                    == BTreeMap::from([
                        ("teacher_accepted".to_owned(), 2),
                        ("teacher_corrected".to_owned(), 4),
                    ]),
                "published 阶段评分来源必须保持不变",
            )?;
            expect(
                report.subjective_components == 7,
                "published 阶段必须保留 7 条逐项人工结论",
            )?;
            expect(
                report.attempt_states == BTreeMap::from([("published".to_owned(), 2)]),
                "published 阶段 attempt 应全部已发布",
            )?;
            expect(
                report.published_publications == 2,
                "必须显式发布 2 份答题卡",
            )?;
            expect(
                report.active_learning_evidence == 20,
                "发布后必须按单槽、评分点和多槽组成生成 20 条正式证据",
            )?;
        }
        _ => return Err(invalid("--phase 只能是 seeded、reviewed 或 published")),
    }
    Ok(report)
}

#[cfg(test)]
#[derive(Clone, Copy)]
struct FixtureComponentDecision<'a> {
    stable_id: &'a str,
    teacher_score: f64,
    evidence_text: Option<&'a str>,
    teacher_note: &'a str,
}

#[cfg(test)]
fn fixture_component_inputs(
    row: &SubjectiveWorkbenchRow,
    decisions: &[FixtureComponentDecision<'_>],
) -> AppResult<Vec<SubjectiveComponentGradeInput>> {
    let (raw, field, source_type) = if row.question_type == "fill_blank" {
        (&row.answer_slots_json, "answer_slots", "answer_slot")
    } else if row.question_type == "short_answer" {
        (&row.rubric_points_json, "rubric_points", "rubric_point")
    } else {
        return Err(invalid("夹具只支持填空和简答逐项终审"));
    };
    let value: serde_json::Value = serde_json::from_str(raw)?;
    let components = value
        .get(field)
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| invalid(format!("夹具工作台缺少 {field}")))?;
    if components.len() != decisions.len() {
        return Err(invalid("夹具逐项决策数量与当前答案版本不一致"));
    }
    components
        .iter()
        .map(|component| {
            let stable_id = component
                .get("stable_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| invalid("夹具逐项定义缺少 stable_id"))?;
            let source_public_id = component
                .get("source_public_id")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(|| invalid("夹具逐项定义缺少 source_public_id"))?;
            let decision = decisions
                .iter()
                .find(|decision| decision.stable_id == stable_id)
                .ok_or_else(|| invalid(format!("夹具缺少 {stable_id} 的人工结论")))?;
            Ok(SubjectiveComponentGradeInput {
                source_type: source_type.to_owned(),
                source_public_id: source_public_id.to_owned(),
                teacher_score: decision.teacher_score,
                evidence_text: decision.evidence_text.map(str::to_owned),
                teacher_note: Some(decision.teacher_note.to_owned()),
            })
        })
        .collect()
}

#[cfg(test)]
fn review_and_publish_with_services(data_dir: &Path) -> AppResult<()> {
    let assessment_version_id = marker_assessment_version(data_dir)?;
    let conn = suite_core::db::open(&data_dir.join("data.db"))?;
    let workbench = list_subjective_workbench(&conn, Some(assessment_version_id), 100)?;
    for row in &workbench.rows {
        match (row.student_no.as_str(), row.question_no.as_str()) {
            ("S001", "1") | ("S001", "2") => {
                accept_subjective_suggestion(&conn, row.suggestion_id, TEACHER)?;
            }
            ("S001", "3") => {
                let components = fixture_component_inputs(
                    row,
                    &[
                        FixtureComponentDecision {
                            stable_id: "treaty_year_multi",
                            teacher_score: 1.0,
                            evidence_text: Some("1842年"),
                            teacher_note: "年份正确",
                        },
                        FixtureComponentDecision {
                            stable_id: "treaty_port",
                            teacher_score: 1.0,
                            evidence_text: Some("广州"),
                            teacher_note: "通商口岸正确",
                        },
                    ],
                )?;
                correct_subjective_components(
                    &conn,
                    row.suggestion_id,
                    &components,
                    "两个填空槽位均按原图逐项确认",
                    TEACHER,
                )?;
            }
            ("S002", "1") => {
                let components = fixture_component_inputs(
                    row,
                    &[FixtureComponentDecision {
                        stable_id: "treaty_year",
                        teacher_score: 0.0,
                        evidence_text: Some("1840年"),
                        teacher_note: "原图年份与确认答案不一致",
                    }],
                )?;
                correct_subjective_components(
                    &conn,
                    row.suggestion_id,
                    &components,
                    "按原图确认年份错误",
                    TEACHER,
                )?;
            }
            ("S002", "2") => {
                let components = fixture_component_inputs(
                    row,
                    &[
                        FixtureComponentDecision {
                            stable_id: "institution_limit",
                            teacher_score: 0.0,
                            evidence_text: Some("彻底改变了封建制度"),
                            teacher_note: "与评分点形成明确矛盾",
                        },
                        FixtureComponentDecision {
                            stable_id: "internal_corruption",
                            teacher_score: 2.0,
                            evidence_text: Some("内部管理腐败"),
                            teacher_note: "该评分点完整覆盖",
                        },
                    ],
                )?;
                correct_subjective_components(
                    &conn,
                    row.suggestion_id,
                    &components,
                    "制度局限不得分，内部管理腐败得满分",
                    TEACHER,
                )?;
            }
            ("S002", "3") => {
                let components = fixture_component_inputs(
                    row,
                    &[
                        FixtureComponentDecision {
                            stable_id: "treaty_year_multi",
                            teacher_score: 0.0,
                            evidence_text: Some("1840年"),
                            teacher_note: "年份错误",
                        },
                        FixtureComponentDecision {
                            stable_id: "treaty_port",
                            teacher_score: 1.0,
                            evidence_text: Some("广州"),
                            teacher_note: "通商口岸正确",
                        },
                    ],
                )?;
                correct_subjective_components(
                    &conn,
                    row.suggestion_id,
                    &components,
                    "多槽填空按原图分别判定",
                    TEACHER,
                )?;
            }
            _ => return Err(invalid("出现未知主观题验收行")),
        }
    }
    drop(conn);
    verify_fixture(data_dir, "reviewed")?;
    let conn = suite_core::db::open(&data_dir.join("data.db"))?;
    let workbench = list_subjective_workbench(&conn, Some(assessment_version_id), 100)?;
    for attempt in &workbench.attempts {
        publish_attempt(&conn, attempt.attempt_id, TEACHER)?;
    }
    drop(conn);
    verify_fixture(data_dir, "published")?;
    Ok(())
}

fn parse_args() -> AppResult<(String, PathBuf, String)> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or_else(|| {
        invalid(
            "用法：t6_subjective_fixture <seed|verify> --data-dir <绝对路径> [--phase seeded|reviewed|published]",
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
            "jiaofu-subjective-fixture-{label}-{}-{}",
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
    fn seeded_fixture_has_machine_analysis_without_teacher_effects() {
        let (root, data_dir) = test_data_dir("seeded");
        let report = seed_fixture(&data_dir).unwrap();
        assert_eq!(report.workbench_rows, 6);
        assert_eq!(report.short_answer_analyses, 2);
        assert_eq!(report.subjective_components, 0);
        assert_eq!(report.confirmed_rows, 0);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn formal_services_reach_reviewed_and_published_phases() {
        let (root, data_dir) = test_data_dir("lifecycle");
        seed_fixture(&data_dir).unwrap();
        review_and_publish_with_services(&data_dir).unwrap();
        let report = verify_fixture(&data_dir, "published").unwrap();
        assert_eq!(report.published_publications, 2);
        assert_eq!(report.subjective_components, 7);
        assert_eq!(report.active_learning_evidence, 20);
        fs::remove_dir_all(root).unwrap();
    }
}
