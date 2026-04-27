//! Custom clap value parsers and clear-aware enums shared by `tc add` and
//! `tc edit`. Kept in one place so the date/duration grammar stays consistent
//! between commands.

use std::str::FromStr;
use std::time::Duration;

use chrono::NaiveDate;

const DATE_FORMAT: &str = "%Y-%m-%d";
const CLEAR_TOKEN: &str = "clear";

/// Parse a calendar date in `YYYY-MM-DD` form. Used as a clap `value_parser`.
pub fn parse_naive_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, DATE_FORMAT)
        .map_err(|e| format!("expected YYYY-MM-DD date, got '{s}': {e}"))
}

/// Parse a humantime-formatted duration like `2h`, `30m`, `1h30m`. Used as a
/// clap `value_parser`.
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    humantime::parse_duration(s)
        .map_err(|e| format!("expected duration like '2h' or '30m', got '{s}': {e}"))
}

/// `--due 2026-05-01` sets a due date; `--due clear` removes it. The
/// `Option<DateArg>` on the args struct tells us whether the flag was passed
/// at all -- absence means "leave the field alone".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateArg {
    Set(NaiveDate),
    Clear,
}

impl DateArg {
    pub fn apply(self, target: &mut Option<NaiveDate>) {
        match self {
            Self::Set(d) => *target = Some(d),
            Self::Clear => *target = None,
        }
    }
}

impl FromStr for DateArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case(CLEAR_TOKEN) {
            return Ok(Self::Clear);
        }
        parse_naive_date(s).map(Self::Set)
    }
}

/// Same shape as [`DateArg`], specialised for `Option<Duration>` fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DurationArg {
    Set(Duration),
    Clear,
}

impl DurationArg {
    pub fn apply(self, target: &mut Option<Duration>) {
        match self {
            Self::Set(d) => *target = Some(d),
            Self::Clear => *target = None,
        }
    }
}

impl FromStr for DurationArg {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.eq_ignore_ascii_case(CLEAR_TOKEN) {
            return Ok(Self::Clear);
        }
        parse_duration(s).map(Self::Set)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_date_accepts_iso() {
        assert_eq!(
            parse_naive_date("2026-06-15").unwrap(),
            NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()
        );
    }

    #[test]
    fn parse_date_rejects_slash() {
        let err = parse_naive_date("2026/06/15").unwrap_err();
        assert!(err.contains("YYYY-MM-DD"), "err = {err}");
    }

    #[test]
    fn parse_duration_accepts_humantime() {
        assert_eq!(parse_duration("2h").unwrap(), Duration::from_secs(7200));
        assert_eq!(parse_duration("1h30m").unwrap(), Duration::from_secs(5400));
    }

    #[test]
    fn parse_duration_rejects_garbage() {
        let err = parse_duration("forever").unwrap_err();
        assert!(err.contains("duration"), "err = {err}");
    }

    #[test]
    fn date_arg_clear_token() {
        assert_eq!("clear".parse::<DateArg>().unwrap(), DateArg::Clear);
        assert_eq!("CLEAR".parse::<DateArg>().unwrap(), DateArg::Clear);
    }

    #[test]
    fn date_arg_apply_set_then_clear() {
        let mut target = None;
        DateArg::Set(NaiveDate::from_ymd_opt(2026, 7, 1).unwrap()).apply(&mut target);
        assert!(target.is_some());
        DateArg::Clear.apply(&mut target);
        assert!(target.is_none());
    }

    #[test]
    fn duration_arg_clear_token() {
        assert_eq!("clear".parse::<DurationArg>().unwrap(), DurationArg::Clear);
    }

    #[test]
    fn duration_arg_apply_round_trip() {
        let mut target = None;
        DurationArg::Set(Duration::from_secs(3600)).apply(&mut target);
        assert_eq!(target, Some(Duration::from_secs(3600)));
        DurationArg::Clear.apply(&mut target);
        assert_eq!(target, None);
    }
}
