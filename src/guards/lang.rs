use std::{borrow::Cow, fmt};

use rocket::{
    http::CookieJar,
    request::{FromRequest, Outcome},
    FromFormField, Request,
};

use super::{headers::AcceptLanguage, Infallible};

const DEFAULT_LANG: Language = Language::Swedish;
const LANG_COOKIE_NAME: &str = "Hive-Lang"; // set by frontend on lang change

#[derive(FromFormField)]
pub enum Language {
    #[field(value = "sv")]
    Swedish,
    #[field(value = "en")]
    English,
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Swedish => write!(f, "sv"),
            Self::English => write!(f, "en-US"),
        }
    }
}

impl Language {
    fn from_tag(tag: &str) -> Option<Self> {
        let tag = tag.to_lowercase();

        if tag == "sv" || tag.starts_with("sv-") {
            Some(Self::Swedish)
        } else if tag == "en" || tag.starts_with("en-") {
            Some(Self::English)
        } else {
            None
        }
    }

    fn i18n_locale(&self) -> &str {
        match self {
            Self::Swedish => "sv",
            Self::English => "en",
        }
    }

    // only works if there are just 2 locales
    pub fn other(&self) -> Self {
        match self {
            Self::Swedish => Self::English,
            Self::English => Self::Swedish,
        }
    }

    pub fn t<'a>(&self, key: &'a str) -> Cow<'a, str> {
        rust_i18n::t!(key, locale = self.i18n_locale())
    }

    // since this isn't a macro, we can't accept an arbitrary # of arguments...
    // (it also shouldn't be a macro because askama doesn't replace variables)
    // https://github.com/rinja-rs/askama/blob/704f8/book/src/template_syntax.md#calling-rust-macros
    pub fn t1<'a, T: fmt::Display>(&self, key: &'a str, x: T) -> Cow<'a, str> {
        rust_i18n::t!(key, locale = self.i18n_locale(), x = x)
    }
}

fn negotiate_language(accept_language: &str) -> Option<Language> {
    for range in accept_language.split(",") {
        if let Some(tag) = range.split(";").next() {
            if let Some(lang) = Language::from_tag(tag) {
                return Some(lang);
            }
        }
    }

    None
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Language {
    type Error = Infallible;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        if let Outcome::Success(jar) = req.guard::<&CookieJar>().await {
            if let Some(cookie) = jar.get(LANG_COOKIE_NAME) {
                if let Some(lang) = Language::from_tag(cookie.value_trimmed()) {
                    return Outcome::Success(lang);
                }
            }
        }

        if let Outcome::Success(header) = req.guard::<AcceptLanguage>().await {
            if let Some(lang) = negotiate_language(header.into()) {
                return Outcome::Success(lang);
            }
        }

        Outcome::Success(DEFAULT_LANG)
    }
}
