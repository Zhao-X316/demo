//! 跨模块稳定公共 ID。
//!
//! 本地整数主键继续用于 SQLite 关联；`public_id` 使用 UUID v7，供未来导出、同步和跨设备引用。

use uuid::Uuid;

/// 生成按时间大致有序的 UUID v7 文本。
pub fn new_public_id() -> String {
    Uuid::now_v7().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_id_is_uuid_v7() {
        let value = new_public_id();
        let parsed = Uuid::parse_str(&value).unwrap();
        assert_eq!(parsed.get_version_num(), 7);
        assert_eq!(value.len(), 36);
    }
}
