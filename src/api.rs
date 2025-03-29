use rocket::catchers;
use serde_json::json;

pub mod v0;

pub fn catchers() -> Vec<rocket::Catcher> {
    catchers![not_found, unknown]
}

#[rocket::catch(404)]
fn not_found() -> serde_json::Value {
    // same format as AppErrorDto when serialized
    json!({
        "error": true,
        "info": {
            "key": "api.path.unknown"
        }
    })
}

#[rocket::catch(default)]
fn unknown() -> serde_json::Value {
    // same format as AppErrorDto when serialized
    json!({
        "error": true,
        "info": {
            "key": "api.error"
        }
    })
}
