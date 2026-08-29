use thiserror::Error;
use time::{Duration, OffsetDateTime, UtcOffset, macros::format_description};

use crate::config::LINK_TTL_SECONDS;

#[derive(Debug, Error)]
pub enum TimePolicyError {
    #[error("expiry time is outside the supported range")]
    ExpiryOutOfRange,
    #[error("could not format UTC date")]
    UtcDateFormatting(#[source] time::error::Format),
}

pub fn expiry_at(now: OffsetDateTime) -> Result<OffsetDateTime, TimePolicyError> {
    now.to_offset(UtcOffset::UTC)
        .checked_add(Duration::seconds(LINK_TTL_SECONDS as i64))
        .ok_or(TimePolicyError::ExpiryOutOfRange)
}

pub fn utc_date(now: OffsetDateTime) -> Result<String, TimePolicyError> {
    now.to_offset(UtcOffset::UTC)
        .date()
        .format(format_description!("[year]-[month]-[day]"))
        .map_err(TimePolicyError::UtcDateFormatting)
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::{expiry_at, utc_date};

    #[test]
    fn calculates_the_exact_ninety_day_expiry_in_utc() {
        let now = datetime!(2024-01-01 0:00 UTC);

        assert_eq!(expiry_at(now).unwrap(), datetime!(2024-03-31 0:00 UTC));
    }

    #[test]
    fn formats_utc_dates_on_each_side_of_a_date_boundary() {
        assert_eq!(
            utc_date(datetime!(2024-12-31 23:59:59 UTC)).unwrap(),
            "2024-12-31"
        );
        assert_eq!(
            utc_date(datetime!(2025-01-01 0:00:00 UTC)).unwrap(),
            "2025-01-01"
        );
    }

    #[test]
    fn converts_instants_to_utc_before_formatting_the_daily_key_date() {
        let now = datetime!(2024-12-31 23:30 -1:00);

        assert_eq!(utc_date(now).unwrap(), "2025-01-01");
    }

    #[test]
    fn reports_an_out_of_range_expiry() {
        assert!(expiry_at(datetime!(9999-12-31 23:59:59.999_999_999 UTC)).is_err());
    }
}
