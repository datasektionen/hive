use rocket::{
    form::{self, FromFormField},
    FromForm,
};

use super::TrimmedStr;

#[derive(FromForm)]
pub struct EditGroupDto<'v> {
    #[field(validate = len(3..))]
    pub name_sv: TrimmedStr<'v>,
    #[field(validate = len(3..))]
    pub name_en: TrimmedStr<'v>,
    #[field(validate = len(10..))]
    pub description_sv: TrimmedStr<'v>,
    #[field(validate = len(10..))]
    pub description_en: TrimmedStr<'v>,
}

#[derive(FromForm)]
pub struct AddSubgroupDto<'v> {
    pub child: GroupRefDto<'v>,
    pub manager: bool,
}

pub struct GroupRefDto<'v> {
    pub id: &'v str,
    pub domain: &'v str,
}

impl<'v> FromFormField<'v> for GroupRefDto<'v> {
    fn from_value(field: form::ValueField<'v>) -> form::Result<'v, Self> {
        let value = field.value.trim();

        let mut split = value.splitn(2, '@');
        let id = split.next().unwrap();
        let domain = split
            .next()
            .ok_or(form::Error::validation("invalid group ref: no @ separator"))?;

        super::valid_slug(id)?;
        super::valid_domain(domain)?;

        Ok(Self { id, domain })
    }
}
