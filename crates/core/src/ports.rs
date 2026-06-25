//! 能力抽象（Ports）。模块按需实现或复用这些 trait，实现"可插拔"。
//!
//! 见平台架构 §5。识别（ASR/OCR）、评分、解析、复习调度、导出、模块契约。

use std::collections::BTreeMap;
use std::path::Path;

use crate::domain::scheduler::{ReviewQuality, ScheduleOutcome};
use crate::error::CoreResult;
use crate::models::{MediaType, ModuleKey};

// ───────────────────────── 识别 Recognizer（ASR / OCR）─────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecognizerKind {
    /// 语音识别（音频 → 文本，可带词级时间戳）
    Asr,
    /// 光学字符识别（图片 → 文本/版面）
    Ocr,
}

pub struct MediaInput<'a> {
    pub path: &'a Path,
    pub media_type: MediaType,
    pub lang: &'a str,
}

/// 识别出的最小单元（词/字/分句），带毫秒时间戳（OCR 可不填）。
#[derive(Debug, Clone)]
pub struct RecognizedWord {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct RecognizeResult {
    pub text: String,
    pub words: Vec<RecognizedWord>,
    pub duration_ms: u64,
    /// 原始返回（JSON 字符串），脱敏后可入库审计。
    pub raw_meta: Option<String>,
}

#[async_trait::async_trait]
pub trait Recognizer: Send + Sync {
    async fn recognize(&self, input: MediaInput<'_>) -> CoreResult<RecognizeResult>;
    fn kind(&self) -> RecognizerKind;
    fn name(&self) -> &'static str;
}

// ───────────────────────── 评分 Grader ─────────────────────────

/// 统一评分结果。`primary_score`/`pass` 为主判定（M1=正确率门控；M2=对错），
/// `secondary_score`/`quality` 为副维度（M1=熟练度/记忆质量）。
#[derive(Debug, Clone)]
pub struct GradeResult {
    pub primary_score: f64,
    pub pass: bool,
    pub secondary_score: Option<f64>,
    pub quality: Option<String>,
    pub confidence: f64,
    pub metrics_json: String,
    pub note: String,
}

/// 评分抽象。各模块用关联类型声明自己的输入上下文。
pub trait Grader {
    type Input<'a>;
    fn grade(&self, input: Self::Input<'_>) -> CoreResult<GradeResult>;
}

// ───────────────────────── 导入解析 IngestParser ─────────────────────────

pub struct IngestFile<'a> {
    pub path: &'a Path,
    pub file_stem: &'a str,
    pub ext: &'a str,
}

/// 解析得到的结构化键值（如 date/student_no/content_no）。
#[derive(Debug, Clone, Default)]
pub struct ParsedMeta {
    pub fields: BTreeMap<String, String>,
}

impl ParsedMeta {
    pub fn get(&self, k: &str) -> Option<&str> {
        self.fields.get(k).map(|s| s.as_str())
    }
}

pub trait IngestParser {
    fn parse(&self, file: &IngestFile<'_>) -> CoreResult<ParsedMeta>;
}

// ───────────────────────── 复习调度 Scheduler ─────────────────────────

/// 间隔重复调度抽象。背诵复习与错题复习共用。默认实现见 `domain::scheduler`。
pub trait Scheduler {
    fn next(&self, prev_stage: i32, quality: ReviewQuality, first: bool) -> ScheduleOutcome;
}

// ───────────────────────── 导出 Exporter ─────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum ExportFormat {
    Pdf,
    Excel,
    Csv,
}

pub trait Exporter {
    fn export(&self, payload: &str, fmt: ExportFormat) -> CoreResult<Vec<u8>>;
}

// ───────────────────────── 模块契约 Module ─────────────────────────

/// 一条数据库迁移（按 id 顺序执行，幂等）。
pub struct Migration {
    pub id: &'static str,
    pub sql: &'static str,
}

/// 每个业务模块实现，向平台外壳声明自己的元数据与迁移。
/// 命令注册（Tauri）在应用外壳完成，故此处不依赖 tauri。
pub trait Module {
    fn key(&self) -> ModuleKey;
    fn display_name(&self) -> &'static str;
    fn migrations(&self) -> &'static [Migration];
}
