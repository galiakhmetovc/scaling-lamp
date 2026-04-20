#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    None,
    Exit,
    ActivateSelectedSession,
    OpenSessionScreen,
    OpenNewSessionDialog,
    OpenDeleteDialog,
    OpenRenameDialog,
    OpenClearDialog,
    ConfirmDialog,
    SubmitChatInput(String),
    QueueChatInput(String),
    CyclePreviousCommand,
}
