//! 背诵清单文档解析：把一份「第X课 → 1．2．3．点」的背诵清单切成一条条 content。
//!
//! 决策（2026-06-26 用户拍板）：
//! - **按点切**（一条 content = 一个知识点，可指派/可打卡的最小单位）。
//! - content_no 用中文分层 `{学科册}-{NN}课-{MM}`（如 `道法8上-03课-05`）。
//! - title 带课题前缀：`第X课·课题 ｜ 设问`。
//! - 点序号**按出现顺序重排**（不信印刷号——印刷号有重复/跳号），印刷号存备注。
//! - 清洗页眉水印/全角数字。
//!
//! 纯逻辑、可单测。乱的点（无分隔编号 / 重复 / 跳号）由前端预览让老师改。

use regex::Regex;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ParsedContent {
    pub content_no: String,  // {prefix}-{NN}课-{MM}
    pub title: String,       // 第X课·课题 ｜ 设问
    pub answer_text: String, // 该点全文（清洗后，保留①②③）
    pub is_key: bool,        // ★ 重点
    pub lesson_no: u32,
    pub point_seq: u32,       // 按出现顺序，从 1
    pub original_no: String,  // 印刷编号（备注）
    pub lesson_title: String, // 课题
}

const WATERMARK: &str = "学科网（北京）股份有限公司";

/// 清洗：去页眉水印、零宽字符、全角数字转半角。
fn clean(text: &str) -> String {
    let s = text.replace(WATERMARK, "");
    s.chars()
        .filter(|c| *c != '\u{200b}' && *c != '\u{feff}')
        .map(|c| match c {
            '０'..='９' => char::from(b'0' + (c as u32 - '０' as u32) as u8),
            _ => c,
        })
        .collect()
}

/// 中文数字 → 阿拉伯（支持 一..十, 十一.., 二十..）。
fn cn_num(s: &str) -> u32 {
    let digit = |c: char| -> Option<u32> {
        "零一二三四五六七八九".find(c).map(|i| (i / 3) as u32)
    };
    let chars: Vec<char> = s.chars().collect();
    if let Some(pos) = chars.iter().position(|&c| c == '十') {
        let tens = if pos == 0 {
            1
        } else {
            digit(chars[0]).unwrap_or(1)
        };
        let ones = chars.get(pos + 1).and_then(|&c| digit(c)).unwrap_or(0);
        tens * 10 + ones
    } else {
        chars
            .iter()
            .filter_map(|&c| digit(c))
            .fold(0, |a, d| a * 10 + d)
    }
}

/// 从某点正文抽设问：到第一个 ？/：/。 或子标记 ①/（/( 之前；？ 保留，其余截断符不保留。限长 40 字。
fn extract_question(s: &str) -> String {
    let stops = ['？', '?', '：', ':', '。', '①', '（', '('];
    let mut end = s.len();
    for (i, c) in s.char_indices() {
        if stops.contains(&c) {
            end = if c == '？' || c == '?' {
                i + c.len_utf8()
            } else {
                i
            };
            break;
        }
    }
    s[..end].trim().chars().take(40).collect()
}

