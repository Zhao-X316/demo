//! 背诵老师试点的无正文成对观察合同与固定指标报告。
//!
//! 每条录音必须由同一老师、同一去标识样本分别完成纯人工基线和 AI 辅助复核；
//! 每日积压另用成对快照记录。本层只接受不透明 ID、hash、时间、枚举和计数，
//! 不保存学生姓名、文件路径、录音、ASR 或答案正文。合成合同只能验证计算与门禁，
//! 不能声明真实减负；任何报告都不能自行授权发布。

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::{Deserialize, Serialize};
use suite_core::domain::{hashing, ids};
use suite_core::error::{CoreError, CoreResult};

use crate::golden::{RecitationGoldenSuggestion, RecitationGoldenTeacherOutcome};

pub const RECITATION_PILOT_SCHEMA_VERSION: i64 = 1;
pub const RECITATION_PILOT_REPORT_SCHEMA_VERSION: i64 = 1;
pub const RECITATION_PILOT_MINIMUM_PAIR_COUNT: u64 = 50;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationPilotSourceKind {
    SyntheticContract,
    RealRestricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationPilotStorageScope {
    RepositorySynthetic,
    LocalRestricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationPilotWorkflow {
    ManualBaseline,
    AssistedReview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationPilotSequenceArm {
    ManualFirst,
    AssistedFirst,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationPilotListeningMode {
    FullPlayback,
    SuspicionOnly,
    NoPlayback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationPilotAsrOutcome {
    NotApplicable,
    CacheHit,
    SucceededFirstAttempt,
    FailedUnrecovered,
    RecoveredAfterRetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecitationPilotAuthorizationState {
    Draft,
    Approved,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationPilotGovernance {
    pub storage_scope: RecitationPilotStorageScope,
    pub personal_identifiers_removed: bool,
    pub content_excluded: bool,
    pub paths_excluded: bool,
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
pub struct RecitationPilotObservation {
    pub observation_id: String,
    pub comparison_group_id: String,
    pub workflow: RecitationPilotWorkflow,
    pub sequence_arm: RecitationPilotSequenceArm,
    pub teacher_ref_sha256: String,
    pub case_ref_sha256: String,
    pub started_at: String,
    pub completed_at: String,
    pub teacher_active_milliseconds: u64,
    pub queue_wait_seconds: u64,
    pub machine_wait_milliseconds: u64,
    pub teacher_outcome: RecitationGoldenTeacherOutcome,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine_suggestion: Option<RecitationGoldenSuggestion>,
    pub listening_mode: RecitationPilotListeningMode,
    pub reviewed_point_count: u64,
    pub corrected_point_count: u64,
    pub asr_outcome: RecitationPilotAsrOutcome,
    pub association_anomaly_detected: bool,
    pub completed_first_attempt: bool,
    pub completed_without_help: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationPilotDailySnapshot {
    pub snapshot_id: String,
    pub comparison_day_group_id: String,
    pub workflow: RecitationPilotWorkflow,
    pub teacher_ref_sha256: String,
    pub business_date: String,
    pub eligible_count: u64,
    pub completed_count: u64,
    pub backlog_count: u64,
    pub longest_wait_seconds: u64,
    pub completed_first_attempt_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationPilotObservationSet {
    pub schema_version: i64,
    pub session_id: String,
    pub source_kind: RecitationPilotSourceKind,
    pub dataset_id: String,
    pub dataset_version: String,
    pub sample_scope_sha256: String,
    pub golden_report_sha256: String,
    pub observation_policy_version: String,
    pub time_saving_claim_requested: bool,
    pub governance: RecitationPilotGovernance,
    pub observations: Vec<RecitationPilotObservation>,
    pub daily_snapshots: Vec<RecitationPilotDailySnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationPilotAuthorization {
    pub schema_version: i64,
    pub gate_id: String,
    pub dataset_id: String,
    pub dataset_version: String,
    pub state: RecitationPilotAuthorizationState,
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
pub struct RecitationPilotPercentiles {
    pub p50_milliseconds: u64,
    pub p80_milliseconds: u64,
    pub p95_milliseconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationPilotWorkflowMetrics {
    pub observation_count: u64,
    pub total_teacher_active_milliseconds: u64,
    pub seconds_per_50_items: u64,
    pub average_item_milliseconds: u64,
    pub item_time_percentiles: RecitationPilotPercentiles,
    pub full_playback_count: u64,
    pub full_playback_rate_basis_points: u64,
    pub suspicion_only_count: u64,
    pub suspicion_only_rate_basis_points: u64,
    pub no_playback_count: u64,
    pub no_playback_rate_basis_points: u64,
    pub total_queue_wait_seconds: u64,
    pub longest_item_queue_wait_seconds: u64,
    pub total_machine_wait_milliseconds: u64,
    pub reviewed_point_count: u64,
    pub corrected_point_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point_correction_rate_basis_points: Option<u64>,
    pub asr_attempt_count: u64,
    pub asr_cache_hit_count: u64,
    pub initial_asr_failure_count: u64,
    pub recovered_asr_count: u64,
    pub unrecovered_asr_failure_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asr_failure_rate_basis_points: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asr_recovery_rate_basis_points: Option<u64>,
    pub association_anomaly_count: u64,
    pub association_anomaly_rate_basis_points: u64,
    pub completed_first_attempt_count: u64,
    pub completed_first_attempt_rate_basis_points: u64,
    pub completed_without_help_count: u64,
    pub completed_without_help_rate_basis_points: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationPilotDailyMetrics {
    pub snapshot_count: u64,
    pub eligible_count: u64,
    pub completed_count: u64,
    pub backlog_count_total: u64,
    pub maximum_day_end_backlog_count: u64,
    pub longest_wait_seconds: u64,
    pub completed_first_attempt_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationPilotComparisonMetrics {
    pub pair_count: u64,
    pub manual_first_pair_count: u64,
    pub assisted_first_pair_count: u64,
    pub teacher_outcome_disagreement_count: u64,
    /// 正值表示辅助流程节省主动操作时间，负值表示辅助流程更慢；10000 = 100%。
    pub active_time_reduction_basis_points: i64,
    pub machine_pass_teacher_fail_count: u64,
    pub machine_pass_teacher_fail_rate_basis_points: u64,
    pub machine_fail_teacher_pass_count: u64,
    pub machine_fail_teacher_pass_rate_basis_points: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationPilotReportPayload {
    pub schema_version: i64,
    pub session_id: String,
    pub source_kind: RecitationPilotSourceKind,
    pub dataset_id: String,
    pub dataset_version: String,
    pub sample_scope_sha256: String,
    pub golden_report_sha256: String,
    pub observation_policy_version: String,
    pub generated_at: String,
    pub evaluated_on: String,
    pub teacher_ref_sha256s: Vec<String>,
    pub comparison: RecitationPilotComparisonMetrics,
    pub manual_baseline: RecitationPilotWorkflowMetrics,
    pub assisted_review: RecitationPilotWorkflowMetrics,
    pub manual_daily: RecitationPilotDailyMetrics,
    pub assisted_daily: RecitationPilotDailyMetrics,
    pub real_data_authorization_verified: bool,
    pub minimum_pair_count_met: bool,
    pub counterbalanced_sequence_present: bool,
    pub time_saving_claim_allowed: bool,
    pub release_authorized: bool,
    pub content_excluded: bool,
    pub paths_excluded: bool,
    pub student_identity_excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecitationPilotReport {
    pub payload: RecitationPilotReportPayload,
    pub report_sha256: String,
}

#[derive(Debug, Clone)]
struct ObservationPair {
    baseline: RecitationPilotObservation,
    assisted: RecitationPilotObservation,
}

#[derive(Debug, Clone)]
struct DailyPair {
    baseline: RecitationPilotDailySnapshot,
    assisted: RecitationPilotDailySnapshot,
}

fn required_opaque_id(value: &str, field: &str) -> CoreResult<()> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是小写字母、数字和短横线组成的不透明 ID，且不超过 128 字符"
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

fn normalized_sha256(value: &str, field: &str) -> CoreResult<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是 64 位十六进制 sha256"
        )));
    }
    Ok(value)
}

fn parse_time(value: &str, field: &str) -> CoreResult<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value.trim())
        .map_err(|_| CoreError::Invalid(format!("{field} 必须是 RFC3339")))
}

fn parse_date(value: &str, field: &str) -> CoreResult<NaiveDate> {
    NaiveDate::parse_from_str(value.trim(), "%Y-%m-%d")
        .map_err(|_| CoreError::Invalid(format!("{field} 必须是 YYYY-MM-DD")))
}

impl RecitationPilotObservation {
    fn validate(&self) -> CoreResult<()> {
        required_opaque_id(&self.observation_id, "observation_id")?;
        required_opaque_id(&self.comparison_group_id, "comparison_group_id")?;
        normalized_sha256(&self.teacher_ref_sha256, "老师引用")?;
        normalized_sha256(&self.case_ref_sha256, "样本引用")?;
        let started_at = parse_time(&self.started_at, "开始时间")?;
        let completed_at = parse_time(&self.completed_at, "完成时间")?;
        if completed_at <= started_at {
            return Err(CoreError::Invalid("完成时间必须晚于开始时间".into()));
        }
        let elapsed_milliseconds = (completed_at - started_at).num_milliseconds() as u64;
        if self.teacher_active_milliseconds == 0
            || self.teacher_active_milliseconds > elapsed_milliseconds
        {
            return Err(CoreError::Invalid(
                "老师主动操作毫秒必须为正且不能超过观察时长".into(),
            ));
        }
        if self.machine_wait_milliseconds > elapsed_milliseconds {
            return Err(CoreError::Invalid("机器等待毫秒不能超过观察时长".into()));
        }
        if self.corrected_point_count > self.reviewed_point_count {
            return Err(CoreError::Invalid(
                "老师修正评分点数不能超过已评审评分点数".into(),
            ));
        }
        match self.workflow {
            RecitationPilotWorkflow::ManualBaseline => {
                if self.machine_suggestion.is_some()
                    || self.listening_mode != RecitationPilotListeningMode::FullPlayback
                    || self.machine_wait_milliseconds != 0
                    || self.reviewed_point_count != 0
                    || self.corrected_point_count != 0
                    || self.asr_outcome != RecitationPilotAsrOutcome::NotApplicable
                    || self.association_anomaly_detected
                {
                    return Err(CoreError::Invalid(
                        "人工基线必须完整听音且不能填写机器、ASR、逐点或关联异常字段".into(),
                    ));
                }
            }
            RecitationPilotWorkflow::AssistedReview => {
                if self.asr_outcome == RecitationPilotAsrOutcome::NotApplicable {
                    return Err(CoreError::Invalid(
                        "AI 辅助观察必须记录 ASR 处理结果".into(),
                    ));
                }
                if self.asr_outcome == RecitationPilotAsrOutcome::FailedUnrecovered {
                    if self.machine_suggestion.is_some()
                        || self.listening_mode != RecitationPilotListeningMode::FullPlayback
                    {
                        return Err(CoreError::Invalid(
                            "ASR 未恢复时不能存在机器建议，且老师必须完整回听".into(),
                        ));
                    }
                } else if self.machine_suggestion.is_none() {
                    return Err(CoreError::Invalid(
                        "ASR 可用的辅助观察必须记录机器建议".into(),
                    ));
                }
            }
        }
        Ok(())
    }
}

impl RecitationPilotDailySnapshot {
    fn validate(&self) -> CoreResult<()> {
        required_opaque_id(&self.snapshot_id, "snapshot_id")?;
        required_opaque_id(&self.comparison_day_group_id, "comparison_day_group_id")?;
        normalized_sha256(&self.teacher_ref_sha256, "每日快照老师引用")?;
        parse_date(&self.business_date, "business_date")?;
        if self.eligible_count == 0
            || self.completed_count > self.eligible_count
            || self.backlog_count > self.eligible_count
            || self.completed_count.checked_add(self.backlog_count) != Some(self.eligible_count)
            || self.completed_first_attempt_count > self.completed_count
        {
            return Err(CoreError::Invalid(
                "每日 eligible 必须为正且等于完成数加日终积压，一次完成不得超过完成数".into(),
            ));
        }
        Ok(())
    }
}

fn pair_observations(set: &RecitationPilotObservationSet) -> CoreResult<Vec<ObservationPair>> {
    if set.observations.is_empty() {
        return Err(CoreError::Invalid("背诵试点观察不能为空".into()));
    }
    let mut observation_ids = BTreeSet::new();
    let mut groups: BTreeMap<
        String,
        BTreeMap<RecitationPilotWorkflow, RecitationPilotObservation>,
    > = BTreeMap::new();
    for observation in &set.observations {
        observation.validate()?;
        if !observation_ids.insert(observation.observation_id.clone()) {
            return Err(CoreError::Invalid("observation_id 不能重复".into()));
        }
        if groups
            .entry(observation.comparison_group_id.clone())
            .or_default()
            .insert(observation.workflow, observation.clone())
            .is_some()
        {
            return Err(CoreError::Invalid(
                "同一 comparison group 的 workflow 不能重复".into(),
            ));
        }
    }
    let mut pairs = Vec::with_capacity(groups.len());
    let mut case_refs = BTreeSet::new();
    for (group_id, mut workflows) in groups {
        let baseline = workflows
            .remove(&RecitationPilotWorkflow::ManualBaseline)
            .ok_or_else(|| CoreError::Invalid(format!("{group_id} 缺少人工基线")))?;
        let assisted = workflows
            .remove(&RecitationPilotWorkflow::AssistedReview)
            .ok_or_else(|| CoreError::Invalid(format!("{group_id} 缺少辅助复核")))?;
        if baseline.teacher_ref_sha256 != assisted.teacher_ref_sha256
            || baseline.case_ref_sha256 != assisted.case_ref_sha256
            || baseline.sequence_arm != assisted.sequence_arm
        {
            return Err(CoreError::Invalid(format!(
                "{group_id} 的人工基线与辅助复核不是同一老师、样本或顺序分组"
            )));
        }
        if !case_refs.insert(baseline.case_ref_sha256.trim().to_ascii_lowercase()) {
            return Err(CoreError::Invalid(
                "同一试点会话不能重复使用同一去标识样本扩充样本量".into(),
            ));
        }
        pairs.push(ObservationPair { baseline, assisted });
    }
    Ok(pairs)
}

fn pair_daily_snapshots(set: &RecitationPilotObservationSet) -> CoreResult<Vec<DailyPair>> {
    if set.daily_snapshots.is_empty() {
        return Err(CoreError::Invalid("背诵试点必须记录每日积压快照".into()));
    }
    let mut snapshot_ids = BTreeSet::new();
    let mut groups: BTreeMap<
        String,
        BTreeMap<RecitationPilotWorkflow, RecitationPilotDailySnapshot>,
    > = BTreeMap::new();
    for snapshot in &set.daily_snapshots {
        snapshot.validate()?;
        if !snapshot_ids.insert(snapshot.snapshot_id.clone()) {
            return Err(CoreError::Invalid("snapshot_id 不能重复".into()));
        }
        if groups
            .entry(snapshot.comparison_day_group_id.clone())
            .or_default()
            .insert(snapshot.workflow, snapshot.clone())
            .is_some()
        {
            return Err(CoreError::Invalid(
                "同一每日对照组的 workflow 不能重复".into(),
            ));
        }
    }
    let mut pairs = Vec::with_capacity(groups.len());
    for (group_id, mut workflows) in groups {
        let baseline = workflows
            .remove(&RecitationPilotWorkflow::ManualBaseline)
            .ok_or_else(|| CoreError::Invalid(format!("{group_id} 缺少人工每日快照")))?;
        let assisted = workflows
            .remove(&RecitationPilotWorkflow::AssistedReview)
            .ok_or_else(|| CoreError::Invalid(format!("{group_id} 缺少辅助每日快照")))?;
        if baseline.teacher_ref_sha256 != assisted.teacher_ref_sha256
            || baseline.eligible_count != assisted.eligible_count
        {
            return Err(CoreError::Invalid(format!(
                "{group_id} 的每日基线与辅助快照不是同一老师和工作量"
            )));
        }
        pairs.push(DailyPair { baseline, assisted });
    }
    Ok(pairs)
}

impl RecitationPilotObservationSet {
    pub fn contains_real_data(&self) -> bool {
        self.source_kind == RecitationPilotSourceKind::RealRestricted
    }

    pub fn validate(&self) -> CoreResult<()> {
        if self.schema_version != RECITATION_PILOT_SCHEMA_VERSION {
            return Err(CoreError::Invalid("背诵试点观察 schema 版本不支持".into()));
        }
        required_opaque_id(&self.session_id, "session_id")?;
        required_opaque_id(&self.dataset_id, "dataset_id")?;
        validate_version(&self.dataset_version, "dataset_version")?;
        validate_version(
            &self.observation_policy_version,
            "observation_policy_version",
        )?;
        normalized_sha256(&self.sample_scope_sha256, "样本范围引用")?;
        normalized_sha256(&self.golden_report_sha256, "黄金集报告引用")?;
        normalized_sha256(
            &self.governance.privacy_reviewed_by_ref_sha256,
            "隐私审查者引用",
        )?;
        validate_version(
            &self.governance.retention_policy_version,
            "retention_policy_version",
        )?;
        if !self.governance.personal_identifiers_removed
            || !self.governance.content_excluded
            || !self.governance.paths_excluded
            || !self.governance.privacy_reviewed
        {
            return Err(CoreError::Invalid(
                "背诵试点观察必须去标识、排除正文/路径并完成隐私审查".into(),
            ));
        }
        match self.source_kind {
            RecitationPilotSourceKind::SyntheticContract => {
                if self.governance.storage_scope != RecitationPilotStorageScope::RepositorySynthetic
                    || self.time_saving_claim_requested
                    || self.governance.pilot_gate_id.is_some()
                    || self.governance.pilot_gate_policy_sha256.is_some()
                {
                    return Err(CoreError::Invalid(
                        "合成试点合同不得伪装为受限真实数据或申请减负声明".into(),
                    ));
                }
            }
            RecitationPilotSourceKind::RealRestricted => {
                if self.governance.storage_scope != RecitationPilotStorageScope::LocalRestricted {
                    return Err(CoreError::Invalid(
                        "真实背诵观察只能登记为本机受限存储".into(),
                    ));
                }
                required_opaque_id(
                    self.governance
                        .pilot_gate_id
                        .as_deref()
                        .ok_or_else(|| CoreError::Invalid("真实试点缺少 gate_id".into()))?,
                    "pilot_gate_id",
                )?;
                normalized_sha256(
                    self.governance
                        .pilot_gate_policy_sha256
                        .as_deref()
                        .ok_or_else(|| CoreError::Invalid("真实试点缺少授权清单 hash".into()))?,
                    "试点授权清单 hash",
                )?;
            }
        }
        pair_observations(self)?;
        pair_daily_snapshots(self)?;
        Ok(())
    }
}

impl RecitationPilotAuthorization {
    fn validate_shape(&self) -> CoreResult<()> {
        if self.schema_version != RECITATION_PILOT_SCHEMA_VERSION {
            return Err(CoreError::Invalid("背诵试点授权 schema 版本不支持".into()));
        }
        required_opaque_id(&self.gate_id, "gate_id")?;
        required_opaque_id(&self.dataset_id, "dataset_id")?;
        validate_version(&self.dataset_version, "dataset_version")?;
        if self.allowed_use != "recitation_pilot_metrics" {
            return Err(CoreError::Invalid(
                "背诵试点授权用途必须是 recitation_pilot_metrics".into(),
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
        parse_time(&self.approved_at, "approved_at")?;
        parse_date(&self.valid_from, "valid_from")?;
        parse_date(&self.valid_through, "valid_through")?;
        Ok(())
    }

    pub fn policy_sha256(&self) -> CoreResult<String> {
        self.validate_shape()?;
        serde_json::to_vec(self)
            .map(|bytes| hashing::sha256_hex(&bytes))
            .map_err(|error| CoreError::Config(format!("背诵试点授权 hash 失败：{error}")))
    }

    pub fn validate_for(
        &self,
        set: &RecitationPilotObservationSet,
        evaluated_on: &str,
    ) -> CoreResult<()> {
        self.validate_shape()?;
        if !set.contains_real_data() {
            return Err(CoreError::Invalid(
                "合成试点合同不需要也不得借用真实数据授权".into(),
            ));
        }
        if self.state != RecitationPilotAuthorizationState::Approved
            || !self.raw_audio_local_only
            || !self.cloud_processing_disclosed
        {
            return Err(CoreError::Invalid(
                "真实试点授权未批准，或本机音频/云处理披露未完成".into(),
            ));
        }
        if self.dataset_id != set.dataset_id
            || self.dataset_version != set.dataset_version
            || set.governance.pilot_gate_id.as_deref() != Some(self.gate_id.as_str())
        {
            return Err(CoreError::Invalid(
                "背诵试点授权与观察集 dataset 或 gate 身份不一致".into(),
            ));
        }
        let frozen_hash = set
            .governance
            .pilot_gate_policy_sha256
            .as_deref()
            .ok_or_else(|| CoreError::Invalid("真实试点没有冻结授权清单 hash".into()))?;
        if !frozen_hash.eq_ignore_ascii_case(&self.policy_sha256()?) {
            return Err(CoreError::Invalid(
                "背诵试点授权清单已漂移，必须重新审批并冻结新 hash".into(),
            ));
        }
        let evaluated_on = parse_date(evaluated_on, "evaluated_on")?;
        let approved_on = parse_time(&self.approved_at, "approved_at")?.date_naive();
        let valid_from = parse_date(&self.valid_from, "valid_from")?;
        let valid_through = parse_date(&self.valid_through, "valid_through")?;
        if valid_through < valid_from
            || approved_on > evaluated_on
            || evaluated_on < valid_from
            || evaluated_on > valid_through
        {
            return Err(CoreError::Invalid(
                "背诵真实试点授权已过期、尚未生效、批准时间在未来或有效期非法".into(),
            ));
        }
        Ok(())
    }
}

fn percentile(values: &[u64], percentile: u64) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (sorted.len() as u64 * percentile).div_ceil(100).max(1) as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn rate_basis_points(numerator: u64, denominator: u64) -> CoreResult<u64> {
    numerator
        .checked_mul(10_000)
        .map(|value| value / denominator)
        .ok_or_else(|| CoreError::Invalid("比例计算溢出".into()))
}

fn optional_rate_basis_points(numerator: u64, denominator: u64) -> CoreResult<Option<u64>> {
    if denominator == 0 {
        Ok(None)
    } else {
        rate_basis_points(numerator, denominator).map(Some)
    }
}

fn rounded_average(total: u64, count: u64, field: &str) -> CoreResult<u64> {
    total
        .checked_add(count / 2)
        .map(|value| value / count)
        .ok_or_else(|| CoreError::Invalid(format!("{field} 平均值计算溢出")))
}

fn seconds_per_50(total_milliseconds: u64, count: u64) -> CoreResult<u64> {
    total_milliseconds
        .checked_mul(50)
        .and_then(|value| value.checked_add(count * 500))
        .map(|value| value / count / 1_000)
        .ok_or_else(|| CoreError::Invalid("每 50 条耗时计算溢出".into()))
}

fn reduction_basis_points(baseline: u64, assisted: u64) -> CoreResult<i64> {
    let baseline = i128::from(baseline);
    let assisted = i128::from(assisted);
    let value = (baseline - assisted)
        .checked_mul(10_000)
        .and_then(|value| value.checked_div(baseline))
        .ok_or_else(|| CoreError::Invalid("减负比例计算失败".into()))?;
    i64::try_from(value).map_err(|_| CoreError::Invalid("减负比例超出范围".into()))
}

fn aggregate_workflow(
    observations: &[&RecitationPilotObservation],
) -> CoreResult<RecitationPilotWorkflowMetrics> {
    let count = observations.len() as u64;
    let durations = observations
        .iter()
        .map(|observation| observation.teacher_active_milliseconds)
        .collect::<Vec<_>>();
    let total = durations.iter().try_fold(0_u64, |sum, value| {
        sum.checked_add(*value)
            .ok_or_else(|| CoreError::Invalid("老师主动耗时汇总溢出".into()))
    })?;
    let full_playback_count = observations
        .iter()
        .filter(|item| item.listening_mode == RecitationPilotListeningMode::FullPlayback)
        .count() as u64;
    let suspicion_only_count = observations
        .iter()
        .filter(|item| item.listening_mode == RecitationPilotListeningMode::SuspicionOnly)
        .count() as u64;
    let no_playback_count = observations
        .iter()
        .filter(|item| item.listening_mode == RecitationPilotListeningMode::NoPlayback)
        .count() as u64;
    let reviewed_point_count = observations.iter().try_fold(0_u64, |sum, item| {
        sum.checked_add(item.reviewed_point_count)
            .ok_or_else(|| CoreError::Invalid("逐点评审数汇总溢出".into()))
    })?;
    let corrected_point_count = observations.iter().try_fold(0_u64, |sum, item| {
        sum.checked_add(item.corrected_point_count)
            .ok_or_else(|| CoreError::Invalid("逐点修正数汇总溢出".into()))
    })?;
    let initial_asr_failure_count = observations
        .iter()
        .filter(|item| {
            matches!(
                item.asr_outcome,
                RecitationPilotAsrOutcome::FailedUnrecovered
                    | RecitationPilotAsrOutcome::RecoveredAfterRetry
            )
        })
        .count() as u64;
    let recovered_asr_count = observations
        .iter()
        .filter(|item| item.asr_outcome == RecitationPilotAsrOutcome::RecoveredAfterRetry)
        .count() as u64;
    let unrecovered_asr_failure_count = observations
        .iter()
        .filter(|item| item.asr_outcome == RecitationPilotAsrOutcome::FailedUnrecovered)
        .count() as u64;
    let asr_attempt_count = observations
        .iter()
        .filter(|item| {
            matches!(
                item.asr_outcome,
                RecitationPilotAsrOutcome::SucceededFirstAttempt
                    | RecitationPilotAsrOutcome::FailedUnrecovered
                    | RecitationPilotAsrOutcome::RecoveredAfterRetry
            )
        })
        .count() as u64;
    let asr_cache_hit_count = observations
        .iter()
        .filter(|item| item.asr_outcome == RecitationPilotAsrOutcome::CacheHit)
        .count() as u64;
    let association_anomaly_count = observations
        .iter()
        .filter(|item| item.association_anomaly_detected)
        .count() as u64;
    let completed_first_attempt_count = observations
        .iter()
        .filter(|item| item.completed_first_attempt)
        .count() as u64;
    let completed_without_help_count = observations
        .iter()
        .filter(|item| item.completed_without_help)
        .count() as u64;
    let total_queue_wait_seconds = observations.iter().try_fold(0_u64, |sum, item| {
        sum.checked_add(item.queue_wait_seconds)
            .ok_or_else(|| CoreError::Invalid("队列等待汇总溢出".into()))
    })?;
    let total_machine_wait_milliseconds = observations.iter().try_fold(0_u64, |sum, item| {
        sum.checked_add(item.machine_wait_milliseconds)
            .ok_or_else(|| CoreError::Invalid("机器等待汇总溢出".into()))
    })?;
    Ok(RecitationPilotWorkflowMetrics {
        observation_count: count,
        total_teacher_active_milliseconds: total,
        seconds_per_50_items: seconds_per_50(total, count)?,
        average_item_milliseconds: rounded_average(total, count, "单条耗时")?,
        item_time_percentiles: RecitationPilotPercentiles {
            p50_milliseconds: percentile(&durations, 50),
            p80_milliseconds: percentile(&durations, 80),
            p95_milliseconds: percentile(&durations, 95),
        },
        full_playback_count,
        full_playback_rate_basis_points: rate_basis_points(full_playback_count, count)?,
        suspicion_only_count,
        suspicion_only_rate_basis_points: rate_basis_points(suspicion_only_count, count)?,
        no_playback_count,
        no_playback_rate_basis_points: rate_basis_points(no_playback_count, count)?,
        total_queue_wait_seconds,
        longest_item_queue_wait_seconds: observations
            .iter()
            .map(|item| item.queue_wait_seconds)
            .max()
            .unwrap_or_default(),
        total_machine_wait_milliseconds,
        reviewed_point_count,
        corrected_point_count,
        point_correction_rate_basis_points: optional_rate_basis_points(
            corrected_point_count,
            reviewed_point_count,
        )?,
        asr_attempt_count,
        asr_cache_hit_count,
        initial_asr_failure_count,
        recovered_asr_count,
        unrecovered_asr_failure_count,
        asr_failure_rate_basis_points: optional_rate_basis_points(
            initial_asr_failure_count,
            asr_attempt_count,
        )?,
        asr_recovery_rate_basis_points: optional_rate_basis_points(
            recovered_asr_count,
            initial_asr_failure_count,
        )?,
        association_anomaly_count,
        association_anomaly_rate_basis_points: rate_basis_points(association_anomaly_count, count)?,
        completed_first_attempt_count,
        completed_first_attempt_rate_basis_points: rate_basis_points(
            completed_first_attempt_count,
            count,
        )?,
        completed_without_help_count,
        completed_without_help_rate_basis_points: rate_basis_points(
            completed_without_help_count,
            count,
        )?,
    })
}

fn aggregate_daily(
    snapshots: &[&RecitationPilotDailySnapshot],
) -> CoreResult<RecitationPilotDailyMetrics> {
    let mut eligible_count = 0_u64;
    let mut completed_count = 0_u64;
    let mut backlog_count_total = 0_u64;
    let mut completed_first_attempt_count = 0_u64;
    for snapshot in snapshots {
        eligible_count = eligible_count
            .checked_add(snapshot.eligible_count)
            .ok_or_else(|| CoreError::Invalid("每日 eligible 汇总溢出".into()))?;
        completed_count = completed_count
            .checked_add(snapshot.completed_count)
            .ok_or_else(|| CoreError::Invalid("每日完成数汇总溢出".into()))?;
        backlog_count_total = backlog_count_total
            .checked_add(snapshot.backlog_count)
            .ok_or_else(|| CoreError::Invalid("每日积压汇总溢出".into()))?;
        completed_first_attempt_count = completed_first_attempt_count
            .checked_add(snapshot.completed_first_attempt_count)
            .ok_or_else(|| CoreError::Invalid("每日一次完成数汇总溢出".into()))?;
    }
    Ok(RecitationPilotDailyMetrics {
        snapshot_count: snapshots.len() as u64,
        eligible_count,
        completed_count,
        backlog_count_total,
        maximum_day_end_backlog_count: snapshots
            .iter()
            .map(|item| item.backlog_count)
            .max()
            .unwrap_or_default(),
        longest_wait_seconds: snapshots
            .iter()
            .map(|item| item.longest_wait_seconds)
            .max()
            .unwrap_or_default(),
        completed_first_attempt_count,
    })
}

fn payload_sha256(payload: &RecitationPilotReportPayload) -> CoreResult<String> {
    serde_json::to_vec(payload)
        .map(|bytes| hashing::sha256_hex(&bytes))
        .map_err(|error| CoreError::Config(format!("背诵试点报告序列化失败：{error}")))
}

impl RecitationPilotReport {
    pub fn validate(&self) -> CoreResult<()> {
        if self.payload.schema_version != RECITATION_PILOT_REPORT_SCHEMA_VERSION {
            return Err(CoreError::Invalid("背诵试点报告 schema 版本不支持".into()));
        }
        required_opaque_id(&self.payload.session_id, "session_id")?;
        required_opaque_id(&self.payload.dataset_id, "dataset_id")?;
        validate_version(&self.payload.dataset_version, "dataset_version")?;
        validate_version(
            &self.payload.observation_policy_version,
            "observation_policy_version",
        )?;
        normalized_sha256(&self.payload.sample_scope_sha256, "样本范围引用")?;
        normalized_sha256(&self.payload.golden_report_sha256, "黄金集报告引用")?;
        parse_time(&self.payload.generated_at, "generated_at")?;
        parse_date(&self.payload.evaluated_on, "evaluated_on")?;
        let teachers = self
            .payload
            .teacher_ref_sha256s
            .iter()
            .map(|value| normalized_sha256(value, "老师引用"))
            .collect::<CoreResult<BTreeSet<_>>>()?;
        if teachers.is_empty()
            || teachers.into_iter().collect::<Vec<_>>() != self.payload.teacher_ref_sha256s
        {
            return Err(CoreError::Invalid(
                "报告老师 hash 必须排序、不重复且不能为空".into(),
            ));
        }
        if self.payload.release_authorized
            || !self.payload.content_excluded
            || !self.payload.paths_excluded
            || !self.payload.student_identity_excluded
        {
            return Err(CoreError::Invalid(
                "背诵试点报告必须排除正文/路径/学生身份且不能授权发布".into(),
            ));
        }
        if self.payload.time_saving_claim_allowed
            && (!self.payload.real_data_authorization_verified
                || !self.payload.minimum_pair_count_met
                || !self.payload.counterbalanced_sequence_present
                || self.payload.source_kind != RecitationPilotSourceKind::RealRestricted)
        {
            return Err(CoreError::Invalid(
                "减负声明缺少真实授权、最小样本或顺序平衡门禁".into(),
            ));
        }
        if normalized_sha256(&self.report_sha256, "报告 hash")? != payload_sha256(&self.payload)?
        {
            return Err(CoreError::Invalid("背诵试点报告 hash 不匹配".into()));
        }
        Ok(())
    }
}

fn evaluate_validated(
    set: &RecitationPilotObservationSet,
    generated_at: &str,
    evaluated_on: &str,
    real_data_authorization_verified: bool,
) -> CoreResult<RecitationPilotReport> {
    let generated_at_value = parse_time(generated_at, "generated_at")?;
    let evaluated_on_value = parse_date(evaluated_on, "evaluated_on")?;
    let pairs = pair_observations(set)?;
    let daily_pairs = pair_daily_snapshots(set)?;
    let latest_completed_at = pairs
        .iter()
        .flat_map(|pair| [&pair.baseline.completed_at, &pair.assisted.completed_at])
        .map(|value| parse_time(value, "完成时间"))
        .collect::<CoreResult<Vec<_>>>()?
        .into_iter()
        .max()
        .ok_or_else(|| CoreError::Invalid("背诵试点观察不能为空".into()))?;
    if generated_at_value < latest_completed_at {
        return Err(CoreError::Invalid(
            "报告生成时间不能早于任何观察完成时间".into(),
        ));
    }
    if evaluated_on_value < latest_completed_at.date_naive() {
        return Err(CoreError::Invalid(
            "evaluated_on 不能早于任何观察完成日期".into(),
        ));
    }

    let baseline_observations = pairs.iter().map(|pair| &pair.baseline).collect::<Vec<_>>();
    let assisted_observations = pairs.iter().map(|pair| &pair.assisted).collect::<Vec<_>>();
    let baseline_daily = daily_pairs
        .iter()
        .map(|pair| &pair.baseline)
        .collect::<Vec<_>>();
    let assisted_daily = daily_pairs
        .iter()
        .map(|pair| &pair.assisted)
        .collect::<Vec<_>>();
    let manual_baseline = aggregate_workflow(&baseline_observations)?;
    let assisted_review = aggregate_workflow(&assisted_observations)?;

    let pair_count = pairs.len() as u64;
    let manual_first_pair_count = pairs
        .iter()
        .filter(|pair| pair.baseline.sequence_arm == RecitationPilotSequenceArm::ManualFirst)
        .count() as u64;
    let assisted_first_pair_count = pair_count - manual_first_pair_count;
    let machine_pass_teacher_fail_count = pairs
        .iter()
        .filter(|pair| {
            pair.assisted.machine_suggestion == Some(RecitationGoldenSuggestion::Pass)
                && pair.assisted.teacher_outcome == RecitationGoldenTeacherOutcome::Fail
        })
        .count() as u64;
    let machine_fail_teacher_pass_count = pairs
        .iter()
        .filter(|pair| {
            pair.assisted.machine_suggestion == Some(RecitationGoldenSuggestion::Fail)
                && pair.assisted.teacher_outcome == RecitationGoldenTeacherOutcome::Pass
        })
        .count() as u64;
    let comparison = RecitationPilotComparisonMetrics {
        pair_count,
        manual_first_pair_count,
        assisted_first_pair_count,
        teacher_outcome_disagreement_count: pairs
            .iter()
            .filter(|pair| pair.baseline.teacher_outcome != pair.assisted.teacher_outcome)
            .count() as u64,
        active_time_reduction_basis_points: reduction_basis_points(
            manual_baseline.total_teacher_active_milliseconds,
            assisted_review.total_teacher_active_milliseconds,
        )?,
        machine_pass_teacher_fail_count,
        machine_pass_teacher_fail_rate_basis_points: rate_basis_points(
            machine_pass_teacher_fail_count,
            pair_count,
        )?,
        machine_fail_teacher_pass_count,
        machine_fail_teacher_pass_rate_basis_points: rate_basis_points(
            machine_fail_teacher_pass_count,
            pair_count,
        )?,
    };
    let minimum_pair_count_met = pair_count >= RECITATION_PILOT_MINIMUM_PAIR_COUNT;
    let counterbalanced_sequence_present =
        manual_first_pair_count > 0 && assisted_first_pair_count > 0;
    let time_saving_claim_allowed = set.time_saving_claim_requested
        && set.contains_real_data()
        && real_data_authorization_verified
        && minimum_pair_count_met
        && counterbalanced_sequence_present
        && comparison.active_time_reduction_basis_points > 0;
    let teacher_ref_sha256s = pairs
        .iter()
        .map(|pair| pair.baseline.teacher_ref_sha256.trim().to_ascii_lowercase())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let payload = RecitationPilotReportPayload {
        schema_version: RECITATION_PILOT_REPORT_SCHEMA_VERSION,
        session_id: set.session_id.clone(),
        source_kind: set.source_kind,
        dataset_id: set.dataset_id.clone(),
        dataset_version: set.dataset_version.clone(),
        sample_scope_sha256: set.sample_scope_sha256.trim().to_ascii_lowercase(),
        golden_report_sha256: set.golden_report_sha256.trim().to_ascii_lowercase(),
        observation_policy_version: set.observation_policy_version.clone(),
        generated_at: generated_at.to_owned(),
        evaluated_on: evaluated_on.to_owned(),
        teacher_ref_sha256s,
        comparison,
        manual_baseline,
        assisted_review,
        manual_daily: aggregate_daily(&baseline_daily)?,
        assisted_daily: aggregate_daily(&assisted_daily)?,
        real_data_authorization_verified,
        minimum_pair_count_met,
        counterbalanced_sequence_present,
        time_saving_claim_allowed,
        release_authorized: false,
        content_excluded: true,
        paths_excluded: true,
        student_identity_excluded: true,
    };
    let report = RecitationPilotReport {
        report_sha256: payload_sha256(&payload)?,
        payload,
    };
    report.validate()?;
    Ok(report)
}

pub fn evaluate_recitation_pilot(
    set: &RecitationPilotObservationSet,
    generated_at: &str,
    evaluated_on: &str,
) -> CoreResult<RecitationPilotReport> {
    set.validate()?;
    if set.contains_real_data() {
        return Err(CoreError::Invalid(
            "真实背诵试点必须使用带有效授权的评估入口".into(),
        ));
    }
    evaluate_validated(set, generated_at, evaluated_on, false)
}

pub fn evaluate_real_recitation_pilot(
    set: &RecitationPilotObservationSet,
    authorization: &RecitationPilotAuthorization,
    generated_at: &str,
    evaluated_on: &str,
) -> CoreResult<RecitationPilotReport> {
    set.validate()?;
    authorization.validate_for(set, evaluated_on)?;
    evaluate_validated(set, generated_at, evaluated_on, true)
}

fn read_existing(path: &Path) -> CoreResult<RecitationPilotReport> {
    let metadata = fs::symlink_metadata(path).map_err(|error| CoreError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CoreError::Invalid(
            "背诵试点报告目标必须是普通文件，不能是符号链接".into(),
        ));
    }
    let report: RecitationPilotReport =
        serde_json::from_slice(&fs::read(path).map_err(|error| CoreError::Io(error.to_string()))?)
            .map_err(|error| CoreError::Invalid(format!("既有背诵试点报告非法：{error}")))?;
    report.validate()?;
    Ok(report)
}

pub fn write_recitation_pilot_report_once(
    path: &Path,
    report: &RecitationPilotReport,
) -> CoreResult<RecitationPilotReport> {
    report.validate()?;
    if path.exists() {
        let existing = read_existing(path)?;
        if existing.report_sha256 == report.report_sha256 {
            return Ok(existing);
        }
        return Err(CoreError::Invalid(
            "同一输出位置已存在不同背诵试点报告，拒绝覆盖".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::Invalid("背诵试点报告缺少父目录".into()))?;
    let parent_meta =
        fs::symlink_metadata(parent).map_err(|error| CoreError::Io(error.to_string()))?;
    if parent_meta.file_type().is_symlink() || !parent_meta.is_dir() {
        return Err(CoreError::Invalid(
            "背诵试点报告父目录必须是已存在的普通目录".into(),
        ));
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CoreError::Invalid("背诵试点报告文件名非法".into()))?;
    let pending_path: PathBuf =
        parent.join(format!(".{file_name}.pending-{}", ids::new_public_id()));
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| CoreError::Config(format!("背诵试点报告序列化失败：{error}")))?;
    let write_result = (|| -> CoreResult<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&pending_path)
            .map_err(|error| CoreError::Io(error.to_string()))?;
        file.write_all(&bytes)
            .map_err(|error| CoreError::Io(error.to_string()))?;
        file.sync_all()
            .map_err(|error| CoreError::Io(error.to_string()))?;
        fs::hard_link(&pending_path, path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                CoreError::Invalid("背诵试点报告输出已被另一请求占用".into())
            } else {
                CoreError::Io(error.to_string())
            }
        })?;
        Ok(())
    })();
    let _ = fs::remove_file(&pending_path);
    write_result?;
    read_existing(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(byte: char) -> String {
        std::iter::repeat_n(byte, 64).collect()
    }

    fn observation(index: usize, workflow: RecitationPilotWorkflow) -> RecitationPilotObservation {
        let assisted = workflow == RecitationPilotWorkflow::AssistedReview;
        let duration = if assisted {
            (index as u64 + 1) * 5_000
        } else {
            (index as u64 + 1) * 10_000
        };
        RecitationPilotObservation {
            observation_id: format!(
                "obs-{index}-{}",
                if assisted { "assisted" } else { "manual" }
            ),
            comparison_group_id: format!("pair-{index}"),
            workflow,
            sequence_arm: if index % 2 == 0 {
                RecitationPilotSequenceArm::ManualFirst
            } else {
                RecitationPilotSequenceArm::AssistedFirst
            },
            teacher_ref_sha256: hash('b'),
            case_ref_sha256: hashing::sha256_hex(format!("case-{index}").as_bytes()),
            started_at: "2026-07-18T08:00:00Z".into(),
            completed_at: "2026-07-18T08:01:00Z".into(),
            teacher_active_milliseconds: duration,
            queue_wait_seconds: index as u64 * 60,
            machine_wait_milliseconds: if assisted { 1_000 } else { 0 },
            teacher_outcome: if index == 0 {
                RecitationGoldenTeacherOutcome::Fail
            } else {
                RecitationGoldenTeacherOutcome::Pass
            },
            machine_suggestion: assisted.then_some(if index == 0 {
                RecitationGoldenSuggestion::Pass
            } else if index == 1 {
                RecitationGoldenSuggestion::Fail
            } else {
                RecitationGoldenSuggestion::Pass
            }),
            listening_mode: if assisted {
                match index {
                    0 => RecitationPilotListeningMode::FullPlayback,
                    1 | 2 => RecitationPilotListeningMode::SuspicionOnly,
                    _ => RecitationPilotListeningMode::NoPlayback,
                }
            } else {
                RecitationPilotListeningMode::FullPlayback
            },
            reviewed_point_count: if assisted { 2 } else { 0 },
            corrected_point_count: if assisted && index < 2 { 1 } else { 0 },
            asr_outcome: if assisted {
                match index {
                    0 => RecitationPilotAsrOutcome::SucceededFirstAttempt,
                    1 => RecitationPilotAsrOutcome::RecoveredAfterRetry,
                    2 => RecitationPilotAsrOutcome::FailedUnrecovered,
                    _ => RecitationPilotAsrOutcome::CacheHit,
                }
            } else {
                RecitationPilotAsrOutcome::NotApplicable
            },
            association_anomaly_detected: assisted && index == 1,
            completed_first_attempt: !assisted || index != 2,
            completed_without_help: !assisted || index != 2,
        }
    }

    fn valid_set() -> RecitationPilotObservationSet {
        let mut observations = Vec::new();
        for index in 0..4 {
            let mut baseline = observation(index, RecitationPilotWorkflow::ManualBaseline);
            let mut assisted = observation(index, RecitationPilotWorkflow::AssistedReview);
            if index == 2 {
                assisted.machine_suggestion = None;
                assisted.listening_mode = RecitationPilotListeningMode::FullPlayback;
            }
            baseline.teacher_outcome = assisted.teacher_outcome;
            observations.extend([baseline, assisted]);
        }
        RecitationPilotObservationSet {
            schema_version: RECITATION_PILOT_SCHEMA_VERSION,
            session_id: "recitation-pilot-contract-001".into(),
            source_kind: RecitationPilotSourceKind::SyntheticContract,
            dataset_id: "recitation-golden-contract-v1".into(),
            dataset_version: "1".into(),
            sample_scope_sha256: hash('a'),
            golden_report_sha256: hash('c'),
            observation_policy_version: "recitation-shadow-v1".into(),
            time_saving_claim_requested: false,
            governance: RecitationPilotGovernance {
                storage_scope: RecitationPilotStorageScope::RepositorySynthetic,
                personal_identifiers_removed: true,
                content_excluded: true,
                paths_excluded: true,
                privacy_reviewed: true,
                privacy_reviewed_by_ref_sha256: hash('d'),
                retention_policy_version: "pilot-local-v1".into(),
                pilot_gate_id: None,
                pilot_gate_policy_sha256: None,
            },
            observations,
            daily_snapshots: vec![
                RecitationPilotDailySnapshot {
                    snapshot_id: "day-1-manual".into(),
                    comparison_day_group_id: "day-pair-1".into(),
                    workflow: RecitationPilotWorkflow::ManualBaseline,
                    teacher_ref_sha256: hash('b'),
                    business_date: "2026-07-18".into(),
                    eligible_count: 4,
                    completed_count: 3,
                    backlog_count: 1,
                    longest_wait_seconds: 240,
                    completed_first_attempt_count: 3,
                },
                RecitationPilotDailySnapshot {
                    snapshot_id: "day-1-assisted".into(),
                    comparison_day_group_id: "day-pair-1".into(),
                    workflow: RecitationPilotWorkflow::AssistedReview,
                    teacher_ref_sha256: hash('b'),
                    business_date: "2026-07-18".into(),
                    eligible_count: 4,
                    completed_count: 4,
                    backlog_count: 0,
                    longest_wait_seconds: 180,
                    completed_first_attempt_count: 3,
                },
            ],
        }
    }

    fn authorization() -> RecitationPilotAuthorization {
        RecitationPilotAuthorization {
            schema_version: RECITATION_PILOT_SCHEMA_VERSION,
            gate_id: "recitation-pilot-gate-001".into(),
            dataset_id: "recitation-golden-contract-v1".into(),
            dataset_version: "1".into(),
            state: RecitationPilotAuthorizationState::Approved,
            allowed_use: "recitation_pilot_metrics".into(),
            valid_from: "2026-07-01".into(),
            valid_through: "2026-07-31".into(),
            authorization_ref_sha256: hash('e'),
            privacy_review_ref_sha256: hash('f'),
            data_rights_drill_ref_sha256: hash('1'),
            approved_by_ref_sha256: hash('2'),
            approved_at: "2026-07-01T00:00:00Z".into(),
            raw_audio_local_only: true,
            cloud_processing_disclosed: true,
        }
    }

    #[test]
    fn evaluates_synthetic_contract_without_claiming_real_savings() {
        let report =
            evaluate_recitation_pilot(&valid_set(), "2026-07-18T09:00:00Z", "2026-07-18").unwrap();
        assert_eq!(report.payload.comparison.pair_count, 4);
        assert_eq!(
            report.payload.comparison.active_time_reduction_basis_points,
            5_000
        );
        assert_eq!(report.payload.manual_baseline.seconds_per_50_items, 1_250);
        assert_eq!(report.payload.assisted_review.seconds_per_50_items, 625);
        assert_eq!(
            report
                .payload
                .manual_baseline
                .item_time_percentiles
                .p50_milliseconds,
            20_000
        );
        assert_eq!(
            report
                .payload
                .assisted_review
                .item_time_percentiles
                .p80_milliseconds,
            20_000
        );
        assert_eq!(report.payload.assisted_review.full_playback_count, 2);
        assert_eq!(report.payload.assisted_review.suspicion_only_count, 1);
        assert_eq!(report.payload.comparison.machine_pass_teacher_fail_count, 1);
        assert_eq!(report.payload.comparison.machine_fail_teacher_pass_count, 1);
        assert_eq!(report.payload.assisted_review.reviewed_point_count, 8);
        assert_eq!(report.payload.assisted_review.corrected_point_count, 2);
        assert_eq!(
            report
                .payload
                .assisted_review
                .point_correction_rate_basis_points,
            Some(2_500)
        );
        assert_eq!(report.payload.assisted_review.initial_asr_failure_count, 2);
        assert_eq!(report.payload.assisted_review.asr_attempt_count, 3);
        assert_eq!(report.payload.assisted_review.asr_cache_hit_count, 1);
        assert_eq!(
            report.payload.assisted_review.asr_failure_rate_basis_points,
            Some(6_666)
        );
        assert_eq!(report.payload.assisted_review.recovered_asr_count, 1);
        assert!(!report.payload.real_data_authorization_verified);
        assert!(!report.payload.minimum_pair_count_met);
        assert!(!report.payload.time_saving_claim_allowed);
        assert!(!report.payload.release_authorized);
        report.validate().unwrap();
    }

    #[test]
    fn repository_contract_produces_fixed_report_hash() {
        let set: RecitationPilotObservationSet = serde_json::from_str(include_str!(
            "../tests/fixtures/pilot_metrics/synthetic_contract_observations_v1.json"
        ))
        .unwrap();
        let report =
            evaluate_recitation_pilot(&set, "2026-07-18T09:00:00Z", "2026-07-18").unwrap();
        assert_eq!(
            report.report_sha256,
            "ce7163281aa3397678c1f277b6787d772b4b6559ea01bc63bfbd1981ac780dfb"
        );
    }

    #[test]
    fn rejects_unpaired_or_mismatched_observations() {
        let mut set = valid_set();
        set.observations.pop();
        assert!(set.validate().is_err());

        let mut set = valid_set();
        set.observations
            .iter_mut()
            .find(|item| item.workflow == RecitationPilotWorkflow::AssistedReview)
            .unwrap()
            .teacher_ref_sha256 = hash('9');
        assert!(set.validate().is_err());

        let mut set = valid_set();
        let first_case = set.observations[0].case_ref_sha256.clone();
        for item in set
            .observations
            .iter_mut()
            .filter(|item| item.comparison_group_id == "pair-1")
        {
            item.case_ref_sha256 = first_case.clone();
        }
        assert!(set.validate().is_err());
    }

    #[test]
    fn rejects_machine_fields_in_manual_baseline() {
        let mut set = valid_set();
        set.observations
            .iter_mut()
            .find(|item| item.workflow == RecitationPilotWorkflow::ManualBaseline)
            .unwrap()
            .machine_suggestion = Some(RecitationGoldenSuggestion::Pass);
        assert!(set.validate().is_err());
    }

    #[test]
    fn rejects_unrecovered_asr_without_full_playback() {
        let mut set = valid_set();
        let item = set
            .observations
            .iter_mut()
            .find(|item| item.asr_outcome == RecitationPilotAsrOutcome::FailedUnrecovered)
            .unwrap();
        item.listening_mode = RecitationPilotListeningMode::SuspicionOnly;
        assert!(set.validate().is_err());
    }

    #[test]
    fn rejects_bad_point_counts_and_timing() {
        let mut set = valid_set();
        let item = set
            .observations
            .iter_mut()
            .find(|item| item.workflow == RecitationPilotWorkflow::AssistedReview)
            .unwrap();
        item.corrected_point_count = item.reviewed_point_count + 1;
        assert!(set.validate().is_err());

        let mut set = valid_set();
        set.observations[0].teacher_active_milliseconds = 61_000;
        assert!(set.validate().is_err());
    }

    #[test]
    fn rejects_unpaired_daily_snapshots() {
        let mut set = valid_set();
        set.daily_snapshots.pop();
        assert!(set.validate().is_err());

        let mut set = valid_set();
        set.daily_snapshots[0].backlog_count = 2;
        assert!(set.validate().is_err());
    }

    #[test]
    fn rejects_synthetic_claim_or_real_evaluation_without_authorization() {
        let mut set = valid_set();
        set.time_saving_claim_requested = true;
        assert!(set.validate().is_err());

        let mut set = valid_set();
        set.source_kind = RecitationPilotSourceKind::RealRestricted;
        set.governance.storage_scope = RecitationPilotStorageScope::LocalRestricted;
        set.governance.pilot_gate_id = Some("recitation-pilot-gate-001".into());
        set.governance.pilot_gate_policy_sha256 = Some(hash('a'));
        assert!(evaluate_recitation_pilot(&set, "2026-07-18T09:00:00Z", "2026-07-18").is_err());
    }

    #[test]
    fn real_claim_requires_authorization_minimum_sample_and_counterbalance() {
        let authorization = authorization();
        let mut set = valid_set();
        let template_pairs = pair_observations(&set).unwrap();
        set.observations.clear();
        for index in 0..RECITATION_PILOT_MINIMUM_PAIR_COUNT as usize {
            let template = &template_pairs[index % template_pairs.len()];
            let mut baseline = template.baseline.clone();
            let mut assisted = template.assisted.clone();
            baseline.observation_id = format!("real-{index}-manual");
            assisted.observation_id = format!("real-{index}-assisted");
            baseline.comparison_group_id = format!("real-pair-{index}");
            assisted.comparison_group_id = baseline.comparison_group_id.clone();
            let arm = if index % 2 == 0 {
                RecitationPilotSequenceArm::ManualFirst
            } else {
                RecitationPilotSequenceArm::AssistedFirst
            };
            baseline.sequence_arm = arm;
            assisted.sequence_arm = arm;
            baseline.case_ref_sha256 = hashing::sha256_hex(format!("real-case-{index}").as_bytes());
            assisted.case_ref_sha256 = baseline.case_ref_sha256.clone();
            set.observations.extend([baseline, assisted]);
        }
        set.source_kind = RecitationPilotSourceKind::RealRestricted;
        set.time_saving_claim_requested = true;
        set.governance.storage_scope = RecitationPilotStorageScope::LocalRestricted;
        set.governance.pilot_gate_id = Some(authorization.gate_id.clone());
        set.governance.pilot_gate_policy_sha256 = Some(authorization.policy_sha256().unwrap());
        let report = evaluate_real_recitation_pilot(
            &set,
            &authorization,
            "2026-07-18T09:00:00Z",
            "2026-07-18",
        )
        .unwrap();
        assert!(report.payload.real_data_authorization_verified);
        assert!(report.payload.minimum_pair_count_met);
        assert!(report.payload.counterbalanced_sequence_present);
        assert!(report.payload.time_saving_claim_allowed);
        assert!(!report.payload.release_authorized);
    }

    #[test]
    fn rejects_expired_or_drifted_authorization() {
        let authorization = authorization();
        let mut set = valid_set();
        set.source_kind = RecitationPilotSourceKind::RealRestricted;
        set.governance.storage_scope = RecitationPilotStorageScope::LocalRestricted;
        set.governance.pilot_gate_id = Some(authorization.gate_id.clone());
        set.governance.pilot_gate_policy_sha256 = Some(authorization.policy_sha256().unwrap());
        assert!(evaluate_real_recitation_pilot(
            &set,
            &authorization,
            "2026-08-01T09:00:00Z",
            "2026-08-01"
        )
        .is_err());

        set.governance.pilot_gate_policy_sha256 = Some(hash('9'));
        assert!(evaluate_real_recitation_pilot(
            &set,
            &authorization,
            "2026-07-18T09:00:00Z",
            "2026-07-18"
        )
        .is_err());
    }

    #[test]
    fn report_hash_detects_tampering() {
        let mut report =
            evaluate_recitation_pilot(&valid_set(), "2026-07-18T09:00:00Z", "2026-07-18").unwrap();
        report.payload.assisted_review.full_playback_count += 1;
        assert!(report.validate().is_err());
    }

    #[test]
    fn writer_is_idempotent_and_rejects_overwrite() {
        let report =
            evaluate_recitation_pilot(&valid_set(), "2026-07-18T09:00:00Z", "2026-07-18").unwrap();
        let root =
            std::env::temp_dir().join(format!("jiaofu-recitation-pilot-{}", ids::new_public_id()));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("report.json");
        let first = write_recitation_pilot_report_once(&output, &report).unwrap();
        let second = write_recitation_pilot_report_once(&output, &report).unwrap();
        assert_eq!(first.report_sha256, second.report_sha256);

        let changed =
            evaluate_recitation_pilot(&valid_set(), "2026-07-18T09:01:00Z", "2026-07-18").unwrap();
        assert!(write_recitation_pilot_report_once(&output, &changed).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn strict_json_rejects_content_fields() {
        let mut value = serde_json::to_value(valid_set()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("student_name".into(), serde_json::json!("not-allowed"));
        assert!(serde_json::from_value::<RecitationPilotObservationSet>(value).is_err());
    }
}
