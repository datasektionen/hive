pub use catchers::catchers;

use crate::perms::HivePermission;

mod catchers;
pub mod v0;
pub mod v1;

pub struct ApiVersionInfo<'a> {
    pub n: u8,
    pub annotation: Option<(&'a str, &'a str)>, // en, sv
    pub deprecated: bool,
    pub recommended: bool, // e.g., not in beta
}

pub const API_VERSIONS: &[ApiVersionInfo<'static>] = &[
    ApiVersionInfo {
        n: 0,
        annotation: Some(("legacy", "äldre")),
        deprecated: true,
        recommended: false,
    },
    ApiVersionInfo {
        n: 1,
        annotation: Some(("preferred", "föredraget")),
        deprecated: false,
        recommended: true,
    },
];

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
