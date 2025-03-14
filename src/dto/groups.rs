use rocket::FromForm;

#[derive(FromForm)]
pub struct EditGroupDto<'v> {
    #[field(validate = len(3..))]
    pub name_sv: &'v str,
    #[field(validate = len(3..))]
    pub name_en: &'v str,
    #[field(validate = len(10..))]
    pub description_sv: &'v str,
    #[field(validate = len(10..))]
    pub description_en: &'v str,
}
