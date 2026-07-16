//! 真实学生材料进入 M2 影子试点前的最小数据闸门。
//!
//! 闸门只记录用途、范围和可验证治理事实，不保存学生姓名、班级名称、原图路径或
//! 授权文件正文。真实授权文件继续留在学校/老师控制的受限位置；本清单只保存版本和
//! 不可逆引用。合同夹具永远不能放行真实数据。

use std::collections::BTreeSet;

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

pub const PILOT_DATA_GATE_SCHEMA_VERSION: i64 = 1;
pub const PILOT_MAX_LOCAL_RETENTION_DAYS: i64 = 31;
pub const PILOT_MAX_CLOUD_TTL_SECONDS: i64 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotGateScopeKind {
    ContractFixture,
    RealPilot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotGateState {
    Draft,
    Approved,
    Revoked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotDataType {
    StudentPageImage,
    AnswerRegionImage,
    OcrTranscript,
    MachineSuggestion,
    TeacherDecision,
    StudentIdentityMap,
    DiagnosticBundle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PilotCloudUploadScope {
    AnswerRegion,
    WholePage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotNoticeGate {
    pub notice_version: String,
    pub notice_completed: bool,
    pub required_authorization_completed: bool,
    pub local_processing_disclosed: bool,
    pub cloud_processing_disclosed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotDataRightsGate {
    pub student_export_procedure_version: String,
    pub batch_export_procedure_version: String,
    pub student_delete_procedure_version: String,
    pub batch_delete_procedure_version: String,
    pub deletion_receipt_excludes_content: bool,
    pub dry_run_verified_at: Option<String>,
    pub dry_run_succeeded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotDiagnosticGate {
    pub default_excludes_student_name: bool,
    pub default_excludes_raw_image: bool,
    pub default_excludes_ocr_text: bool,
    pub default_excludes_answer_text: bool,
    pub explicit_opt_in_required_for_raw_evidence: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotCloudProcessorGate {
    /// 供应商内部代号或不可逆引用，不得写 token、账号或学生身份。
    pub provider_ref: String,
    pub purpose: String,
    pub upload_scope: PilotCloudUploadScope,
    pub object_key_uses_internal_id: bool,
    pub personal_identifiers_removed_before_upload: bool,
    pub ttl_seconds: i64,
    pub delete_on_success: bool,
    pub delete_on_failure: bool,
    pub delete_on_cancel: bool,
    pub logs_exclude_content: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotDataGateManifest {
    pub schema_version: i64,
    pub gate_id: String,
    pub scope_kind: PilotGateScopeKind,
    pub state: PilotGateState,
    pub purpose: String,
    pub responsible_party_ref_sha256: String,
    pub class_scope_sha256: String,
    pub starts_on: String,
    pub ends_on: String,
    pub allowed_data_types: Vec<PilotDataType>,
    pub local_retention_days: i64,
    pub retention_policy_version: String,
    pub notice: PilotNoticeGate,
    pub rights: PilotDataRightsGate,
    pub diagnostics: PilotDiagnosticGate,
    pub cloud_processors: Vec<PilotCloudProcessorGate>,
    pub approved_by_ref_sha256: Option<String>,
    pub approved_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PilotDataGateReport {
    pub schema_version: i64,
    pub gate_id: String,
    pub policy_sha256: String,
    pub evaluated_on: String,
    pub real_data_allowed: bool,
    pub denial_reasons: Vec<String>,
}

fn normalized_sha256(value: &str, field: &str) -> CoreResult<String> {
    let normalized = value.trim().to_ascii_lowercase();
    if normalized.len() != 64 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是 64 位十六进制 sha256"
        )));
    }
    Ok(normalized)
}

fn required_text(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::Invalid(format!("{field} 不能为空")));
    }
    Ok(())
}

fn parse_date(value: &str, field: &str) -> CoreResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid(format!("{field} 必须是 YYYY-MM-DD")))
}

fn parse_rfc3339(value: &str, field: &str) -> CoreResult<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value.trim())
        .map_err(|_| CoreError::Invalid(format!("{field} 必须是 RFC3339")))
}

impl PilotDataGateManifest {
    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != PILOT_DATA_GATE_SCHEMA_VERSION {
            return Err(CoreError::Invalid("试点数据闸门 schema 版本不支持".into()));
        }
        required_text(&self.gate_id, "gate_id")?;
        required_text(&self.purpose, "试点用途")?;
        normalized_sha256(&self.responsible_party_ref_sha256, "责任人引用")?;
        normalized_sha256(&self.class_scope_sha256, "班级范围引用")?;
        required_text(&self.retention_policy_version, "本地保留策略版本")?;
        required_text(&self.notice.notice_version, "告知版本")?;

