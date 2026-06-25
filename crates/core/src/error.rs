//! 统一错误类型。Tauri command 层会把它序列化为字符串返回前端。

use serde::Serialize;

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
