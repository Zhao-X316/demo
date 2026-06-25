//! 通用表仓储（纯 SQLite 读写，不含业务规则）。
//! 业务规则在 services 层（外壳/模块）。

pub mod file_ledger;
pub mod memory_cards;
pub mod settings;
pub mod students;
pub mod submissions;
pub mod tasks;
pub mod verdicts;
