#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RecoveryMode {
    #[default]
    Reconcile,
    MarkInterrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecoveryPolicy {
    pub mode: RecoveryMode,
}
