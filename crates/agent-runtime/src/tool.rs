#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolCatalog {
    pub families: Vec<&'static str>,
}

impl Default for ToolCatalog {
    fn default() -> Self {
        Self {
            families: vec!["fs", "exec"],
        }
    }
}
