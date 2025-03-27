use rocket::FromForm;

use super::{groups::GroupRefDto, TrimmedStr};

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

#[derive(FromForm)]
pub struct AssignTagToGroupDto<'v> {
    pub group: GroupRefDto<'v>,
    #[field(validate = super::option_len(1..))]
    pub content: Option<TrimmedStr<'v>>,
}

#[derive(FromForm)]
pub struct AssignTagToUserDto<'v> {
    #[field(validate = super::valid_username())]
    pub user: TrimmedStr<'v>,
    #[field(validate = super::option_len(1..))]
    pub content: Option<TrimmedStr<'v>>,
}
