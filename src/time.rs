//! Date/time helpers without a date crate. All times are UTC.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_unix() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

/// Days since 1970-01-01 -> (year, month, day). Howard Hinnant's algorithm.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (year, month, day) -> days since 1970-01-01.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn unix_to_iso(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z", y, m, d, rem / 3600, (rem % 3600) / 60, rem % 60)
}

pub fn unix_to_date(secs: i64) -> String {
    let (y, m, d) = civil_from_days(secs.div_euclid(86_400));
    format!("{:04}-{:02}-{:02}", y, m, d)
}

pub fn now_iso() -> String {
    unix_to_iso(now_unix())
}

/// Filename-safe timestamp, e.g. `2026-11-15_10-30-00`.
pub fn now_stamp() -> String {
    now_iso().replace('T', "_").replace(':', "-").trim_end_matches('Z').to_string()
}

/// Parse `YYYY-MM-DD`, optionally followed by `THH:MM:SS` and a `Z`/`±HH:MM`/`±HHMM` offset.
pub fn iso_to_unix(s: &str) -> Option<i64> {
    let s = s.trim();
    let b = s.as_bytes();
    if b.len() < 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let y: i64 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let mut secs = days_from_civil(y, m, d) * 86_400;
    let mut rest = &s[10..];
    if rest.len() >= 9 && (rest.starts_with('T') || rest.starts_with(' ')) {
        let hh: i64 = rest[1..3].parse().ok()?;
        let mm: i64 = rest[4..6].parse().ok()?;
        let ss: i64 = rest[7..9].parse().ok()?;
        secs += hh * 3600 + mm * 60 + ss;
        rest = &rest[9..];
        // Skip fractional seconds.
        if let Some(r) = rest.strip_prefix('.') {
            let n = r.bytes().take_while(|c| c.is_ascii_digit()).count();
            rest = &r[n..];
        }
    }
    if let Some(off) = parse_offset(rest.trim()) {
        secs -= off;
    }
    Some(secs)
}

/// `Z`, `+HH:MM`, `+HHMM` -> offset seconds east of UTC.
fn parse_offset(s: &str) -> Option<i64> {
    if s.is_empty() || s == "Z" || s == "GMT" || s == "UTC" || s == "UT" {
        return Some(0);
    }
    let sign = match s.as_bytes()[0] {
        b'+' => 1,
        b'-' => -1,
        _ => return None,
    };
    let digits: String = s[1..].chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() != 4 {
        return None;
    }
    let hh: i64 = digits[0..2].parse().ok()?;
    let mm: i64 = digits[2..4].parse().ok()?;
    Some(sign * (hh * 3600 + mm * 60))
}

const MONTHS: [&str; 12] = ["jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec"];

/// Parse RSS-style dates: `Fri, 14 Nov 2026 10:00:00 +0000`.
pub fn rfc2822_to_unix(s: &str) -> Option<i64> {
    let toks: Vec<&str> = s.split_whitespace().collect();
    let mut i = 0;
    if toks.first().map(|t| t.ends_with(',')).unwrap_or(false) {
        i = 1;
    }
    if toks.len() < i + 4 {
        return None;
    }
    let d: u32 = toks[i].parse().ok()?;
    let mon = toks[i + 1].to_ascii_lowercase();
    let m = MONTHS.iter().position(|x| mon.starts_with(x))? as u32 + 1;
    let y: i64 = toks[i + 2].parse().ok()?;
    let hms: Vec<i64> = toks[i + 3].split(':').filter_map(|x| x.parse().ok()).collect();
    if hms.len() < 2 {
        return None;
    }
    let mut secs = days_from_civil(y, m, d) * 86_400 + hms[0] * 3600 + hms[1] * 60 + hms.get(2).copied().unwrap_or(0);
    if let Some(tz) = toks.get(i + 4) {
        secs -= parse_offset(tz).unwrap_or(0);
    }
    Some(secs)
}

/// `90s`, `30m`, `2h`, `1d` -> seconds. A bare number means minutes.
pub fn parse_duration(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num, unit) = match s.find(|c: char| !c.is_ascii_digit()) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, "m"),
    };
    let n: u64 = num.parse().ok()?;
    let mult = match unit.trim().to_ascii_lowercase().as_str() {
        "s" | "sec" | "secs" => 1,
        "m" | "min" | "mins" => 60,
        "h" | "hr" | "hrs" => 3600,
        "d" | "day" | "days" => 86_400,
        _ => return None,
    };
    Some(n * mult)
}

pub fn human_duration(secs: u64) -> String {
    if secs % 86_400 == 0 {
        format!("{}d", secs / 86_400)
    } else if secs % 3600 == 0 {
        format!("{}h", secs / 3600)
    } else if secs % 60 == 0 {
        format!("{}m", secs / 60)
    } else {
        format!("{}s", secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_round_trip() {
        for days in [-1_000_000, -1, 0, 1, 19_000, 20_772, 100_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(unix_to_iso(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn iso() {
        assert_eq!(iso_to_unix("1970-01-02"), Some(86_400));
        assert_eq!(iso_to_unix("1970-01-01T01:00:00Z"), Some(3600));
        assert_eq!(iso_to_unix("1970-01-01T01:00:00+01:00"), Some(0));
        assert_eq!(iso_to_unix("1970-01-01T01:00:00.123-0100"), Some(7200));
        assert_eq!(iso_to_unix("nope"), None);
    }

    #[test]
    fn rfc2822() {
        assert_eq!(rfc2822_to_unix("Thu, 01 Jan 1970 00:00:00 +0000"), Some(0));
        assert_eq!(rfc2822_to_unix("Thu, 01 Jan 1970 02:00:00 +0100"), Some(3600));
        assert_eq!(rfc2822_to_unix("1 Jan 1970 00:01 GMT"), Some(60));
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("30m"), Some(1800));
        assert_eq!(parse_duration("2h"), Some(7200));
        assert_eq!(parse_duration("15"), Some(900));
        assert_eq!(parse_duration("x"), None);
        assert_eq!(human_duration(7200), "2h");
    }
}
