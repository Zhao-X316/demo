//! 背诵黄金集的脱敏清单、真实数据授权闸门与只读离线评估器。
//!
//! 仓库内只允许保存合成合同清单。真实录音、转写正文、学生身份和本机路径不得进入
//! 清单或报告；真实集还必须提供仍在有效期内、内容 hash 未漂移的独立授权清单。

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use suite_core::domain::hashing;
use suite_core::error::{CoreError, CoreResult};

pub const RECITATION_GOLDEN_SCHEMA_VERSION: i64 = 1;

const REQUIRED_ISSUE_TAGS: &[&str] = &[
    "paraphrase_correct",
    "slow_correct",
    "fluent_fact_error",
    "cause_effect_reversed",
    "stage_order_reversed",
    "minor_omission",
    "major_omission",
    "self_corrected",
    "dialect",
    "noise",
    "multiple_speakers",
    "low_volume",
    "truncated",
    "proper_noun_asr_error",
    "version_change",
    "wrong_student",
    "wrong_task",
    "duplicate_file",
    "abnormal_reassignment",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationGoldenSourceKind {
    SyntheticContract,
    RealDeidentifiedAudio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationGoldenStorageScope {
    RepositorySynthetic,
    LocalRestricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationGoldenAsrState {
    Ok,
    Uncertain,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationGoldenSuggestion {
    Pass,
    Fail,
    ReviewRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationGoldenTeacherOutcome {
    Pass,
    Fail,
    NotDecidable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationGoldenReviewGate {
    Ready,
    ReviewRequired,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationGoldenPointState {
    Covered,
    Partial,
    Missing,
    Contradicted,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationGoldenGovernance {
    pub storage_scope: RecitationGoldenStorageScope,
    pub personal_identifiers_removed: bool,
    pub privacy_reviewed: bool,
    pub privacy_reviewed_by_ref_sha256: String,
    pub retention_policy_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pilot_gate_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pilot_gate_policy_sha256: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationGoldenPointExpectation {
    pub point_key: String,
    pub state: RecitationGoldenPointState,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationGoldenSuspicionSpan {
    pub issue_tag: String,
    pub start_ms: u64,
    pub end_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationGoldenCase {
    pub case_id: String,
    pub source_kind: RecitationGoldenSourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_artifact_sha256: Option<String>,
    pub answer_version_sha256: String,
    pub rubric_version_sha256: String,
    pub scoring_rule_version_sha256: String,
    pub teacher_annotation_revision: i64,
    pub annotated_by_ref_sha256: String,
    pub license_or_consent_ref_sha256: String,
    pub expected_asr_state: RecitationGoldenAsrState,
    pub expected_machine_suggestion: RecitationGoldenSuggestion,
    pub expected_teacher_outcome: RecitationGoldenTeacherOutcome,
    pub expected_review_gate: RecitationGoldenReviewGate,
    pub expected_point_states: Vec<RecitationGoldenPointExpectation>,
    pub expected_suspicion_spans: Vec<RecitationGoldenSuspicionSpan>,
    pub issue_tags: Vec<String>,
    pub requires_full_playback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationGoldenManifest {
    pub schema_version: i64,
    pub dataset_id: String,
    pub dataset_version: String,
    pub production_accuracy_claim_allowed: bool,
    pub governance: RecitationGoldenGovernance,
    pub cases: Vec<RecitationGoldenCase>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationGoldenAuthorizationState {
    Draft,
    Approved,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationGoldenAuthorization {
    pub schema_version: i64,
    pub gate_id: String,
    pub dataset_id: String,
    pub dataset_version: String,
    pub state: RecitationGoldenAuthorizationState,
    pub allowed_use: String,
    pub valid_from: String,
    pub valid_through: String,
    pub authorization_ref_sha256: String,
    pub privacy_review_ref_sha256: String,
    pub data_rights_drill_ref_sha256: String,
    pub approved_by_ref_sha256: String,
    pub approved_at: String,
    pub raw_audio_local_only: bool,
    pub cloud_processing_disclosed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationGoldenPrediction {
    pub case_id: String,
    pub observed_asr_state: RecitationGoldenAsrState,
    pub predicted_machine_suggestion: RecitationGoldenSuggestion,
    pub predicted_review_gate: RecitationGoldenReviewGate,
    #[serde(default)]
    pub predicted_requires_full_playback: bool,
    pub predicted_point_states: Vec<RecitationGoldenPointExpectation>,
    pub predicted_suspicion_spans: Vec<RecitationGoldenSuspicionSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationGoldenPredictionSet {
    pub schema_version: i64,
    pub dataset_id: String,
    pub dataset_version: String,
    pub predictions: Vec<RecitationGoldenPrediction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationGoldenReport {
    pub schema_version: i64,
    pub dataset_id: String,
    pub dataset_version: String,
    pub case_count: usize,
    pub asr_state_match_count: usize,
    pub machine_suggestion_match_count: usize,
    pub review_gate_match_count: usize,
    pub dangerous_false_pass_count: usize,
    pub conservative_false_fail_count: usize,
    pub unsafe_ready_count: usize,
    pub ready_routed_to_review_count: usize,
    pub missing_prediction_count: usize,
    pub unexpected_prediction_count: usize,
    pub expected_point_count: usize,
    pub point_state_match_count: usize,
    pub missing_point_count: usize,
    pub unexpected_point_count: usize,
    pub expected_suspicion_span_count: usize,
    pub suspicion_span_match_count: usize,
    pub missing_suspicion_span_count: usize,
    pub unexpected_suspicion_span_count: usize,
    pub expected_full_playback_count: usize,
    pub full_playback_match_count: usize,
    pub missed_required_full_playback_count: usize,
    pub unnecessary_full_playback_count: usize,
    pub coverage_gaps: Vec<String>,
    pub real_data_authorization_verified: bool,
    pub production_accuracy_claim_allowed: bool,
    pub release_authorized: bool,
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

fn validate_opaque_key(value: &str, field: &str) -> CoreResult<()> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(CoreError::Invalid(format!(
            "{field} 只能使用小写字母、数字和短横线，且长度不得超过 96"
        )));
    }
    Ok(())
}

fn validate_version(value: &str, field: &str) -> CoreResult<()> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 96
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(CoreError::Invalid(format!(
            "{field} 不能为空、不得像路径，且长度不得超过 96"
        )));
    }
    Ok(())
}

fn validate_issue_tag(value: &str, field: &str) -> CoreResult<()> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 96
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(CoreError::Invalid(format!(
            "{field} 只能使用小写字母、数字和下划线，且长度不得超过 96"
        )));
    }
    Ok(())
}

fn validate_point_expectations(
    points: &[RecitationGoldenPointExpectation],
    field: &str,
) -> CoreResult<()> {
    let mut keys = BTreeSet::new();
    for point in points {
        validate_opaque_key(&point.point_key, field)?;
        if !keys.insert(point.point_key.trim().to_owned()) {
            return Err(CoreError::Invalid(format!("{field} 存在重复 point_key")));
        }
    }
    Ok(())
}

fn validate_spans(spans: &[RecitationGoldenSuspicionSpan], field: &str) -> CoreResult<()> {
    let mut unique = BTreeSet::new();
    for span in spans {
        validate_issue_tag(&span.issue_tag, field)?;
        if span.end_ms <= span.start_ms {
            return Err(CoreError::Invalid(format!(
                "{field} 的 end_ms 必须大于 start_ms"
            )));
        }
        if !unique.insert((span.issue_tag.trim().to_owned(), span.start_ms, span.end_ms)) {
            return Err(CoreError::Invalid(format!("{field} 存在重复疑点时间段")));
        }
    }
    Ok(())
}

impl RecitationGoldenManifest {
    pub fn contains_real_data(&self) -> bool {
        self.cases
            .iter()
            .any(|case| case.source_kind == RecitationGoldenSourceKind::RealDeidentifiedAudio)
    }

    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != RECITATION_GOLDEN_SCHEMA_VERSION || self.cases.is_empty() {
            return Err(CoreError::Invalid(
                "背诵黄金集 schema 不匹配或没有样本".into(),
            ));
        }
        validate_opaque_key(&self.dataset_id, "dataset_id")?;
        validate_version(&self.dataset_version, "dataset_version")?;
        validate_version(
            &self.governance.retention_policy_version,
            "retention_policy_version",
        )?;
        normalized_sha256(
            &self.governance.privacy_reviewed_by_ref_sha256,
            "隐私审查者引用",
        )?;
        if !self.governance.personal_identifiers_removed || !self.governance.privacy_reviewed {
            return Err(CoreError::Invalid(
                "背诵黄金集必须完成去标识和隐私审查".into(),
            ));
        }

        let mut case_ids = BTreeSet::new();
        let mut has_synthetic = false;
        let mut has_real = false;
        for case in &self.cases {
            validate_opaque_key(&case.case_id, "case_id")?;
            if !case_ids.insert(case.case_id.trim().to_owned()) {
                return Err(CoreError::Invalid("背诵黄金集 case_id 重复".into()));
            }
            for (value, field) in [
                (&case.answer_version_sha256, "答案版本 hash"),
                (&case.rubric_version_sha256, "rubric 版本 hash"),
                (&case.scoring_rule_version_sha256, "评分规则版本 hash"),
                (&case.annotated_by_ref_sha256, "标注者引用"),
                (&case.license_or_consent_ref_sha256, "授权或合成许可引用"),
            ] {
                normalized_sha256(value, field)?;
            }
            if case.teacher_annotation_revision <= 0 || case.issue_tags.is_empty() {
                return Err(CoreError::Invalid(
                    "背诵黄金集老师标注 revision 和 issue_tags 必须完整".into(),
                ));
            }
            let mut issue_tags = BTreeSet::new();
            for tag in &case.issue_tags {
                validate_issue_tag(tag, "issue_tag")?;
                if !issue_tags.insert(tag.trim().to_owned()) {
                    return Err(CoreError::Invalid("同一样本 issue_tag 不得重复".into()));
                }
            }
            validate_point_expectations(&case.expected_point_states, "expected_point_states")?;
            validate_spans(&case.expected_suspicion_spans, "expected_suspicion_spans")?;
            if case
                .expected_suspicion_spans
                .iter()
                .any(|span| !issue_tags.contains(span.issue_tag.trim()))
            {
                return Err(CoreError::Invalid(
                    "疑点时间段的 issue_tag 必须同时出现在样本 issue_tags 中".into(),
                ));
            }
            if case.expected_review_gate != RecitationGoldenReviewGate::Blocked
                && case.expected_point_states.is_empty()
            {
                return Err(CoreError::Invalid(
                    "非 blocked 背诵样本必须至少标注一个评分点".into(),
                ));
            }
            match case.source_kind {
                RecitationGoldenSourceKind::SyntheticContract => {
                    has_synthetic = true;
                    if case.audio_artifact_sha256.is_some() {
                        return Err(CoreError::Invalid(
                            "合成合同样本不得冒充真实音频 artifact".into(),
                        ));
                    }
                }
                RecitationGoldenSourceKind::RealDeidentifiedAudio => {
                    has_real = true;
                    normalized_sha256(
                        case.audio_artifact_sha256.as_deref().ok_or_else(|| {
                            CoreError::Invalid("真实背诵样本必须记录音频 hash".into())
                        })?,
                        "真实背诵音频 hash",
                    )?;
                }
            }
        }

        match self.governance.storage_scope {
            RecitationGoldenStorageScope::RepositorySynthetic if has_real => {
                return Err(CoreError::Invalid(
                    "真实背诵录音不得登记为仓库内合成存储".into(),
                ))
            }
            RecitationGoldenStorageScope::LocalRestricted if has_synthetic && !has_real => {
                return Err(CoreError::Invalid(
                    "纯合成集不得伪装成本机受限真实集".into(),
                ))
            }
            _ => {}
        }
        if has_synthetic && has_real {
            return Err(CoreError::Invalid(
                "同一背诵黄金集不得混合合成合同与真实音频".into(),
            ));
        }
        if has_real {
            if !self.governance.personal_identifiers_removed || !self.governance.privacy_reviewed {
                return Err(CoreError::Invalid(
                    "真实背诵黄金集必须完成去标识和隐私审查".into(),
                ));
            }
            let gate_id = self
                .governance
                .pilot_gate_id
                .as_deref()
                .ok_or_else(|| CoreError::Invalid("真实背诵集缺少 pilot_gate_id".into()))?;
            validate_opaque_key(gate_id, "pilot_gate_id")?;
            normalized_sha256(
                self.governance
                    .pilot_gate_policy_sha256
                    .as_deref()
                    .ok_or_else(|| {
                        CoreError::Invalid("真实背诵集缺少 pilot_gate_policy_sha256".into())
                    })?,
                "背诵试点闸门策略 hash",
            )?;
        } else if self.governance.pilot_gate_id.is_some()
            || self.governance.pilot_gate_policy_sha256.is_some()
        {
            return Err(CoreError::Invalid(
                "合成背诵集不得伪装成已获真实数据闸门放行".into(),
            ));
        }
        if has_synthetic && self.production_accuracy_claim_allowed {
            return Err(CoreError::Invalid(
                "包含合成样本的报告不得宣称生产准确率".into(),
            ));
        }
        if self.production_accuracy_claim_allowed && !has_real {
            return Err(CoreError::Invalid(
                "没有真实音频时不得宣称生产准确率".into(),
            ));
        }
        Ok(())
    }

    pub fn coverage_gaps(&self) -> Vec<String> {
        let present = self
            .cases
            .iter()
            .flat_map(|case| case.issue_tags.iter())
            .map(|tag| tag.trim().to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        REQUIRED_ISSUE_TAGS
            .iter()
            .filter(|tag| !present.contains(**tag))
            .map(|tag| (*tag).to_owned())
            .collect()
    }
}

impl RecitationGoldenAuthorization {
    pub fn policy_sha256(&self) -> CoreResult<String> {
        self.validate_shape()?;
        serde_json::to_vec(self)
            .map(|bytes| hashing::sha256_hex(&bytes))
            .map_err(|error| CoreError::Parse(format!("背诵授权清单 hash 失败：{error}")))
    }

    fn validate_shape(&self) -> CoreResult<()> {
        if self.schema_version != RECITATION_GOLDEN_SCHEMA_VERSION {
            return Err(CoreError::Invalid("背诵授权清单 schema 不匹配".into()));
        }
        validate_opaque_key(&self.gate_id, "gate_id")?;
        validate_opaque_key(&self.dataset_id, "dataset_id")?;
        validate_version(&self.dataset_version, "dataset_version")?;
        if self.allowed_use != "recitation_golden_evaluation" {
            return Err(CoreError::Invalid(
                "背诵授权清单用途必须是 recitation_golden_evaluation".into(),
            ));
        }
        for (value, field) in [
            (&self.authorization_ref_sha256, "授权引用"),
            (&self.privacy_review_ref_sha256, "隐私审查引用"),
            (&self.data_rights_drill_ref_sha256, "数据权利演练引用"),
            (&self.approved_by_ref_sha256, "批准者引用"),
        ] {
            normalized_sha256(value, field)?;
        }
        chrono::DateTime::parse_from_rfc3339(&self.approved_at)
            .map_err(|error| CoreError::Parse(format!("approved_at 时间非法：{error}")))?;
        NaiveDate::parse_from_str(&self.valid_from, "%Y-%m-%d")
            .map_err(|error| CoreError::Parse(format!("valid_from 日期非法：{error}")))?;
        NaiveDate::parse_from_str(&self.valid_through, "%Y-%m-%d")
            .map_err(|error| CoreError::Parse(format!("valid_through 日期非法：{error}")))?;
        Ok(())
    }

    pub fn validate_for(
        &self,
        manifest: &RecitationGoldenManifest,
        evaluated_on: &str,
    ) -> CoreResult<()> {
        self.validate_shape()?;
        if !manifest.contains_real_data() {
            return Err(CoreError::Invalid(
                "合成合同集不需要也不得借用真实数据授权".into(),
            ));
        }
        if self.state != RecitationGoldenAuthorizationState::Approved
            || !self.raw_audio_local_only
            || !self.cloud_processing_disclosed
        {
            return Err(CoreError::Invalid(
                "真实背诵授权未批准，或本机音频/云处理披露未完成".into(),
            ));
        }
        if self.dataset_id != manifest.dataset_id
            || self.dataset_version != manifest.dataset_version
            || manifest.governance.pilot_gate_id.as_deref() != Some(self.gate_id.as_str())
        {
            return Err(CoreError::Invalid(
                "背诵授权与黄金集 dataset 或 gate 身份不一致".into(),
            ));
        }
        let expected_policy_hash = manifest
            .governance
            .pilot_gate_policy_sha256
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("真实背诵集没有冻结授权清单 hash".into()))?;
        if !expected_policy_hash.eq_ignore_ascii_case(&self.policy_sha256()?) {
            return Err(CoreError::Invalid(
                "背诵授权清单已漂移，必须重新审批并冻结新 hash".into(),
            ));
        }
        let evaluated_on = NaiveDate::parse_from_str(evaluated_on, "%Y-%m-%d")
            .map_err(|error| CoreError::Parse(format!("评估日期非法：{error}")))?;
        let approved_on = chrono::DateTime::parse_from_rfc3339(&self.approved_at)
            .map_err(|error| CoreError::Parse(format!("approved_at 时间非法：{error}")))?
            .date_naive();
        let valid_from = NaiveDate::parse_from_str(&self.valid_from, "%Y-%m-%d")
            .map_err(|error| CoreError::Parse(format!("valid_from 日期非法：{error}")))?;
        let valid_through = NaiveDate::parse_from_str(&self.valid_through, "%Y-%m-%d")
            .map_err(|error| CoreError::Parse(format!("valid_through 日期非法：{error}")))?;
        if valid_through < valid_from
            || approved_on > evaluated_on
            || evaluated_on < valid_from
            || evaluated_on > valid_through
        {
            return Err(CoreError::Invalid(
                "背诵真实数据授权已过期、尚未生效、批准时间在未来或有效期非法".into(),
            ));
        }
        Ok(())
    }
}

impl RecitationGoldenPredictionSet {
    pub fn validate_for(&self, manifest: &RecitationGoldenManifest) -> CoreResult<()> {
        if self.schema_version != RECITATION_GOLDEN_SCHEMA_VERSION
            || self.dataset_id != manifest.dataset_id
            || self.dataset_version != manifest.dataset_version
        {
            return Err(CoreError::Invalid(
                "背诵黄金集预测 schema、dataset 或 version 不一致".into(),
            ));
        }
        let mut case_ids = BTreeSet::new();
        for prediction in &self.predictions {
            validate_opaque_key(&prediction.case_id, "prediction case_id")?;
            if !case_ids.insert(prediction.case_id.trim().to_owned()) {
                return Err(CoreError::Invalid("背诵黄金集预测 case_id 重复".into()));
            }
            validate_point_expectations(
                &prediction.predicted_point_states,
                "predicted_point_states",
            )?;
            validate_spans(
                &prediction.predicted_suspicion_spans,
                "predicted_suspicion_spans",
            )?;
        }
        Ok(())
    }
}

pub fn evaluate_recitation_golden(
    manifest: &RecitationGoldenManifest,
    predictions: &RecitationGoldenPredictionSet,
) -> CoreResult<RecitationGoldenReport> {
    manifest.validate()?;
    if manifest.contains_real_data() {
        return Err(CoreError::Invalid(
            "真实背诵黄金集必须使用带有效授权的评估入口".into(),
        ));
    }
    predictions.validate_for(manifest)?;
    evaluate_validated(manifest, predictions, false)
}

pub fn evaluate_real_recitation_golden(
    manifest: &RecitationGoldenManifest,
    predictions: &RecitationGoldenPredictionSet,
    authorization: &RecitationGoldenAuthorization,
    evaluated_on: &str,
) -> CoreResult<RecitationGoldenReport> {
    manifest.validate()?;
    authorization.validate_for(manifest, evaluated_on)?;
    predictions.validate_for(manifest)?;
    evaluate_validated(manifest, predictions, true)
}

fn evaluate_validated(
    manifest: &RecitationGoldenManifest,
    prediction_set: &RecitationGoldenPredictionSet,
    real_data_authorization_verified: bool,
) -> CoreResult<RecitationGoldenReport> {
    let predictions = prediction_set
        .predictions
        .iter()
        .map(|prediction| (prediction.case_id.as_str(), prediction))
        .collect::<BTreeMap<_, _>>();
    let expected_case_ids = manifest
        .cases
        .iter()
        .map(|case| case.case_id.as_str())
        .collect::<BTreeSet<_>>();

    let mut report = RecitationGoldenReport {
        schema_version: RECITATION_GOLDEN_SCHEMA_VERSION,
        dataset_id: manifest.dataset_id.clone(),
        dataset_version: manifest.dataset_version.clone(),
        case_count: manifest.cases.len(),
        asr_state_match_count: 0,
        machine_suggestion_match_count: 0,
        review_gate_match_count: 0,
        dangerous_false_pass_count: 0,
        conservative_false_fail_count: 0,
        unsafe_ready_count: 0,
        ready_routed_to_review_count: 0,
        missing_prediction_count: 0,
        unexpected_prediction_count: predictions
            .keys()
            .filter(|case_id| !expected_case_ids.contains(**case_id))
            .count(),
        expected_point_count: 0,
        point_state_match_count: 0,
        missing_point_count: 0,
        unexpected_point_count: 0,
        expected_suspicion_span_count: 0,
        suspicion_span_match_count: 0,
        missing_suspicion_span_count: 0,
        unexpected_suspicion_span_count: 0,
        expected_full_playback_count: manifest
            .cases
            .iter()
            .filter(|case| case.requires_full_playback)
            .count(),
        full_playback_match_count: 0,
        missed_required_full_playback_count: 0,
        unnecessary_full_playback_count: 0,
        coverage_gaps: manifest.coverage_gaps(),
        real_data_authorization_verified,
        production_accuracy_claim_allowed: manifest.production_accuracy_claim_allowed
            && real_data_authorization_verified
            && manifest.coverage_gaps().is_empty(),
        release_authorized: false,
    };

    for case in &manifest.cases {
        report.expected_point_count += case.expected_point_states.len();
        report.expected_suspicion_span_count += case.expected_suspicion_spans.len();
        let Some(prediction) = predictions.get(case.case_id.as_str()) else {
            report.missing_prediction_count += 1;
            report.missing_point_count += case.expected_point_states.len();
            report.missing_suspicion_span_count += case.expected_suspicion_spans.len();
            continue;
        };
        report.asr_state_match_count +=
            usize::from(prediction.observed_asr_state == case.expected_asr_state);
        report.machine_suggestion_match_count += usize::from(
            prediction.predicted_machine_suggestion == case.expected_machine_suggestion,
        );
        report.review_gate_match_count +=
            usize::from(prediction.predicted_review_gate == case.expected_review_gate);
        if case.expected_teacher_outcome == RecitationGoldenTeacherOutcome::Fail
            && prediction.predicted_machine_suggestion == RecitationGoldenSuggestion::Pass
        {
            report.dangerous_false_pass_count += 1;
        }
        if case.expected_teacher_outcome == RecitationGoldenTeacherOutcome::Pass
            && prediction.predicted_machine_suggestion == RecitationGoldenSuggestion::Fail
        {
            report.conservative_false_fail_count += 1;
        }
        if case.expected_review_gate != RecitationGoldenReviewGate::Ready
            && prediction.predicted_review_gate == RecitationGoldenReviewGate::Ready
        {
            report.unsafe_ready_count += 1;
        }
        if case.expected_review_gate == RecitationGoldenReviewGate::Ready
            && prediction.predicted_review_gate != RecitationGoldenReviewGate::Ready
        {
            report.ready_routed_to_review_count += 1;
        }
        if prediction.predicted_requires_full_playback == case.requires_full_playback {
            report.full_playback_match_count += 1;
        } else if case.requires_full_playback {
            report.missed_required_full_playback_count += 1;
        } else {
            report.unnecessary_full_playback_count += 1;
        }

        let predicted_points = prediction
            .predicted_point_states
            .iter()
            .map(|point| (point.point_key.as_str(), point))
            .collect::<BTreeMap<_, _>>();
        let expected_point_keys = case
            .expected_point_states
            .iter()
            .map(|point| point.point_key.as_str())
            .collect::<BTreeSet<_>>();
        report.unexpected_point_count += predicted_points
            .keys()
            .filter(|key| !expected_point_keys.contains(**key))
            .count();
        for expected in &case.expected_point_states {
            match predicted_points.get(expected.point_key.as_str()) {
                Some(predicted) if predicted.state == expected.state => {
                    report.point_state_match_count += 1
                }
                Some(_) => {}
                None => report.missing_point_count += 1,
            }
        }

        let predicted_spans = prediction
            .predicted_suspicion_spans
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        let expected_spans = case
            .expected_suspicion_spans
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        report.suspicion_span_match_count += expected_spans.intersection(&predicted_spans).count();
        report.missing_suspicion_span_count += expected_spans.difference(&predicted_spans).count();
        report.unexpected_suspicion_span_count +=
            predicted_spans.difference(&expected_spans).count();
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract_manifest() -> RecitationGoldenManifest {
        serde_json::from_str(include_str!(
            "../tests/fixtures/golden/synthetic_contract_manifest_v1.json"
        ))
        .expect("fixture manifest")
    }

    fn contract_predictions() -> RecitationGoldenPredictionSet {
        serde_json::from_str(include_str!(
            "../tests/fixtures/golden/synthetic_contract_predictions_v1.json"
        ))
        .expect("fixture predictions")
    }

    #[test]
    fn synthetic_contract_covers_required_scenarios_without_accuracy_claim() {
        let manifest = contract_manifest();
        manifest.validate().expect("valid manifest");
        assert!(!manifest.contains_real_data());
        assert!(manifest.coverage_gaps().is_empty());
        assert!(!manifest.production_accuracy_claim_allowed);
    }

    #[test]
    fn perfect_contract_predictions_produce_fixed_report() {
        let manifest = contract_manifest();
        let predictions = contract_predictions();
        let report = evaluate_recitation_golden(&manifest, &predictions).expect("report");
        assert_eq!(report.case_count, 19);
        assert_eq!(report.asr_state_match_count, 19);
        assert_eq!(report.machine_suggestion_match_count, 19);
        assert_eq!(report.review_gate_match_count, 19);
        assert_eq!(report.point_state_match_count, report.expected_point_count);
        assert_eq!(
            report.suspicion_span_match_count,
            report.expected_suspicion_span_count
        );
        assert_eq!(report.dangerous_false_pass_count, 0);
        assert_eq!(report.conservative_false_fail_count, 0);
        assert_eq!(report.unsafe_ready_count, 0);
        assert_eq!(report.full_playback_match_count, 19);
        assert_eq!(report.missed_required_full_playback_count, 0);
        assert_eq!(report.unnecessary_full_playback_count, 0);
        assert!(report.coverage_gaps.is_empty());
        assert!(!report.production_accuracy_claim_allowed);
        assert!(!report.release_authorized);
    }

    #[test]
    fn dangerous_false_pass_and_unsafe_ready_are_separate() {
        let manifest = contract_manifest();
        let mut predictions = contract_predictions();
        let prediction = predictions
            .predictions
            .iter_mut()
            .find(|prediction| prediction.case_id == "rec-case-fluent-fact-error")
            .expect("fact error prediction");
        prediction.predicted_machine_suggestion = RecitationGoldenSuggestion::Pass;
        prediction.predicted_review_gate = RecitationGoldenReviewGate::Ready;
        prediction.predicted_requires_full_playback = true;
        let report = evaluate_recitation_golden(&manifest, &predictions).expect("report");
        assert_eq!(report.dangerous_false_pass_count, 1);
        assert_eq!(report.unsafe_ready_count, 1);
        assert_eq!(report.unnecessary_full_playback_count, 1);
    }

    #[test]
    fn missing_and_unexpected_predictions_and_points_are_reported() {
        let manifest = contract_manifest();
        let mut predictions = contract_predictions();
        predictions.predictions.pop();
        predictions.predictions[0]
            .predicted_point_states
            .push(RecitationGoldenPointExpectation {
                point_key: "unexpected-point".into(),
                state: RecitationGoldenPointState::Covered,
            });
        predictions.predictions.push(RecitationGoldenPrediction {
            case_id: "rec-case-unexpected".into(),
            observed_asr_state: RecitationGoldenAsrState::Ok,
            predicted_machine_suggestion: RecitationGoldenSuggestion::Pass,
            predicted_review_gate: RecitationGoldenReviewGate::Ready,
            predicted_requires_full_playback: false,
            predicted_point_states: vec![],
            predicted_suspicion_spans: vec![],
        });
        let report = evaluate_recitation_golden(&manifest, &predictions).expect("report");
        assert_eq!(report.missing_prediction_count, 1);
        assert_eq!(report.unexpected_prediction_count, 1);
        assert_eq!(report.unexpected_point_count, 1);
    }

    #[test]
    fn strict_json_rejects_personal_identity_field() {
        let manifest = include_str!("../tests/fixtures/golden/synthetic_contract_manifest_v1.json");
        let poisoned = manifest.replacen(
            "\"case_id\": \"rec-case-paraphrase-correct\",",
            "\"case_id\": \"rec-case-paraphrase-correct\", \"student_name\": \"not-allowed\",",
            1,
        );
        let error = serde_json::from_str::<RecitationGoldenManifest>(&poisoned)
            .expect_err("unknown student_name must fail");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn real_data_requires_matching_current_authorization_hash() {
        let mut manifest = contract_manifest();
        manifest.dataset_id = "recitation-real-pilot-001".into();
        manifest.dataset_version = "real-v1".into();
        manifest.cases.truncate(1);
        let case = &mut manifest.cases[0];
        case.case_id = "rec-case-real-0001".into();
        case.source_kind = RecitationGoldenSourceKind::RealDeidentifiedAudio;
        case.audio_artifact_sha256 = Some("a".repeat(64));
        manifest.governance.storage_scope = RecitationGoldenStorageScope::LocalRestricted;
        manifest.governance.pilot_gate_id = Some("rec-gate-001".into());
        manifest.production_accuracy_claim_allowed = true;
        let authorization = RecitationGoldenAuthorization {
            schema_version: 1,
            gate_id: "rec-gate-001".into(),
            dataset_id: manifest.dataset_id.clone(),
            dataset_version: manifest.dataset_version.clone(),
            state: RecitationGoldenAuthorizationState::Approved,
            allowed_use: "recitation_golden_evaluation".into(),
            valid_from: "2026-07-01".into(),
            valid_through: "2026-07-31".into(),
            authorization_ref_sha256: "b".repeat(64),
            privacy_review_ref_sha256: "c".repeat(64),
            data_rights_drill_ref_sha256: "d".repeat(64),
            approved_by_ref_sha256: "e".repeat(64),
            approved_at: "2026-07-01T08:00:00Z".into(),
            raw_audio_local_only: true,
            cloud_processing_disclosed: true,
        };
        manifest.governance.pilot_gate_policy_sha256 =
            Some(authorization.policy_sha256().expect("policy hash"));
        manifest.validate().expect("real manifest");
        authorization
            .validate_for(&manifest, "2026-07-18")
            .expect("valid authorization");

        let mut drifted = authorization.clone();
        drifted.valid_through = "2026-07-30".into();
        let error = drifted
            .validate_for(&manifest, "2026-07-18")
            .expect_err("drift must fail");
        assert!(error.to_string().contains("漂移"));
        let error = authorization
            .validate_for(&manifest, "2026-08-01")
            .expect_err("expired gate must fail");
        assert!(error.to_string().contains("过期"));
    }
}
