//! 纯算法层：无 I/O、无网络、无 Tauri 依赖，全部可单测。
//!
//! - [`normalize`]：文本归一化
//! - [`pinyin_util`]：汉字转拼音（正确率的拼音容错）
//! - [`similarity`]：LCS / 编辑距离 / 覆盖率
//! - [`accuracy`]：正确率门控（覆盖率为主，汉字/拼音两路取较高值）
//! - [`scheduler`]：间隔重复调度（记忆质量 → 下次间隔）

pub mod accuracy;
pub mod hashing;
pub mod ids;
pub mod normalize;
pub mod pinyin_util;
pub mod scheduler;
pub mod similarity;
