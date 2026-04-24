use super::*;

pub(super) fn bootstrap_schema(connection: &Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "PRAGMA foreign_keys = ON;

         CREATE TABLE IF NOT EXISTS sessions (
             id TEXT PRIMARY KEY,
             title TEXT NOT NULL,
             prompt_override TEXT,
             settings_json TEXT NOT NULL,
             agent_profile_id TEXT NOT NULL DEFAULT 'default',
             active_mission_id TEXT,
             parent_session_id TEXT,
             parent_job_id TEXT,
             delegation_label TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             FOREIGN KEY(active_mission_id) REFERENCES missions(id) ON DELETE SET NULL
         );

         CREATE TABLE IF NOT EXISTS missions (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             objective TEXT NOT NULL,
             status TEXT NOT NULL,
             execution_intent TEXT NOT NULL,
             schedule_json TEXT NOT NULL,
             acceptance_json TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             completed_at INTEGER,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
         );

         CREATE TABLE IF NOT EXISTS runs (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             mission_id TEXT,
             status TEXT NOT NULL,
             error TEXT,
             result TEXT,
             provider_usage_json TEXT NOT NULL DEFAULT 'null',
             active_processes_json TEXT NOT NULL DEFAULT '[]',
             recent_steps_json TEXT NOT NULL,
             evidence_refs_json TEXT NOT NULL,
             pending_approvals_json TEXT NOT NULL,
             provider_loop_json TEXT NOT NULL,
             delegate_runs_json TEXT NOT NULL,
             started_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             finished_at INTEGER,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
             FOREIGN KEY(mission_id) REFERENCES missions(id) ON DELETE SET NULL
         );

         CREATE TABLE IF NOT EXISTS jobs (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             mission_id TEXT,
             run_id TEXT,
             parent_job_id TEXT,
             kind TEXT NOT NULL,
             status TEXT NOT NULL,
             input_json TEXT,
             result_json TEXT,
             error TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             started_at INTEGER,
             finished_at INTEGER,
             attempt_count INTEGER NOT NULL DEFAULT 0,
             max_attempts INTEGER NOT NULL DEFAULT 1,
             lease_owner TEXT,
             lease_expires_at INTEGER,
             heartbeat_at INTEGER,
             cancel_requested_at INTEGER,
             last_progress_message TEXT,
             callback_json TEXT,
             callback_sent_at INTEGER,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
             FOREIGN KEY(mission_id) REFERENCES missions(id) ON DELETE SET NULL,
             FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE SET NULL,
             FOREIGN KEY(parent_job_id) REFERENCES jobs(id) ON DELETE SET NULL
         );

         CREATE TABLE IF NOT EXISTS transcripts (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             run_id TEXT,
             kind TEXT NOT NULL,
             storage_key TEXT NOT NULL,
             byte_len INTEGER NOT NULL,
             sha256 TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
             FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE SET NULL
         );

         CREATE TABLE IF NOT EXISTS tool_calls (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             run_id TEXT NOT NULL,
             provider_tool_call_id TEXT NOT NULL,
             tool_name TEXT NOT NULL,
             arguments_json TEXT NOT NULL,
             summary TEXT NOT NULL,
             status TEXT NOT NULL,
             error TEXT,
             result_summary TEXT,
             result_preview TEXT,
             result_artifact_id TEXT,
             result_truncated INTEGER NOT NULL DEFAULT 0,
             result_byte_len INTEGER,
             requested_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
             FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE CASCADE
         );

         CREATE TABLE IF NOT EXISTS session_inbox_events (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             job_id TEXT,
             kind TEXT NOT NULL,
             payload_json TEXT NOT NULL,
             status TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             available_at INTEGER NOT NULL,
             claimed_at INTEGER,
             processed_at INTEGER,
             error TEXT,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
             FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
         );

         CREATE TABLE IF NOT EXISTS agent_profiles (
             id TEXT PRIMARY KEY,
             name TEXT NOT NULL,
             template_kind TEXT NOT NULL,
             agent_home TEXT NOT NULL,
             allowed_tools_json TEXT NOT NULL,
             created_from_template_id TEXT,
             created_by_session_id TEXT,
             created_by_agent_profile_id TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );

         CREATE TABLE IF NOT EXISTS daemon_state (
             key TEXT PRIMARY KEY,
             value TEXT
         );

         CREATE TABLE IF NOT EXISTS agent_chain_continuations (
             chain_id TEXT PRIMARY KEY,
             reason TEXT NOT NULL,
             granted_hops INTEGER NOT NULL,
             granted_at INTEGER NOT NULL
         );

         CREATE TABLE IF NOT EXISTS agent_schedules (
             id TEXT PRIMARY KEY,
             agent_profile_id TEXT NOT NULL,
             workspace_root TEXT NOT NULL,
             prompt TEXT NOT NULL,
             mode TEXT NOT NULL DEFAULT 'interval',
             delivery_mode TEXT NOT NULL DEFAULT 'fresh_session',
             target_session_id TEXT,
             interval_seconds INTEGER NOT NULL,
             next_fire_at INTEGER NOT NULL,
             enabled INTEGER NOT NULL DEFAULT 1,
             last_triggered_at INTEGER,
             last_finished_at INTEGER,
             last_session_id TEXT,
             last_job_id TEXT,
             last_result TEXT,
             last_error TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );

         CREATE TABLE IF NOT EXISTS context_summaries (
             session_id TEXT PRIMARY KEY,
             summary_text TEXT NOT NULL,
             covered_message_count INTEGER NOT NULL,
             summary_token_estimate INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
         );

         CREATE TABLE IF NOT EXISTS context_offloads (
             session_id TEXT PRIMARY KEY,
             refs_json TEXT NOT NULL,
             updated_at INTEGER NOT NULL,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
         );

         CREATE TABLE IF NOT EXISTS session_retention (
             session_id TEXT PRIMARY KEY,
             tier TEXT NOT NULL,
             last_accessed_at INTEGER NOT NULL,
             archived_at INTEGER,
             archive_manifest_path TEXT,
             archive_version INTEGER,
             updated_at INTEGER NOT NULL,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
         );

         CREATE TABLE IF NOT EXISTS session_search_docs (
             doc_id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             source_kind TEXT NOT NULL,
             source_ref TEXT NOT NULL,
             body TEXT NOT NULL,
             updated_at INTEGER NOT NULL,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
         );

         CREATE VIRTUAL TABLE IF NOT EXISTS session_search_fts USING fts5(
             doc_id UNINDEXED,
             session_id UNINDEXED,
             source_kind,
             source_ref,
             body
         );

         CREATE TABLE IF NOT EXISTS knowledge_sources (
             source_id TEXT PRIMARY KEY,
             path TEXT NOT NULL UNIQUE,
             kind TEXT NOT NULL,
             sha256 TEXT NOT NULL,
             byte_len INTEGER NOT NULL,
             mtime INTEGER NOT NULL,
             indexed_at INTEGER NOT NULL
         );

         CREATE TABLE IF NOT EXISTS mcp_connectors (
             id TEXT PRIMARY KEY,
             transport TEXT NOT NULL,
             command TEXT NOT NULL,
             args_json TEXT NOT NULL,
             env_json TEXT NOT NULL,
             cwd TEXT,
             enabled INTEGER NOT NULL DEFAULT 1,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );

         CREATE TABLE IF NOT EXISTS telegram_user_pairings (
             token TEXT PRIMARY KEY,
             telegram_user_id INTEGER NOT NULL UNIQUE,
             telegram_chat_id INTEGER NOT NULL,
             telegram_username TEXT,
             telegram_display_name TEXT NOT NULL,
             status TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             expires_at INTEGER NOT NULL,
             activated_at INTEGER
         );

         CREATE TABLE IF NOT EXISTS telegram_chat_bindings (
             telegram_chat_id INTEGER PRIMARY KEY,
             scope TEXT NOT NULL,
             owner_telegram_user_id INTEGER,
             selected_session_id TEXT,
             last_delivered_transcript_created_at INTEGER,
             last_delivered_transcript_id TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );

         CREATE TABLE IF NOT EXISTS telegram_update_cursors (
             consumer TEXT PRIMARY KEY,
             update_id INTEGER NOT NULL,
             updated_at INTEGER NOT NULL
         );

         CREATE TABLE IF NOT EXISTS knowledge_search_docs (
             doc_id TEXT PRIMARY KEY,
             source_id TEXT NOT NULL,
             path TEXT NOT NULL,
             kind TEXT NOT NULL,
             body TEXT NOT NULL,
             updated_at INTEGER NOT NULL,
             FOREIGN KEY(source_id) REFERENCES knowledge_sources(source_id) ON DELETE CASCADE
         );

         CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_search_fts USING fts5(
             doc_id UNINDEXED,
             path,
             kind,
             body
         );

         CREATE TABLE IF NOT EXISTS plans (
             session_id TEXT PRIMARY KEY,
             items_json TEXT NOT NULL,
             updated_at INTEGER NOT NULL,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
         );

         CREATE TABLE IF NOT EXISTS artifacts (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             kind TEXT NOT NULL,
             path TEXT NOT NULL,
             metadata_json TEXT NOT NULL,
             byte_len INTEGER NOT NULL,
             sha256 TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
         );",
    )?;

    migrate_schema(connection)?;

    connection.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_missions_session_id ON missions(session_id);
         CREATE INDEX IF NOT EXISTS idx_runs_session_id ON runs(session_id);
         CREATE INDEX IF NOT EXISTS idx_runs_mission_id ON runs(mission_id);
         CREATE INDEX IF NOT EXISTS idx_jobs_session_id ON jobs(session_id);
         CREATE INDEX IF NOT EXISTS idx_jobs_mission_id ON jobs(mission_id);
         CREATE INDEX IF NOT EXISTS idx_jobs_run_id ON jobs(run_id);
         CREATE INDEX IF NOT EXISTS idx_jobs_parent_job_id ON jobs(parent_job_id);
         CREATE INDEX IF NOT EXISTS idx_transcripts_session_id ON transcripts(session_id);
         CREATE INDEX IF NOT EXISTS idx_transcripts_run_id ON transcripts(run_id);
         CREATE INDEX IF NOT EXISTS idx_tool_calls_session_id ON tool_calls(session_id);
         CREATE INDEX IF NOT EXISTS idx_tool_calls_run_id ON tool_calls(run_id);
         CREATE INDEX IF NOT EXISTS idx_tool_calls_status ON tool_calls(status);
         CREATE INDEX IF NOT EXISTS idx_session_inbox_events_session_id ON session_inbox_events(session_id);
         CREATE INDEX IF NOT EXISTS idx_session_inbox_events_status_available_at ON session_inbox_events(status, available_at);
         CREATE INDEX IF NOT EXISTS idx_agent_profiles_updated_at ON agent_profiles(updated_at);
         CREATE INDEX IF NOT EXISTS idx_agent_chain_continuations_granted_at ON agent_chain_continuations(granted_at);
         CREATE INDEX IF NOT EXISTS idx_agent_schedules_next_fire_at ON agent_schedules(next_fire_at);
         CREATE INDEX IF NOT EXISTS idx_context_summaries_updated_at ON context_summaries(updated_at);
         CREATE INDEX IF NOT EXISTS idx_context_offloads_updated_at ON context_offloads(updated_at);
         CREATE INDEX IF NOT EXISTS idx_session_retention_tier_updated_at ON session_retention(tier, updated_at);
         CREATE INDEX IF NOT EXISTS idx_session_search_docs_session_id ON session_search_docs(session_id);
         CREATE INDEX IF NOT EXISTS idx_knowledge_sources_kind_mtime ON knowledge_sources(kind, mtime DESC);
         CREATE INDEX IF NOT EXISTS idx_mcp_connectors_enabled_updated_at ON mcp_connectors(enabled, updated_at DESC);
         CREATE INDEX IF NOT EXISTS idx_telegram_user_pairings_status_expires_at ON telegram_user_pairings(status, expires_at);
         CREATE INDEX IF NOT EXISTS idx_telegram_chat_bindings_scope_updated_at ON telegram_chat_bindings(scope, updated_at DESC);
         CREATE INDEX IF NOT EXISTS idx_knowledge_search_docs_source_id ON knowledge_search_docs(source_id);
         CREATE INDEX IF NOT EXISTS idx_artifacts_session_id ON artifacts(session_id);",
    )?;

    Ok(())
}

