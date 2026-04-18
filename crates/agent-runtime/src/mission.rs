#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissionSpec {
    pub objective: String,
}

impl Default for MissionSpec {
    fn default() -> Self {
        Self {
            objective: "bootstrap autonomous runtime".to_string(),
        }
    }
}
