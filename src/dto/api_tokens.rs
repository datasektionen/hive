use std::fmt;

use chrono::{DateTime, Local, NaiveDateTime};
use rocket::{
    form::{self, Contextual, Form},
    FromForm,
};
use serde::Serialize;

use super::TrimmedStr;

// `input type="datetime-local"` accepts this format exactly,
// with absolutely no room for variation, per MDN
const BROWSER_DATE_TIME_FORMAT: &str = "%Y-%m-%dT%H:%M";

#[derive(sqlx::Type, Serialize)]
#[sqlx(transparent)]
#[serde(transparent)]
pub struct BrowserDateTime(DateTime<Local>);

impl fmt::Display for BrowserDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.format(BROWSER_DATE_TIME_FORMAT))
    }
}

#[rocket::async_trait]
impl<'f> form::FromFormField<'f> for BrowserDateTime {
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

#[derive(FromForm)]
pub struct CreateApiTokenDto<'v> {
    #[field(validate = len(3..))]
    pub description: TrimmedStr<'v>,
    // cannot validate here; errors (lifetimes/borrow checker)
    pub expiration: Option<BrowserDateTime>,
}

impl CreateApiTokenDto<'_> {
    pub fn extra_validation<'v>(form: &mut Form<Contextual<'v, CreateApiTokenDto<'v>>>) {
        // FIXME: this doesn't really work if both fields have errors,
        // since we only validate expiration if everything else looks good,
        // but in general this function shouldn't even exist: there must be
        // a way to do this with #field[validate = ...] instead

        if let Some(token) = &form.value {
            if let Some(expiration) = &token.expiration {
                if expiration.0 < Local::now() {
                    let mut err = form::Error::validation("invalid past expiration")
                        .with_entity(form::error::Entity::Field)
                        .with_name("expiration");

                    if let Some(val) = form.context.field_value("expiration") {
                        err = err.with_value(val)
                    }

                    form.context.push_error(err);
                    form.value = None;
                }
            }
        }
    }
}
