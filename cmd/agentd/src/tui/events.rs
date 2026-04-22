#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TuiAction {
    None,
    Exit,
    ActivateSelectedSession,
    OpenSessionScreen,
    OpenAgentsScreen,
    OpenSchedulesScreen,
    OpenNewSessionDialog,
    OpenDeleteDialog,
    OpenRenameDialog,
    OpenClearDialog,
    ConfirmDialog,
    SubmitChatInput(String),
    QueueChatInput(String),
    CyclePreviousCommand,
}
