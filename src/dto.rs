use std::ops::{Deref, RangeBounds};

use regex::Regex;
use rocket::form::{self, FromFormField};
use serde::Serialize;

pub mod api_tokens;
pub mod datetime;
pub mod errors;
pub mod groups;
pub mod permissions;
pub mod systems;

#[derive(sqlx::Type, Serialize, Clone, Copy)]
#[sqlx(transparent)]
#[serde(transparent)]
pub struct TrimmedStr<'v>(&'v str);

#[rocket::async_trait]
impl<'v> FromFormField<'v> for TrimmedStr<'v> {
    fn from_value(field: form::ValueField<'v>) -> form::Result<'v, Self> {
        Ok(Self(field.value.trim()))
    }
}

impl<'v> Deref for TrimmedStr<'v> {
    type Target = &'v str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'v> From<&TrimmedStr<'v>> for &'v str {
    fn from(t: &TrimmedStr<'v>) -> Self {
        **t
    }
}

impl From<TrimmedStr<'_>> for serde_json::Value {
    fn from(t: TrimmedStr) -> Self {
        (*t).into()
    }
}

impl form::validate::Len<usize> for TrimmedStr<'_> {
    fn len(&self) -> usize {
        self.0.len()
    }

    fn len_into_u64(len: usize) -> u64 {
        len as u64
    }

    fn zero_len() -> usize {
        0
    }
}

fn valid_slug<'v, T: Into<&'v str>>(s: T) -> form::Result<'v, ()> {
    let re = Regex::new("^[a-z0-9]+(-[a-z0-9]+)*$").unwrap();

    if re.is_match(s.into()) {
        Ok(())
    } else {
        Err(form::Error::validation("invalid slug").into())
    }
}

fn valid_domain<'v, T: Into<&'v str>>(s: T) -> form::Result<'v, ()> {
    let re = Regex::new("^[-a-z0-9]+\\.[a-z]+$").unwrap();

    if re.is_match(s.into()) {
        Ok(())
    } else {
        Err(form::Error::validation("invalid domain").into())
    }
}

fn valid_username<'v, T: Into<&'v str>>(s: T) -> form::Result<'v, ()> {
    let re = Regex::new("^[a-z0-9]{2,}$").unwrap();

    if re.is_match(s.into()) {
        Ok(())
    } else {
        Err(form::Error::validation("invalid username").into())
    }
}

fn option_len<'v, V, L, R>(opt: &Option<V>, range: R) -> form::Result<'v, ()>
where
    V: form::validate::Len<L>,
    L: Copy + PartialOrd,
    R: RangeBounds<L>,
{
    if let Some(inner) = opt {
        form::validate::len(inner, range)
    } else {
        Ok(())
    }
}