/// 解析整份清单。`prefix` = 学科册（如 "道法8上"），由老师首次确认。
pub fn parse_syllabus(text: &str, prefix: &str) -> Vec<ParsedContent> {
    let cleaned = clean(text);
    let lesson_re = Regex::new(r"第([一二三四五六七八九十]+)课").unwrap();
    let point_re = Regex::new(r"(★?)\s*(\d{1,2})\s*[．.、]").unwrap();

    // 所有课的 (匹配start, 匹配end, 中文课号)
    let lessons: Vec<(usize, usize, String)> = lesson_re
        .captures_iter(&cleaned)
        .map(|c| {
            let m = c.get(0).unwrap();
            (m.start(), m.end(), c.get(1).unwrap().as_str().to_string())
        })
        .collect();

    let mut out = Vec::new();
    for (li, (_l_start, l_end, l_num_cn)) in lessons.iter().enumerate() {
        let body_end = lessons.get(li + 1).map(|n| n.0).unwrap_or(cleaned.len());
        let body = &cleaned[*l_end..body_end];
        let lesson_no = cn_num(l_num_cn);

        // 课题 = body 到第一个点标记之前
        let first_pt = point_re.find(body).map(|m| m.start()).unwrap_or(body.len());
        let lesson_title = body[..first_pt].trim().to_string();
        let region = &body[first_pt..];

        // 点标记位置
        let marks: Vec<(usize, bool, String)> = point_re
            .captures_iter(region)
            .map(|c| {
                let m = c.get(0).unwrap();
                let star = !c.get(1).unwrap().as_str().is_empty();
                let no = c.get(2).unwrap().as_str().to_string();
                (m.start(), star, no)
            })
            .collect();

        for (pi, (start, star, printed_no)) in marks.iter().enumerate() {
            let end = marks.get(pi + 1).map(|n| n.0).unwrap_or(region.len());
            let raw = &region[*start..end];
            // 去掉开头的标记（★?数字[．.、]）
            let answer = point_re.replacen(raw, 1, "").trim().to_string();
            if answer.is_empty() {
                continue;
            }
            let seq = (out_seq(&out, lesson_no)) + 1;
            let question = extract_question(&answer);
            out.push(ParsedContent {
                content_no: format!("{prefix}-{lesson_no:02}课-{seq:02}"),
                title: format!("第{l_num_cn}课·{lesson_title} ｜ {question}"),
                answer_text: answer,
                is_key: *star,
                lesson_no,
                point_seq: seq,
                original_no: printed_no.clone(),
                lesson_title: lesson_title.clone(),
            });
        }
    }
    out
}

/// 当前已产出里该课的点数（用于按出现顺序连续重排，跳过空点也不断号）。
fn out_seq(out: &[ParsedContent], lesson_no: u32) -> u32 {
    out.iter().filter(|c| c.lesson_no == lesson_no).count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "第一课 丰富的社会生活1．个人与社会的关系：①个人是社会的有机组成部分。②人的身份是在社会关系中确定的。2．如何认识社会关系？①人们在社会交往中形成各种社会关系。★5．为什么要养成亲社会行为？①青少年正处于走向社会的关键时期。第二课  网络生活新空间★1．网络的积极作用①丰富了日常生活②推动社会进步。";

    #[test]
    fn splits_lessons_points_and_numbers() {
        let r = parse_syllabus(SAMPLE, "道法8上");
        // 第一课 3 个点（1/2/★5 → 按序重排为 01/02/03），第二课 1 个点
        assert_eq!(r.len(), 4);
        assert_eq!(r[0].content_no, "道法8上-01课-01");
        assert_eq!(r[0].lesson_no, 1);
        assert_eq!(r[1].content_no, "道法8上-01课-02");
        // ★5 重排成第 3 点，且标重点，印刷号留 5
        assert_eq!(r[2].content_no, "道法8上-01课-03");
        assert!(r[2].is_key);
        assert_eq!(r[2].original_no, "5");
        // 第二课从 01 重新计数
        assert_eq!(r[3].content_no, "道法8上-02课-01");
        assert!(r[3].is_key);
        // 标题带课题前缀 + 设问
        assert!(
            r[0].title.starts_with("第一课·丰富的社会生活 ｜ "),
            "got: {}",
            r[0].title
        );
        assert!(
            r[1].title.contains("如何认识社会关系？"),
            "got: {}",
            r[1].title
        );
        // 答案保留子点 ①②，去掉开头点号
        assert!(
            r[0].answer_text.starts_with("个人与社会的关系"),
            "got: {}",
            r[0].answer_text
        );
        assert!(r[0].answer_text.contains('①'));
    }

    #[test]
    fn cleans_watermark() {
        let t = "第一课 测试1．甲乙丙学科网（北京）股份有限公司丁戊。";
        let r = parse_syllabus(t, "x");
        assert!(
            !r[0].answer_text.contains("学科网"),
            "水印未清: {}",
            r[0].answer_text
        );
        assert!(r[0].answer_text.contains("丁戊"));
    }
}
