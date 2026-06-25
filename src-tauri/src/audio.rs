//! 可选 ffmpeg 集成：系统若装了 ffmpeg/ffprobe，则用于「转码为 16k 单声道 wav」(提升火山兼容性)
//! 与「时长探测」。未安装时全部优雅降级（用原文件 / 返回 None），绝不阻断主流程。

use std::path::{Path, PathBuf};
use std::process::Command;

/// 用 ffprobe 读取时长(ms)。失败/未装返回 None。
pub fn ffprobe_duration_ms(path: &str) -> Option<u64> {
    let out = Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            path,
        ])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    let secs: f64 = s.trim().parse().ok()?;
    Some((secs * 1000.0) as u64)
}

/// 转 16k 单声道 wav 到 out_dir 临时文件。ffmpeg 不可用/失败则返回 None（调用方退回原文件）。
pub fn transcode_to_wav16k(path: &str, out_dir: &Path) -> Option<PathBuf> {
    let stem = Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("audio");
    let out = out_dir.join(format!("{stem}.asr16k.wav"));
    let status = Command::new("ffmpeg")
        .args(["-y", "-i", path, "-ar", "16000", "-ac", "1", "-f", "wav"])
        .arg(&out)
        .status()
        .ok()?;
    if status.success() && out.exists() {
        Some(out)
    } else {
        None
    }
}
