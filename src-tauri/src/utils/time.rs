use time::format_description::well_known::Rfc3339;
use time::format_description::FormatItem;
use time::macros::format_description;
use time::{OffsetDateTime, PrimitiveDateTime};

use crate::error::{AppError, Result};

const TS_FORMAT: &[FormatItem<'static>] =
    format_description!("[year]-[month]-[day]_[hour]-[minute]-[second]");

pub fn now_timestamp() -> Result<String> {
    format_timestamp(OffsetDateTime::now_utc())
}

pub fn format_timestamp(dt: OffsetDateTime) -> Result<String> {
    dt.format(TS_FORMAT).map_err(AppError::time)
}

pub fn parse_timestamp(value: &str) -> Result<OffsetDateTime> {
    if let Ok(parsed) = PrimitiveDateTime::parse(value, TS_FORMAT) {
        return Ok(parsed.assume_utc());
    }

    OffsetDateTime::parse(value, &Rfc3339).map_err(AppError::time)
}
