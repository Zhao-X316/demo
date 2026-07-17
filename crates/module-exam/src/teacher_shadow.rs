//! 三类材料老师并行影子试点的无正文耗时合同。
//!
//! 本层只接收不透明 ID、hash、时间与聚合计数，不保存学生姓名、文件路径、原图、
//! OCR/答案正文。每个 comparison group 必须同时有人工基线和 AI 辅助复核，并绑定
//! 同一材料、数据集、样本范围与工作量；结果只用于验证减负，不自行授权发布。

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};
use suite_core::domain::{hashing, ids};
use suite_core::error::{CoreError, CoreResult};

use crate::material_golden::MaterialGoldenKind;

pub const TEACHER_SHADOW_SCHEMA_VERSION: i64 = 1;
pub const TEACHER_SHADOW_REPORT_SCHEMA_VERSION: i64 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeacherShadowWorkflow {
    ManualBaseline,
    AssistedReview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherShadowObservation {
    pub observation_id: String,
    pub comparison_group_id: String,
    pub material_kind: MaterialGoldenKind,
    pub workflow: TeacherShadowWorkflow,
    pub dataset_id: String,
    pub sample_scope_sha256: String,
    pub teacher_ref_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_result_sha256: Option<String>,
    pub started_at: String,
    pub completed_at: String,
    pub submission_count: u64,
    pub page_count: u64,
    pub item_count: u64,
    pub teacher_active_seconds: u64,
    pub machine_wait_seconds: u64,
    pub teacher_action_count: u64,
    pub exception_review_count: u64,
    pub corrected_machine_decision_count: u64,
    pub completed_without_help: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherShadowObservationSet {
    pub schema_version: i64,
    pub session_id: String,
    pub observations: Vec<TeacherShadowObservation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherShadowPercentiles {
    pub p50: u64,
    pub p80: u64,
    pub p95: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherShadowMetrics {
    pub pair_count: u64,
    pub submission_count: u64,
    pub page_count: u64,
    pub item_count: u64,
    pub baseline_active_seconds: u64,
    pub assisted_active_seconds: u64,
    pub assisted_machine_wait_seconds: u64,
    pub assisted_teacher_action_count: u64,
    pub assisted_exception_review_count: u64,
    pub assisted_corrected_machine_decision_count: u64,
    pub completed_without_help_pair_count: u64,
    /// 正值表示节省，负值表示辅助流程更慢。10000 = 100%。
    pub active_time_reduction_basis_points: i64,
    pub baseline_seconds_per_100_submissions: TeacherShadowPercentiles,
    pub assisted_seconds_per_100_submissions: TeacherShadowPercentiles,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherShadowMaterialResult {
    pub material_kind: MaterialGoldenKind,
    /// 本材料老师观察实际引用的数据集；排序后写入，便于与机器影子结果逐类核对。
    pub dataset_ids: Vec<String>,
    /// 本材料实际抽样范围的不可逆引用；不保存学生、题目或答案正文。
    pub sample_scope_sha256s: Vec<String>,
    pub metrics: TeacherShadowMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherShadowReportPayload {
    pub schema_version: i64,
    pub session_id: String,
    pub shadow_result_sha256: String,
    pub generated_at: String,
    pub material_results: Vec<TeacherShadowMaterialResult>,
    pub overall: TeacherShadowMetrics,
    pub release_authorized: bool,
    pub content_excluded: bool,
    pub paths_excluded: bool,
    pub student_identity_excluded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeacherShadowReport {
    pub payload: TeacherShadowReportPayload,
    pub report_sha256: String,
}

#[derive(Debug, Clone)]
struct ObservationPair {
    material_kind: MaterialGoldenKind,
    baseline: TeacherShadowObservation,
    assisted: TeacherShadowObservation,
}

fn required_opaque_id(value: &str, field: &str) -> CoreResult<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(CoreError::Invalid(format!(
            "{field} 必须是不超过 128 字符的不透明内部 ID"
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

impl TeacherShadowObservation {
    fn validate(&self) -> CoreResult<()> {
        required_opaque_id(&self.observation_id, "observation_id")?;
        required_opaque_id(&self.comparison_group_id, "comparison_group_id")?;
        required_opaque_id(&self.dataset_id, "dataset_id")?;
        normalized_sha256(&self.sample_scope_sha256, "样本范围引用")?;
        normalized_sha256(&self.teacher_ref_sha256, "老师引用")?;
        let started_at = parse_time(&self.started_at, "开始时间")?;
        let completed_at = parse_time(&self.completed_at, "完成时间")?;
        if completed_at <= started_at {
            return Err(CoreError::Invalid("完成时间必须晚于开始时间".into()));
        }
        let elapsed_seconds = (completed_at - started_at).num_seconds() as u64;
        if self.submission_count == 0 || self.page_count == 0 || self.item_count == 0 {
            return Err(CoreError::Invalid(
                "学生份数、页数和评分单元数都必须为正数".into(),
            ));
        }
        if self.page_count < self.submission_count || self.item_count < self.submission_count {
            return Err(CoreError::Invalid(
                "页数和评分单元数不能少于学生份数".into(),
            ));
        }
        if self.teacher_active_seconds == 0 || self.teacher_active_seconds > elapsed_seconds {
            return Err(CoreError::Invalid(
                "老师主动操作秒数必须为正且不能超过会话时长".into(),
            ));
        }
        if self.machine_wait_seconds > elapsed_seconds {
            return Err(CoreError::Invalid("机器等待秒数不能超过会话时长".into()));
        }
        if self.exception_review_count > self.item_count
            || self.corrected_machine_decision_count > self.item_count
        {
            return Err(CoreError::Invalid(
                "异常复核或机器结论修正数不能超过评分单元数".into(),
            ));
        }
        if self.teacher_action_count < self.exception_review_count {
            return Err(CoreError::Invalid(
                "老师必要操作数不能少于异常复核数".into(),
            ));
        }
        match self.workflow {
            TeacherShadowWorkflow::ManualBaseline => {
                if self.shadow_result_sha256.is_some()
                    || self.machine_wait_seconds != 0
                    || self.exception_review_count != 0
                    || self.corrected_machine_decision_count != 0
                {
                    return Err(CoreError::Invalid(
                        "人工基线不能绑定影子结果或填写机器流程计数".into(),
                    ));
                }
            }
            TeacherShadowWorkflow::AssistedReview => {
                let hash = self
                    .shadow_result_sha256
                    .as_deref()
                    .ok_or_else(|| CoreError::Invalid("辅助复核必须绑定影子结果 hash".into()))?;
                normalized_sha256(hash, "影子结果引用")?;
            }
        }
        Ok(())
    }
}

fn pair_observations(set: &TeacherShadowObservationSet) -> CoreResult<Vec<ObservationPair>> {
    if set.schema_version != TEACHER_SHADOW_SCHEMA_VERSION {
        return Err(CoreError::Invalid("老师影子观察 schema 版本不支持".into()));
    }
    required_opaque_id(&set.session_id, "session_id")?;
    if set.observations.is_empty() {
        return Err(CoreError::Invalid("老师影子观察不能为空".into()));
    }
    let mut observation_ids = BTreeSet::new();
    let mut groups: BTreeMap<String, BTreeMap<TeacherShadowWorkflow, TeacherShadowObservation>> =
        BTreeMap::new();
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
    let observed_materials = set
        .observations
        .iter()
        .map(|observation| observation.material_kind)
        .collect::<BTreeSet<_>>();
    let required_materials = BTreeSet::from([
        MaterialGoldenKind::OrdinaryPaper,
        MaterialGoldenKind::AnswerSheet,
        MaterialGoldenKind::Dictation,
    ]);
    if observed_materials != required_materials {
        return Err(CoreError::Invalid(
            "老师影子试点必须同时包含普通试卷、答题卡和默写三类材料".into(),
        ));
    }
    let mut pairs = Vec::with_capacity(groups.len());
    for (group_id, mut workflows) in groups {
        let baseline = workflows
            .remove(&TeacherShadowWorkflow::ManualBaseline)
            .ok_or_else(|| CoreError::Invalid(format!("{group_id} 缺少人工基线")))?;
        let assisted = workflows
            .remove(&TeacherShadowWorkflow::AssistedReview)
            .ok_or_else(|| CoreError::Invalid(format!("{group_id} 缺少辅助复核")))?;
        if baseline.material_kind != assisted.material_kind
            || baseline.dataset_id != assisted.dataset_id
            || baseline.sample_scope_sha256 != assisted.sample_scope_sha256
            || baseline.teacher_ref_sha256 != assisted.teacher_ref_sha256
            || baseline.submission_count != assisted.submission_count
            || baseline.page_count != assisted.page_count
            || baseline.item_count != assisted.item_count
        {
            return Err(CoreError::Invalid(format!(
                "{group_id} 的人工基线与辅助复核不是同一老师、材料和工作量"
            )));
        }
        pairs.push(ObservationPair {
            material_kind: baseline.material_kind,
            baseline,
            assisted,
        });
    }
    Ok(pairs)
}

impl TeacherShadowObservationSet {
    pub fn validate(&self) -> CoreResult<()> {
        pair_observations(self).map(|_| ())
    }
}

fn percentile(values: &[u64], percentile: u64) -> u64 {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let rank = (sorted.len() as u64 * percentile).div_ceil(100).max(1) as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

fn percentiles(values: &[u64]) -> TeacherShadowPercentiles {
    TeacherShadowPercentiles {
        p50: percentile(values, 50),
        p80: percentile(values, 80),
        p95: percentile(values, 95),
    }
}

fn seconds_per_100(seconds: u64, submissions: u64) -> CoreResult<u64> {
    seconds
        .checked_mul(100)
        .and_then(|value| value.checked_add(submissions / 2))
        .map(|value| value / submissions)
        .ok_or_else(|| CoreError::Invalid("耗时归一化发生溢出".into()))
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

fn aggregate(pairs: &[ObservationPair]) -> CoreResult<TeacherShadowMetrics> {
    let mut submission_count = 0_u64;
    let mut page_count = 0_u64;
    let mut item_count = 0_u64;
    let mut baseline_active_seconds = 0_u64;
    let mut assisted_active_seconds = 0_u64;
    let mut machine_wait_seconds = 0_u64;
    let mut action_count = 0_u64;
    let mut exception_count = 0_u64;
    let mut correction_count = 0_u64;
    let mut completed_without_help = 0_u64;
    let mut baseline_per_100 = Vec::with_capacity(pairs.len());
    let mut assisted_per_100 = Vec::with_capacity(pairs.len());
    for pair in pairs {
        submission_count = submission_count
            .checked_add(pair.baseline.submission_count)
            .ok_or_else(|| CoreError::Invalid("学生份数汇总溢出".into()))?;
        page_count = page_count
            .checked_add(pair.baseline.page_count)
            .ok_or_else(|| CoreError::Invalid("页数汇总溢出".into()))?;
        item_count = item_count
            .checked_add(pair.baseline.item_count)
            .ok_or_else(|| CoreError::Invalid("评分单元数汇总溢出".into()))?;
        baseline_active_seconds = baseline_active_seconds
            .checked_add(pair.baseline.teacher_active_seconds)
            .ok_or_else(|| CoreError::Invalid("人工基线耗时汇总溢出".into()))?;
        assisted_active_seconds = assisted_active_seconds
            .checked_add(pair.assisted.teacher_active_seconds)
            .ok_or_else(|| CoreError::Invalid("辅助复核耗时汇总溢出".into()))?;
        machine_wait_seconds = machine_wait_seconds
            .checked_add(pair.assisted.machine_wait_seconds)
            .ok_or_else(|| CoreError::Invalid("机器等待耗时汇总溢出".into()))?;
        action_count = action_count
            .checked_add(pair.assisted.teacher_action_count)
            .ok_or_else(|| CoreError::Invalid("老师操作数汇总溢出".into()))?;
        exception_count = exception_count
            .checked_add(pair.assisted.exception_review_count)
            .ok_or_else(|| CoreError::Invalid("异常复核数汇总溢出".into()))?;
        correction_count = correction_count
            .checked_add(pair.assisted.corrected_machine_decision_count)
            .ok_or_else(|| CoreError::Invalid("机器结论修正数汇总溢出".into()))?;
        if pair.baseline.completed_without_help && pair.assisted.completed_without_help {
            completed_without_help += 1;
        }
        baseline_per_100.push(seconds_per_100(
            pair.baseline.teacher_active_seconds,
            pair.baseline.submission_count,
        )?);
        assisted_per_100.push(seconds_per_100(
            pair.assisted.teacher_active_seconds,
            pair.assisted.submission_count,
        )?);
    }
    Ok(TeacherShadowMetrics {
        pair_count: pairs.len() as u64,
        submission_count,
        page_count,
        item_count,
        baseline_active_seconds,
        assisted_active_seconds,
        assisted_machine_wait_seconds: machine_wait_seconds,
        assisted_teacher_action_count: action_count,
        assisted_exception_review_count: exception_count,
        assisted_corrected_machine_decision_count: correction_count,
        completed_without_help_pair_count: completed_without_help,
        active_time_reduction_basis_points: reduction_basis_points(
            baseline_active_seconds,
            assisted_active_seconds,
        )?,
        baseline_seconds_per_100_submissions: percentiles(&baseline_per_100),
        assisted_seconds_per_100_submissions: percentiles(&assisted_per_100),
    })
}

fn payload_sha256(payload: &TeacherShadowReportPayload) -> CoreResult<String> {
    serde_json::to_vec(payload)
        .map(|bytes| hashing::sha256_hex(&bytes))
        .map_err(|error| CoreError::Config(format!("老师影子报告序列化失败：{error}")))
}

impl TeacherShadowReport {
    pub fn validate(&self) -> CoreResult<()> {
        if self.payload.schema_version != TEACHER_SHADOW_REPORT_SCHEMA_VERSION {
            return Err(CoreError::Invalid("老师影子报告 schema 版本不支持".into()));
        }
        required_opaque_id(&self.payload.session_id, "session_id")?;
        normalized_sha256(&self.payload.shadow_result_sha256, "影子结果引用")?;
        parse_time(&self.payload.generated_at, "报告生成时间")?;
        if self.payload.release_authorized
            || !self.payload.content_excluded
            || !self.payload.paths_excluded
            || !self.payload.student_identity_excluded
        {
            return Err(CoreError::Invalid(
                "老师影子报告必须排除内容/路径/学生身份且不能授权发布".into(),
            ));
        }
        let kinds = self
            .payload
            .material_results
            .iter()
            .map(|result| result.material_kind)
            .collect::<BTreeSet<_>>();
        if self.payload.material_results.len() != 3
            || kinds
                != BTreeSet::from([
                    MaterialGoldenKind::OrdinaryPaper,
                    MaterialGoldenKind::AnswerSheet,
                    MaterialGoldenKind::Dictation,
                ])
        {
            return Err(CoreError::Invalid(
                "老师影子报告必须完整覆盖三类材料".into(),
            ));
        }
        for result in &self.payload.material_results {
            if result.dataset_ids.is_empty() || result.sample_scope_sha256s.is_empty() {
                return Err(CoreError::Invalid(
                    "老师影子报告必须保留每类材料的数据集与抽样范围引用".into(),
                ));
            }
            let dataset_ids = result
                .dataset_ids
                .iter()
                .map(|value| {
                    required_opaque_id(value, "dataset_id")?;
                    Ok(value.clone())
                })
                .collect::<CoreResult<BTreeSet<_>>>()?;
            if dataset_ids.into_iter().collect::<Vec<_>>() != result.dataset_ids {
                return Err(CoreError::Invalid(
                    "老师影子报告 dataset_ids 必须排序且不重复".into(),
                ));
            }
            let sample_hashes = result
                .sample_scope_sha256s
                .iter()
                .map(|value| normalized_sha256(value, "样本范围引用"))
                .collect::<CoreResult<BTreeSet<_>>>()?;
            if sample_hashes.into_iter().collect::<Vec<_>>() != result.sample_scope_sha256s {
                return Err(CoreError::Invalid(
                    "老师影子报告样本范围 hash 必须排序且不重复".into(),
                ));
            }
        }
        let expected = payload_sha256(&self.payload)?;
        if normalized_sha256(&self.report_sha256, "报告 hash")? != expected {
            return Err(CoreError::Invalid("老师影子报告 hash 不匹配".into()));
        }
        Ok(())
    }
}

pub fn evaluate_teacher_shadow(
    set: &TeacherShadowObservationSet,
    generated_at: &str,
) -> CoreResult<TeacherShadowReport> {
    let generated_at_value = parse_time(generated_at, "报告生成时间")?;
    let pairs = pair_observations(set)?;
    let latest_completed_at = pairs
        .iter()
        .flat_map(|pair| [&pair.baseline.completed_at, &pair.assisted.completed_at])
        .map(|value| parse_time(value, "完成时间"))
        .collect::<CoreResult<Vec<_>>>()?
        .into_iter()
        .max()
        .ok_or_else(|| CoreError::Invalid("老师影子观察不能为空".into()))?;
    if generated_at_value < latest_completed_at {
        return Err(CoreError::Invalid(
            "报告生成时间不能早于任何观察完成时间".into(),
        ));
    }
    let shadow_hashes = pairs
        .iter()
        .map(|pair| {
            normalized_sha256(
                pair.assisted
                    .shadow_result_sha256
                    .as_deref()
                    .unwrap_or_default(),
                "影子结果引用",
            )
        })
        .collect::<CoreResult<BTreeSet<_>>>()?;
    if shadow_hashes.len() != 1 {
        return Err(CoreError::Invalid(
            "同一次老师影子试点必须绑定同一影子结果 hash".into(),
        ));
    }
    let mut by_material: BTreeMap<MaterialGoldenKind, Vec<ObservationPair>> = BTreeMap::new();
    for pair in pairs {
        by_material
            .entry(pair.material_kind)
            .or_default()
            .push(pair);
    }
    let required = BTreeSet::from([
        MaterialGoldenKind::OrdinaryPaper,
        MaterialGoldenKind::AnswerSheet,
        MaterialGoldenKind::Dictation,
    ]);
    if by_material.keys().copied().collect::<BTreeSet<_>>() != required {
        return Err(CoreError::Invalid(
            "老师影子试点必须同时包含普通试卷、答题卡和默写".into(),
        ));
    }
    let all_pairs = by_material
        .values()
        .flat_map(|pairs| pairs.iter().cloned())
        .collect::<Vec<_>>();
    let material_results = by_material
        .iter()
        .map(|(kind, pairs)| {
            let dataset_ids = pairs
                .iter()
                .map(|pair| pair.baseline.dataset_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            let sample_scope_sha256s = pairs
                .iter()
                .map(|pair| {
                    pair.baseline
                        .sample_scope_sha256
                        .trim()
                        .to_ascii_lowercase()
                })
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect();
            Ok(TeacherShadowMaterialResult {
                material_kind: *kind,
                dataset_ids,
                sample_scope_sha256s,
                metrics: aggregate(pairs)?,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    let payload = TeacherShadowReportPayload {
        schema_version: TEACHER_SHADOW_REPORT_SCHEMA_VERSION,
        session_id: set.session_id.clone(),
        shadow_result_sha256: shadow_hashes.into_iter().next().unwrap_or_default(),
        generated_at: generated_at.to_owned(),
        material_results,
        overall: aggregate(&all_pairs)?,
        release_authorized: false,
        content_excluded: true,
        paths_excluded: true,
        student_identity_excluded: true,
    };
    let report = TeacherShadowReport {
        report_sha256: payload_sha256(&payload)?,
        payload,
    };
    report.validate()?;
    Ok(report)
}

fn read_existing(path: &Path) -> CoreResult<TeacherShadowReport> {
    let metadata = fs::symlink_metadata(path).map_err(|error| CoreError::Io(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(CoreError::Invalid(
            "老师影子报告目标必须是普通文件，不能是符号链接".into(),
        ));
    }
    let report: TeacherShadowReport =
        serde_json::from_slice(&fs::read(path).map_err(|error| CoreError::Io(error.to_string()))?)
            .map_err(|error| CoreError::Invalid(format!("既有老师影子报告非法：{error}")))?;
    report.validate()?;
    Ok(report)
}

pub fn write_teacher_shadow_report_once(
    path: &Path,
    report: &TeacherShadowReport,
) -> CoreResult<TeacherShadowReport> {
    report.validate()?;
    if path.exists() {
        let existing = read_existing(path)?;
        if existing.report_sha256 == report.report_sha256 {
            return Ok(existing);
        }
        return Err(CoreError::Invalid(
            "同一输出位置已存在不同老师影子报告，拒绝覆盖".into(),
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| CoreError::Invalid("老师影子报告缺少父目录".into()))?;
    let parent_meta =
        fs::symlink_metadata(parent).map_err(|error| CoreError::Io(error.to_string()))?;
    if parent_meta.file_type().is_symlink() || !parent_meta.is_dir() {
        return Err(CoreError::Invalid(
            "老师影子报告父目录必须是已存在的普通目录".into(),
        ));
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| CoreError::Invalid("老师影子报告文件名非法".into()))?;
    let pending_path: PathBuf =
        parent.join(format!(".{file_name}.pending-{}", ids::new_public_id()));
    let bytes = serde_json::to_vec_pretty(report)
        .map_err(|error| CoreError::Config(format!("老师影子报告序列化失败：{error}")))?;
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
                CoreError::Invalid("老师影子报告输出已被另一请求占用".into())
            } else {
                CoreError::Io(error.to_string())
            }
        })?;
        Ok(())
    })();
    let _ = fs::remove_file(&pending_path);
    if let Err(error) = write_result {
        if path.exists() {
            let existing = read_existing(path)?;
            if existing.report_sha256 == report.report_sha256 {
                return Ok(existing);
            }
        }
        return Err(error);
    }
    read_existing(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(
        group: &str,
        material_kind: MaterialGoldenKind,
        workflow: TeacherShadowWorkflow,
        active: u64,
    ) -> TeacherShadowObservation {
        TeacherShadowObservation {
            observation_id: format!(
                "obs-{group}-{}",
                match workflow {
                    TeacherShadowWorkflow::ManualBaseline => "manual",
                    TeacherShadowWorkflow::AssistedReview => "assisted",
                }
            ),
            comparison_group_id: group.into(),
            material_kind,
            workflow,
            dataset_id: format!("dataset-{group}"),
            sample_scope_sha256: "a".repeat(64),
            teacher_ref_sha256: "b".repeat(64),
            shadow_result_sha256: (workflow == TeacherShadowWorkflow::AssistedReview)
                .then(|| "c".repeat(64)),
            started_at: "2026-07-16T08:00:00Z".into(),
            completed_at: "2026-07-16T09:00:00Z".into(),
            submission_count: 40,
            page_count: 40,
            item_count: 400,
            teacher_active_seconds: active,
            machine_wait_seconds: if workflow == TeacherShadowWorkflow::AssistedReview {
                300
            } else {
                0
            },
            teacher_action_count: if workflow == TeacherShadowWorkflow::AssistedReview {
                30
            } else {
                400
            },
            exception_review_count: if workflow == TeacherShadowWorkflow::AssistedReview {
                20
            } else {
                0
            },
            corrected_machine_decision_count: if workflow == TeacherShadowWorkflow::AssistedReview {
                4
            } else {
                0
            },
            completed_without_help: true,
        }
    }

    fn valid_set() -> TeacherShadowObservationSet {
        let mut observations = Vec::new();
        for (group, kind) in [
            ("ordinary-01", MaterialGoldenKind::OrdinaryPaper),
            ("answer-sheet-01", MaterialGoldenKind::AnswerSheet),
            ("dictation-01", MaterialGoldenKind::Dictation),
        ] {
            observations.push(observation(
                group,
                kind,
                TeacherShadowWorkflow::ManualBaseline,
                2400,
            ));
            observations.push(observation(
                group,
                kind,
                TeacherShadowWorkflow::AssistedReview,
                1200,
            ));
        }
        TeacherShadowObservationSet {
            schema_version: TEACHER_SHADOW_SCHEMA_VERSION,
            session_id: "teacher-shadow-contract-001".into(),
            observations,
        }
    }

    #[test]
    fn three_paired_materials_produce_reproducible_time_report() {
        let report = evaluate_teacher_shadow(&valid_set(), "2026-07-16T09:05:00Z").unwrap();
        assert_eq!(report.payload.material_results.len(), 3);
        assert_eq!(report.payload.overall.pair_count, 3);
        assert_eq!(report.payload.overall.submission_count, 120);
        assert_eq!(
            report.payload.overall.active_time_reduction_basis_points,
            5000
        );
        assert_eq!(
            report
                .payload
                .overall
                .baseline_seconds_per_100_submissions
                .p95,
            6000
        );
        assert_eq!(
            report
                .payload
                .overall
                .assisted_seconds_per_100_submissions
                .p95,
            3000
        );
        assert!(!report.payload.release_authorized);
        report.validate().unwrap();
    }

    #[test]
    fn missing_material_or_pair_fails_closed() {
        let mut set = valid_set();
        set.observations
            .retain(|item| item.material_kind != MaterialGoldenKind::Dictation);
        assert!(evaluate_teacher_shadow(&set, "2026-07-16T09:05:00Z")
            .unwrap_err()
            .to_string()
            .contains("三类材料"));

        let mut set = valid_set();
        set.observations.retain(|item| {
            item.comparison_group_id != "ordinary-01"
                || item.workflow != TeacherShadowWorkflow::AssistedReview
        });
        assert!(evaluate_teacher_shadow(&set, "2026-07-16T09:05:00Z")
            .unwrap_err()
            .to_string()
            .contains("缺少辅助复核"));
    }

    #[test]
    fn mismatched_pair_or_shadow_hash_is_rejected() {
        let mut set = valid_set();
        set.observations
            .iter_mut()
            .find(|item| {
                item.comparison_group_id == "ordinary-01"
                    && item.workflow == TeacherShadowWorkflow::AssistedReview
            })
            .unwrap()
            .item_count = 399;
        assert!(evaluate_teacher_shadow(&set, "2026-07-16T09:05:00Z")
            .unwrap_err()
            .to_string()
            .contains("不是同一老师、材料和工作量"));

        let mut set = valid_set();
        set.observations
            .iter_mut()
            .find(|item| {
                item.comparison_group_id == "dictation-01"
                    && item.workflow == TeacherShadowWorkflow::AssistedReview
            })
            .unwrap()
            .shadow_result_sha256 = Some("d".repeat(64));
        assert!(evaluate_teacher_shadow(&set, "2026-07-16T09:05:00Z")
            .unwrap_err()
            .to_string()
            .contains("同一影子结果"));
    }

    #[test]
    fn manual_and_assisted_semantics_cannot_be_forged() {
        let mut set = valid_set();
        set.observations
            .iter_mut()
            .find(|item| item.workflow == TeacherShadowWorkflow::ManualBaseline)
            .unwrap()
            .shadow_result_sha256 = Some("c".repeat(64));
        assert!(evaluate_teacher_shadow(&set, "2026-07-16T09:05:00Z")
            .unwrap_err()
            .to_string()
            .contains("人工基线"));

        let mut set = valid_set();
        set.observations
            .iter_mut()
            .find(|item| item.workflow == TeacherShadowWorkflow::AssistedReview)
            .unwrap()
            .shadow_result_sha256 = None;
        assert!(evaluate_teacher_shadow(&set, "2026-07-16T09:05:00Z")
            .unwrap_err()
            .to_string()
            .contains("必须绑定影子结果"));
    }

    #[test]
    fn report_is_write_once_and_rejects_different_content() {
        let report = evaluate_teacher_shadow(&valid_set(), "2026-07-16T09:05:00Z").unwrap();
        let root = std::env::temp_dir().join(format!("teacher-shadow-{}", ids::new_public_id()));
        fs::create_dir_all(&root).unwrap();
        let output = root.join("report.json");
        let first = write_teacher_shadow_report_once(&output, &report).unwrap();
        let repeated = write_teacher_shadow_report_once(&output, &report).unwrap();
        assert_eq!(first, repeated);

        let changed = evaluate_teacher_shadow(&valid_set(), "2026-07-16T09:06:00Z").unwrap();
        assert!(write_teacher_shadow_report_once(&output, &changed)
            .unwrap_err()
            .to_string()
            .contains("拒绝覆盖"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn observation_json_rejects_hidden_content_fields() {
        let mut value = serde_json::to_value(valid_set()).unwrap();
        value["observations"][0]["student_name"] = serde_json::json!("不应进入合同");
        let error = serde_json::from_value::<TeacherShadowObservationSet>(value).unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }
}
