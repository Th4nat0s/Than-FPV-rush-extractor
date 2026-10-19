//! How GRDB writes the values rusqlite has no single answer for.

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef};
use rusqlite::ToSql;
use uuid::Uuid;

/// A UUID as the Swift records store it: Foundation's upper-case `uuidString`.
pub struct Id(pub Uuid);

impl ToSql for Id {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(fpv_core::uuid_string(self.0)))
    }
}

impl FromSql for Id {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let text = value.as_str()?;
        Uuid::parse_str(text).map(Id).map_err(|e| FromSqlError::Other(Box::new(e)))
    }
}

/// A date as GRDB stores one: `"2026-08-23 21:06:32.099"`, UTC, to the
/// millisecond. Reading also accepts the other shapes GRDB itself decodes —
/// a `T` separator, no fraction, a bare day — and a Unix timestamp.
pub struct Date(pub DateTime<Utc>);

const WRITE_FORMAT: &str = "%Y-%m-%d %H:%M:%S%.3f";

impl ToSql for Date {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(ToSqlOutput::from(self.0.format(WRITE_FORMAT).to_string()))
    }
}

impl FromSql for Date {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        match value {
            ValueRef::Integer(seconds) => DateTime::from_timestamp(seconds, 0)
                .map(Date).ok_or(FromSqlError::OutOfRange(seconds)),
            ValueRef::Real(seconds) => {
                DateTime::from_timestamp_millis((seconds * 1000.0).round() as i64)
                    .map(Date).ok_or(FromSqlError::InvalidType)
            }
            _ => parse(value.as_str()?).map(Date).ok_or(FromSqlError::InvalidType),
        }
    }
}

fn parse(text: &str) -> Option<DateTime<Utc>> {
    let text = text.trim_end_matches('Z');
    ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M"]
        .iter()
        .find_map(|format| NaiveDateTime::parse_from_str(text, format).ok())
        .or_else(|| NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?.and_hms_opt(0, 0, 0))
        .map(|naive| naive.and_utc())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_shape_grdb_writes() {
        let expected = "2026-10-18T21:06:32.099Z";
        assert_eq!(parse("2026-10-18 21:06:32.099").unwrap().to_rfc3339_opts(
            chrono::SecondsFormat::Millis, true), expected);
        assert_eq!(parse("2026-10-18T21:06:32.099Z").unwrap().to_rfc3339_opts(
            chrono::SecondsFormat::Millis, true), expected);
        assert_eq!(parse("2026-08-23").unwrap().to_rfc3339(), "2026-08-23T00:00:00+00:00");
        assert_eq!(parse("2026-08-23 10:15").unwrap().to_rfc3339(), "2026-08-23T10:15:00+00:00");
        assert!(parse("yesterday").is_none());
    }
}
