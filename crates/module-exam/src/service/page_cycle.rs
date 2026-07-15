//! 连续拍摄页面的重复版式周期识别。
//!
//! 只比较低分辨率边缘指纹，用于建议“每名学生几页”。它不识别学生、不判题，也不能
//! 代替页面质量、页型和老师页组确认。

use image::imageops::FilterType;
use suite_core::error::{CoreError, CoreResult};

const SIGNATURE_WIDTH: u32 = 24;
const SIGNATURE_HEIGHT: u32 = 32;
// 合成契约阈值；真实拍照黄金集完成前不得据此声明生产准确率。
const MAX_ACCEPTED_DISTANCE: f64 = 0.035;

#[derive(Debug, Clone, PartialEq)]
pub struct PageLayoutSignature {
    values: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PageCycleInference {
    pub pages_per_attempt: usize,
    pub confidence: f64,
    pub mean_distance: f64,
    pub comparison_count: usize,
}

/// 从 JPEG 内容提取抗亮度变化的版式边缘指纹。
pub fn signature_from_jpeg(bytes: &[u8]) -> CoreResult<PageLayoutSignature> {
    let image = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        .map_err(|error| CoreError::Parse(format!("JPEG 页面无法解码：{error}")))?
        .resize_exact(
            SIGNATURE_WIDTH + 1,
            SIGNATURE_HEIGHT + 1,
            FilterType::Triangle,
        )
        .to_luma8();
    let mut edges = Vec::with_capacity((SIGNATURE_WIDTH * SIGNATURE_HEIGHT) as usize);
    for y in 0..SIGNATURE_HEIGHT {
        for x in 0..SIGNATURE_WIDTH {
            let center = i16::from(image.get_pixel(x, y).0[0]);
            let right = i16::from(image.get_pixel(x + 1, y).0[0]);
            let below = i16::from(image.get_pixel(x, y + 1).0[0]);
            let gradient = (center - right).unsigned_abs() + (center - below).unsigned_abs();
            edges.push(gradient.min(u16::from(u8::MAX)) as u8);
        }
    }
    Ok(PageLayoutSignature { values: edges })
}

pub fn signature_distance(left: &PageLayoutSignature, right: &PageLayoutSignature) -> f64 {
    debug_assert_eq!(left.values.len(), right.values.len());
    let total = left
        .values
        .iter()
        .zip(&right.values)
        .map(|(left, right)| f64::from(left.abs_diff(*right)))
        .sum::<f64>();
    total / left.values.len().max(1) as f64 / f64::from(u8::MAX)
}

/// 选择满足重复版式阈值的最短周期。至少需要观察到两轮完整页面周期。
pub fn infer_repeating_cycle(
    signatures: &[PageLayoutSignature],
    max_pages_per_attempt: usize,
) -> Option<PageCycleInference> {
    if signatures.len() < 2 || max_pages_per_attempt == 0 {
        return None;
    }
    let max_period = max_pages_per_attempt.min(signatures.len() / 2);
    for period in 1..=max_period {
        if signatures.len() < period * 2 {
            continue;
        }
        let distances = (period..signatures.len())
            .map(|index| signature_distance(&signatures[index], &signatures[index - period]))
            .collect::<Vec<_>>();
        let comparison_count = distances.len();
        let mean_distance = distances.iter().sum::<f64>() / comparison_count as f64;
        let accepted_ratio = distances
            .iter()
            .filter(|distance| **distance <= MAX_ACCEPTED_DISTANCE)
            .count() as f64
            / comparison_count as f64;
        if mean_distance <= MAX_ACCEPTED_DISTANCE && accepted_ratio >= 0.8 {
            let repeat_factor = (comparison_count as f64 / period as f64).min(3.0) / 3.0;
            let confidence = ((1.0 - mean_distance / MAX_ACCEPTED_DISTANCE) * 0.65
                + accepted_ratio * 0.25
                + repeat_factor * 0.10)
                .clamp(0.0, 0.99);
            return Some(PageCycleInference {
                pages_per_attempt: period,
                confidence,
                mean_distance,
                comparison_count,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::codecs::jpeg::JpegEncoder;
    use image::{GrayImage, Luma};

    fn page_bytes(template: usize, student: usize) -> Vec<u8> {
        let mut page = GrayImage::from_pixel(480, 640, Luma([248]));
        let heading_y = if template == 0 { 70 } else { 120 };
        for y in heading_y..heading_y + 12 {
            for x in 45..435 {
                page.put_pixel(x, y, Luma([30]));
            }
        }
        for line in 0..8 {
            let y = 180 + line * 42 + template * 8;
            for x in (55 + template * 24)..425 {
                if x % 9 < 6 {
                    page.put_pixel(x as u32, y as u32, Luma([75]));
                    page.put_pixel(x as u32, y as u32 + 1, Luma([75]));
                }
            }
        }
        // 模拟不同学生的少量手写，不改变印刷版式主体。
        let mark_x = 80 + student * 17;
        for y in 520..535 {
            for x in mark_x..(mark_x + 38) {
                page.put_pixel(x as u32, y, Luma([95]));
            }
        }
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, 92)
            .encode_image(&page)
            .unwrap();
        bytes
    }

    #[test]
    fn detects_two_page_cycle_with_student_marks() {
        let signatures = (0..6)
            .map(|index| signature_from_jpeg(&page_bytes(index % 2, index / 2)).unwrap())
            .collect::<Vec<_>>();
        let inferred = infer_repeating_cycle(&signatures, 4).unwrap();
        assert_eq!(inferred.pages_per_attempt, 2);
        assert!(inferred.confidence >= 0.8, "{inferred:?}");
    }

    #[test]
    fn detects_single_page_cycle() {
        let signatures = (0..4)
            .map(|student| signature_from_jpeg(&page_bytes(0, student)).unwrap())
            .collect::<Vec<_>>();
        let inferred = infer_repeating_cycle(&signatures, 4).unwrap();
        assert_eq!(inferred.pages_per_attempt, 1);
    }

    #[test]
    fn one_photo_is_not_enough_to_claim_a_cycle() {
        let signature = signature_from_jpeg(&page_bytes(0, 0)).unwrap();
        assert!(infer_repeating_cycle(&[signature], 4).is_none());
    }
}
