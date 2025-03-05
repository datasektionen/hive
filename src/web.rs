use rocket::{
    response::{content::RawHtml, Redirect},
    uri,
};

use crate::routing::RouteTree;

mod api_tokens;
mod groups;
mod systems;

type RenderedTemplate = RawHtml<String>;

pub fn tree() -> RouteTree {
    RouteTree::Branch(vec![
        api_tokens::routes(),
        groups::routes(),
        systems::routes(),
        rocket::routes![favicon].into(),
    ])
}

#[rocket::get("/favicon.ico")]
async fn favicon() -> Redirect {
    // browsers expect favicon at root; redirect to real path
    Redirect::permanent(uri!("/static/icons/favicon.ico"))
}

mod filters {
    use regex::RegexBuilder;
    use rinja::filters::Safe;

    pub fn highlight<T: ToString>(s: Safe<T>, term: &str) -> rinja::Result<Safe<String>> {
        let s = s.0.to_string();

        let result = if term.is_empty() {
            s
        } else {
            let re = RegexBuilder::new(&regex::escape(term))
                .case_insensitive(true)
                .build()
                .unwrap();

            re.replace_all(&s, "<mark>$0</mark>").to_string()
        };

        Ok(Safe(result))
    }
}
