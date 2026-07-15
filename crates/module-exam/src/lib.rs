//! M2 改作业模块。
//!
//! 依赖 `core`，复用 学生/提交/任务/判定/设置；只实现批改特有部分：
//! 知识点树、题库（含逐选项解析+知识点）、作答对比、错题、掌握度聚合，以及豆包视觉大模型接入。

pub mod db;
pub mod objective_recognition;
pub mod ordinary_paper_recognition;
pub mod service;
pub mod vlm;

use suite_core::models::ModuleKey;
use suite_core::ports::{Migration, Module};

static MIGRATIONS: &[Migration] = &[
    Migration {
        id: "exam_0001",
        sql: include_str!("../migrations/0001_exam.sql"),
    },
    Migration {
        id: "exam_0002",
        sql: include_str!("../migrations/0002_exam_grading.sql"),
    },
    Migration {
        id: "exam_0003",
        sql: include_str!("../migrations/0003_assessment_contracts.sql"),
    },
    Migration {
        id: "exam_0004",
        sql: include_str!("../migrations/0004_paper_ingest.sql"),
    },
    Migration {
        id: "exam_0005",
        sql: include_str!("../migrations/0005_page_regions.sql"),
    },
    Migration {
        id: "exam_0006",
        sql: include_str!("../migrations/0006_question_ingest.sql"),
    },
    Migration {
        id: "exam_0007",
        sql: include_str!("../migrations/0007_objective_grading.sql"),
    },
    Migration {
        id: "exam_0008",
        sql: include_str!("../migrations/0008_fixed_paper_preflight.sql"),
    },
    Migration {
        id: "exam_0009",
        sql: include_str!("../migrations/0009_ordered_material_routing.sql"),
    },
    Migration {
        id: "exam_0010",
        sql: include_str!("../migrations/0010_ordered_grouping_confirmation.sql"),
    },
    Migration {
        id: "exam_0011",
        sql: include_str!("../migrations/0011_ordered_grouping_activation.sql"),
    },
    Migration {
        id: "exam_0012",
        sql: include_str!("../migrations/0012_ordered_page_retake.sql"),
    },
];

/// 模块迁移（供外壳/测试在 core 迁移之后运行）。
pub fn exam_migrations() -> &'static [Migration] {
    MIGRATIONS
}

pub struct ExamModule;

impl Module for ExamModule {
    fn key(&self) -> ModuleKey {
        ModuleKey::Exam
    }
    fn display_name(&self) -> &'static str {
        "改作业"
    }
    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }
}