        let starts_on = parse_date(&self.starts_on, "试点开始日期")?;
        let ends_on = parse_date(&self.ends_on, "试点结束日期")?;
        if ends_on < starts_on {
            return Err(CoreError::Invalid("试点结束日期不能早于开始日期".into()));
        }
        if self.local_retention_days <= 0 {
            return Err(CoreError::Invalid("本地保留天数必须为正数".into()));
        }
        if self.allowed_data_types.is_empty() {
            return Err(CoreError::Invalid("试点至少声明一种允许的数据类型".into()));
        }
        let mut data_types = BTreeSet::new();
        if self
            .allowed_data_types
            .iter()
            .any(|data_type| !data_types.insert(*data_type))
        {
            return Err(CoreError::Invalid("允许的数据类型不能重复".into()));
        }

        for (field, value) in [
            (
                "按学生导出流程版本",
                self.rights.student_export_procedure_version.as_str(),
            ),
            (
                "按批次导出流程版本",
                self.rights.batch_export_procedure_version.as_str(),
            ),
            (
                "按学生删除流程版本",
                self.rights.student_delete_procedure_version.as_str(),
            ),
            (
                "按批次删除流程版本",
                self.rights.batch_delete_procedure_version.as_str(),
            ),
        ] {
            required_text(value, field)?;
        }
        if let Some(verified_at) = self.rights.dry_run_verified_at.as_deref() {
            parse_rfc3339(verified_at, "导出/删除演练时间")?;
        }

        let mut provider_refs = BTreeSet::new();
        for processor in &self.cloud_processors {
            required_text(&processor.provider_ref, "云处理供应商引用")?;
            required_text(&processor.purpose, "云处理用途")?;
            if !provider_refs.insert(processor.provider_ref.trim().to_owned()) {
                return Err(CoreError::Invalid("云处理供应商引用不能重复".into()));
            }
            if processor.ttl_seconds <= 0 {
                return Err(CoreError::Invalid("云端对象 TTL 必须为正数".into()));
            }
        }

