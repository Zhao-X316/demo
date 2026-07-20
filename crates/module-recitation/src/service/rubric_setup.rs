//! 老师端背诵评分点首用设置。
//!
//! 预览只在内存中按句拆分标准答案，不写数据库，也不伪装成 AI 结果。
//! 老师确认时才在一个事务中创建不可变 rubric、评分点并启用该版本。

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use suite_core::error::{CoreError, CoreResult};

use crate::db::contents;
use crate::db::structured::{
    self, CreateRubricDraftInput, RecRubricVersion, RubricPointDraftInput,
};

const EMPTY_ITEMS: &str = r#"{"schema_version":1,"items":[]}"#;
const MAX_POINT_COUNT: usize = 50;
const MAX_POINT_TEXT_CHARS: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RubricSetupPoint {
    pub canonical_text: String,
    pub required: bool,
    pub order_index: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RubricSetupView {
    pub content_id: i64,
    pub content_no: String,
    pub title: String,
    pub answer_text: String,
    pub answer_version: i64,
    pub source_kind: String,
    pub rubric_version_id: Option<i64>,
    pub rubric_revision: Option<i64>,
    pub rubric_status: Option<String>,
    pub points: Vec<RubricSetupPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RubricSetupPointInput {
    pub canonical_text: String,
    pub required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ConfirmRubricSetupInput {
    pub content_id: i64,
    pub expected_answer_version: i64,
    pub points: Vec<RubricSetupPointInput>,
}

/// 用换行和明确句末标点拆分答案。逗号不拆，避免把一个完整史实切碎。
pub fn split_answer_into_points(answer_text: &str) -> Vec<String> {
    let mut points = Vec::new();
    let mut current = String::new();
    for character in answer_text.chars() {
        if matches!(character, '\n' | '\r' | '。' | '！' | '？' | '；' | ';') {
            let text = current.trim();
            if !text.is_empty() && !points.iter().any(|point| point == text) {
                points.push(text.to_string());
            }
            current.clear();
        } else {
            current.push(character);
        }
    }
    let text = current.trim();
    if !text.is_empty() && !points.iter().any(|point| point == text) {
        points.push(text.to_string());
    }
    points
}

fn build_view(
    conn: &Connection,
    content_id: i64,
    rubric: Option<&RecRubricVersion>,
) -> CoreResult<RubricSetupView> {
    let content = contents::get_by_id(conn, content_id)?
        .ok_or_else(|| CoreError::NotFound(format!("recitation content {content_id}")))?;
    if content.answer_text.trim().is_empty() {
        return Err(CoreError::Invalid(
            "标准答案为空，请先在内容设置中补充答案".into(),
        ));
    }

    let points: Vec<RubricSetupPoint> = if let Some(rubric) = rubric {
        structured::list_rubric_points(conn, rubric.id)?
            .into_iter()
            .map(|point| RubricSetupPoint {
                canonical_text: point.canonical_text,
                required: point.required,
                order_index: point.order_index,
            })
            .collect()
    } else {
        split_answer_into_points(&content.answer_text)
            .into_iter()
            .enumerate()
            .map(|(index, canonical_text)| RubricSetupPoint {
                canonical_text,
                required: true,
                order_index: index as i64,
            })
            .collect()
    };
    if points.is_empty() {
        return Err(CoreError::Invalid(
            "未能从标准答案生成评分点，请先补充答案".into(),
        ));
    }

    Ok(RubricSetupView {
        content_id: content.id,
        content_no: content.content_no,
        title: content.title,
        answer_text: content.answer_text,
        answer_version: content.answer_version,
        source_kind: if rubric.is_some() {
            "confirmed".into()
        } else {
            "generated_by_sentence".into()
        },
        rubric_version_id: rubric.map(|value| value.id),
        rubric_revision: rubric.map(|value| value.revision),
        rubric_status: rubric.map(|value| value.status.clone()),
        points,
    })
}

/// 读取当前已启用评分点；没有时按句生成本地草稿。此函数不写数据库。
pub fn preview(conn: &Connection, content_id: i64) -> CoreResult<RubricSetupView> {
    let rubric = structured::current_confirmed_rubric_for_content(conn, content_id)?;
    build_view(conn, content_id, rubric.as_ref())
}

fn validate_points(points: &[RubricSetupPointInput]) -> CoreResult<()> {
    if points.is_empty() {
        return Err(CoreError::Invalid("至少保留一个评分点".into()));
    }
    if points.len() > MAX_POINT_COUNT {
        return Err(CoreError::Invalid(format!(
            "单篇内容最多设置 {MAX_POINT_COUNT} 个评分点"
        )));
    }
    for (index, point) in points.iter().enumerate() {
        let text = point.canonical_text.trim();
        if text.is_empty() {
            return Err(CoreError::Invalid(format!(
                "第 {} 个评分点不能为空",
                index + 1
            )));
        }
        if text.chars().count() > MAX_POINT_TEXT_CHARS {
            return Err(CoreError::Invalid(format!(
                "第 {} 个评分点超过 {MAX_POINT_TEXT_CHARS} 字",
                index + 1
            )));
        }
    }
    Ok(())
}

/// 将老师调整后的评分点一次性保存并启用。
///
/// `expected_answer_version` 防止老师打开弹窗后标准答案被另一操作改版，
/// 从而把评分点错绑到新答案。任何失败都会回滚本次新建的 rubric 与审计事件。
pub fn confirm(
    conn: &Connection,
    input: &ConfirmRubricSetupInput,
    confirmed_by: &str,
) -> CoreResult<RubricSetupView> {
    if confirmed_by.trim().is_empty() {
        return Err(CoreError::Invalid("确认人不能为空".into()));
    }
    validate_points(&input.points)?;

    let transaction = conn.unchecked_transaction()?;
    let content = contents::get_by_id(&transaction, input.content_id)?
        .ok_or_else(|| CoreError::NotFound(format!("recitation content {}", input.content_id)))?;
    if content.answer_version != input.expected_answer_version {
        return Err(CoreError::Invalid(
            "标准答案已发生变化，请刷新评分点后再确认".into(),
        ));
    }
    let answer = structured::current_answer_version(&transaction, input.content_id)?
        .ok_or_else(|| CoreError::Invalid("当前答案版本尚未建立，无法设置评分点".into()))?;
    if answer.answer_version != input.expected_answer_version {
        return Err(CoreError::Invalid(
            "当前答案版本不一致，请刷新评分点后再确认".into(),
        ));
    }

    let stable_keys = (0..input.points.len())
        .map(|index| format!("point-{:03}", index + 1))
        .collect::<Vec<_>>();
    let draft_points = input
        .points
        .iter()
        .enumerate()
        .map(|(index, point)| RubricPointDraftInput {
            stable_key: stable_keys[index].as_str(),
            canonical_text: point.canonical_text.trim(),
            required_entities_json: EMPTY_ITEMS,
            allowed_paraphrases_json: EMPTY_ITEMS,
            contradiction_rules_json: EMPTY_ITEMS,
            required: point.required,
            weight: 1.0,
            order_index: index as i64,
            knowledge_node_id: None,
            knowledge_link_state: "none",
            verified_by: None,
            verified_at: None,
        })
        .collect::<Vec<_>>();
    let draft = structured::create_rubric_draft_inner(
        &transaction,
        &CreateRubricDraftInput {
            answer_version_id: answer.id,
            generated_by_ai_run_id: None,
            created_by: confirmed_by,
            points: &draft_points,
        },
    )?;
    let rubric = structured::confirm_rubric_inner(&transaction, draft.id, confirmed_by)?;
    let view = build_view(&transaction, input.content_id, Some(&rubric))?;
    transaction.commit()?;
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::*;
    use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    use crate::db::contents::{self, ContentInput};

    fn setup() -> Connection {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        run_migrations(&conn, module_knowledge::knowledge_migrations()).unwrap();
        run_migrations(&conn, crate::recitation_migrations()).unwrap();
        conn
    }

    fn add_content(conn: &Connection, answer_text: &str) -> i64 {
        contents::upsert(
            conn,
            &ContentInput {
                content_no: "H8-01",
                title: "鸦片战争",
                answer_text,
                subject_id: None,
                enabled: true,
            },
        )
        .unwrap()
        .id
    }

    fn count(conn: &Connection, table: &str) -> i64 {
        conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
        .unwrap()
    }

    fn point(text: &str) -> RubricSetupPointInput {
        RubricSetupPointInput {
            canonical_text: text.into(),
            required: true,
        }
    }

    #[test]
    fn preview_splits_answer_without_writing_database() {
        let conn = setup();
        let content_id = add_content(
            &conn,
            "1840年鸦片战争爆发。清政府战败；1842年签订《南京条约》。",
        );
        let before_rubrics = count(&conn, "rec_rubric_versions");
        let before_points = count(&conn, "rec_rubric_points");

        let view = preview(&conn, content_id).unwrap();

        assert_eq!(view.source_kind, "generated_by_sentence");
        assert_eq!(view.answer_version, 1);
        assert_eq!(
            view.points
                .iter()
                .map(|value| value.canonical_text.as_str())
                .collect::<Vec<_>>(),
            vec!["1840年鸦片战争爆发", "清政府战败", "1842年签订《南京条约》"]
        );
        assert_eq!(count(&conn, "rec_rubric_versions"), before_rubrics);
        assert_eq!(count(&conn, "rec_rubric_points"), before_points);
    }

    #[test]
    fn confirm_creates_and_enables_first_rubric_atomically() {
        let conn = setup();
        let content_id = add_content(&conn, "前期口号是自强。后期口号是求富。");

        let view = confirm(
            &conn,
            &ConfirmRubricSetupInput {
                content_id,
                expected_answer_version: 1,
                points: vec![point("前期口号是自强"), point("后期口号是求富")],
            },
            "teacher-1",
        )
        .unwrap();

        assert_eq!(view.source_kind, "confirmed");
        assert_eq!(view.rubric_status.as_deref(), Some("confirmed"));
        assert_eq!(view.rubric_revision, Some(1));
        assert_eq!(count(&conn, "rec_rubric_versions"), 1);
        assert_eq!(count(&conn, "rec_rubric_points"), 2);
    }

    #[test]
    fn confirming_same_definition_is_idempotent() {
        let conn = setup();
        let content_id = add_content(&conn, "前期自强。后期求富。");
        let input = ConfirmRubricSetupInput {
            content_id,
            expected_answer_version: 1,
            points: vec![point("前期自强"), point("后期求富")],
        };

        let first = confirm(&conn, &input, "teacher-1").unwrap();
        let second = confirm(&conn, &input, "teacher-1").unwrap();

        assert_eq!(first.rubric_version_id, second.rubric_version_id);
        assert_eq!(count(&conn, "rec_rubric_versions"), 1);
        assert_eq!(count(&conn, "rec_rubric_points"), 2);
    }

    #[test]
    fn edited_definition_retires_old_version() {
        let conn = setup();
        let content_id = add_content(&conn, "前期自强。后期求富。");
        let first = confirm(
            &conn,
            &ConfirmRubricSetupInput {
                content_id,
                expected_answer_version: 1,
                points: vec![point("前期自强"), point("后期求富")],
            },
            "teacher-1",
        )
        .unwrap();
        let second = confirm(
            &conn,
            &ConfirmRubricSetupInput {
                content_id,
                expected_answer_version: 1,
                points: vec![
                    point("前期口号是自强"),
                    point("后期口号是求富"),
                    point("先军用后民用"),
                ],
            },
            "teacher-1",
        )
        .unwrap();

        assert_ne!(first.rubric_version_id, second.rubric_version_id);
        assert_eq!(second.rubric_revision, Some(2));
        let statuses = {
            let mut statement = conn
                .prepare("SELECT status FROM rec_rubric_versions ORDER BY revision")
                .unwrap();
            statement
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<Result<Vec<_>, _>>()
                .unwrap()
        };
        assert_eq!(statuses, vec!["retired", "confirmed"]);
    }

    #[test]
    fn answer_version_drift_rejects_without_writes() {
        let conn = setup();
        let content_id = add_content(&conn, "旧答案。");
        let stale = preview(&conn, content_id).unwrap();
        add_content(&conn, "新答案。");

        let error = confirm(
            &conn,
            &ConfirmRubricSetupInput {
                content_id,
                expected_answer_version: stale.answer_version,
                points: vec![point("旧答案")],
            },
            "teacher-1",
        )
        .unwrap_err();

        assert!(error.to_string().contains("标准答案已发生变化"));
        assert_eq!(count(&conn, "rec_rubric_versions"), 0);
        assert_eq!(count(&conn, "rec_rubric_points"), 0);
    }

    #[test]
    fn confirmation_failure_rolls_back_new_draft_and_events() {
        let conn = setup();
        let content_id = add_content(&conn, "前期自强。");
        conn.execute_batch(
            "CREATE TRIGGER test_reject_rubric_confirmation
             BEFORE UPDATE OF status ON rec_rubric_versions
             WHEN NEW.status='confirmed'
             BEGIN
               SELECT RAISE(ABORT, 'injected confirmation failure');
             END;",
        )
        .unwrap();

        let error = confirm(
            &conn,
            &ConfirmRubricSetupInput {
                content_id,
                expected_answer_version: 1,
                points: vec![point("前期自强")],
            },
            "teacher-1",
        )
        .unwrap_err();

        assert!(error.to_string().contains("injected confirmation failure"));
        assert_eq!(count(&conn, "rec_rubric_versions"), 0);
        assert_eq!(count(&conn, "rec_rubric_points"), 0);
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM audit_events
                 WHERE action IN (
                   'recitation.rubric_draft.created',
                   'recitation.rubric.confirmed'
                 )",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
            0
        );
    }
}
