use rocket::{
    http::Status,
    request::{FromRequest, Outcome},
    Request,
};

pub struct PreflightRequestHeaders<'r> {
    pub origin: &'r str,
    pub acr_method: Option<&'r str>,
    pub acr_headers: Vec<&'r str>,
}

#[derive(Debug)]
pub enum PreflightRequestHeadersParseFailure {
    NoOrigin,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for PreflightRequestHeaders<'r> {
    type Error = PreflightRequestHeadersParseFailure;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let origin = match req.headers().get_one("Origin") {
            Some(origin) => origin,
            None => return Outcome::Error((Status::NotFound, Self::Error::NoOrigin)),
        };

        let acr_method = req.headers().get_one("Access-Control-Request-Method");

        let acr_headers = req
            .headers()
            .get("Access-Control-Request-Headers")
            .flat_map(|v| v.split(','))
            .map(str::trim)
            .collect();

        Outcome::Success(Self {
            origin,
            acr_method,
            acr_headers,
        })
    }
}
