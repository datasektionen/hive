use rocket::catchers;
use serde_json::json;

use crate::perms::HivePermission;

pub mod v0;
pub mod v1;

#[derive(Clone)]
pub enum HiveApiPermission {
    CheckPermissions,
    ListTagged,
}

impl From<HiveApiPermission> for HivePermission {
    fn from(perm: HiveApiPermission) -> Self {
        match perm {
            HiveApiPermission::CheckPermissions => HivePermission::ApiCheckPermissions,
            HiveApiPermission::ListTagged => HivePermission::ApiListTagged,
        }
    }
}

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
