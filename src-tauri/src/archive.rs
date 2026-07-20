//! 录音归档：按 sha256 复制到 app-managed 目录，原文件保留。

use std::path::{Path, PathBuf};

use suite_core::error::{CoreError, CoreResult};

pub struct ArchivedFile {
    pub path: PathBuf,
    created: bool,
}

impl ArchivedFile {
    /// 仅用于后续数据库事务失败时补偿本次新建的归档；复用文件绝不删除。
    pub fn rollback_new_file(&self) {
        if self.created {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn io_error(context: &str, err: std::io::Error) -> CoreError {
    CoreError::Io(format!("{context}: {err}"))
}

fn safe_extension(source: &Path) -> &'static str {
    match source
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("wav") => "wav",
        Some("mp3") => "mp3",
        Some("aac") => "aac",
        Some("flac") => "flac",
        Some("ogg") => "ogg",
        _ => "m4a",
    }
}

pub fn archive_audio(source: &Path, expected_hash: &str, dir: &Path) -> CoreResult<ArchivedFile> {
    if !source.is_file() {
        return Err(CoreError::Io(format!("原录音不存在: {}", source.display())));
    }
    let actual_hash = suite_core::domain::hashing::sha256_file(source)?;
    if actual_hash != expected_hash {
        return Err(CoreError::Invalid(
            "归档前录音 hash 已变化，请重新导入".into(),
        ));
    }
    std::fs::create_dir_all(dir).map_err(|err| io_error("创建录音归档目录失败", err))?;
    let existing_prefix = format!("{expected_hash}.");
    for entry in std::fs::read_dir(dir).map_err(|err| io_error("读取录音归档目录失败", err))?
    {
        let entry = entry.map_err(|err| io_error("读取录音归档条目失败", err))?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with(&existing_prefix) && entry.path().is_file() {
            let archived_hash = suite_core::domain::hashing::sha256_file(&entry.path())?;
            if archived_hash != expected_hash {
                return Err(CoreError::Invalid(
                    "同 hash 归档文件内容不一致，拒绝覆盖".into(),
                ));
            }
            return Ok(ArchivedFile {
                path: entry.path(),
                created: false,
            });
        }
    }
    let destination = dir.join(format!("{expected_hash}.{}", safe_extension(source)));

    let temp = dir.join(format!(".{expected_hash}.{}.tmp", std::process::id()));
    let copied = (|| -> CoreResult<()> {
        std::fs::copy(source, &temp).map_err(|err| io_error("复制录音到归档失败", err))?;
        let copied_hash = suite_core::domain::hashing::sha256_file(&temp)?;
        if copied_hash != expected_hash {
            return Err(CoreError::Invalid("归档副本 hash 校验失败".into()));
        }
        std::fs::rename(&temp, &destination).map_err(|err| io_error("提交录音归档失败", err))?;
        Ok(())
    })();
    if let Err(err) = copied {
        let _ = std::fs::remove_file(&temp);
        return Err(err);
    }
    Ok(ArchivedFile {
        path: destination,
        created: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archives_by_hash_reuses_copy_and_survives_source_deletion() {
        let root = std::env::temp_dir().join(format!("jiaofu-archive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("original.wav");
        std::fs::write(&source, b"audio evidence").unwrap();
        let hash = suite_core::domain::hashing::sha256_file(&source).unwrap();
        let archive_dir = root.join("archive");

        let first = archive_audio(&source, &hash, &archive_dir).unwrap();
        let same_content_other_extension = root.join("same.mp3");
        std::fs::write(&same_content_other_extension, b"audio evidence").unwrap();
        let second = archive_audio(&same_content_other_extension, &hash, &archive_dir).unwrap();
        assert_eq!(first.path, second.path);
        std::fs::remove_file(&source).unwrap();
        assert_eq!(std::fs::read(first.path).unwrap(), b"audio evidence");
    }

    #[test]
    fn copy_failure_never_creates_archive_record() {
        let root = std::env::temp_dir().join(format!("jiaofu-archive-fail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let missing = root.join("missing.m4a");
        assert!(archive_audio(&missing, "deadbeef", &root.join("archive")).is_err());
        assert!(!root.join("archive").exists());
    }

    #[test]
    fn new_archive_can_be_compensated_but_reused_archive_is_preserved() {
        let root =
            std::env::temp_dir().join(format!("jiaofu-archive-rollback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let source = root.join("original.m4a");
        std::fs::write(&source, b"audio evidence").unwrap();
        let hash = suite_core::domain::hashing::sha256_file(&source).unwrap();
        let archive_dir = root.join("archive");

        let created = archive_audio(&source, &hash, &archive_dir).unwrap();
        let path = created.path.clone();
        created.rollback_new_file();
        assert!(!path.exists());

        let first = archive_audio(&source, &hash, &archive_dir).unwrap();
        let reused = archive_audio(&source, &hash, &archive_dir).unwrap();
        reused.rollback_new_file();
        assert!(first.path.exists());
    }
}
