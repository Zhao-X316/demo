//! `artifacts` 仓储：跨模块不可变资产元数据与业务引用。

use std::io;

use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::ids;
use crate::error::{CoreError, CoreResult};
use crate::models::{ArchiveStatus, Artifact, ArtifactKind, PrivacyClass};

const COLS: &str = "id, public_id, kind, sha256, mime_type, byte_size, original_name, \
                    original_path, archived_path, parent_artifact_id, derivative_type, \
                    processing_version, privacy_class, archive_status, created_at";

/// 新资产入参。原始资产不带 parent/derivative；派生资产必须同时提供两者。
#[derive(Clone, Copy)]
pub struct NewArtifact<'a> {
    pub kind: ArtifactKind,
    pub sha256: &'a str,
    pub mime_type: &'a str,
    pub byte_size: i64,
    pub original_name: Option<&'a str>,
    pub original_path: Option<&'a str>,
    pub archived_path: &'a str,
    pub parent_artifact_id: Option<i64>,
    pub derivative_type: Option<&'a str>,
    pub processing_version: &'a str,
    pub privacy_class: PrivacyClass,
    pub archive_status: ArchiveStatus,
}

fn invalid_enum(index: usize, field: &str, value: String) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        Type::Text,
        Box::new(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid {field}: {value}"),
        )),
    )
}

fn row_to_artifact(row: &rusqlite::Row<'_>) -> rusqlite::Result<Artifact> {
    let kind_raw: String = row.get(2)?;
    let privacy_raw: String = row.get(12)?;
    let status_raw: String = row.get(13)?;
    Ok(Artifact {
        id: row.get(0)?,
        public_id: row.get(1)?,
        kind: ArtifactKind::from_db(&kind_raw)
            .ok_or_else(|| invalid_enum(2, "artifact kind", kind_raw))?,
        sha256: row.get(3)?,
        mime_type: row.get(4)?,
        byte_size: row.get(5)?,
        original_name: row.get(6)?,
        original_path: row.get(7)?,
        archived_path: row.get(8)?,
        parent_artifact_id: row.get(9)?,
        derivative_type: row.get(10)?,
        processing_version: row.get(11)?,
        privacy_class: PrivacyClass::from_db(&privacy_raw)
            .ok_or_else(|| invalid_enum(12, "privacy class", privacy_raw))?,
        archive_status: ArchiveStatus::from_db(&status_raw)
            .ok_or_else(|| invalid_enum(13, "archive status", status_raw))?,
        created_at: row.get(14)?,
    })
}

fn normalize_and_validate(input: &NewArtifact<'_>) -> CoreResult<String> {
    let sha256 = input.sha256.trim().to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(
            "artifact sha256 must be 64 hexadecimal characters".into(),
        ));
    }
    if input.mime_type.trim().is_empty() {
        return Err(CoreError::Invalid("artifact mime_type is required".into()));
    }
    if input.byte_size < 0 {
        return Err(CoreError::Invalid(
            "artifact byte_size cannot be negative".into(),
        ));
    }
    if input.archived_path.trim().is_empty() {
        return Err(CoreError::Invalid(
            "artifact archived_path is required".into(),
        ));
    }
    if input.processing_version.trim().is_empty() {
        return Err(CoreError::Invalid(
            "artifact processing_version is required".into(),
        ));
    }
    let derivative_present = input
        .derivative_type
        .is_some_and(|value| !value.trim().is_empty());
    if input.parent_artifact_id.is_some() != derivative_present {
        return Err(CoreError::Invalid(
            "derived artifact requires both parent_artifact_id and derivative_type".into(),
        ));
    }
    Ok(sha256)
}

