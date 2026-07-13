//! 统一错误类型。Tauri command 层会把它序列化为字符串返回前端。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug)]
pub enum CoreError {
    #[error("db error: {0}")]
    Db(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("recognize error: {0}")]
    Recognize(String),
    #[error("grade error: {0}")]
    Grade(String),
    #[error("config error: {0}")]
    Config(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("invalid input: {0}")]
    Invalid(String),
}

/// 稳定 IPC 错误合同。旧 command 仍可返回字符串，新 command 逐步迁移到本结构。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppErrorDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub action_hint: Option<String>,
    pub details: BTreeMap<String, String>,
}

impl AppErrorDto {
    pub fn from_core(error: &CoreError) -> Self {
        let (code, message, retryable, action_hint) = match error {
            CoreError::Db(_) => ("CORE_DB_ERROR", "数据库操作失败", true, Some("请稍后重试")),
            CoreError::NotFound(_) => (
                "CORE_NOT_FOUND",
                "未找到请求的数据",
                false,
                Some("请刷新后重试"),
            ),
            CoreError::Parse(_) => (
                "CORE_PARSE_ERROR",
                "输入内容无法解析",
                false,
                Some("请检查输入格式"),
            ),
            CoreError::Recognize(_) => (
                "AI_RECOGNIZE_ERROR",
                "智能识别失败",
                true,
                Some("请检查网络或稍后重试"),
            ),
            CoreError::Grade(_) => (
                "AI_GRADE_ERROR",
                "智能评分失败",
                true,
                Some("请保留证据并重新分析"),
            ),
            CoreError::Config(_) => (
                "CORE_CONFIG_ERROR",
                "系统配置无效",
                false,
                Some("请检查设置"),
            ),
            CoreError::Io(_) => (
                "CORE_IO_ERROR",
                "文件操作失败",
                true,
                Some("请检查文件和磁盘状态"),
            ),
            CoreError::Invalid(message) => (
                "CORE_INVALID_INPUT",
                message.as_str(),
                false,
                Some("请检查输入后重试"),
            ),
        };
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            action_hint: action_hint.map(str::to_string),
            details: BTreeMap::new(),
        }
    }
}

/// 让 `CoreError` 可直接作为 Tauri command 的 `Err`（序列化成可读字符串）。
impl Serialize for CoreError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type CoreResult<T> = Result<T, CoreError>;

impl From<rusqlite::Error> for CoreError {
    fn from(e: rusqlite::Error) -> Self {
        CoreError::Db(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_error_contract_matches_shared_fixture() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../../contracts/app_error.v1.json")).unwrap();
        let dto = AppErrorDto {
            code: "CORE_INVALID_INPUT".into(),
            message: "示例错误".into(),
            retryable: false,
            action_hint: Some("请检查输入后重试".into()),
            details: BTreeMap::from([("field".into(), "example".into())]),
        };
        assert_eq!(serde_json::to_value(dto).unwrap(), fixture);
    }

    #[test]
    fn core_error_mapping_exposes_code_without_internal_details() {
        let dto = AppErrorDto::from_core(&CoreError::Invalid("答案为空".into()));
        assert_eq!(dto.code, "CORE_INVALID_INPUT");
        assert_eq!(dto.message, "答案为空");
        assert!(!dto.retryable);
        assert!(dto.details.is_empty());
    }

    #[test]
    fn internal_database_details_are_not_exposed() {
        let dto = AppErrorDto::from_core(&CoreError::Db(
            "SQL failed at /Users/teacher/private/data.db".into(),
        ));
        assert_eq!(dto.message, "数据库操作失败");
        assert!(!dto.message.contains("/Users/"));
    }
}
