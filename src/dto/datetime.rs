use std::fmt;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime};
use rocket::form;
use serde::Serialize;

/// Rocket only implements FromFormField for `time` types, not `chrono`,
/// so we need to implement everything ourselves
//////

// `input type="datetime-local"` accepts this format exactly,
// with absolutely no room for variation, per MDN
const BROWSER_DATE_TIME_FORMAT: &str = "%Y-%m-%dT%H:%M";
const BROWSER_DATE_FORMAT: &str = "%Y-%m-%d";

#[derive(sqlx::Type, Serialize)]
#[sqlx(transparent)]
#[serde(transparent)]
pub struct BrowserDateTimeDto(pub DateTime<Local>);

impl fmt::Display for BrowserDateTimeDto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format(BROWSER_DATE_TIME_FORMAT))
    }
}

#[rocket::async_trait]
impl<'f> form::FromFormField<'f> for BrowserDateTimeDto {
    fn from_value(field: form::ValueField<'f>) -> form::Result<'f, Self> {
        if let Ok(naive) = NaiveDateTime::parse_from_str(field.value, BROWSER_DATE_TIME_FORMAT) {
            if let Some(local) = naive.and_local_timezone(Local).single() {
                Ok(Self(local))
            } else {
                Err(
                    form::Error::validation("invalid or ambiguous datetime in local timezone")
                        .into(),
                )
            }
        } else {
            Err(form::Error::validation("invalid datetime format").into())
        }
    }
}

#[derive(sqlx::Type, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[sqlx(transparent)]
#[serde(transparent)]
pub struct BrowserDateDto(pub NaiveDate);

impl fmt::Display for BrowserDateDto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format(BROWSER_DATE_FORMAT))
    }
}

#[rocket::async_trait]
impl<'f> form::FromFormField<'f> for BrowserDateDto {
    fn from_value(field: form::ValueField<'f>) -> form::Result<'f, Self> {
        if let Ok(naive) = NaiveDate::parse_from_str(field.value, BROWSER_DATE_FORMAT) {
            Ok(Self(naive))
        } else {
            Err(form::Error::validation("invalid date format").into())
        }
    }
}