/// 按内容身份创建资产；完全相同的内容身份返回既有行，不覆盖其来源元数据。
pub fn create_or_get(conn: &Connection, input: &NewArtifact<'_>) -> CoreResult<Artifact> {
    let sha256 = normalize_and_validate(input)?;
    let public_id = ids::new_public_id();
    let derivative_type = input.derivative_type.map(str::trim);
    let processing_version = input.processing_version.trim();
    conn.execute(
        "INSERT INTO artifacts
            (public_id, kind, sha256, mime_type, byte_size, original_name, original_path,
             archived_path, parent_artifact_id, derivative_type, processing_version,
             privacy_class, archive_status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT DO NOTHING",
        params![
            public_id,
            input.kind.as_str(),
            sha256,
            input.mime_type.trim(),
            input.byte_size,
            input.original_name,
            input.original_path,
            input.archived_path,
            input.parent_artifact_id,
            derivative_type,
            processing_version,
            input.privacy_class.as_str(),
            input.archive_status.as_str(),
        ],
    )?;

    get_by_identity(
        conn,
        &sha256,
        input.kind,
        derivative_type,
        processing_version,
    )?
    .ok_or_else(|| CoreError::Db("artifact insert did not produce an identity row".into()))
}

/// 按幂等内容身份查询。
pub fn get_by_identity(
    conn: &Connection,
    sha256: &str,
    kind: ArtifactKind,
    derivative_type: Option<&str>,
    processing_version: &str,
) -> CoreResult<Option<Artifact>> {
    let sql = format!(
        "SELECT {COLS} FROM artifacts
         WHERE sha256=?1 AND kind=?2
           AND COALESCE(derivative_type, '')=COALESCE(?3, '')
           AND processing_version=?4"
    );
    let artifact = conn
        .query_row(
            &sql,
            params![
                sha256.trim().to_ascii_lowercase(),
                kind.as_str(),
                derivative_type,
                processing_version
            ],
            row_to_artifact,
        )
        .optional()?;
    Ok(artifact)
}

pub fn get_by_id(conn: &Connection, id: i64) -> CoreResult<Option<Artifact>> {
    let sql = format!("SELECT {COLS} FROM artifacts WHERE id=?1");
    Ok(conn.query_row(&sql, [id], row_to_artifact).optional()?)
}

pub fn get_by_public_id(conn: &Connection, public_id: &str) -> CoreResult<Option<Artifact>> {
    let sql = format!("SELECT {COLS} FROM artifacts WHERE public_id=?1");
    Ok(conn
        .query_row(&sql, [public_id], row_to_artifact)
        .optional()?)
}

pub fn list_children(conn: &Connection, parent_artifact_id: i64) -> CoreResult<Vec<Artifact>> {
    let sql = format!("SELECT {COLS} FROM artifacts WHERE parent_artifact_id=?1 ORDER BY id");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([parent_artifact_id], row_to_artifact)?;
    let mut artifacts = Vec::new();
    for row in rows {
        artifacts.push(row?);
    }
    Ok(artifacts)
}

/// 只允许更新归档可用性；内容身份和来源元数据保持不可变。
pub fn set_archive_status(
    conn: &Connection,
    id: i64,
    status: ArchiveStatus,
) -> CoreResult<Artifact> {
    let changed = conn.execute(
        "UPDATE artifacts SET archive_status=?2 WHERE id=?1",
        params![id, status.as_str()],
    )?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!("artifact {id}")));
    }
    get_by_id(conn, id)?.ok_or_else(|| CoreError::NotFound(format!("artifact {id}")))
}

/// 将现有通用提交绑定到资产。旧路径列继续保留，兼容期不做猜测式回填。
pub fn attach_to_submission(
    conn: &Connection,
    submission_id: i64,
    artifact_id: i64,
) -> CoreResult<()> {
    let changed = conn.execute(
        "UPDATE submissions SET artifact_id=?2, updated_at=datetime('now') WHERE id=?1",
        params![submission_id, artifact_id],
    )?;
    if changed == 0 {
        return Err(CoreError::NotFound(format!("submission {submission_id}")));
    }
    Ok(())
}

