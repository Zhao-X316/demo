//! M2 改作业模块。
//!
//! 依赖 `core`，复用 学生/提交/任务/判定/设置；只实现批改特有部分：
//! 知识点树、题库（含逐选项解析+知识点）、作答对比、错题、掌握度聚合，以及豆包视觉大模型接入。

pub mod db;

use suite_core::models::ModuleKey;
use suite_core::ports::{Migration, Module};

static MIGRATIONS: &[Migration] = &[Migration {
    id: "exam_0001",
    sql: include_str!("../migrations/0001_exam.sql"),
}];

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
