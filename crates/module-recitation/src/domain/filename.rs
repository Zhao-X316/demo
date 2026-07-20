//! 录音文件名解析：`YYYYMMDD_学号_姓名_内容编号[_序号]`。
//! 学号为权威匹配字段；姓名仅校验。实现 core 的 `IngestParser`。

use suite_core::error::{CoreError, CoreResult};
use suite_core::ports::{IngestFile, IngestParser, ParsedMeta};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedName {
    pub date: String, // YYYY-MM-DD
    pub student_no: String,
    pub name: String,
    pub content_no: String,
    pub seq: Option<u32>,
}

pub fn parse(file_stem: &str) -> CoreResult<ParsedName> {
    let parts: Vec<&str> = file_stem.split('_').collect();
    if parts.len() < 4 {
        return Err(CoreError::Parse(format!(
            "字段不足（需至少4段）: {file_stem}"
        )));
    }
    let raw = parts[0];
    if raw.len() != 8 || !raw.chars().all(|c| c.is_ascii_digit()) {
        return Err(CoreError::Parse(format!("日期非法（需YYYYMMDD）: {raw}")));
    }
    let date = format!("{}-{}-{}", &raw[0..4], &raw[4..6], &raw[6..8]);
    if parts[1].is_empty() {
        return Err(CoreError::Parse("学号为空".into()));
    }
    if parts[3].is_empty() {
        return Err(CoreError::Parse("内容编号为空".into()));
    }
    Ok(ParsedName {
        date,
        student_no: parts[1].to_string(),
        name: parts[2].to_string(),
        content_no: parts[3].to_string(),
        seq: parts.get(4).and_then(|s| s.parse::<u32>().ok()),
    })
}

pub struct FilenameParser;

impl IngestParser for FilenameParser {
    fn parse(&self, file: &IngestFile<'_>) -> CoreResult<ParsedMeta> {
        let p = parse(file.file_stem)?;
        let mut m = ParsedMeta::default();
        m.fields.insert("date".to_string(), p.date);
        m.fields.insert("student_no".to_string(), p.student_no);
        m.fields.insert("name".to_string(), p.name);
        m.fields.insert("content_no".to_string(), p.content_no);
        if let Some(seq) = p.seq {
            m.fields.insert("seq".to_string(), seq.to_string());
        }
        Ok(m)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_full() {
        let p = parse("20260625_2023001_张三_C012_1").unwrap();
        assert_eq!(p.date, "2026-06-25");
        assert_eq!(p.student_no, "2023001");
        assert_eq!(p.name, "张三");
        assert_eq!(p.content_no, "C012");
        assert_eq!(p.seq, Some(1));
    }

    #[test]
    fn ok_without_seq() {
        let p = parse("20260625_2023001_张三_C012").unwrap();
        assert_eq!(p.seq, None);
    }

    #[test]
    fn bad_date() {
        assert!(parse("2026_2023001_张三_C012").is_err());
    }

    #[test]
    fn missing_fields() {
        assert!(parse("20260625_2023001").is_err());
    }

    #[test]
    fn parser_port_fills_meta() {
        let f = IngestFile {
            path: std::path::Path::new("x"),
            file_stem: "20260625_2023001_张三_C012_2",
            ext: "m4a",
        };
        let meta = FilenameParser.parse(&f).unwrap();
        assert_eq!(meta.get("student_no"), Some("2023001"));
        assert_eq!(meta.get("content_no"), Some("C012"));
        assert_eq!(meta.get("seq"), Some("2"));
    }
}
