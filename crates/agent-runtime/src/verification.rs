#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceBundle {
    pub required_checks: Vec<&'static str>,
}

impl Default for EvidenceBundle {
    fn default() -> Self {
        Self {
            required_checks: vec!["fmt", "clippy", "test"],
        }
    }
}
