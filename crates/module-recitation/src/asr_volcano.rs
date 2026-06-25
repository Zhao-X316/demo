//! 火山/豆包 录音文件识别 provider（占位骨架）。
//!
//! 真正的网络调用（reqwest 上传/轮询、鉴权、utterances→words 映射）在应用外壳
//! 接入 HTTP 客户端后实现（见 P1）。凭据来自设置页，绝不写死。

use suite_core::error::{CoreError, CoreResult};
use suite_core::ports::{MediaInput, RecognizeResult, Recognizer, RecognizerKind};

/// 火山 ASR 配置（运行时从 secrets.json 注入）。
#[derive(Debug, Clone, Default)]
pub struct VolcanoConfig {
    pub app_id: String,
    pub access_token: String,
    pub cluster: String,
    pub endpoint: String,
    pub language: String,
}

pub struct VolcanoAsr {
    pub cfg: VolcanoConfig,
}

impl VolcanoAsr {
    pub fn new(cfg: VolcanoConfig) -> Self {
        Self { cfg }
    }
}

#[async_trait::async_trait]
impl Recognizer for VolcanoAsr {
    async fn recognize(&self, _input: MediaInput<'_>) -> CoreResult<RecognizeResult> {
        // TODO(P1): 接入火山/豆包录音文件识别 API：
        //   1) 用 self.cfg.app_id / access_token 鉴权
        //   2) 上传音频（必要时先 ffmpeg 转 16k 单声道 wav）
        //   3) 轮询/回调取结果，解析整句文本与 utterances(start/end ms)
        //   4) 映射为 RecognizeResult{ text, words, duration_ms, raw_meta(脱敏) }
        Err(CoreError::Recognize(
            "火山 ASR 尚未接线（占位）。在应用外壳接入 reqwest 后实现。".into(),
        ))
    }

    fn kind(&self) -> RecognizerKind {
        RecognizerKind::Asr
    }

    fn name(&self) -> &'static str {
        "volcano"
    }
}
