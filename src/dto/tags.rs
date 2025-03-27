use rocket::FromForm;

use super::TrimmedStr;

#[derive(FromForm)]
pub struct CreateTagDto<'v> {
    #[field(validate = super::valid_slug())]
    pub id: TrimmedStr<'v>,
    #[field(validate = len(3..))]
    pub description: TrimmedStr<'v>,
    #[field(validate = with(|this| *this || self.supports_users, "tag must support something"))]
    pub supports_groups: bool,
    #[field(validate = with(|this| *this || self.supports_groups, "tag must support something"))]
    pub supports_users: bool,
    pub has_content: bool,
}
