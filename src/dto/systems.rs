use rocket::FromForm;

#[derive(FromForm)]
pub struct CreateSystemDto<'v> {
    #[field(validate = super::valid_slug())]
    pub id: &'v str,
    #[field(validate = len(3..))]
    pub description: &'v str,
}
