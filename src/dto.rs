use regex::Regex;
use rocket::form;

pub mod api_tokens;
pub mod errors;
pub mod permissions;
pub mod systems;

fn valid_slug(s: &str) -> form::Result<()> {
    let re = Regex::new("^[a-z0-9]+(-[a-z0-9]+)*$").unwrap();

    if re.is_match(s) {
        Ok(())
    } else {
        Err(form::Error::validation("invalid slug").into())
    }
}
