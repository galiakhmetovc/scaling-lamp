use super::*;
use agent_runtime::prompt::MemoryRecall;
use agent_runtime::tool::{
    KvDeleteInput, KvDeleteOutput, KvListInput, KvListOutput, KvPutInput, KvPutOutput,
    MemoryDeleteInput, MemoryDeleteOutput, MemoryListInput, MemoryListOutput, MemorySearchInput,
    MemorySearchOutput, MemoryUpdateInput, MemoryUpdateOutput,
};

impl App {
    pub fn semantic_memory_search(
        &self,
        session_id: &str,
        input: MemorySearchInput,
    ) -> Result<MemorySearchOutput, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .semantic_memory_search_for_session(&store, session_id, &input)
            .map_err(BootstrapError::Execution)
    }

    pub fn semantic_memory_search_context(
        &self,
        session_id: Option<&str>,
        input: MemorySearchInput,
    ) -> Result<MemorySearchOutput, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .semantic_memory_search_for_context(&store, session_id, &input)
            .map_err(BootstrapError::Execution)
    }

    pub fn semantic_memory_list(
        &self,
        session_id: &str,
        input: MemoryListInput,
    ) -> Result<MemoryListOutput, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .semantic_memory_list_for_session(&store, session_id, &input)
            .map_err(BootstrapError::Execution)
    }

    pub fn semantic_memory_list_context(
        &self,
        session_id: Option<&str>,
        input: MemoryListInput,
    ) -> Result<MemoryListOutput, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .semantic_memory_list_for_context(&store, session_id, &input)
            .map_err(BootstrapError::Execution)
    }

    pub fn semantic_memory_update(
        &self,
        input: MemoryUpdateInput,
    ) -> Result<MemoryUpdateOutput, BootstrapError> {
        self.execution_service()
            .semantic_memory_update(&input)
            .map_err(BootstrapError::Execution)
    }

    pub fn semantic_memory_delete(
        &self,
        input: MemoryDeleteInput,
    ) -> Result<MemoryDeleteOutput, BootstrapError> {
        self.execution_service()
            .semantic_memory_delete(&input)
            .map_err(BootstrapError::Execution)
    }

    pub fn kv_list(
        &self,
        session_id: &str,
        input: KvListInput,
        now: i64,
    ) -> Result<KvListOutput, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .kv_list_for_session(&store, session_id, &input, now)
            .map_err(BootstrapError::Execution)
    }

    pub fn kv_list_context(
        &self,
        session_id: Option<&str>,
        input: KvListInput,
        now: i64,
    ) -> Result<KvListOutput, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .kv_list_for_context(&store, session_id, &input, now)
            .map_err(BootstrapError::Execution)
    }

    pub fn kv_put(
        &self,
        session_id: &str,
        input: KvPutInput,
        now: i64,
    ) -> Result<KvPutOutput, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .kv_put_for_session(&store, session_id, &input, now)
            .map_err(BootstrapError::Execution)
    }

    pub fn kv_put_context(
        &self,
        session_id: Option<&str>,
        input: KvPutInput,
        now: i64,
    ) -> Result<KvPutOutput, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .kv_put_for_context(&store, session_id, &input, now)
            .map_err(BootstrapError::Execution)
    }

    pub fn kv_delete(
        &self,
        session_id: &str,
        input: KvDeleteInput,
    ) -> Result<KvDeleteOutput, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .kv_delete_for_session(&store, session_id, &input)
            .map_err(BootstrapError::Execution)
    }

    pub fn kv_delete_context(
        &self,
        session_id: Option<&str>,
        input: KvDeleteInput,
    ) -> Result<KvDeleteOutput, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .kv_delete_for_context(&store, session_id, &input)
            .map_err(BootstrapError::Execution)
    }

    pub fn memory_recall_preview(
        &self,
        session_id: &str,
        query: Option<&str>,
    ) -> Result<Option<MemoryRecall>, BootstrapError> {
        let store = self.store()?;
        self.execution_service()
            .preview_memory_recall_for_session(&store, session_id, query)
            .map_err(BootstrapError::Execution)
    }
}
