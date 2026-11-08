//! Minimal date helpers for HTTP/WebDAV/SigV4 headers (UTC only).

use std::time::{SystemTime, UNIX_EPOCH};

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

pub(crate) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(crate) fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i64;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub(crate) fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub(crate) fn parse_month(name: &str) -> Option<u32> {
    MONTHS
        .iter()
        .position(|month| month.eq_ignore_ascii_case(name))
        .map(|index| index as u32 + 1)
}

pub(crate) fn unix_from_parts(
    year: i64,
    month: u32,
    day: u32,
    hour: u64,
    minute: u64,
    second: u64,
) -> u64 {
    let days = days_from_civil(year, month, day);
    (days.max(0) as u64) * 86_400 + hour * 3_600 + minute * 60 + second
}

/// Parses RFC 1123 dates as sent by WebDAV (`Wed, 21 Oct 2015 07:28:00 GMT`).
pub(crate) fn parse_http_date(value: &str) -> u64 {
    let mut parts = value.split_whitespace();
    let _weekday = parts.next();
    let day = parts.next().and_then(|v| v.parse::<u32>().ok());
    let month = parts.next().and_then(parse_month);
    let year = parts.next().and_then(|v| v.parse::<i64>().ok());
    let time = parts.next().and_then(|value| {
        let mut parts = value.split(':');
        Some((
            parts.next()?.parse::<u64>().ok()?,
            parts.next()?.parse::<u64>().ok()?,
            parts.next()?.parse::<u64>().ok()?,
        ))
    });
    match (day, month, year, time) {
        (Some(day), Some(month), Some(year), Some((h, m, s))) => {
            unix_from_parts(year, month, day, h, m, s)
        }
        _ => 0,
    }
}

/// Parses ISO 8601 timestamps as sent by S3 (`2024-01-31T12:00:00.000Z`).
pub(crate) fn parse_iso8601(value: &str) -> u64 {
    let value = value.trim();
    let (date, rest) = match value.split_once('T') {
        Some(parts) => parts,
        None => return 0,
    };
    let mut date_parts = date.split('-');
    let year = date_parts.next().and_then(|v| v.parse::<i64>().ok());
    let month = date_parts.next().and_then(|v| v.parse::<u32>().ok());
    let day = date_parts.next().and_then(|v| v.parse::<u32>().ok());
    let rest = rest.trim_end_matches('Z');
    let mut time_parts = rest.split(':');
    let hour = time_parts.next().and_then(|v| v.parse::<u64>().ok());
    let minute = time_parts.next().and_then(|v| v.parse::<u64>().ok());
    let second = time_parts
        .next()
        .and_then(|v| v.split('.').next())
        .and_then(|v| v.parse::<u64>().ok());
    match (year, month, day, hour, minute, second) {
        (Some(y), Some(m), Some(d), Some(h), Some(min), Some(s)) => {
            unix_from_parts(y, m, d, h, min, s)
        }
        _ => 0,
    }
}

/// Returns `(YYYYMMDDTHHMMSSZ, YYYYMMDD)` for SigV4.
pub(crate) fn amz_datetime(unix: u64) -> (String, String) {
    let days = (unix / 86_400) as i64;
    let seconds = unix % 86_400;
    let (year, month, day) = civil_from_days(days);
    let date = format!("{year:04}{month:02}{day:02}");
    let timestamp = format!(
        "{date}T{:02}{:02}{:02}Z",
        seconds / 3_600,
        (seconds % 3_600) / 60,
        seconds % 60
    );
    (timestamp, date)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_dates_both_ways() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(
            parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT"),
            1_445_412_480
        );
        assert_eq!(parse_iso8601("2024-01-31T12:00:00.000Z"), 1_706_702_400);
        let (timestamp, date) = amz_datetime(1_445_412_480);
        assert_eq!(timestamp, "20151021T072800Z");
        assert_eq!(date, "20151021");
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_http_date("not a date"), 0);
        assert_eq!(parse_iso8601("nope"), 0);
    }
}