pub fn get_for_submission(conn: &Connection, submission_id: i64) -> CoreResult<Option<Artifact>> {
    let sql = format!(
        "SELECT {COLS} FROM artifacts
         WHERE id=(SELECT artifact_id FROM submissions WHERE id=?1)"
    );
    Ok(conn
        .query_row(&sql, [submission_id], row_to_artifact)
        .optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{open_in_memory, run_migrations, CORE_MIGRATIONS};

    const AUDIO_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const CROP_HASH: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn source_input() -> NewArtifact<'static> {
        NewArtifact {
            kind: ArtifactKind::Audio,
            sha256: AUDIO_HASH,
            mime_type: "audio/wav",
            byte_size: 123,
            original_name: Some("student.wav"),
            original_path: Some("/incoming/student.wav"),
            archived_path: "/archive/audio/aa/source.wav",
            parent_artifact_id: None,
            derivative_type: None,
            processing_version: "source-v1",
            privacy_class: PrivacyClass::StudentSensitive,
            archive_status: ArchiveStatus::Ready,
        }
    }

    #[test]
    fn create_is_idempotent_and_preserves_original_metadata() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();

        let first = create_or_get(&conn, &source_input()).unwrap();
        let second = create_or_get(
            &conn,
            &NewArtifact {
                original_name: Some("renamed.wav"),
                original_path: Some("/other/renamed.wav"),
                ..source_input()
            },
        )
        .unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(second.original_name.as_deref(), Some("student.wav"));
        assert_eq!(
            uuid::Uuid::parse_str(&first.public_id)
                .unwrap()
                .get_version_num(),
            7
        );
        let count: i64 = conn
            .query_row("SELECT count(*) FROM artifacts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn derivative_requires_parent_and_parent_is_restricted() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let parent = create_or_get(&conn, &source_input()).unwrap();
        let child = create_or_get(
            &conn,
            &NewArtifact {
                kind: ArtifactKind::Crop,
                sha256: CROP_HASH,
                mime_type: "image/png",
                byte_size: 45,
                original_name: None,
                original_path: None,
                archived_path: "/archive/crop/bb/crop.png",
                parent_artifact_id: Some(parent.id),
                derivative_type: Some("answer_crop"),
                processing_version: "crop-v1",
                privacy_class: PrivacyClass::StudentSensitive,
                archive_status: ArchiveStatus::Ready,
            },
        )
        .unwrap();

        assert_eq!(list_children(&conn, parent.id).unwrap(), vec![child]);
        assert!(conn
            .execute("DELETE FROM artifacts WHERE id=?1", [parent.id])
            .is_err());
        assert!(create_or_get(
            &conn,
            &NewArtifact {
                derivative_type: Some("answer_crop"),
                ..source_input()
            }
        )
        .is_err());
        assert!(create_or_get(
            &conn,
            &NewArtifact {
                kind: ArtifactKind::Crop,
                sha256: "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc",
                mime_type: "image/png",
                archived_path: "/archive/crop/cc/missing-parent.png",
                parent_artifact_id: Some(i64::MAX),
                derivative_type: Some("answer_crop"),
                processing_version: "crop-v1",
                ..source_input()
            }
        )
        .is_err());
    }

    #[test]
    fn invalid_content_identity_is_rejected_before_insert() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();

        assert!(create_or_get(
            &conn,
            &NewArtifact {
                sha256: "not-a-sha256",
                ..source_input()
            }
        )
        .is_err());
        let count: i64 = conn
            .query_row("SELECT count(*) FROM artifacts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn submission_reference_uses_foreign_key() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let artifact = create_or_get(&conn, &source_input()).unwrap();
        conn.execute(
            "INSERT INTO submissions (module, media_type, file_path, file_hash)
             VALUES ('recitation', 'audio', '/incoming/student.wav', 'submission-hash')",
            [],
        )
        .unwrap();
        let submission_id = conn.last_insert_rowid();

        attach_to_submission(&conn, submission_id, artifact.id).unwrap();
        assert_eq!(
            get_for_submission(&conn, submission_id)
                .unwrap()
                .unwrap()
                .id,
            artifact.id
        );
        assert!(attach_to_submission(&conn, submission_id, i64::MAX).is_err());
    }

    #[test]
    fn archive_status_changes_without_mutating_identity() {
        let conn = open_in_memory().unwrap();
        run_migrations(&conn, CORE_MIGRATIONS).unwrap();
        let before = create_or_get(&conn, &source_input()).unwrap();
        let after = set_archive_status(&conn, before.id, ArchiveStatus::Missing).unwrap();

        assert_eq!(after.archive_status, ArchiveStatus::Missing);
        assert_eq!(after.public_id, before.public_id);
        assert_eq!(after.sha256, before.sha256);
        assert_eq!(after.archived_path, before.archived_path);
    }
}