pub(super) fn validate_schema(connection: &Connection) -> Result<(), StoreError> {
    validate_column(connection, "missions", "execution_intent", true)?;
    validate_column(connection, "missions", "schedule_json", true)?;
    validate_column(connection, "missions", "acceptance_json", true)?;
    validate_column(connection, "jobs", "session_id", true)?;
    validate_column(connection, "jobs", "mission_id", false)?;
    validate_column(connection, "jobs", "attempt_count", true)?;
    validate_column(connection, "jobs", "max_attempts", true)?;
    validate_column(connection, "jobs", "lease_owner", false)?;
    validate_column(connection, "jobs", "lease_expires_at", false)?;
    validate_column(connection, "jobs", "heartbeat_at", false)?;
    validate_column(connection, "jobs", "cancel_requested_at", false)?;
    validate_column(connection, "jobs", "last_progress_message", false)?;
    validate_column(connection, "jobs", "callback_json", false)?;
    validate_column(connection, "jobs", "callback_sent_at", false)?;
    validate_column(connection, "sessions", "settings_json", true)?;
    validate_column(connection, "sessions", "agent_profile_id", true)?;
    validate_column(connection, "sessions", "parent_session_id", false)?;
    validate_column(connection, "sessions", "parent_job_id", false)?;
    validate_column(connection, "sessions", "delegation_label", false)?;
    validate_column(connection, "agent_profiles", "id", true)?;
    validate_column(connection, "agent_profiles", "name", true)?;
    validate_column(connection, "agent_profiles", "template_kind", true)?;
    validate_column(connection, "agent_profiles", "agent_home", true)?;
    validate_column(connection, "agent_profiles", "allowed_tools_json", true)?;
    validate_column(
        connection,
        "agent_profiles",
        "created_from_template_id",
        false,
    )?;
    validate_column(connection, "agent_profiles", "created_by_session_id", false)?;
    validate_column(
        connection,
        "agent_profiles",
        "created_by_agent_profile_id",
        false,
    )?;
    validate_column(connection, "agent_profiles", "created_at", true)?;
    validate_column(connection, "agent_profiles", "updated_at", true)?;
    validate_column(connection, "daemon_state", "key", true)?;
    validate_column(connection, "daemon_state", "value", false)?;
    validate_column(connection, "agent_chain_continuations", "chain_id", true)?;
    validate_column(connection, "agent_chain_continuations", "reason", true)?;
    validate_column(
        connection,
        "agent_chain_continuations",
        "granted_hops",
        true,
    )?;
    validate_column(connection, "agent_chain_continuations", "granted_at", true)?;
    validate_column(connection, "agent_schedules", "id", true)?;
    validate_column(connection, "agent_schedules", "agent_profile_id", true)?;
    validate_column(connection, "agent_schedules", "workspace_root", true)?;
    validate_column(connection, "agent_schedules", "prompt", true)?;
    validate_column(connection, "agent_schedules", "mode", true)?;
    validate_column(connection, "agent_schedules", "delivery_mode", true)?;
    validate_column(connection, "agent_schedules", "target_session_id", false)?;
    validate_column(connection, "agent_schedules", "interval_seconds", true)?;
    validate_column(connection, "agent_schedules", "next_fire_at", true)?;
    validate_column(connection, "agent_schedules", "enabled", true)?;
    validate_column(connection, "agent_schedules", "last_triggered_at", false)?;
    validate_column(connection, "agent_schedules", "last_finished_at", false)?;
    validate_column(connection, "agent_schedules", "last_session_id", false)?;
    validate_column(connection, "agent_schedules", "last_job_id", false)?;
    validate_column(connection, "agent_schedules", "last_result", false)?;
    validate_column(connection, "agent_schedules", "last_error", false)?;
    validate_column(connection, "agent_schedules", "created_at", true)?;
    validate_column(connection, "agent_schedules", "updated_at", true)?;
    validate_column(connection, "runs", "evidence_refs_json", true)?;
    validate_column(connection, "runs", "recent_steps_json", true)?;
    validate_column(connection, "runs", "provider_usage_json", true)?;
    validate_column(connection, "runs", "active_processes_json", true)?;
    validate_column(connection, "runs", "pending_approvals_json", true)?;
    validate_column(connection, "runs", "provider_loop_json", true)?;
    validate_column(connection, "runs", "delegate_runs_json", true)?;
    validate_column(connection, "runs", "result", false)?;
    validate_column(connection, "transcripts", "sha256", true)?;
    validate_column(connection, "tool_calls", "id", true)?;
    validate_column(connection, "tool_calls", "session_id", true)?;
    validate_column(connection, "tool_calls", "run_id", true)?;
    validate_column(connection, "tool_calls", "provider_tool_call_id", true)?;
    validate_column(connection, "tool_calls", "tool_name", true)?;
    validate_column(connection, "tool_calls", "arguments_json", true)?;
    validate_column(connection, "tool_calls", "summary", true)?;
    validate_column(connection, "tool_calls", "status", true)?;
    validate_column(connection, "tool_calls", "error", false)?;
    validate_column(connection, "tool_calls", "result_summary", false)?;
    validate_column(connection, "tool_calls", "result_preview", false)?;
    validate_column(connection, "tool_calls", "result_artifact_id", false)?;
    validate_column(connection, "tool_calls", "result_truncated", true)?;
    validate_column(connection, "tool_calls", "result_byte_len", false)?;
    validate_column(connection, "tool_calls", "requested_at", true)?;
    validate_column(connection, "tool_calls", "updated_at", true)?;
    validate_column(connection, "session_inbox_events", "session_id", true)?;
    validate_column(connection, "session_inbox_events", "kind", true)?;
    validate_column(connection, "session_inbox_events", "payload_json", true)?;
    validate_column(connection, "session_inbox_events", "status", true)?;
    validate_column(connection, "context_summaries", "summary_text", true)?;
    validate_column(
        connection,
        "context_summaries",
        "covered_message_count",
        true,
    )?;
    validate_column(
        connection,
        "context_summaries",
        "summary_token_estimate",
        true,
    )?;
    validate_column(connection, "context_offloads", "session_id", true)?;
    validate_column(connection, "context_offloads", "refs_json", true)?;
    validate_column(connection, "context_offloads", "updated_at", true)?;
    validate_foreign_key(
        connection,
        "context_offloads",
        "session_id",
        "sessions",
        "CASCADE",
    )?;
    validate_column(connection, "session_retention", "session_id", true)?;
    validate_column(connection, "session_retention", "tier", true)?;
    validate_column(connection, "session_retention", "last_accessed_at", true)?;
    validate_column(connection, "session_retention", "archived_at", false)?;
    validate_column(
        connection,
        "session_retention",
        "archive_manifest_path",
        false,
    )?;
    validate_column(connection, "session_retention", "archive_version", false)?;
    validate_column(connection, "session_retention", "updated_at", true)?;
    validate_foreign_key(
        connection,
        "session_retention",
        "session_id",
        "sessions",
        "CASCADE",
    )?;
    validate_column(connection, "session_search_docs", "doc_id", true)?;
    validate_column(connection, "session_search_docs", "session_id", true)?;
    validate_column(connection, "session_search_docs", "source_kind", true)?;
    validate_column(connection, "session_search_docs", "source_ref", true)?;
    validate_column(connection, "session_search_docs", "body", true)?;
    validate_column(connection, "session_search_docs", "updated_at", true)?;
    validate_foreign_key(
        connection,
        "session_search_docs",
        "session_id",
        "sessions",
        "CASCADE",
    )?;
    validate_column(connection, "knowledge_sources", "source_id", true)?;
    validate_column(connection, "knowledge_sources", "path", true)?;
    validate_column(connection, "knowledge_sources", "kind", true)?;
    validate_column(connection, "knowledge_sources", "sha256", true)?;
    validate_column(connection, "knowledge_sources", "byte_len", true)?;
    validate_column(connection, "knowledge_sources", "mtime", true)?;
    validate_column(connection, "knowledge_sources", "indexed_at", true)?;
    validate_column(connection, "mcp_connectors", "id", true)?;
    validate_column(connection, "mcp_connectors", "transport", true)?;
    validate_column(connection, "mcp_connectors", "command", true)?;
    validate_column(connection, "mcp_connectors", "args_json", true)?;
    validate_column(connection, "mcp_connectors", "env_json", true)?;
    validate_column(connection, "mcp_connectors", "cwd", false)?;
    validate_column(connection, "mcp_connectors", "enabled", true)?;
    validate_column(connection, "mcp_connectors", "created_at", true)?;
    validate_column(connection, "mcp_connectors", "updated_at", true)?;
    validate_column(connection, "telegram_user_pairings", "token", true)?;
    validate_column(
        connection,
        "telegram_user_pairings",
        "telegram_user_id",
        true,
    )?;
    validate_column(
        connection,
        "telegram_user_pairings",
        "telegram_chat_id",
        true,
    )?;
    validate_column(
        connection,
        "telegram_user_pairings",
        "telegram_username",
        false,
    )?;
    validate_column(
        connection,
        "telegram_user_pairings",
        "telegram_display_name",
        true,
    )?;
    validate_column(connection, "telegram_user_pairings", "status", true)?;
    validate_column(connection, "telegram_user_pairings", "created_at", true)?;
    validate_column(connection, "telegram_user_pairings", "expires_at", true)?;
    validate_column(connection, "telegram_user_pairings", "activated_at", false)?;
    validate_column(
        connection,
        "telegram_chat_bindings",
        "telegram_chat_id",
        true,
    )?;
    validate_column(connection, "telegram_chat_bindings", "scope", true)?;
    validate_column(
        connection,
        "telegram_chat_bindings",
        "owner_telegram_user_id",
        false,
    )?;
    validate_column(
        connection,
        "telegram_chat_bindings",
        "selected_session_id",
        false,
    )?;
    validate_column(
        connection,
        "telegram_chat_bindings",
        "last_delivered_transcript_created_at",
        false,
    )?;
    validate_column(
        connection,
        "telegram_chat_bindings",
        "last_delivered_transcript_id",
        false,
    )?;
    validate_column(connection, "telegram_chat_bindings", "created_at", true)?;
    validate_column(connection, "telegram_chat_bindings", "updated_at", true)?;
    validate_column(connection, "telegram_update_cursors", "consumer", true)?;
    validate_column(connection, "telegram_update_cursors", "update_id", true)?;
    validate_column(connection, "telegram_update_cursors", "updated_at", true)?;
    validate_column(connection, "knowledge_search_docs", "doc_id", true)?;
    validate_column(connection, "knowledge_search_docs", "source_id", true)?;
    validate_column(connection, "knowledge_search_docs", "path", true)?;
    validate_column(connection, "knowledge_search_docs", "kind", true)?;
    validate_column(connection, "knowledge_search_docs", "body", true)?;
    validate_column(connection, "knowledge_search_docs", "updated_at", true)?;
    validate_foreign_key(
        connection,
        "knowledge_search_docs",
        "source_id",
        "knowledge_sources",
        "CASCADE",
    )?;
    validate_column(connection, "plans", "session_id", true)?;
    validate_column(connection, "plans", "items_json", true)?;
    validate_column(connection, "plans", "updated_at", true)?;
    validate_foreign_key(connection, "plans", "session_id", "sessions", "CASCADE")?;
    validate_column(connection, "artifacts", "session_id", true)?;
    validate_column(connection, "artifacts", "metadata_json", true)?;
    validate_column(connection, "artifacts", "sha256", true)?;
    validate_foreign_key(
        connection,
        "context_summaries",
        "session_id",
        "sessions",
        "CASCADE",
    )?;
    validate_foreign_key(
        connection,
        "session_inbox_events",
        "session_id",
        "sessions",
        "CASCADE",
    )?;
    validate_foreign_key(
        connection,
        "session_inbox_events",
        "job_id",
        "jobs",
        "SET NULL",
    )?;
    validate_foreign_key(connection, "artifacts", "session_id", "sessions", "CASCADE")?;
    validate_foreign_key(
        connection,
        "tool_calls",
        "session_id",
        "sessions",
        "CASCADE",
    )?;
    validate_foreign_key(connection, "tool_calls", "run_id", "runs", "CASCADE")?;
    validate_foreign_key(connection, "jobs", "session_id", "sessions", "CASCADE")?;
    validate_foreign_key(connection, "jobs", "mission_id", "missions", "SET NULL")?;
    validate_foreign_key(
        connection,
        "sessions",
        "active_mission_id",
        "missions",
        "SET NULL",
    )?;
    Ok(())
}

