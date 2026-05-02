use agent_persistence::PersistenceStore;
use agent_runtime::provider::ProviderDriver;
use agent_runtime::tool::ToolCall;

#[derive(Clone, Copy)]
pub(super) struct ProviderToolExecutionContext<'a> {
    pub(super) store: &'a PersistenceStore,
    pub(super) provider: &'a dyn ProviderDriver,
    pub(super) session_id: &'a str,
    pub(super) run_id: &'a str,
    pub(super) now: i64,
}

#[derive(Clone, Copy)]
pub(super) struct ModelToolExecutionContext<'a> {
    pub(super) store: &'a PersistenceStore,
    pub(super) provider: Option<&'a dyn ProviderDriver>,
    pub(super) session_id: &'a str,
    pub(super) run_id: &'a str,
    pub(super) now: i64,
}

pub(super) struct ProviderToolCallInvocation<'a> {
    pub(super) tool_call_id: &'a str,
    pub(super) arguments_json: &'a str,
    pub(super) parsed: &'a ToolCall,
}
