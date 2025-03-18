use rocket::FromForm;

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
