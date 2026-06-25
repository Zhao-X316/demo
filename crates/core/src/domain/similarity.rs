//! 字符序列相似度基础算法：最长公共子序列(LCS) 与 编辑距离(Levenshtein)。
//! 均为 O(n·m) 时间、O(min(n,m)) 空间。

/// 最长公共子序列长度。
pub fn lcs_len(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    if n == 0 || m == 0 {
        return 0;
    }
    let mut dp = vec![0usize; m + 1];
    for i in 1..=n {
        let mut prev = 0; // dp[i-1][j-1]
        for j in 1..=m {
            let tmp = dp[j];
            dp[j] = if a[i - 1] == b[j - 1] {
                prev + 1
            } else {
                dp[j].max(dp[j - 1])
            };
            prev = tmp;
        }
    }
    dp[m]
}

/// 编辑距离（增删改各计 1）。
pub fn levenshtein(a: &[char], b: &[char]) -> usize {
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut dp: Vec<usize> = (0..=m).collect();
    for i in 1..=n {
        let mut prev = dp[0]; // dp[i-1][j-1]
        dp[0] = i;
        for j in 1..=m {
            let tmp = dp[j];
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            dp[j] = (dp[j] + 1).min(dp[j - 1] + 1).min(prev + cost);
            prev = tmp;
        }
    }
    dp[m]
}

/// 覆盖率 = LCS(answer, asr) / len(answer)，即"答案被正确背出的比例"。范围 [0,1]。
pub fn coverage(answer: &[char], asr: &[char]) -> f64 {
    if answer.is_empty() {
        return 0.0;
    }
    (lcs_len(answer, asr) as f64 / answer.len() as f64).min(1.0)
}

/// 编辑相似度 = 1 - Levenshtein / max(len)。范围 [0,1]。
pub fn edit_sim(answer: &[char], asr: &[char]) -> f64 {
    let maxlen = answer.len().max(asr.len());
    if maxlen == 0 {
        return 1.0;
    }
    (1.0 - levenshtein(answer, asr) as f64 / maxlen as f64).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cv(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn lcs_basic() {
        assert_eq!(lcs_len(&cv("床前明月光"), &cv("床前明月光")), 5);
        assert_eq!(lcs_len(&cv("床前明月光"), &cv("床前月光")), 4);
        assert_eq!(lcs_len(&cv("abc"), &cv("xyz")), 0);
    }

    #[test]
    fn lev_basic() {
        assert_eq!(levenshtein(&cv("床前明月光"), &cv("床前明月光")), 0);
        assert_eq!(levenshtein(&cv("kitten"), &cv("sitting")), 3);
    }

    #[test]
    fn coverage_full_and_partial() {
        assert_eq!(coverage(&cv("床前明月光"), &cv("床前明月光")), 1.0);
        assert!((coverage(&cv("床前明月光"), &cv("床前月光")) - 0.8).abs() < 1e-9);
        assert_eq!(coverage(&cv(""), &cv("x")), 0.0);
    }
}
