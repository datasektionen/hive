use std::fmt;

use rocket::{
    request::{FromRequest, Outcome},
    Request,
};

use super::headers::AcceptLanguage;

const DEFAULT_LANG: Language = Language::Swedish;

pub enum Language {
    Swedish,
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
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        // TODO: read cookie to check explicit language

        if let Outcome::Success(header) = req.guard::<AcceptLanguage>().await {
            if let Some(lang) = negotiate_language(header.into()) {
                return Outcome::Success(lang);
            }
        }

        Outcome::Success(DEFAULT_LANG)
    }
}
