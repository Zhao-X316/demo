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
    /// 从数据库字符串解析（未知值兜底为 Recitation）。
    pub fn from_db(s: &str) -> Self {
        match s {
            "exam" => ModuleKey::Exam,
            "wrongbook" => ModuleKey::Wrongbook,
            _ => ModuleKey::Recitation,
        }
    }
}

impl MediaType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MediaType::Audio => "audio",
            MediaType::Image => "image",
            MediaType::Text => "text",
        }
    }
    pub fn from_db(s: &str) -> Self {
        match s {
            "image" => MediaType::Image,
            "text" => MediaType::Text,
            _ => MediaType::Audio,
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
    /// 绑定教材（学科+年级册，如「道法8上」），布置时默认到该班教材。
    pub textbook: Option<String>,
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
    pub last_reviewed_at: Option<String>,
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

/// 统一资产类型。原始文件和所有派生文件都在 core 中登记。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Audio,
    Document,
    Page,
    Crop,
    Image,
    Export,
}

impl ArtifactKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArtifactKind::Audio => "audio",
            ArtifactKind::Document => "document",
            ArtifactKind::Page => "page",
            ArtifactKind::Crop => "crop",
            ArtifactKind::Image => "image",
            ArtifactKind::Export => "export",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "audio" => Some(ArtifactKind::Audio),
            "document" => Some(ArtifactKind::Document),
            "page" => Some(ArtifactKind::Page),
            "crop" => Some(ArtifactKind::Crop),
            "image" => Some(ArtifactKind::Image),
            "export" => Some(ArtifactKind::Export),
            _ => None,
        }
    }
}

/// 资产隐私等级。学生原始证据不能静默降级成可共享内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyClass {
    StudentSensitive,
    TeachingContent,
    PublicSafe,
}

impl PrivacyClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrivacyClass::StudentSensitive => "student_sensitive",
            PrivacyClass::TeachingContent => "teaching_content",
            PrivacyClass::PublicSafe => "public_safe",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "student_sensitive" => Some(PrivacyClass::StudentSensitive),
            "teaching_content" => Some(PrivacyClass::TeachingContent),
            "public_safe" => Some(PrivacyClass::PublicSafe),
            _ => None,
        }
    }
}

/// app-managed 归档文件的当前状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveStatus {
    Pending,
    Ready,
    Missing,
    Failed,
    Deleted,
}

impl ArchiveStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ArchiveStatus::Pending => "pending",
            ArchiveStatus::Ready => "ready",
            ArchiveStatus::Missing => "missing",
            ArchiveStatus::Failed => "failed",
            ArchiveStatus::Deleted => "deleted",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(ArchiveStatus::Pending),
            "ready" => Some(ArchiveStatus::Ready),
            "missing" => Some(ArchiveStatus::Missing),
            "failed" => Some(ArchiveStatus::Failed),
            "deleted" => Some(ArchiveStatus::Deleted),
            _ => None,
        }
    }
}

/// 跨模块统一资产。内容字段不可覆盖；校正、裁剪和脱敏通过 parent 创建新资产。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub id: i64,
    pub public_id: String,
    pub kind: ArtifactKind,
    pub sha256: String,
    pub mime_type: String,
    pub byte_size: i64,
    pub original_name: Option<String>,
    pub original_path: Option<String>,
    pub archived_path: String,
    pub parent_artifact_id: Option<i64>,
    pub derivative_type: Option<String>,
    pub processing_version: String,
    pub privacy_class: PrivacyClass,
    pub archive_status: ArchiveStatus,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiRunStatus {
    Pending,
    Processing,
    Succeeded,
    Failed,
    Voided,
}

