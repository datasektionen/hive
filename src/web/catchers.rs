use rocket::{
    catchers,
    http::{Header, Method, Status},
    response::{content::RawHtml, Redirect},
    uri, Request, Responder,
};

use super::RenderedTemplate;
use crate::{
    errors::render_error_page,
    guards::{context::PageContext, headers::HxRequest},
};

pub fn catchers() -> Vec<rocket::Catcher> {
    catchers![not_found, unauthenticated, unknown]
}

#[derive(Responder)]
pub enum Caught {
    Partial(RenderedTemplate, Header<'static>),
    Full(RenderedTemplate),
}

// note: an alternative implementation would be for catchers to simply return
// some AppError variant, which would then respond with a JSON representation
// intercepted by the main error page generator fairing, which would finally
// render the HTML based on the AppError. this would make each catcher much
// simpler, but would mean serializing and deserializing for no reason when
// we already know for sure what we want to do. as such, we instead just render
// the error page directly ourselves, and then skip all actions in the fairing

macro_rules! show_error_page {
    ($name:ident, $num:expr, $status:expr, $i18n_key:expr) => {
        #[rocket::catch($num)]
        async fn $name(req: &Request<'_>) -> Caught {
            let ctx = req
                .guard::<PageContext>()
                .await
                .succeeded()
                .expect("infallible page context guard");

            let title = ctx.t(concat!("errors.caught.", $i18n_key, ".title"));
            let description = ctx.t(concat!("errors.caught.", $i18n_key, ".description"));

            let partial = req.guard::<HxRequest>().await.succeeded();
            let html = render_error_page(title, description, $status, ctx, partial.is_some());

            if partial.is_some() {
                Caught::Partial(
                    RawHtml(html),
                    Header::new("HX-Reswap", "none"), // only oob swaps, ignore the rest
                )
            } else {
                Caught::Full(RawHtml(html))
            }
        }
    };
}

show_error_page!(not_found, 404, Status::NotFound, "not-found");
show_error_page!(unknown, default, Status::InternalServerError, "unknown");

#[rocket::catch(401)]
fn unauthenticated(req: &Request<'_>) -> Redirect {
    let next = if req.method() == Method::Get {
        // ensure user is redirected to this page after logging in
        Some(req.uri().to_string())
    } else {
        None
    };

    Redirect::to(uri!(super::auth::login(next)))
}
