#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanSnapshot {
    pub active_step: String,
}

impl Default for PlanSnapshot {
    fn default() -> Self {
        Self {
            active_step: "scaffold workspace".to_string(),
        }
    }
}
