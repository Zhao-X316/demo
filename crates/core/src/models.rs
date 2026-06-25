//! 通用实体与值对象（跨模块）。
//!
//! 与平台架构 §4 通用数据模型对齐。通用表带 `module` 维度，模块专属表在各模块 crate 内定义。
//! 这里只放结构定义（serde 可序列化，供 Tauri IPC 与 DB 行映射复用）。

use serde::{Deserialize, Serialize};

/// 模块标识：通用表用它区分归属。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleKey {
    Recitation,
    Exam,
    Wrongbook,
}

impl ModuleKey {
    pub fn as_str(&self) -> &'static str {
        match self {
            ModuleKey::Recitation => "recitation",
            ModuleKey::Exam => "exam",
            ModuleKey::Wrongbook => "wrongbook",
        }
    }
}

/// 学生（核心实体，全模块共用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Student {
    pub id: i64,
    pub student_no: String,
    pub name: String,
    pub class_id: Option<i64>,
    pub enabled: bool,
}

/// 班级。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Class {
    pub id: i64,
    pub name: String,
    pub term: Option<String>,
}

/// 知识点（自引用成树；M2/M3/报表共用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgePoint {
    pub id: i64,
    pub subject_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub code: Option<String>,
    pub name: String,
}

/// 任务种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskKind {
    Normal,
    Makeup,
    Review,
}

/// 任务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Open,
    Submitted,
    Passed,
    Failed,
    Reopened,
    Closed,
    Expired,
}

/// 通用任务（M1/M2/M3 共用，`module` + `ref_id` 指向具体对象）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: i64,
    pub module: ModuleKey,
    pub student_id: i64,
    pub subject_id: Option<i64>,
    pub ref_type: String,
    pub ref_id: i64,
    pub kind: TaskKind,
    pub due_date: String, // YYYY-MM-DD（Asia/Shanghai）
    pub status: TaskStatus,
    pub source_task_id: Option<i64>,
    pub card_id: Option<i64>,
}

/// 复习卡片状态（间隔重复内核；背诵/错题共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardState {
    Learning,
    Review,
    Lapsed,
}

/// 通用复习卡片（每个 module×student×ref 一张）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryCard {
    pub id: i64,
    pub module: ModuleKey,
    pub student_id: i64,
    pub ref_type: String,
    pub ref_id: i64,
    pub state: CardState,
    pub stage: i32,
    pub interval_days: i32,
    pub ease: f64,
    pub last_quality: Option<String>,
    pub due_date: Option<String>,
    pub reps: i32,
    pub lapses: i32,
}

/// 媒体类型（提交内容）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaType {
    Audio,
    Image,
    Text,
}
