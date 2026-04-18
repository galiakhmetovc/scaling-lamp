#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunStatus {
    #[default]
    Queued,
    Running,
    WaitingApproval,
    Completed,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RunSnapshot {
    pub status: RunStatus,
}
