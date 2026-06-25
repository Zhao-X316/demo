//! M1 背诵批改模块。
//!
//! 详规见 `背诵批改系统_技术规格与实施方案_v1.md`。
//! 依赖 `core`，复用其通用算法（归一化/相似度/正确率门控/复习调度），
//! 只实现背诵特有部分：文件名解析、熟练度、评分编排、火山 ASR provider。

pub mod asr_volcano;
pub mod config;
pub mod db;
pub mod domain;
pub mod grader;
pub mod service;

use suite_core::models::ModuleKey;
use suite_core::ports::{Migration, Module};

/// 模块入口，向平台外壳声明元数据与迁移。
pub struct RecitationModule;

static MIGRATIONS: &[Migration] = &[Migration {
    id: "recitation_0001",
    sql: include_str!("../migrations/0001_recitation.sql"),
}];

/// 模块迁移（供外壳/测试在 core 迁移之后运行）。
pub fn recitation_migrations() -> &'static [Migration] {
    MIGRATIONS
}

impl Module for RecitationModule {
    fn key(&self) -> ModuleKey {
        ModuleKey::Recitation
    }
    fn display_name(&self) -> &'static str {
        "背诵批改"
    }
    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }
}