pub(super) fn validate_identifier(id: &str) -> Result<(), StoreError> {
    if id.is_empty() {
        return Err(StoreError::InvalidIdentifier {
            id: id.to_string(),
            reason: "must not be empty",
        });
    }

    if id == "." || id == ".." || id.contains('/') || id.contains('\\') {
        return Err(StoreError::InvalidIdentifier {
            id: id.to_string(),
            reason: "must not contain path traversal or separators",
        });
    }

    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(StoreError::InvalidIdentifier {
            id: id.to_string(),
            reason: "must use only ascii letters, digits, hyphen, or underscore",
        });
    }

    Ok(())
}

pub(super) fn migrate_schema(connection: &Connection) -> Result<(), StoreError> {
    add_column_if_missing(
        connection,
        "sessions",
        "agent_profile_id",
        "TEXT NOT NULL DEFAULT 'default'",
    )?;
    add_column_if_missing(connection, "sessions", "parent_session_id", "TEXT")?;
    add_column_if_missing(connection, "sessions", "parent_job_id", "TEXT")?;
    add_column_if_missing(connection, "sessions", "delegation_label", "TEXT")?;
    add_column_if_missing(
        connection,
        "agent_profiles",
        "created_from_template_id",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "agent_profiles",
        "created_by_session_id",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "agent_profiles",
        "created_by_agent_profile_id",
        "TEXT",
    )?;
    add_column_if_missing(
        connection,
        "agent_schedules",
        "mode",
        "TEXT NOT NULL DEFAULT 'interval'",
    )?;
    add_column_if_missing(
        connection,
        "agent_schedules",
        "delivery_mode",
        "TEXT NOT NULL DEFAULT 'fresh_session'",
    )?;
    add_column_if_missing(connection, "agent_schedules", "target_session_id", "TEXT")?;
    add_column_if_missing(
        connection,
        "agent_schedules",
        "enabled",
        "INTEGER NOT NULL DEFAULT 1",
    )?;
    add_column_if_missing(connection, "agent_schedules", "last_finished_at", "INTEGER")?;
    add_column_if_missing(connection, "agent_schedules", "last_result", "TEXT")?;
    add_column_if_missing(connection, "agent_schedules", "last_error", "TEXT")?;
    add_column_if_missing(
        connection,
        "missions",
        "execution_intent",
        "TEXT NOT NULL DEFAULT 'autonomous'",
    )?;
    add_column_if_missing(
        connection,
        "missions",
        "schedule_json",
        "TEXT NOT NULL DEFAULT '{\"not_before\":null,\"interval_seconds\":null}'",
    )?;
    add_column_if_missing(
        connection,
        "runs",
        "provider_usage_json",
        "TEXT NOT NULL DEFAULT 'null'",
    )?;
    add_column_if_missing(
        connection,
        "runs",
        "active_processes_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(
        connection,
        "runs",
        "recent_steps_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(
        connection,
        "runs",
        "evidence_refs_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(
        connection,
        "runs",
        "pending_approvals_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(
        connection,
        "runs",
        "provider_loop_json",
        "TEXT NOT NULL DEFAULT 'null'",
    )?;
    add_column_if_missing(
        connection,
        "runs",
        "delegate_runs_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(connection, "jobs", "callback_json", "TEXT")?;
    add_column_if_missing(connection, "jobs", "callback_sent_at", "INTEGER")?;
    add_column_if_missing(
        connection,
        "telegram_chat_bindings",
        "last_delivered_transcript_created_at",
        "INTEGER",
    )?;
    add_column_if_missing(
        connection,
        "telegram_chat_bindings",
        "last_delivered_transcript_id",
        "TEXT",
    )?;
    add_column_if_missing(connection, "tool_calls", "result_summary", "TEXT")?;
    add_column_if_missing(connection, "tool_calls", "result_preview", "TEXT")?;
    add_column_if_missing(connection, "tool_calls", "result_artifact_id", "TEXT")?;
    add_column_if_missing(
        connection,
        "tool_calls",
        "result_truncated",
        "INTEGER NOT NULL DEFAULT 0",
    )?;
    add_column_if_missing(connection, "tool_calls", "result_byte_len", "INTEGER")?;
    add_column_if_missing(
        connection,
        "missions",
        "acceptance_json",
        "TEXT NOT NULL DEFAULT '[]'",
    )?;
    migrate_jobs_table(connection)?;
    migrate_session_inbox_events_table(connection)?;
    Ok(())
}

pub(super) fn add_column_if_missing(
    connection: &Connection,
    table: &'static str,
    column: &'static str,
    definition: &'static str,
) -> Result<(), StoreError> {
    if table_has_column(connection, table, column)? {
        return Ok(());
    }

    connection.execute_batch(&format!(
        "ALTER TABLE {table} ADD COLUMN {column} {definition};"
    ))?;
    Ok(())
}

pub(super) fn migrate_jobs_table(connection: &Connection) -> Result<(), StoreError> {
    if !table_exists(connection, "jobs")? {
        return Ok(());
    }

    let legacy_has_mission_id = table_has_column(connection, "jobs", "mission_id")?;
    if table_has_column(connection, "jobs", "session_id")?
        && table_has_column(connection, "jobs", "mission_id")?
        && table_has_column(connection, "jobs", "attempt_count")?
        && table_has_column(connection, "jobs", "max_attempts")?
        && table_has_column(connection, "jobs", "lease_owner")?
        && table_has_column(connection, "jobs", "lease_expires_at")?
        && table_has_column(connection, "jobs", "heartbeat_at")?
        && table_has_column(connection, "jobs", "cancel_requested_at")?
        && table_has_column(connection, "jobs", "last_progress_message")?
        && foreign_key_exists(connection, "jobs", "session_id", "sessions", "CASCADE")?
        && foreign_key_exists(connection, "jobs", "mission_id", "missions", "SET NULL")?
        && foreign_key_exists(connection, "jobs", "run_id", "runs", "SET NULL")?
    {
        return Ok(());
    }

    let mission_id_expr = if legacy_has_mission_id {
        format!(
            "COALESCE(jobs_legacy.mission_id, runs.mission_id, '{LEGACY_MISSION_PREFIX}' || runs.id)"
        )
    } else {
        format!("COALESCE(runs.mission_id, '{LEGACY_MISSION_PREFIX}' || runs.id)")
    };

    connection.execute_batch(&format!(
        "PRAGMA foreign_keys = OFF;
         BEGIN IMMEDIATE;
         ALTER TABLE jobs RENAME TO jobs_legacy;
         INSERT OR IGNORE INTO missions (
             id, session_id, objective, status, execution_intent, schedule_json, acceptance_json,
             created_at, updated_at, completed_at
         )
         SELECT DISTINCT
             '{LEGACY_MISSION_PREFIX}' || runs.id,
             runs.session_id,
             'Recovered legacy mission for run ' || runs.id,
             CASE
                 WHEN runs.finished_at IS NULL THEN 'ready'
                 ELSE 'completed'
             END,
             '{DEFAULT_MISSION_EXECUTION_INTENT}',
             '{DEFAULT_MISSION_SCHEDULE_JSON}',
             '{DEFAULT_MISSION_ACCEPTANCE_JSON}',
             runs.started_at,
             runs.updated_at,
             runs.finished_at
         FROM jobs_legacy
         INNER JOIN runs ON runs.id = jobs_legacy.run_id
         WHERE runs.mission_id IS NULL;
         UPDATE runs
         SET mission_id = '{LEGACY_MISSION_PREFIX}' || id
         WHERE mission_id IS NULL
           AND EXISTS (
               SELECT 1
               FROM jobs_legacy
               WHERE jobs_legacy.run_id = runs.id
           );
         CREATE TABLE jobs (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             mission_id TEXT,
             run_id TEXT,
             parent_job_id TEXT,
             kind TEXT NOT NULL,
             status TEXT NOT NULL,
             input_json TEXT,
             result_json TEXT,
             error TEXT,
             created_at INTEGER NOT NULL,
             updated_at INTEGER NOT NULL,
             started_at INTEGER,
             finished_at INTEGER,
             attempt_count INTEGER NOT NULL DEFAULT 0,
             max_attempts INTEGER NOT NULL DEFAULT 1,
             lease_owner TEXT,
             lease_expires_at INTEGER,
             heartbeat_at INTEGER,
             cancel_requested_at INTEGER,
             last_progress_message TEXT,
             callback_json TEXT,
             callback_sent_at INTEGER,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
             FOREIGN KEY(mission_id) REFERENCES missions(id) ON DELETE SET NULL,
             FOREIGN KEY(run_id) REFERENCES runs(id) ON DELETE SET NULL,
             FOREIGN KEY(parent_job_id) REFERENCES jobs(id) ON DELETE SET NULL
         );
         INSERT INTO jobs (
             id, session_id, mission_id, run_id, parent_job_id, kind, status, input_json,
             result_json, error, created_at, updated_at, started_at, finished_at, attempt_count,
             max_attempts, lease_owner, lease_expires_at, heartbeat_at, cancel_requested_at,
             last_progress_message, callback_json, callback_sent_at
         )
         SELECT
             jobs_legacy.id,
             COALESCE(missions.session_id, runs.session_id),
             {mission_id_expr},
             jobs_legacy.run_id,
             jobs_legacy.parent_job_id,
             jobs_legacy.kind,
             jobs_legacy.status,
             jobs_legacy.input_json,
             jobs_legacy.result_json,
             jobs_legacy.error,
             jobs_legacy.created_at,
             jobs_legacy.updated_at,
             jobs_legacy.started_at,
             jobs_legacy.finished_at,
             0,
             1,
             NULL,
             NULL,
             NULL,
             NULL,
             NULL,
             NULL,
             NULL
         FROM jobs_legacy
         INNER JOIN runs ON runs.id = jobs_legacy.run_id
         LEFT JOIN missions ON missions.id = {mission_id_expr};
         DROP TABLE jobs_legacy;
         COMMIT;
         PRAGMA foreign_keys = ON;"
    ))?;

    Ok(())
}

pub(super) fn migrate_session_inbox_events_table(
    connection: &Connection,
) -> Result<(), StoreError> {
    if !table_exists(connection, "session_inbox_events")? {
        return Ok(());
    }

    if table_has_column(connection, "session_inbox_events", "session_id")?
        && table_has_column(connection, "session_inbox_events", "job_id")?
        && table_has_column(connection, "session_inbox_events", "kind")?
        && table_has_column(connection, "session_inbox_events", "payload_json")?
        && table_has_column(connection, "session_inbox_events", "status")?
        && table_has_column(connection, "session_inbox_events", "created_at")?
        && table_has_column(connection, "session_inbox_events", "available_at")?
        && table_has_column(connection, "session_inbox_events", "claimed_at")?
        && table_has_column(connection, "session_inbox_events", "processed_at")?
        && table_has_column(connection, "session_inbox_events", "error")?
        && foreign_key_exists(
            connection,
            "session_inbox_events",
            "session_id",
            "sessions",
            "CASCADE",
        )?
        && foreign_key_exists(
            connection,
            "session_inbox_events",
            "job_id",
            "jobs",
            "SET NULL",
        )?
    {
        return Ok(());
    }

    connection.execute_batch(
        "PRAGMA foreign_keys = OFF;
         BEGIN IMMEDIATE;
         ALTER TABLE session_inbox_events RENAME TO session_inbox_events_legacy;
         CREATE TABLE session_inbox_events (
             id TEXT PRIMARY KEY,
             session_id TEXT NOT NULL,
             job_id TEXT,
             kind TEXT NOT NULL,
             payload_json TEXT NOT NULL,
             status TEXT NOT NULL,
             created_at INTEGER NOT NULL,
             available_at INTEGER NOT NULL,
             claimed_at INTEGER,
             processed_at INTEGER,
             error TEXT,
             FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE,
             FOREIGN KEY(job_id) REFERENCES jobs(id) ON DELETE SET NULL
         );
         INSERT INTO session_inbox_events (
             id, session_id, job_id, kind, payload_json, status, created_at, available_at,
             claimed_at, processed_at, error
         )
         SELECT
             id, session_id, job_id, kind, payload_json, status, created_at, available_at,
             claimed_at, processed_at, error
         FROM session_inbox_events_legacy;
         DROP TABLE session_inbox_events_legacy;
         COMMIT;
         PRAGMA foreign_keys = ON;",
    )?;

    Ok(())
}

pub(super) fn validate_column(
    connection: &Connection,
    table: &'static str,
    column: &'static str,
    required_not_null: bool,
) -> Result<(), StoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;

    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        let not_null: i64 = row.get(3)?;
        let primary_key_position: i64 = row.get(5)?;

        if name == column {
            if required_not_null && not_null != 1 && primary_key_position == 0 {
                return Err(StoreError::SchemaMismatch {
                    table,
                    reason: format!("{column} must be NOT NULL"),
                });
            }
            return Ok(());
        }
    }

    Err(StoreError::SchemaMismatch {
        table,
        reason: format!("missing required column {column}"),
    })
}

