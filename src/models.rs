use sqlx::FromRow;

#[derive(FromRow)]
pub struct Group {
    pub id: String,
    pub domain: String,
    pub name_sv: String,
    pub name_en: String,
    pub description_sv: String,
    pub description_en: String,
}

#[derive(FromRow)]
pub struct System {
    pub id: String,
    pub description: String,
}

#[derive(FromRow)]
pub struct BasePermissionAssignment {
    pub system_id: String,
    pub perm_id: String,
    pub scope: Option<String>,
}