        match self.state {
            PilotGateState::Approved => {
                normalized_sha256(
                    self.approved_by_ref_sha256.as_deref().ok_or_else(|| {
                        CoreError::Invalid("approved 闸门必须记录审批人引用".into())
                    })?,
                    "审批人引用",
                )?;
                parse_rfc3339(
                    self.approved_at.as_deref().ok_or_else(|| {
                        CoreError::Invalid("approved 闸门必须记录审批时间".into())
                    })?,
                    "审批时间",
                )?;
            }
            PilotGateState::Draft | PilotGateState::Revoked => {}
        }
        Ok(())
    }

    pub fn policy_sha256(&self) -> CoreResult<String> {
        self.validate()?;
        let bytes = serde_json::to_vec(self)
            .map_err(|error| CoreError::Config(format!("闸门序列化失败：{error}")))?;
        Ok(hashing::sha256_hex(&bytes))
    }

    pub fn evaluate(&self, evaluated_on: &str) -> CoreResult<PilotDataGateReport> {
        self.validate()?;
        let evaluated_date = parse_date(evaluated_on, "闸门评估日期")?;
        let starts_on = parse_date(&self.starts_on, "试点开始日期")?;
        let ends_on = parse_date(&self.ends_on, "试点结束日期")?;
        let mut reasons = BTreeSet::new();

        if self.scope_kind != PilotGateScopeKind::RealPilot {
            reasons.insert("CONTRACT_FIXTURE_ONLY".to_owned());
        }
        match self.state {
            PilotGateState::Approved => {
                let approved_at = parse_rfc3339(
                    self.approved_at.as_deref().ok_or_else(|| {
                        CoreError::Invalid("approved 闸门必须记录审批时间".into())
                    })?,
                    "审批时间",
                )?;
                if approved_at.date_naive() > evaluated_date {
                    reasons.insert("GATE_APPROVAL_IN_FUTURE".to_owned());
                }
            }
            PilotGateState::Draft => {
                reasons.insert("GATE_NOT_APPROVED".to_owned());
            }
            PilotGateState::Revoked => {
                reasons.insert("GATE_REVOKED".to_owned());
            }
        }
        if evaluated_date < starts_on {
            reasons.insert("GATE_NOT_STARTED".to_owned());
        }
        if evaluated_date > ends_on {
            reasons.insert("GATE_EXPIRED".to_owned());
        }
        if self.local_retention_days > PILOT_MAX_LOCAL_RETENTION_DAYS {
            reasons.insert("LOCAL_RETENTION_EXCEEDS_PILOT_LIMIT".to_owned());
        }
        if !self.notice.notice_completed
            || !self.notice.local_processing_disclosed
            || !self.notice.cloud_processing_disclosed
        {
            reasons.insert("NOTICE_INCOMPLETE".to_owned());
        }
        if !self.notice.required_authorization_completed {
            reasons.insert("AUTHORIZATION_INCOMPLETE".to_owned());
        }
        let dry_run_verified = match self.rights.dry_run_verified_at.as_deref() {
            Some(value) => {
                parse_rfc3339(value, "导出/删除演练时间")?.date_naive() <= evaluated_date
            }
            None => false,
        };
        if !self.rights.dry_run_succeeded
            || !dry_run_verified
            || !self.rights.deletion_receipt_excludes_content
        {
            reasons.insert("DATA_RIGHTS_NOT_VERIFIED".to_owned());
        }
        if !self.diagnostics.default_excludes_student_name
            || !self.diagnostics.default_excludes_raw_image
            || !self.diagnostics.default_excludes_ocr_text
            || !self.diagnostics.default_excludes_answer_text
            || !self.diagnostics.explicit_opt_in_required_for_raw_evidence
        {
            reasons.insert("DIAGNOSTIC_DEFAULTS_UNSAFE".to_owned());
        }
        for processor in &self.cloud_processors {
            if processor.ttl_seconds > PILOT_MAX_CLOUD_TTL_SECONDS
                || !processor.object_key_uses_internal_id
                || !processor.personal_identifiers_removed_before_upload
                || !processor.delete_on_success
                || !processor.delete_on_failure
                || !processor.delete_on_cancel
                || !processor.logs_exclude_content
            {
                reasons.insert(format!(
                    "CLOUD_PROCESSOR_POLICY_UNSAFE:{}",
                    processor.provider_ref.trim()
                ));
            }
        }

        let denial_reasons = reasons.into_iter().collect::<Vec<_>>();
        Ok(PilotDataGateReport {
            schema_version: PILOT_DATA_GATE_SCHEMA_VERSION,
            gate_id: self.gate_id.clone(),
            policy_sha256: self.policy_sha256()?,
            evaluated_on: evaluated_date.format("%Y-%m-%d").to_string(),
            real_data_allowed: denial_reasons.is_empty(),
            denial_reasons,
        })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn approved_real_gate() -> PilotDataGateManifest {
        PilotDataGateManifest {
            schema_version: 1,
            gate_id: "pilot-gate-opaque-001".into(),
            scope_kind: PilotGateScopeKind::RealPilot,
            state: PilotGateState::Approved,
            purpose: "固定版式批改影子试点，只比较机器建议与老师结果".into(),
            responsible_party_ref_sha256: "a".repeat(64),
            class_scope_sha256: "b".repeat(64),
            starts_on: "2026-07-01".into(),
            ends_on: "2026-07-31".into(),
            allowed_data_types: vec![
                PilotDataType::StudentPageImage,
                PilotDataType::AnswerRegionImage,
                PilotDataType::OcrTranscript,
                PilotDataType::MachineSuggestion,
                PilotDataType::TeacherDecision,
            ],
            local_retention_days: 30,
            retention_policy_version: "pilot-retention-v1".into(),
            notice: PilotNoticeGate {
                notice_version: "pilot-notice-v1".into(),
                notice_completed: true,
                required_authorization_completed: true,
                local_processing_disclosed: true,
                cloud_processing_disclosed: true,
            },
            rights: PilotDataRightsGate {
                student_export_procedure_version: "student-export-v1".into(),
                batch_export_procedure_version: "batch-export-v1".into(),
                student_delete_procedure_version: "student-delete-v1".into(),
                batch_delete_procedure_version: "batch-delete-v1".into(),
                deletion_receipt_excludes_content: true,
                dry_run_verified_at: Some("2026-06-30T08:00:00Z".into()),
                dry_run_succeeded: true,
            },
            diagnostics: PilotDiagnosticGate {
                default_excludes_student_name: true,
                default_excludes_raw_image: true,
                default_excludes_ocr_text: true,
                default_excludes_answer_text: true,
                explicit_opt_in_required_for_raw_evidence: true,
            },
            cloud_processors: vec![PilotCloudProcessorGate {
                provider_ref: "managed-ocr-provider".into(),
                purpose: "单页或题区 OCR".into(),
                upload_scope: PilotCloudUploadScope::AnswerRegion,
                object_key_uses_internal_id: true,
                personal_identifiers_removed_before_upload: true,
                ttl_seconds: 3_600,
                delete_on_success: true,
                delete_on_failure: true,
                delete_on_cancel: true,
                logs_exclude_content: true,
            }],
            approved_by_ref_sha256: Some("c".repeat(64)),
            approved_at: Some("2026-06-30T09:00:00Z".into()),
        }
    }

    #[test]
    fn complete_real_gate_allows_only_its_active_window() {
        let gate = approved_real_gate();
        let report = gate.evaluate("2026-07-15").unwrap();
        assert!(report.real_data_allowed);
        assert!(report.denial_reasons.is_empty());
        assert_eq!(report.policy_sha256, gate.policy_sha256().unwrap());

        let expired = gate.evaluate("2026-08-01").unwrap();
        assert!(!expired.real_data_allowed);
        assert_eq!(expired.denial_reasons, vec!["GATE_EXPIRED"]);
    }

    #[test]
    fn contract_fixture_and_draft_can_never_allow_real_student_data() {
        let mut gate = approved_real_gate();
        gate.scope_kind = PilotGateScopeKind::ContractFixture;
        gate.state = PilotGateState::Draft;
        gate.approved_by_ref_sha256 = None;
        gate.approved_at = None;
        let report = gate.evaluate("2026-07-15").unwrap();
        assert!(!report.real_data_allowed);
        assert_eq!(
            report.denial_reasons,
            vec!["CONTRACT_FIXTURE_ONLY", "GATE_NOT_APPROVED"]
        );
    }

    #[test]
    fn unsafe_cloud_and_unverified_rights_fail_closed_with_specific_reasons() {
        let mut gate = approved_real_gate();
        gate.rights.dry_run_succeeded = false;
        gate.cloud_processors[0].ttl_seconds = PILOT_MAX_CLOUD_TTL_SECONDS + 1;
        gate.cloud_processors[0].delete_on_failure = false;
        let report = gate.evaluate("2026-07-15").unwrap();
        assert!(!report.real_data_allowed);
        assert_eq!(
            report.denial_reasons,
            vec![
                "CLOUD_PROCESSOR_POLICY_UNSAFE:managed-ocr-provider",
                "DATA_RIGHTS_NOT_VERIFIED"
            ]
        );
    }

    #[test]
    fn future_approval_or_rights_dry_run_cannot_unlock_an_earlier_date() {
        let mut gate = approved_real_gate();
        gate.approved_at = Some("2026-07-20T09:00:00Z".into());
        gate.rights.dry_run_verified_at = Some("2026-07-21T08:00:00Z".into());
        let report = gate.evaluate("2026-07-15").unwrap();
        assert!(!report.real_data_allowed);
        assert_eq!(
            report.denial_reasons,
            vec!["DATA_RIGHTS_NOT_VERIFIED", "GATE_APPROVAL_IN_FUTURE"]
        );
    }

    #[test]
    fn malformed_or_duplicate_scope_metadata_is_rejected() {
        let mut gate = approved_real_gate();
        gate.class_scope_sha256 = "not-a-hash".into();
        assert!(gate.validate().is_err());

        let mut gate = approved_real_gate();
        gate.allowed_data_types.push(PilotDataType::OcrTranscript);
        assert!(gate.validate().is_err());
    }

    #[test]
    fn repository_contract_template_is_valid_but_fails_closed() {
        let gate: PilotDataGateManifest = serde_json::from_str(include_str!(
            "../tests/fixtures/material_golden/pilot_data_gate_contract_template_v1.json"
        ))
        .unwrap();
        gate.validate().unwrap();
        let report = gate.evaluate("2026-07-15").unwrap();
        assert!(!report.real_data_allowed);
        assert!(report
            .denial_reasons
            .contains(&"CONTRACT_FIXTURE_ONLY".to_owned()));
        assert!(report
            .denial_reasons
            .contains(&"GATE_NOT_APPROVED".to_owned()));
        assert!(report
            .denial_reasons
            .contains(&"DATA_RIGHTS_NOT_VERIFIED".to_owned()));
    }
}
