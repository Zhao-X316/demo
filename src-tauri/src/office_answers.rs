//! 本地提取 DOCX/XLSX 中可供答案结构化使用的文本。
//!
//! 原始 Office 文件仍作为不可变 teaching_content artifact 归档；这里仅做确定性 OOXML
//! 解包和文本投影，不执行宏、不联网，也不把投影结果直接确认为正式答案。

use std::io::{Cursor, Read};

use quick_xml::events::Event;
use quick_xml::Reader;
use suite_core::error::{CoreError, CoreResult};
use zip::ZipArchive;

pub const DOCX_EXTRACTION_VERSION: &str = "ooxml-docx-text-v1";
pub const XLSX_EXTRACTION_VERSION: &str = "ooxml-xlsx-cells-v1";

const MAX_XML_ENTRY_BYTES: u64 = 12 * 1024 * 1024;
const MAX_EXTRACTED_TEXT_BYTES: usize = 2 * 1024 * 1024;

fn parse_error(label: &str, error: impl std::fmt::Display) -> CoreError {
    CoreError::Parse(format!("{label}无法解析：{error}"))
}

fn open_archive(bytes: &[u8]) -> CoreResult<ZipArchive<Cursor<&[u8]>>> {
    ZipArchive::new(Cursor::new(bytes)).map_err(|error| parse_error("Office 文件", error))
}

