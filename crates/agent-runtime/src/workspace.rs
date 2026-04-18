use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRef {
    pub root: PathBuf,
}

impl Default for WorkspaceRef {
    fn default() -> Self {
        Self {
            root: PathBuf::from("."),
        }
    }
}
