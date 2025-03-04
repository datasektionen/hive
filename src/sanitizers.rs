// Sanitizes search terms for PostgreSQL `LIKE` and `ILIKE` pattern matching
pub struct SearchTerm {
    sanitized: String,
}

impl From<&str> for SearchTerm {
    fn from(raw: &str) -> Self {
        let sanitized = raw
            .replace("\\", "\\\\")
            .replace("%", "\\%")
            .replace("_", "\\_");

        Self { sanitized }
    }
}

impl SearchTerm {
    pub fn anywhere(&self) -> String {
        format!("%{}%", self.sanitized)
    }
}