impl AiRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            AiRunStatus::Pending => "pending",
            AiRunStatus::Processing => "processing",
            AiRunStatus::Succeeded => "succeeded",
            AiRunStatus::Failed => "failed",
            AiRunStatus::Voided => "voided",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "pending" => Some(AiRunStatus::Pending),
            "processing" => Some(AiRunStatus::Processing),
            "succeeded" => Some(AiRunStatus::Succeeded),
            "failed" => Some(AiRunStatus::Failed),
            "voided" => Some(AiRunStatus::Voided),
            _ => None,
        }
    }
}

/// 一次不可覆盖的 AI 运行记录。重试通过 `retry_of_ai_run_id` 追加新行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiRun {
    pub id: i64,
    pub public_id: String,
    pub idempotency_key: String,
    pub run_type: String,
    pub source_module: String,
    pub business_ref_type: String,
    pub business_ref_id: String,
    pub input_artifact_id: Option<i64>,
    pub provider: String,
    pub model_name: String,
    pub model_version: String,
    pub config_version: String,
    pub prompt_or_rule_version: String,
    pub input_hash: String,
    pub output_hash: Option<String>,
    pub status: AiRunStatus,
    pub retry_of_ai_run_id: Option<i64>,
    pub remote_run_id: Option<String>,
    pub confidence: Option<f64>,
    pub output_json: Option<String>,
    pub error_meta_json: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackgroundJobStatus {
    Queued,
    Claimed,
    Processing,
    Succeeded,
    Failed,
    Cancelled,
}

impl BackgroundJobStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            BackgroundJobStatus::Queued => "queued",
            BackgroundJobStatus::Claimed => "claimed",
            BackgroundJobStatus::Processing => "processing",
            BackgroundJobStatus::Succeeded => "succeeded",
            BackgroundJobStatus::Failed => "failed",
            BackgroundJobStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_db(value: &str) -> Option<Self> {
        match value {
            "queued" => Some(BackgroundJobStatus::Queued),
            "claimed" => Some(BackgroundJobStatus::Claimed),
            "processing" => Some(BackgroundJobStatus::Processing),
            "succeeded" => Some(BackgroundJobStatus::Succeeded),
            "failed" => Some(BackgroundJobStatus::Failed),
            "cancelled" => Some(BackgroundJobStatus::Cancelled),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackgroundJob {
    pub id: i64,
    pub public_id: String,
    pub job_type: String,
    pub business_key: String,
    pub source_module: String,
    pub business_ref_type: String,
    pub business_ref_id: String,
    pub ai_run_id: Option<i64>,
    pub stage: String,
    pub status: BackgroundJobStatus,
    pub attempts: i64,
    pub max_attempts: i64,
    pub lease_token: Option<String>,
    pub lease_expires_at: Option<String>,
    pub next_retry_at: Option<String>,
    pub progress_json: Option<String>,
    pub error_meta_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 通用提交（音频/图片/文本）。状态用字符串以便模块灵活扩展。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Submission {
    pub id: i64,
    pub module: ModuleKey,
    pub task_id: Option<i64>,
    pub student_id: Option<i64>,
    pub ref_id: Option<i64>,
    pub media_type: MediaType,
    pub file_path: String,
    pub archived_path: Option<String>,
    pub file_hash: String,
    pub artifact_id: Option<i64>,
    pub duration_ms: Option<i64>,
    pub parsed_meta: Option<String>,
    pub recognized_text: Option<String>,
    pub recognize_meta: Option<String>,
    pub recognize_status: String, // pending|processing|ok|failed
    pub anomaly_type: Option<String>,
    pub status: String, // pending|scored|confirmed|anomaly|voided
}

/// 通用判定（两套体系并列）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verdict {
    pub id: i64,
    pub submission_id: i64,
    pub module: ModuleKey,
    pub primary_score: Option<f64>,
    pub pass: Option<bool>,
    pub secondary_score: Option<f64>,
    pub quality: Option<String>,
    pub confidence: Option<f64>,
    pub answer_version: i64,
    pub metrics_json: Option<String>,
    pub machine_note: Option<String>,
    pub human_result: Option<String>, // pass|fail|reopen
    pub human_note: Option<String>,
}
