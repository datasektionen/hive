use rocket::FromForm;

use super::TrimmedStr;

#[derive(FromForm)]
pub struct CreateSystemDto<'v> {
    #[field(validate = super::valid_slug())]
    pub id: TrimmedStr<'v>,
    #[field(validate = len(3..))]
    pub description: TrimmedStr<'v>,
}

#[derive(FromForm)]
pub struct EditSystemDto<'v> {
    #[field(validate = len(3..))]
    pub description: TrimmedStr<'v>,
}
