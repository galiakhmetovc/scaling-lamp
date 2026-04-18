#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub title: String,
}

impl Default for Session {
    fn default() -> Self {
        Self {
            title: "bootstrap".to_string(),
        }
    }
}
