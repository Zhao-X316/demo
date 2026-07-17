//! M6 学生学习掌握快照。
//!
//! 本模块只消费老师确认且 active 的正式学习证据，生成可追溯、不可变的个人
//! 快照。未评估、证据不足和需要支持严格分开；快照不会修改成绩、任务、错题
//! 或上游证据。

pub mod action_drafts;
pub mod class_exports;
pub mod class_profile;
pub mod profile;
pub mod teaching_events;

use suite_core::models::ModuleKey;
use suite_core::ports::{Migration, Module};

static MIGRATIONS: &[Migration] = &[
    Migration {
        id: "profile_0001",
        sql: include_str!("../migrations/0001_student_profile_snapshots.sql"),
    },
    Migration {
        id: "profile_0002",
        sql: include_str!("../migrations/0002_class_profile_snapshots.sql"),
    },
    Migration {
        id: "profile_0003",
        sql: include_str!("../migrations/0003_class_teaching_events.sql"),
    },
    Migration {
        id: "profile_0004",
        sql: include_str!("../migrations/0004_class_action_drafts.sql"),
    },
    Migration {
        id: "profile_0005",
        sql: include_str!("../migrations/0005_class_profile_exports.sql"),
    },
];

pub fn profile_migrations() -> &'static [Migration] {
    MIGRATIONS
}

pub struct ProfileModule;

impl Module for ProfileModule {
    fn key(&self) -> ModuleKey {
        ModuleKey::Profile
    }

    fn display_name(&self) -> &'static str {
        "学习掌握图谱"
    }

    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }
}
