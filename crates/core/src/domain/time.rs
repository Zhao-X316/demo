//! 统一时间契约：持久化时间使用 UTC RFC3339，校历日期使用 Asia/Shanghai。

use chrono::{DateTime, FixedOffset, NaiveDate, SecondsFormat, Utc};

const SHANGHAI_OFFSET_SECONDS: i32 = 8 * 60 * 60;

/// 将时间稳定序列化为带毫秒的 UTC RFC3339 文本。
pub fn utc_rfc3339(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn utc_now_rfc3339() -> String {
    utc_rfc3339(Utc::now())
}

/// 业务日切固定使用 Asia/Shanghai（UTC+8），不受运行设备时区影响。
pub fn shanghai_business_date_at(value: DateTime<Utc>) -> NaiveDate {
    let offset = FixedOffset::east_opt(SHANGHAI_OFFSET_SECONDS).expect("valid UTC+8 offset");
    value.with_timezone(&offset).date_naive()
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn utc_text_is_rfc3339_with_z_suffix() {
        let value = Utc.with_ymd_and_hms(2026, 7, 13, 1, 2, 3).unwrap();
        assert_eq!(utc_rfc3339(value), "2026-07-13T01:02:03.000Z");
    }

    #[test]
    fn shanghai_business_date_crosses_utc_boundary_explicitly() {
        let value = Utc.with_ymd_and_hms(2026, 7, 12, 16, 30, 0).unwrap();
        assert_eq!(
            shanghai_business_date_at(value),
            NaiveDate::from_ymd_opt(2026, 7, 13).unwrap()
        );
    }
}
