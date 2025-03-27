use rocket::{
    form::{self, FromFormField},
    FromForm,
};

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
pub struct AssignTagDto<'v> {
    pub tag: TagKey<'v>,
    #[field(validate = super::option_len(1..))]
    pub content: Option<TrimmedStr<'v>>,
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

pub struct TagKey<'v> {
    pub system_id: &'v str,
    pub tag_id: &'v str,
}

#[rocket::async_trait]
impl<'v> FromFormField<'v> for TagKey<'v> {
    fn from_value(field: form::ValueField<'v>) -> form::Result<'v, Self> {
        if let Some(value) = field.value.trim().strip_prefix('#') {
            let mut split = value.splitn(2, ':');

            let system_id = split.next().unwrap();
            let tag_id = split
                .next()
                .ok_or(form::Error::validation("missing : separator"))?;

            super::valid_slug(system_id)?;
            super::valid_slug(tag_id)?;

            Ok(Self { system_id, tag_id })
        } else {
            Err(form::Error::validation("missing # prefix").into())
        }
    }
}
