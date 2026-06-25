//! 文本归一化：全角转半角、删标点/空白、可选删语气词、英文小写。
//! 中文字符与数字保留；其余符号/空白一律剔除（用 `char::is_alphanumeric` 判定）。

#[derive(Debug, Clone)]
pub struct NormalizeCfg {
    /// 是否删除语气词（如 嗯/啊/呃）。默认对答案影响小、对 ASR 文本去噪有益。
    pub remove_fillers: bool,
    pub fillers: Vec<String>,
}

impl Default for NormalizeCfg {
    fn default() -> Self {
        Self {
            remove_fillers: true,
            fillers: ["嗯", "啊", "呃", "唉"].iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// 全角字符（U+FF01..=U+FF5E）转半角；其余原样返回。
fn fullwidth_to_half(c: char) -> char {
    let u = c as u32;
    if (0xFF01..=0xFF5E).contains(&u) {
        char::from_u32(u - 0xFEE0).unwrap_or(c)
    } else {
        c
    }
}

pub fn normalize(input: &str, cfg: &NormalizeCfg) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        let c = fullwidth_to_half(ch);
        // 保留字母/数字/中文（is_alphanumeric 对 CJK 返回 true），其余（标点/空白/符号）丢弃
        if c.is_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        }
    }
    if cfg.remove_fillers {
        for f in &cfg.fillers {
            if !f.is_empty() {
                out = out.replace(f.as_str(), "");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_punctuation_and_space() {
        let cfg = NormalizeCfg { remove_fillers: false, fillers: vec![] };
        assert_eq!(normalize("床前明月光，疑是地上霜。", &cfg), "床前明月光疑是地上霜");
        assert_eq!(normalize("  hello, world! ", &cfg), "helloworld");
    }

    #[test]
    fn fullwidth_converted() {
        let cfg = NormalizeCfg { remove_fillers: false, fillers: vec![] };
        // 全角 ABC123 → 半角小写
        assert_eq!(normalize("ＡＢＣ１２３", &cfg), "abc123");
    }

    #[test]
    fn removes_fillers_when_enabled() {
        let cfg = NormalizeCfg::default();
        assert_eq!(normalize("嗯床前啊明月光", &cfg), "床前明月光");
    }
}
