//! 通用表仓储（纯 SQLite 读写，不含业务规则）。
//! 业务规则在 services 层（外壳/模块）。

pub mod ai_runs;
pub mod artifacts;
pub mod audit;
pub mod background_jobs;
pub mod classes;
pub mod decision_effects;
pub mod file_ledger;
pub mod learning_evidence;
pub mod memory_cards;
pub mod outbox;
pub mod settings;
pub mod students;
pub mod submissions;
pub mod tasks;
pub mod verdicts;
