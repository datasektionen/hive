use chrono::Local;
use rocket::FromForm;

use super::{datetime::BrowserDateTimeDto, TrimmedStr};

#[derive(FromForm)]
pub struct CreateApiTokenDto<'v> {
    #[field(validate = len(3..))]
    pub description: TrimmedStr<'v>,
    #[field(validate = with(|o| o.as_ref().map(|e| e.0 >= Local::now()).unwrap_or(true), "invalid past expiration"))]
    pub expiration: Option<BrowserDateTimeDto>,
}
