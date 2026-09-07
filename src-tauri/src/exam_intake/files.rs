//! Prepare and archive selected local files. No database or provider access.
use super::{io_error, required};
use crate::pdf_pages;
use std::path::{Path, PathBuf};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};
use suite_core::models::ArtifactKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceFormat {
    Jpeg,
    Pdf,
    Text,
    Docx,
    Xlsx,
}

impl SourceFormat {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Jpeg => "jpeg",
            Self::Pdf => "pdf",
            Self::Text => "text",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
        }
    }

    pub(super) fn mime_type(self) -> &'static str {
        match self {
            Self::Jpeg => "image/jpeg",
            Self::Pdf => "application/pdf",
            Self::Text => "text/plain",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    pub(super) fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Pdf => "pdf",
            Self::Text => "txt",
            Self::Docx => "docx",
            Self::Xlsx => "xlsx",
        }
    }

    pub(super) fn artifact_kind(self) -> ArtifactKind {
        match self {
            Self::Jpeg => ArtifactKind::Image,
            Self::Pdf | Self::Text | Self::Docx | Self::Xlsx => ArtifactKind::Document,
        }
    }

    pub(super) fn registration_format(self) -> &'static str {
        match self {
            Self::Docx | Self::Xlsx => "text",
            _ => self.as_str(),
        }
    }
}

pub(super) struct PreparedSource {
    pub(super) path: Option<PathBuf>,
    pub(super) original_name: String,
    pub(super) format: SourceFormat,
    pub(super) bytes: Option<Vec<u8>>,
    pub(super) expected_hash: String,
    pub(super) pdf_pages: Vec<Vec<u8>>,
    pub(super) original_request_index: i64,
    pub(super) capture_time: Option<String>,
    pub(super) file_created_ms: Option<i64>,
    pub(super) file_modified_ms: Option<i64>,
}

pub(crate) struct ArchivedFile {
    pub(crate) path: PathBuf,
    created: bool,
}

impl ArchivedFile {
    pub(crate) fn rollback_new_file(&self) {
        if self.created {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub(super) fn source_format(path: &Path, allow_text: bool) -> CoreResult<SourceFormat> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("jpg" | "jpeg") => Ok(SourceFormat::Jpeg),
        Some("pdf") => Ok(SourceFormat::Pdf),
        Some("txt") if allow_text => Ok(SourceFormat::Text),
        Some("docx") if allow_text => Ok(SourceFormat::Docx),
        Some("xlsx") if allow_text => Ok(SourceFormat::Xlsx),
        _ if allow_text => Err(CoreError::Invalid(
            "答案资料只支持 JPG、JPEG、PDF、DOCX、XLSX 或 TXT".into(),
        )),
        _ => Err(CoreError::Invalid("学生试卷只支持 JPG、JPEG 或 PDF".into())),
    }
}

fn split_pdf_pages(path: &Path) -> CoreResult<Vec<Vec<u8>>> {
    pdf_pages::split_to_single_page_pdfs(path)
}

fn system_time_ms(value: std::io::Result<std::time::SystemTime>) -> Option<i64> {
    value
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

#[derive(Clone, Copy)]
enum TiffEndian {
    Little,
    Big,
}

fn tiff_u16(bytes: &[u8], offset: usize, endian: TiffEndian) -> Option<u16> {
    let raw = bytes.get(offset..offset + 2)?;
    Some(match endian {
        TiffEndian::Little => u16::from_le_bytes([raw[0], raw[1]]),
        TiffEndian::Big => u16::from_be_bytes([raw[0], raw[1]]),
    })
}

fn tiff_u32(bytes: &[u8], offset: usize, endian: TiffEndian) -> Option<u32> {
    let raw = bytes.get(offset..offset + 4)?;
    Some(match endian {
        TiffEndian::Little => u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]),
        TiffEndian::Big => u32::from_be_bytes([raw[0], raw[1], raw[2], raw[3]]),
    })
}

fn tiff_ifd_entry(
    bytes: &[u8],
    ifd_offset: usize,
    wanted_tag: u16,
    endian: TiffEndian,
) -> Option<(u16, u32, usize)> {
    let count = usize::from(tiff_u16(bytes, ifd_offset, endian)?);
    for index in 0..count {
        let offset = ifd_offset.checked_add(2 + index * 12)?;
        if tiff_u16(bytes, offset, endian)? == wanted_tag {
            return Some((
                tiff_u16(bytes, offset + 2, endian)?,
                tiff_u32(bytes, offset + 4, endian)?,
                offset + 8,
            ));
        }
    }
    None
}

fn tiff_ascii_tag(bytes: &[u8], ifd_offset: usize, tag: u16, endian: TiffEndian) -> Option<String> {
    let (value_type, count, value_offset) = tiff_ifd_entry(bytes, ifd_offset, tag, endian)?;
    if value_type != 2 || count == 0 {
        return None;
    }
    let count = usize::try_from(count).ok()?;
    let start = if count <= 4 {
        value_offset
    } else {
        usize::try_from(tiff_u32(bytes, value_offset, endian)?).ok()?
    };
    let value = std::str::from_utf8(bytes.get(start..start.checked_add(count)?)?)
        .ok()?
        .trim_matches(char::from(0))
        .trim()
        .to_string();
    let raw = value.as_bytes();
    let plausible = raw.len() >= 19
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|index| raw.get(*index).is_some_and(u8::is_ascii_digit))
        && raw.get(4) == Some(&b':')
        && raw.get(7) == Some(&b':')
        && raw.get(10) == Some(&b' ')
        && raw.get(13) == Some(&b':')
        && raw.get(16) == Some(&b':');
    plausible.then_some(value)
}

