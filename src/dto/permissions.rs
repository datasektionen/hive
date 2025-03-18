use rocket::FromForm;

use super::TrimmedStr;

#[derive(FromForm)]
pub struct CreatePermissionDto<'v> {
    #[field(validate = super::valid_slug())]
    pub id: TrimmedStr<'v>,
    #[field(validate = len(3..))]
    pub description: TrimmedStr<'v>,
    pub scoped: bool,
}