fn read_entry(archive: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> CoreResult<String> {
    let mut entry = archive
        .by_name(name)
        .map_err(|_| CoreError::Invalid(format!("Office 文件缺少 {name}")))?;
    if entry.size() > MAX_XML_ENTRY_BYTES {
        return Err(CoreError::Invalid(format!("Office XML {name} 过大")));
    }
    let mut value = String::new();
    entry
        .read_to_string(&mut value)
        .map_err(|error| parse_error(name, error))?;
    Ok(value)
}

fn push_bounded(output: &mut String, value: &str) -> CoreResult<()> {
    if output.len().saturating_add(value.len()) > MAX_EXTRACTED_TEXT_BYTES {
        return Err(CoreError::Invalid("Office 答案提取文本超过 2MB".into()));
    }
    output.push_str(value);
    Ok(())
}

fn has_local_name(name: quick_xml::name::QName<'_>, expected: &[u8]) -> bool {
    name.local_name().as_ref() == expected
}

pub fn extract_docx(bytes: &[u8]) -> CoreResult<String> {
    let mut archive = open_archive(bytes)?;
    let xml = read_entry(&mut archive, "word/document.xml")?;
    let mut reader = Reader::from_str(&xml);
    reader.trim_text(true);
    let mut buffer = Vec::new();
    let mut paragraph = String::new();
    let mut paragraphs = Vec::new();
    let mut in_text = false;
    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| parse_error("DOCX document.xml", error))?
        {
            Event::Start(event) if has_local_name(event.name(), b"t") => in_text = true,
            Event::Start(event)
                if (has_local_name(event.name(), b"br")
                    || has_local_name(event.name(), b"tab"))
                    && !paragraph.ends_with(' ') =>
            {
                paragraph.push(' ');
            }
            Event::Empty(event)
                if (has_local_name(event.name(), b"br")
                    || has_local_name(event.name(), b"tab"))
                    && !paragraph.ends_with(' ') =>
            {
                paragraph.push(' ');
            }
            Event::Text(event) if in_text => {
                paragraph.push_str(
                    &event
                        .unescape()
                        .map_err(|error| parse_error("DOCX 文本", error))?,
                );
            }
            Event::End(event) => {
                if has_local_name(event.name(), b"t") {
                    in_text = false;
                } else if has_local_name(event.name(), b"p") {
                    let value = paragraph.trim();
                    if !value.is_empty() {
                        paragraphs.push(value.to_string());
                    }
                    paragraph.clear();
                    in_text = false;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if !paragraph.trim().is_empty() {
        paragraphs.push(paragraph.trim().into());
    }
    if paragraphs.is_empty() {
        return Err(CoreError::Invalid("DOCX 未提取到答案文字".into()));
    }
    let mut output = String::new();
    for (index, paragraph) in paragraphs.iter().enumerate() {
        push_bounded(&mut output, &format!("第{}段\t{}\n", index + 1, paragraph))?;
    }
    Ok(output)
}

fn shared_strings(xml: &str) -> CoreResult<Vec<String>> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut buffer = Vec::new();
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_text = false;
    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| parse_error("XLSX sharedStrings.xml", error))?
        {
            Event::Start(event) if has_local_name(event.name(), b"t") => in_text = true,
            Event::Text(event) if in_text => current.push_str(
                &event
                    .unescape()
                    .map_err(|error| parse_error("XLSX 共享文本", error))?,
            ),
            Event::End(event) if has_local_name(event.name(), b"t") => in_text = false,
            Event::End(event) if has_local_name(event.name(), b"si") => {
                values.push(std::mem::take(&mut current));
                in_text = false;
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(values)
}

fn attribute(event: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> CoreResult<Option<String>> {
    for value in event.attributes().with_checks(false) {
        let value = value.map_err(|error| parse_error("XLSX 单元格属性", error))?;
        if value.key.as_ref() == name {
            return value
                .unescape_value()
                .map(|value| Some(value.into_owned()))
                .map_err(|error| parse_error("XLSX 单元格属性", error));
        }
    }
    Ok(None)
}

fn sheet_sort_key(name: &str) -> (u64, String) {
    let number = name
        .rsplit('/')
        .next()
        .unwrap_or(name)
        .chars()
        .filter(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .unwrap_or(u64::MAX);
    (number, name.to_string())
}

fn extract_sheet(xml: &str, shared: &[String]) -> CoreResult<Vec<(String, String)>> {
    let mut reader = Reader::from_str(xml);
    reader.trim_text(true);
    let mut buffer = Vec::new();
    let mut cells = Vec::new();
    let mut cell_ref = String::new();
    let mut cell_type = String::new();
    let mut raw = String::new();
    let mut capture_value = false;
    loop {
        match reader
            .read_event_into(&mut buffer)
            .map_err(|error| parse_error("XLSX worksheet", error))?
        {
            Event::Start(event) if has_local_name(event.name(), b"c") => {
                cell_ref = attribute(&event, b"r")?.unwrap_or_else(|| "未知单元格".into());
                cell_type = attribute(&event, b"t")?.unwrap_or_default();
                raw.clear();
            }
            Event::Start(event)
                if has_local_name(event.name(), b"v") || has_local_name(event.name(), b"t") =>
            {
                capture_value = true;
            }
            Event::Text(event) if capture_value => raw.push_str(
                &event
                    .unescape()
                    .map_err(|error| parse_error("XLSX 单元格文本", error))?,
            ),
            Event::End(event)
                if has_local_name(event.name(), b"v") || has_local_name(event.name(), b"t") =>
            {
                capture_value = false;
            }
            Event::End(event) if has_local_name(event.name(), b"c") => {
                let value = if cell_type == "s" {
                    raw.parse::<usize>()
                        .ok()
                        .and_then(|index| shared.get(index))
                        .cloned()
                        .ok_or_else(|| CoreError::Invalid("XLSX 共享字符串索引越界".into()))?
                } else {
                    raw.trim().to_string()
                };
                if !value.is_empty() {
                    cells.push((cell_ref.clone(), value));
                }
                capture_value = false;
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(cells)
}

pub fn extract_xlsx(bytes: &[u8]) -> CoreResult<String> {
    let mut archive = open_archive(bytes)?;
    let shared = match archive.by_name("xl/sharedStrings.xml") {
        Ok(mut entry) => {
            if entry.size() > MAX_XML_ENTRY_BYTES {
                return Err(CoreError::Invalid("XLSX sharedStrings.xml 过大".into()));
            }
            let mut xml = String::new();
            entry
                .read_to_string(&mut xml)
                .map_err(|error| parse_error("XLSX sharedStrings.xml", error))?;
            shared_strings(&xml)?
        }
        Err(zip::result::ZipError::FileNotFound) => vec![],
        Err(error) => return Err(parse_error("XLSX sharedStrings.xml", error)),
    };
    let mut sheets = (0..archive.len())
        .filter_map(|index| {
            archive
                .by_index(index)
                .ok()
                .map(|entry| entry.name().to_string())
        })
        .filter(|name| name.starts_with("xl/worksheets/") && name.ends_with(".xml"))
        .collect::<Vec<_>>();
    sheets.sort_by_key(|name| sheet_sort_key(name));
    if sheets.is_empty() {
        return Err(CoreError::Invalid("XLSX 不包含工作表".into()));
    }
    let mut output = String::new();
    for (sheet_index, name) in sheets.iter().enumerate() {
        let xml = read_entry(&mut archive, name)?;
        let cells = extract_sheet(&xml, &shared)?;
        if cells.is_empty() {
            continue;
        }
        push_bounded(
            &mut output,
            &format!("工作表{}\t{}\n", sheet_index + 1, name),
        )?;
        for (cell_ref, value) in cells {
            push_bounded(&mut output, &format!("{cell_ref}\t{value}\n"))?;
        }
    }
    if output.trim().is_empty() {
        return Err(CoreError::Invalid("XLSX 未提取到答案单元格".into()));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::write::FileOptions;
    use zip::ZipWriter;

    use super::*;

    fn package(files: &[(&str, &str)]) -> Vec<u8> {
        let mut output = Cursor::new(Vec::new());
        {
            let mut writer = ZipWriter::new(&mut output);
            for (name, value) in files {
                writer
                    .start_file(
                        *name,
                        FileOptions::default().compression_method(zip::CompressionMethod::Stored),
                    )
                    .unwrap();
                writer.write_all(value.as_bytes()).unwrap();
            }
            writer.finish().unwrap();
        }
        output.into_inner()
    }

    #[test]
    fn docx_extracts_paragraphs_without_executing_any_external_content() {
        let bytes = package(&[(
            "word/document.xml",
            r#"<w:document xmlns:w="w"><w:body><w:p><w:r><w:t>1.A</w:t></w:r></w:p><w:p><w:r><w:t>2.1842年</w:t></w:r></w:p></w:body></w:document>"#,
        )]);
        let text = extract_docx(&bytes).unwrap();
        assert!(text.contains("第1段\t1.A"));
        assert!(text.contains("第2段\t2.1842年"));
    }

    #[test]
    fn xlsx_extracts_shared_and_inline_cell_values_with_coordinates() {
        let bytes = package(&[
            (
                "xl/sharedStrings.xml",
                r#"<sst><si><t>题号</t></si><si><t>答案</t></si><si><t>A</t></si></sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="s"><v>0</v></c><c r="B1" t="s"><v>1</v></c></row><row r="2"><c r="A2"><v>1</v></c><c r="B2" t="s"><v>2</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let text = extract_xlsx(&bytes).unwrap();
        assert!(text.contains("A1\t题号"));
        assert!(text.contains("B2\tA"));
    }
}
