//! M3 错题事实与订正状态。
//!
//! 本模块不复制 M2 分数，也不把一次订正解释成“已掌握”。错题事实基于
//! 当前有效发布快照重建；老师确认的错因以不可变修订独立保存。后续复习
//! 任务与 M6 掌握分析继续使用独立的业务语义。

pub mod correction;
pub mod error_cause;
pub mod read_model;
pub mod reinforcement;

use suite_core::models::ModuleKey;
use suite_core::ports::{Migration, Module};

static MIGRATIONS: &[Migration] = &[
    Migration {
        id: "wrongbook_0001",
        sql: include_str!("../migrations/0001_error_cause_reviews.sql"),
    },
    Migration {
        id: "wrongbook_0002",
        sql: include_str!("../migrations/0002_correction_assignments.sql"),
    },
    Migration {
        id: "wrongbook_0003",
        sql: include_str!("../migrations/0003_reinforcement_scheduling.sql"),
    },
];

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
