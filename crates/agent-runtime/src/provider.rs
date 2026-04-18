#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderDescriptor {
    pub name: String,
    pub model_family: String,
}

impl Default for ProviderDescriptor {
    fn default() -> Self {
        Self {
            name: "unconfigured".to_string(),
            model_family: "none".to_string(),
        }
    }
}