pub(super) fn table_exists(
    connection: &Connection,
    table: &'static str,
) -> Result<bool, StoreError> {
    connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |_row| Ok(()),
        )
        .optional()
        .map(|row| row.is_some())
        .map_err(StoreError::Sqlite)
}

pub(super) fn table_has_column(
    connection: &Connection,
    table: &'static str,
    column: &'static str,
) -> Result<bool, StoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let mut rows = statement.query([])?;

    while let Some(row) = rows.next()? {
        let name: String = row.get(1)?;
        if name == column {
            return Ok(true);
        }
    }

    Ok(false)
}

pub(super) fn foreign_key_exists(
    connection: &Connection,
    table: &'static str,
    from_column: &'static str,
    target_table: &'static str,
    on_delete: &'static str,
) -> Result<bool, StoreError> {
    let mut statement = connection.prepare(&format!("PRAGMA foreign_key_list({table})"))?;
    let mut rows = statement.query([])?;

    while let Some(row) = rows.next()? {
        let fk_table: String = row.get(2)?;
        let fk_from: String = row.get(3)?;
        let fk_on_delete: String = row.get(6)?;

        if fk_table == target_table && fk_from == from_column && fk_on_delete == on_delete {
            return Ok(true);
        }
    }

    Ok(false)
}

pub(super) fn validate_foreign_key(
    connection: &Connection,
    table: &'static str,
    from_column: &'static str,
    target_table: &'static str,
    on_delete: &'static str,
) -> Result<(), StoreError> {
    if foreign_key_exists(connection, table, from_column, target_table, on_delete)? {
        return Ok(());
    }

    Err(StoreError::SchemaMismatch {
        table,
        reason: format!(
            "missing foreign key for {from_column} -> {target_table} with ON DELETE {on_delete}"
        ),
    })
}
