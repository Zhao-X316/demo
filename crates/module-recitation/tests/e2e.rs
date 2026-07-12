//! M1 端到端集成测试（对应 背诵批改系统 §13.2 集成测试）。
//!
//! 场景：建学生/内容 → 生成任务 → 批量导入(含正常/重复/坏命名/未知学生) →
//! ASR(直接写识别文本) → 评分 → 通过排复习 / 未通过排补背 → 到期生成复习任务。

use chrono::NaiveDate;
use module_recitation::db::contents;
use module_recitation::service::{import, scoring, tasks as task_svc};
use suite_core::db::repo::students::{upsert as upsert_student, StudentInput};
use suite_core::db::repo::{memory_cards, submissions, tasks as task_repo};
use suite_core::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};
use suite_core::models::{ModuleKey, TaskKind, TaskStatus};

const M: ModuleKey = ModuleKey::Recitation;

fn imp<'a>(stem: &'a str, hash: &'a str) -> import::ImportItem<'a> {
    import::ImportItem {
        file_path: "/tmp/jiaofu-suite-e2e.m4a",
        file_stem: stem,
        file_hash: hash,
        duration_ms: Some(5000),
    }
}

#[test]
fn full_recitation_flow() {
    std::fs::write("/tmp/jiaofu-suite-e2e.m4a", b"test audio evidence").unwrap();
    let conn = open_in_memory().unwrap();
    run_migrations(&conn, CORE_MIGRATIONS).unwrap();
    run_migrations(&conn, module_recitation::recitation_migrations()).unwrap();

    // 学生 + 内容
    let s1 = upsert_student(
        &conn,
        &StudentInput {
            student_no: "2023001",
            name: "张三",
            class_id: None,
            enabled: true,
        },
    )
    .unwrap();
    let s2 = upsert_student(
        &conn,
        &StudentInput {
            student_no: "2023002",
            name: "李四",
            class_id: None,
            enabled: true,
        },
    )
    .unwrap();
    let answer = "床前明月光疑是地上霜";
    let c1 = contents::upsert(
        &conn,
        &contents::ContentInput {
            content_no: "C012",
            title: "静夜思",
            answer_text: answer,
            subject_id: None,
            enabled: true,
        },
    )
    .unwrap();

    // 生成今日任务
    let due = "2026-06-25";
    let made = task_svc::generate_normal(&conn, due, &[(s1.id, c1.id), (s2.id, c1.id)]).unwrap();
    assert_eq!(made.created, 2);

    // 批量导入
    let o1 = import::import_one(&conn, &imp("20260625_2023001_张三_C012", "hA")).unwrap();
    let o2 = import::import_one(&conn, &imp("20260625_2023002_李四_C012", "hB")).unwrap();
    let odup = import::import_one(&conn, &imp("20260625_2023001_张三_C012", "hA")).unwrap();
    let obad = import::import_one(&conn, &imp("garbage-name", "hC")).unwrap();
    let ounknown = import::import_one(&conn, &imp("20260625_9999999_王五_C012", "hD")).unwrap();

    let sub1 = match o1 {
        import::ImportOutcome::Imported { submission_id, .. } => submission_id,
        o => panic!("{o:?}"),
    };
    let sub2 = match o2 {
        import::ImportOutcome::Imported { submission_id, .. } => submission_id,
        o => panic!("{o:?}"),
    };
    assert!(matches!(odup, import::ImportOutcome::Duplicate { .. }));
    assert!(
        matches!(obad, import::ImportOutcome::Anomaly { ref anomaly_type, .. } if anomaly_type == "parse_error")
    );
    assert!(
        matches!(ounknown, import::ImportOutcome::Anomaly { ref anomaly_type, .. } if anomaly_type == "student_not_found")
    );
    assert_eq!(submissions::list_anomalies(&conn, M).unwrap().len(), 2);

    let today = NaiveDate::from_ymd_opt(2026, 6, 25).unwrap();
    let cfg = scoring::ScoreCfg::default();

    // S1 完美背诵 → 通过
    submissions::set_recognition(&conn, sub1, Some(answer), "ok", None, Some(5000)).unwrap();
    let r1 = scoring::score_submission(&conn, sub1, &[], today, &cfg).unwrap();
    assert!(r1.pass);
    assert_eq!(r1.accuracy, 100.0);
    assert_eq!(r1.next, scoring::NextAction::AwaitingHumanReview);

    // S2 只背一半 → 未通过 → 补背
    submissions::set_recognition(&conn, sub2, Some("床前明月光"), "ok", None, Some(3000)).unwrap();
    let r2 = scoring::score_submission(&conn, sub2, &[], today, &cfg).unwrap();
    assert!(!r2.pass);
    assert_eq!(r2.next, scoring::NextAction::AwaitingHumanReview);

    // 机器评分只给建议；老师确认后才产生任务/复习副作用。
    scoring::human_decide(
        &conn,
        sub1,
        "pass",
        Some("老师确认"),
        Some("teacher"),
        today,
        &cfg,
    )
    .unwrap();
    scoring::human_decide(
        &conn,
        sub2,
        "fail",
        Some("老师确认"),
        Some("teacher"),
        today,
        &cfg,
    )
    .unwrap();

    // 任务状态
    let t_s1 = task_repo::find_open_match(&conn, M, s1.id, c1.id, due).unwrap();
    assert!(t_s1.is_none()); // 已 passed，不再是 open/submitted
    assert!(task_repo::exists_open_kind(&conn, M, s2.id, c1.id, TaskKind::Makeup).unwrap());

    // S1 记忆卡片已排程
    let card = memory_cards::get(&conn, M, s1.id, "content", c1.id)
        .unwrap()
        .unwrap();
    assert!(card.due_date.is_some());

    // 远期日切：S1 卡片到期 → 生成复习任务
    let future = card.due_date.clone().unwrap();
    let reviews = task_svc::generate_due_reviews(&conn, &future).unwrap();
    assert_eq!(reviews.len(), 1);
    let rt = task_repo::get(&conn, reviews[0]).unwrap().unwrap();
    assert_eq!(rt.kind, TaskKind::Review);
    assert_eq!(rt.status, TaskStatus::Open);

    // 人工把 S2 从 fail 改判为 pass → 按效果账本回滚补背，再排复习
    let confirmed = scoring::human_decide(
        &conn,
        sub2,
        "pass",
        Some("老师确认"),
        Some("teacher"),
        today,
        &cfg,
    )
    .unwrap();
    assert!(matches!(confirmed, scoring::NextAction::Scheduled { .. }));
    assert!(!task_repo::exists_open_kind(&conn, M, s2.id, c1.id, TaskKind::Makeup).unwrap());
}
