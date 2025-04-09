use std::path::PathBuf;

use rocket::{
    fairing::{self, Fairing},
    http::{Header, Method},
    response::{self, Responder},
    routes, Build, Request, Response, Rocket,
};

use crate::guards::cors::PreflightRequestHeaders;

// rocket_cors crate fairing wouldn't support different options for /api/, so
// we must implement this ourselves...

// reference: https://www.w3.org/TR/2020/SPSD-cors-20200602/#resource-processing-model
// ^ superseded, but the new spec doesn't have procedure from server pov

const ALLOWED_CROSS_ORIGIN_API_METHODS: &[Method] = &[Method::Get];

fn allow_cross_origin(path: &str) -> bool {
    // we allow all origins iff path is /api/**/* (but not just /api)

    if let Some(rest) = path.trim_matches('/').strip_prefix("api") {
        !rest.is_empty()
    } else {
        false
    }
}

pub struct Cors;

#[rocket::async_trait]
impl Fairing for Cors {
    fn info(&self) -> fairing::Info {
        fairing::Info {
            name: "CORS Handler",
            kind: fairing::Kind::Ignite | fairing::Kind::Response,
        }
    }

    async fn on_ignite(&self, rocket: Rocket<Build>) -> fairing::Result {
        let rocket = rocket.mount("/", routes![preflight]);

        Ok(rocket)
    }

    // https://www.w3.org/TR/2020/SPSD-cors-20200602/#resource-requests
    async fn on_response<'r>(&self, req: &'r Request<'_>, res: &mut Response<'r>) {
        if req.method() == Method::Options {
            // we don't deal with preflight requests here
            return;
        }

        // 1. If the Origin header is not present terminate this set of steps.
        // The request is outside the scope of this specification.
        let origin = match req.headers().get_one("Origin") {
            Some(origin) => origin,
            None => return,
        };

        // 2. If the value of the Origin header is not a case-sensitive match
        // for any of the values in list of origins, do not set any additional
        // headers and terminate this set of steps.
        if !allow_cross_origin(req.uri().path().as_str()) {
            // we either allow all origins or none
            return;
        }

        // 3. If the resource supports credentials add a single
        // Access-Control-Allow-Origin header, with the value of the Origin
        // header as value, and add a single Access-Control-Allow-Credentials
        // header with the case-sensitive string "true" as value.
        // Otherwise, add a single Access-Control-Allow-Origin header, with
        // either the value of the Origin header or the string "*" as value.
        res.set_raw_header("Access-Control-Allow-Origin", origin);
        res.set_raw_header("Access-Control-Allow-Credentials", "true");

        // 4. If the list of exposed headers is not empty add one or more
        // Access-Control-Expose-Headers headers, with as values the header
        // field names given in the list of exposed headers.
        // [we don't expose any]
    }
}

#[derive(Responder)]
enum CorsError {
    #[response(status = 403)]
    OriginNotAllowed(()),
    #[response(status = 400)]
    NoRequestedMethod(()),
    #[response(status = 400)]
    InvalidRequestedHeader(()),
    #[response(status = 403)]
    MethodNotAllowed(()),
}

struct PreflightResponse<'r> {
    origin: &'r str,
    supports_credentials: bool,
    supported_headers: Vec<&'r str>,
}

impl<'r> Responder<'r, 'r> for PreflightResponse<'r> {
    fn respond_to(self, req: &'r Request<'_>) -> response::Result<'r> {
        let mut builder = Response::build_from(().respond_to(req)?);

        builder.raw_header("Access-Control-Allow-Origin", self.origin);

        if self.supports_credentials {
            builder.raw_header("Access-Control-Allow-Credentials", "true");
        }

        let supported_methods = ALLOWED_CROSS_ORIGIN_API_METHODS
            .iter()
            .map(Method::to_string)
            .collect::<Vec<_>>()
            .join(",");
        builder.raw_header("Access-Control-Allow-Methods", supported_methods);

        if !self.supported_headers.is_empty() {
            builder.raw_header(
                "Access-Control-Allow-Headers",
                self.supported_headers.join(","),
            );
        }

        Ok(builder.finalize())
    }
}

// https://www.w3.org/TR/2020/SPSD-cors-20200602/#resource-preflight-requests
#[rocket::options("/<path..>")]
fn preflight(
    path: PathBuf,
    headers: PreflightRequestHeaders<'_>,
) -> Result<PreflightResponse<'_>, CorsError> {
    // 1. If the Origin header is not present terminate this set of steps. The
    // request is outside the scope of this specification.
    // [already verified by CorsRequestHeaders guard]

    // 2. If the value of the Origin header is not a case-sensitive match for
    // any of the values in list of origins do not set any additional headers
    // and terminate this set of steps.
    let path_str = path.as_os_str().to_str();
    if !path_str.map(allow_cross_origin).unwrap_or(false) {
        // we either allow all origins or none
        return Err(CorsError::OriginNotAllowed(()));
    };

    // 3. Let `method` be the value as result of parsing the
    // Access-Control-Request-Method header. If there is no
    // Access-Control-Request-Method header or if parsing failed, do not set any
    // additional headers and terminate this set of steps. The request is
    // outside the scope of this specification.
    let method = match headers.acr_method {
        Some(method) => method,
        None => return Err(CorsError::NoRequestedMethod(())),
    };

    // 4. Let `header field-names` be the values as result of parsing the
    // Access-Control-Request-Headers headers. If there are no
    // Access-Control-Request-Headers headers let `header field-names` be the
    // empty list. If parsing failed do not set any additional headers and
    // terminate this set of steps. The request is outside the scope of this
    // specification.
    let valid_headers = headers
        .acr_headers
        .iter()
        .map(std::ops::Deref::deref)
        .all(Header::is_valid_name);
    if !valid_headers {
        return Err(CorsError::InvalidRequestedHeader(()));
    }

    // 5. If `method` is not a case-sensitive match for any of the values in
    // list of methods do not set any additional headers and terminate this set
    // of steps.
    let method_matches = ALLOWED_CROSS_ORIGIN_API_METHODS
        .iter()
        .any(|m| m.as_str() == method);
    if !method_matches {
        return Err(CorsError::MethodNotAllowed(()));
    }

    // 6. If any of the header field-names is not a ASCII case-insensitive match
    // for any of the values in list of headers do not set any additional
    // headers and terminate this set of steps.
    // [we allow all headers; no check is necessary]

    // 7. If the resource supports credentials add a single
    // Access-Control-Allow-Origin header, with the value of the Origin header
    // as value, and add a single Access-Control-Allow-Credentials header with
    // the case-sensitive string "true" as value.
    // Otherwise, add a single Access-Control-Allow-Origin header, with either
    // the value of the Origin header or the string "*" as value.
    // [handled by SuccessfulPreflight responder]

    // 8. Optionally add a single Access-Control-Max-Age header with as value
    // the amount of seconds the user agent is allowed to cache the result of
    // the request.
    // [we don't want to]

    // 9. If `method` is a simple method this step may be skipped. Add one or
    // more Access-Control-Allow-Methods headers consisting of (a subset of) the
    // list of methods.
    // [handled by SuccessfulPreflight responder]

    // 10. If each of the `header field-names` is a simple header and none is
    // `Content-Type`, this step may be skipped. Add one or more
    // Access-Control-Allow-Headers headers consisting of (a subset of) the
    // list of headers.
    // [handled by SuccessfulPreflight responder]

    Ok(PreflightResponse {
        origin: headers.origin,
        supports_credentials: true, // we already checked
        supported_headers: headers.acr_headers,
    })
}
