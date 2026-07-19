//! K1 题目与知识语义层。
//!
//! K1 是教材、知识点、考点、能力、题目和评价规则的唯一正式语义层。
//! 旧 `knowledge_points` / `exam_knowledge_points` / `exam_questions` 只通过
//! 显式映射兼容，不能直接改名或猜测迁移。

pub mod db;

use suite_core::models::ModuleKey;
use suite_core::ports::{Migration, Module};

static MIGRATIONS: &[Migration] = &[
    Migration {
        id: "k1_0001",
        sql: include_str!("../migrations/0001_taxonomy.sql"),
    },
    Migration {
        id: "k1_0002",
        sql: include_str!("../migrations/0002_content_versions.sql"),
    },
    Migration {
        id: "k1_0003",
        sql: include_str!("../migrations/0003_search_duplicate_reviews.sql"),
    },
];

pub fn knowledge_migrations() -> &'static [Migration] {
    MIGRATIONS
}

pub struct KnowledgeModule;

impl Module for KnowledgeModule {
    fn key(&self) -> ModuleKey {
        ModuleKey::Knowledge
    }

    fn display_name(&self) -> &'static str {
        "题目与知识库"
    }

    fn migrations(&self) -> &'static [Migration] {
        MIGRATIONS
    }
}
