pub use catchers::catchers;

use crate::perms::HivePermission;

mod catchers;
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
