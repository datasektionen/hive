use rocket::catchers;
use serde_json::json;

pub fn catchers() -> Vec<rocket::Catcher> {
    catchers![not_found, unauthorized, unknown]
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

#[rocket::catch(401)]
fn unauthorized() -> serde_json::Value {
    // same format as AppErrorDto when serialized
    json!({
        "error": true,
        "info": {
            "key": "api.unauthorized"
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
