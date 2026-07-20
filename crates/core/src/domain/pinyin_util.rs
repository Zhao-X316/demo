//! 汉字 → 拼音转换，用于正确率的"拼音容错"。
//!
//! 背诵是"念"出来的，ASR 只能猜字，同音字（是/事/世）在语音里无法区分；
//! 按读音比对更贴合背诵场景。非汉字字符原样保留（数字/字母仍可比对）。

use pinyin::ToPinyin;

/// 将字符串转为拼音流（拼接，无分隔符，便于字符级相似度计算）。
/// `ignore_tone=true` 用不带声调拼音（更宽松）。
pub fn to_pinyin(input: &str, ignore_tone: bool) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for (c, p) in input.chars().zip(input.to_pinyin()) {
        match p {
            Some(py) => out.push_str(if ignore_tone {
                py.plain()
            } else {
                py.with_tone()
            }),
            None => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn homophones_collapse_without_tone() {
        // 是/事/世 同音，忽略声调后拼音一致
        assert_eq!(to_pinyin("是", true), to_pinyin("事", true));
        assert_eq!(to_pinyin("是", true), to_pinyin("世", true));
    }

    #[test]
    fn keeps_non_chinese() {
        assert_eq!(to_pinyin("abc123", true), "abc123");
    }

    #[test]
    fn converts_phrase() {
        assert_eq!(to_pinyin("中国", true), "zhongguo");
    }
}
