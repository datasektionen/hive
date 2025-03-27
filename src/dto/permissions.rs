use rocket::{
    form::{self, FromFormField},
    FromForm,
};

use super::{groups::GroupRefDto, TrimmedStr};

#[derive(FromForm)]
pub struct CreatePermissionDto<'v> {
    #[field(validate = super::valid_slug())]
    pub id: TrimmedStr<'v>,
    #[field(validate = len(3..))]
    pub description: TrimmedStr<'v>,
    pub scoped: bool,
}

#[derive(FromForm)]
pub struct AssignPermissionDto<'v> {
    pub perm: PermissionKey<'v>,
    #[field(validate = with(|o| o.map(|s| !s.is_empty()).unwrap_or(true), "invalid empty scope"))]
    pub scope: Option<TrimmedStr<'v>>,
}

#[derive(FromForm)]
pub struct AssignPermissionToGroupDto<'v> {
    pub group: GroupRefDto<'v>,
    #[field(validate = with(|o| o.map(|s| !s.is_empty()).unwrap_or(true), "invalid empty scope"))]
    pub scope: Option<TrimmedStr<'v>>,
}

pub struct PermissionKey<'v> {
    pub system_id: &'v str,
    pub perm_id: &'v str,
}

#[rocket::async_trait]
impl<'v> FromFormField<'v> for PermissionKey<'v> {
    fn from_value(field: form::ValueField<'v>) -> form::Result<'v, Self> {
        if let Some(value) = field.value.trim().strip_prefix('$') {
            let mut split = value.splitn(2, ':');

            let system_id = split.next().unwrap();
            let perm_id = split
                .next()
                .ok_or(form::Error::validation("missing : separator"))?;

            super::valid_slug(system_id)?;
            super::valid_slug(perm_id)?;

            Ok(Self { system_id, perm_id })
        } else {
            Err(form::Error::validation("missing $ prefix").into())
        }
    }
}
