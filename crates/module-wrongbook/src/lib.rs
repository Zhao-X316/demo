//! M3 错题事实与订正状态。
//!
//! 本模块不复制 M2 分数，也不把一次订正解释成“已掌握”。第一批只提供
//! 基于当前有效发布快照的可重建只读模型；后续复习任务与 M6 掌握分析继续
//! 使用独立的业务语义。

pub mod read_model;

use suite_core::models::ModuleKey;
use suite_core::ports::{Migration, Module};

static MIGRATIONS: &[Migration] = &[];

pub fn wrongbook_migrations() -> &'static [Migration] {
    MIGRATIONS
}

pub struct WrongbookModule;

impl Module for WrongbookModule {
    fn key(&self) -> ModuleKey {
        ModuleKey::Wrongbook
    }

    fn display_name(&self) -> &'static str {
        "错题与掌握"
    }

    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }
}
