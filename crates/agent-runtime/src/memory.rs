#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryIndex {
    pub working_set_limit: usize,
}

impl Default for MemoryIndex {
    fn default() -> Self {
        Self {
            working_set_limit: 64,
        }
    }
}
