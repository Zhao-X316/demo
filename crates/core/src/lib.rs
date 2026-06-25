//! 教辅平台共享内核 `core`
//!
//! 见 `教辅系统_平台总体架构_v1.md`。本 crate 提供跨模块通用能力：
//! - `error`：统一错误类型 [`CoreError`]
//! - `models`：通用实体与值对象（学生/任务/复习卡片/提交/判定 等）
//! - `ports`：能力抽象 trait（识别 / 评分 / 解析 / 复习调度 / 导出 / 模块契约）
//! - `domain`：纯算法（文本归一化 / 拼音 / 相似度 / 正确率门控 / 间隔重复调度）
//!
//! 业务模块（M1 背诵、M2 题目批改、M3 错题统计…）依赖本 crate，
//! 只实现各自差异部分；模块之间不互相依赖。

pub mod db;
pub mod domain;
pub mod error;
pub mod models;
pub mod ports;

pub use error::CoreError;
