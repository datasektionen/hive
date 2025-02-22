use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request,
};

// hack: this is cursed, but I can't use
// `Header<const NAME: &str>` because &str is a
// forbidden const type; instead, we use an index
// to this array
const HEADER_NAMES: &[&str] = &["Accept-Language"];

pub struct Header<'r, const N: usize>(&'r str);

pub type AcceptLanguage<'r> = Header<'r, 0>;

#[derive(Debug)]
pub struct MissingHeader;

#[rocket::async_trait]
impl<'r, const N: usize> FromRequest<'r> for Header<'r, N> {
    type Error = MissingHeader;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        if let Some(value) = req.headers().get_one(HEADER_NAMES[N]) {
            Outcome::Success(Self(value))
        } else {
            Outcome::Error((Status::BadRequest, MissingHeader))
        }
    }
}

impl<'r, const N: usize> From<Header<'r, N>> for &'r str {
    fn from(header: Header<'r, N>) -> Self {
        header.0
    }
}
