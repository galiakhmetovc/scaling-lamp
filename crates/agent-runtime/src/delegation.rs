#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DelegateHandle {
    pub label: String,
}

impl Default for DelegateHandle {
    fn default() -> Self {
        Self {
            label: "root".to_string(),
        }
    }
}