pub(super) fn exif_capture_time_from_tiff(bytes: &[u8]) -> Option<String> {
    let endian = match bytes.get(0..2)? {
        b"II" => TiffEndian::Little,
        b"MM" => TiffEndian::Big,
        _ => return None,
    };
    if tiff_u16(bytes, 2, endian)? != 42 {
        return None;
    }
    let ifd0 = usize::try_from(tiff_u32(bytes, 4, endian)?).ok()?;
    let original = tiff_ifd_entry(bytes, ifd0, 0x8769, endian)
        .and_then(|(value_type, count, offset)| {
            (value_type == 4 && count == 1)
                .then(|| usize::try_from(tiff_u32(bytes, offset, endian)?).ok())?
        })
        .and_then(|exif_ifd| tiff_ascii_tag(bytes, exif_ifd, 0x9003, endian));
    original.or_else(|| tiff_ascii_tag(bytes, ifd0, 0x0132, endian))
}

fn jpeg_capture_time(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.get(0..2)? != [0xff, 0xd8] {
        return None;
    }
    let mut offset = 2_usize;
    while offset + 4 <= bytes.len() {
        if bytes[offset] != 0xff {
            offset += 1;
            continue;
        }
        let marker = bytes[offset + 1];
        offset += 2;
        if matches!(marker, 0xd8 | 0xd9) || (0xd0..=0xd7).contains(&marker) {
            continue;
        }
        let length = usize::from(u16::from_be_bytes([
            *bytes.get(offset)?,
            *bytes.get(offset + 1)?,
        ]));
        if length < 2 || offset + length > bytes.len() {
            return None;
        }
        let payload = &bytes[offset + 2..offset + length];
        if marker == 0xe1 && payload.starts_with(b"Exif\0\0") {
            return exif_capture_time_from_tiff(&payload[6..]);
        }
        if marker == 0xda {
            break;
        }
        offset += length;
    }
    None
}

pub(super) fn prepare_path(
    path: &str,
    allow_text: bool,
    original_request_index: i64,
) -> CoreResult<PreparedSource> {
    let path = PathBuf::from(path);
    if !path.is_file() {
        return Err(CoreError::Io(format!("文件不存在：{}", path.display())));
    }
    let format = source_format(&path, allow_text)?;
    let original_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("未命名资料")
        .to_string();
    let expected_hash = hashing::sha256_file(&path)?;
    let metadata = std::fs::metadata(&path).map_err(|error| io_error("读取文件时间失败", error))?;
    let capture_time = (format == SourceFormat::Jpeg)
        .then(|| jpeg_capture_time(&path))
        .flatten();
    let pdf_pages = if format == SourceFormat::Pdf {
        split_pdf_pages(&path)?
    } else {
        Vec::new()
    };
    Ok(PreparedSource {
        path: Some(path),
        original_name,
        format,
        bytes: None,
        expected_hash,
        pdf_pages,
        original_request_index,
        capture_time,
        file_created_ms: system_time_ms(metadata.created()),
        file_modified_ms: system_time_ms(metadata.modified()),
    })
}

pub(super) fn prepare_text(text: &str) -> CoreResult<PreparedSource> {
    required(text, "粘贴答案")?;
    let bytes = text.trim().as_bytes().to_vec();
    Ok(PreparedSource {
        path: None,
        original_name: "粘贴答案.txt".into(),
        format: SourceFormat::Text,
        expected_hash: hashing::sha256_hex(&bytes),
        bytes: Some(bytes),
        pdf_pages: Vec::new(),
        original_request_index: 0,
        capture_time: None,
        file_created_ms: None,
        file_modified_ms: None,
    })
}

pub(crate) fn archive_bytes(
    bytes: &[u8],
    hash: &str,
    extension: &str,
    dir: &Path,
) -> CoreResult<ArchivedFile> {
    std::fs::create_dir_all(dir).map_err(|error| io_error("创建试卷归档目录失败", error))?;
    let destination = dir.join(format!("{hash}.{extension}"));
    if destination.is_file() {
        let existing_hash = hashing::sha256_file(&destination)?;
        if existing_hash != hash {
            return Err(CoreError::Invalid("同名归档内容不一致，拒绝覆盖".into()));
        }
        return Ok(ArchivedFile {
            path: destination,
            created: false,
        });
    }
    let temp = dir.join(format!(".{hash}.{}.tmp", std::process::id()));
    let result = (|| -> CoreResult<()> {
        std::fs::write(&temp, bytes).map_err(|error| io_error("写入试卷归档失败", error))?;
        let copied_hash = hashing::sha256_file(&temp)?;
        if copied_hash != hash {
            return Err(CoreError::Invalid("试卷归档 hash 校验失败".into()));
        }
        std::fs::rename(&temp, &destination)
            .map_err(|error| io_error("提交试卷归档失败", error))?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(error);
    }
    Ok(ArchivedFile {
        path: destination,
        created: true,
    })
}

pub(super) fn archive_source(
    source: &PreparedSource,
    dir: &Path,
) -> CoreResult<(ArchivedFile, String, i64)> {
    let bytes = match (&source.path, &source.bytes) {
        (Some(path), None) => {
            std::fs::read(path).map_err(|error| io_error("读取待归档试卷失败", error))?
        }
        (None, Some(bytes)) => bytes.clone(),
        _ => return Err(CoreError::Invalid("试卷资料来源状态非法".into())),
    };
    let hash = hashing::sha256_hex(&bytes);
    if hash != source.expected_hash {
        return Err(CoreError::Invalid(
            "文件在导入期间发生变化，请重新选择后再试".into(),
        ));
    }
    let archived = archive_bytes(&bytes, &hash, source.format.extension(), dir)?;
    Ok((archived, hash, bytes.len() as i64))
}
