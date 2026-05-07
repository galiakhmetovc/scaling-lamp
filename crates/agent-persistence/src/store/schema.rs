use super::*;

pub(super) fn bootstrap_schema(client: &mut Client) -> Result<(), StoreError> {
    client.batch_execute(
        "
        CREATE TABLE IF NOT EXISTS sessions (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            prompt_override TEXT,
            settings_json TEXT NOT NULL,
            workspace_root TEXT NOT NULL DEFAULT '.',
            agent_profile_id TEXT NOT NULL DEFAULT 'default',
            active_mission_id TEXT,
            parent_session_id TEXT,
            parent_job_id TEXT,
            delegation_label TEXT,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS missions (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            objective TEXT NOT NULL,
            status TEXT NOT NULL,
            execution_intent TEXT NOT NULL,
            schedule_json TEXT NOT NULL,
            acceptance_json TEXT NOT NULL,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL,
            completed_at BIGINT
        );

        ALTER TABLE sessions
            DROP CONSTRAINT IF EXISTS sessions_active_mission_id_fkey;
        ALTER TABLE sessions
            ADD CONSTRAINT sessions_active_mission_id_fkey
            FOREIGN KEY(active_mission_id) REFERENCES missions(id) ON DELETE SET NULL;

        CREATE TABLE IF NOT EXISTS runs (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            mission_id TEXT REFERENCES missions(id) ON DELETE SET NULL,
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
            started_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL,
            finished_at BIGINT
        );

        CREATE TABLE IF NOT EXISTS jobs (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            mission_id TEXT REFERENCES missions(id) ON DELETE SET NULL,
            run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
            parent_job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
            kind TEXT NOT NULL,
            status TEXT NOT NULL,
            input_json TEXT,
            result_json TEXT,
            error TEXT,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL,
            started_at BIGINT,
            finished_at BIGINT,
            attempt_count BIGINT NOT NULL DEFAULT 0,
            max_attempts BIGINT NOT NULL DEFAULT 1,
            lease_owner TEXT,
            lease_expires_at BIGINT,
            heartbeat_at BIGINT,
            cancel_requested_at BIGINT,
            last_progress_message TEXT,
            callback_json TEXT,
            callback_sent_at BIGINT
        );

        CREATE TABLE IF NOT EXISTS transcripts (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
            kind TEXT NOT NULL,
            storage_key TEXT NOT NULL,
            byte_len BIGINT NOT NULL,
            sha256 TEXT NOT NULL,
            created_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tool_calls (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
            provider_tool_call_id TEXT NOT NULL,
            tool_name TEXT NOT NULL,
            arguments_json TEXT NOT NULL,
            summary TEXT NOT NULL,
            status TEXT NOT NULL,
            error TEXT,
            result_summary TEXT,
            result_preview TEXT,
            result_artifact_id TEXT,
            result_truncated BOOLEAN NOT NULL DEFAULT FALSE,
            result_byte_len BIGINT,
            requested_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS trace_links (
            entity_kind TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            trace_id TEXT NOT NULL,
            span_id TEXT NOT NULL,
            parent_span_id TEXT,
            surface TEXT,
            entrypoint TEXT,
            attributes_json TEXT NOT NULL,
            created_at BIGINT NOT NULL,
            PRIMARY KEY(entity_kind, entity_id)
        );

        CREATE TABLE IF NOT EXISTS session_inbox_events (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
            kind TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at BIGINT NOT NULL,
            available_at BIGINT NOT NULL,
            claimed_at BIGINT,
            processed_at BIGINT,
            error TEXT
        );

        CREATE TABLE IF NOT EXISTS agent_profiles (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            template_kind TEXT NOT NULL,
            agent_home TEXT NOT NULL,
            allowed_tools_json TEXT NOT NULL,
            default_workspace_root TEXT,
            created_from_template_id TEXT,
            created_by_session_id TEXT,
            created_by_agent_profile_id TEXT,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS daemon_state (
            key TEXT PRIMARY KEY,
            value TEXT
        );

        CREATE TABLE IF NOT EXISTS kv_entries (
            scope TEXT NOT NULL,
            namespace_id TEXT NOT NULL,
            key TEXT NOT NULL,
            value_json TEXT NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT 'null',
            revision BIGINT NOT NULL,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL,
            expires_at BIGINT,
            PRIMARY KEY(scope, namespace_id, key)
        );

        CREATE TABLE IF NOT EXISTS agent_chain_continuations (
            chain_id TEXT PRIMARY KEY,
            reason TEXT NOT NULL,
            granted_hops BIGINT NOT NULL,
            granted_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS agent_schedules (
            id TEXT PRIMARY KEY,
            agent_profile_id TEXT NOT NULL,
            workspace_root TEXT NOT NULL,
            prompt TEXT NOT NULL,
            mode TEXT NOT NULL DEFAULT 'interval',
            delivery_mode TEXT NOT NULL DEFAULT 'fresh_session',
            target_session_id TEXT,
            interval_seconds BIGINT NOT NULL,
            next_fire_at BIGINT NOT NULL,
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            last_triggered_at BIGINT,
            last_finished_at BIGINT,
            last_session_id TEXT,
            last_job_id TEXT,
            last_result TEXT,
            last_error TEXT,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS context_summaries (
            session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
            summary_text TEXT NOT NULL,
            covered_message_count BIGINT NOT NULL,
            summary_token_estimate BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS context_offloads (
            session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
            refs_json TEXT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS session_retention (
            session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
            tier TEXT NOT NULL,
            last_accessed_at BIGINT NOT NULL,
            archived_at BIGINT,
            archive_manifest_path TEXT,
            archive_version BIGINT,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS session_search_docs (
            doc_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            source_kind TEXT NOT NULL,
            source_ref TEXT NOT NULL,
            body TEXT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS knowledge_sources (
            source_id TEXT PRIMARY KEY,
            path TEXT NOT NULL UNIQUE,
            kind TEXT NOT NULL,
            sha256 TEXT NOT NULL,
            byte_len BIGINT NOT NULL,
            mtime BIGINT NOT NULL,
            indexed_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS knowledge_search_docs (
            doc_id TEXT PRIMARY KEY,
            source_id TEXT NOT NULL REFERENCES knowledge_sources(source_id) ON DELETE CASCADE,
            path TEXT NOT NULL,
            kind TEXT NOT NULL,
            body TEXT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS mcp_connectors (
            id TEXT PRIMARY KEY,
            transport TEXT NOT NULL,
            command TEXT NOT NULL,
            args_json TEXT NOT NULL,
            env_json TEXT NOT NULL,
            cwd TEXT,
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS telegram_user_pairings (
            token TEXT PRIMARY KEY,
            telegram_user_id BIGINT NOT NULL UNIQUE,
            telegram_chat_id BIGINT NOT NULL,
            telegram_username TEXT,
            telegram_display_name TEXT NOT NULL,
            status TEXT NOT NULL,
            created_at BIGINT NOT NULL,
            expires_at BIGINT NOT NULL,
            activated_at BIGINT
        );

        CREATE TABLE IF NOT EXISTS telegram_chat_bindings (
            telegram_chat_id BIGINT PRIMARY KEY,
            scope TEXT NOT NULL,
            owner_telegram_user_id BIGINT,
            selected_session_id TEXT,
            default_agent_profile_id TEXT,
            last_delivered_transcript_created_at BIGINT,
            last_delivered_transcript_id TEXT,
            inbound_queue_mode TEXT NOT NULL DEFAULT 'coalesce',
            inbound_coalesce_window_ms BIGINT,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS telegram_chat_statuses (
            telegram_chat_id BIGINT PRIMARY KEY,
            message_id INTEGER NOT NULL,
            state TEXT NOT NULL,
            expires_at BIGINT,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS telegram_update_cursors (
            consumer TEXT PRIMARY KEY,
            update_id BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS plans (
            session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
            items_json TEXT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS artifacts (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            path TEXT NOT NULL,
            metadata_json TEXT NOT NULL,
            byte_len BIGINT NOT NULL,
            sha256 TEXT NOT NULL,
            created_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS file_delivery_requests (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
            artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
            target TEXT NOT NULL,
            file_name TEXT NOT NULL,
            caption TEXT,
            status TEXT NOT NULL,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL,
            delivered_at BIGINT,
            error TEXT
        );

        CREATE TABLE IF NOT EXISTS delivery_targets (
            target_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            address TEXT NOT NULL,
            scope TEXT NOT NULL,
            owner_user_id TEXT,
            allowed_agent_ids_json TEXT NOT NULL DEFAULT '[]',
            allowed_session_ids_json TEXT NOT NULL DEFAULT '[]',
            send_policy_json TEXT NOT NULL DEFAULT 'null',
            format_policy TEXT NOT NULL DEFAULT 'full_text',
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS session_output_routes (
            route_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            target_id TEXT NOT NULL REFERENCES delivery_targets(target_id) ON DELETE CASCADE,
            filter_json TEXT NOT NULL DEFAULT 'null',
            format_policy TEXT NOT NULL DEFAULT 'full_text',
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            last_delivered_transcript_created_at BIGINT,
            last_delivered_transcript_id TEXT,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS event_sources (
            source_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            address TEXT NOT NULL,
            display_name TEXT,
            owner_user_id TEXT,
            auth_policy_json TEXT NOT NULL DEFAULT '{}',
            default_route_policy_json TEXT NOT NULL DEFAULT '{}',
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS router_rules (
            rule_id TEXT PRIMARY KEY,
            priority BIGINT NOT NULL,
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            source_filter_json TEXT NOT NULL DEFAULT '{}',
            operator_filter_json TEXT NOT NULL DEFAULT '{}',
            condition_json TEXT NOT NULL DEFAULT '{}',
            route_policy_json TEXT NOT NULL,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS inbound_events (
            event_id TEXT PRIMARY KEY,
            dedupe_key TEXT NOT NULL UNIQUE,
            source_kind TEXT NOT NULL,
            source_id TEXT NOT NULL,
            operator_id TEXT,
            payload_json TEXT NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL,
            received_at BIGINT NOT NULL,
            published_at BIGINT,
            error TEXT
        );

        CREATE TABLE IF NOT EXISTS routed_events (
            routed_event_id TEXT PRIMARY KEY,
            inbound_event_id TEXT NOT NULL REFERENCES inbound_events(event_id) ON DELETE CASCADE,
            rule_id TEXT REFERENCES router_rules(rule_id) ON DELETE SET NULL,
            session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            agent_id TEXT NOT NULL,
            queue_policy TEXT NOT NULL,
            priority BIGINT NOT NULL,
            payload_json TEXT NOT NULL,
            metadata_json TEXT NOT NULL DEFAULT '{}',
            status TEXT NOT NULL,
            routed_at BIGINT NOT NULL,
            published_at BIGINT,
            error TEXT
        );

        CREATE TABLE IF NOT EXISTS event_outbox (
            outbox_id TEXT PRIMARY KEY,
            subject TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            status TEXT NOT NULL,
            attempt_count BIGINT NOT NULL DEFAULT 0,
            next_attempt_at BIGINT NOT NULL,
            created_at BIGINT NOT NULL,
            published_at BIGINT,
            last_error TEXT
        );

        CREATE TABLE IF NOT EXISTS event_deliveries (
            delivery_event_id TEXT PRIMARY KEY,
            source_event_id TEXT NOT NULL,
            target_id TEXT NOT NULL,
            status TEXT NOT NULL,
            attempt_count BIGINT NOT NULL DEFAULT 0,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL,
            delivered_at BIGINT,
            last_error TEXT
        );

        CREATE TABLE IF NOT EXISTS task_registry (
            task_id TEXT PRIMARY KEY,
            kind TEXT NOT NULL,
            source_session_id TEXT,
            owner_agent_id TEXT,
            executor_agent_id TEXT,
            parent_task_id TEXT,
            status TEXT NOT NULL,
            dependency_json TEXT NOT NULL DEFAULT '[]',
            context_ref_json TEXT NOT NULL DEFAULT '[]',
            result_ref_json TEXT,
            retry_policy_json TEXT NOT NULL DEFAULT '{}',
            attempt_count BIGINT NOT NULL DEFAULT 0,
            max_attempts BIGINT NOT NULL DEFAULT 1,
            timeout_at BIGINT,
            chain_id TEXT,
            hop_count BIGINT,
            max_hops BIGINT,
            trace_id TEXT,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL,
            started_at BIGINT,
            finished_at BIGINT,
            error TEXT
        );

        CREATE TABLE IF NOT EXISTS task_followers (
            follower_id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL REFERENCES task_registry(task_id) ON DELETE CASCADE,
            target_id TEXT NOT NULL REFERENCES delivery_targets(target_id) ON DELETE CASCADE,
            enabled BOOLEAN NOT NULL DEFAULT TRUE,
            created_by_user_id TEXT,
            created_at BIGINT NOT NULL,
            updated_at BIGINT NOT NULL,
            delivered_at BIGINT,
            last_error TEXT,
            UNIQUE(task_id, target_id)
        );

        CREATE INDEX IF NOT EXISTS idx_missions_session_id ON missions(session_id);
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
        CREATE INDEX IF NOT EXISTS idx_tool_calls_session_updated_at ON tool_calls(session_id, updated_at DESC, requested_at DESC);
        CREATE INDEX IF NOT EXISTS idx_trace_links_trace_id ON trace_links(trace_id);
        CREATE INDEX IF NOT EXISTS idx_trace_links_created_at ON trace_links(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_session_inbox_events_session_id ON session_inbox_events(session_id);
        CREATE INDEX IF NOT EXISTS idx_session_inbox_events_status_available_at ON session_inbox_events(status, available_at);
        CREATE INDEX IF NOT EXISTS idx_agent_profiles_updated_at ON agent_profiles(updated_at);
        CREATE INDEX IF NOT EXISTS idx_kv_entries_scope_namespace_key ON kv_entries(scope, namespace_id, key);
        CREATE INDEX IF NOT EXISTS idx_kv_entries_expires_at ON kv_entries(expires_at);
        CREATE INDEX IF NOT EXISTS idx_agent_chain_continuations_granted_at ON agent_chain_continuations(granted_at);
        CREATE INDEX IF NOT EXISTS idx_agent_schedules_next_fire_at ON agent_schedules(next_fire_at);
        CREATE INDEX IF NOT EXISTS idx_context_summaries_updated_at ON context_summaries(updated_at);
        CREATE INDEX IF NOT EXISTS idx_context_offloads_updated_at ON context_offloads(updated_at);
        CREATE INDEX IF NOT EXISTS idx_session_retention_tier_updated_at ON session_retention(tier, updated_at);
        CREATE INDEX IF NOT EXISTS idx_session_search_docs_session_id ON session_search_docs(session_id);
        CREATE INDEX IF NOT EXISTS idx_session_search_docs_body_fts ON session_search_docs USING GIN (to_tsvector('simple', body));
        CREATE INDEX IF NOT EXISTS idx_knowledge_sources_kind_mtime ON knowledge_sources(kind, mtime DESC);
        CREATE INDEX IF NOT EXISTS idx_mcp_connectors_enabled_updated_at ON mcp_connectors(enabled, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_telegram_user_pairings_status_expires_at ON telegram_user_pairings(status, expires_at);
        CREATE INDEX IF NOT EXISTS idx_telegram_chat_bindings_scope_updated_at ON telegram_chat_bindings(scope, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_telegram_chat_statuses_state_expires_at ON telegram_chat_statuses(state, expires_at);
        CREATE INDEX IF NOT EXISTS idx_knowledge_search_docs_source_id ON knowledge_search_docs(source_id);
        CREATE INDEX IF NOT EXISTS idx_knowledge_search_docs_body_fts ON knowledge_search_docs USING GIN (to_tsvector('simple', body));
        CREATE INDEX IF NOT EXISTS idx_artifacts_session_id ON artifacts(session_id);
        CREATE INDEX IF NOT EXISTS idx_file_delivery_requests_session_status ON file_delivery_requests(session_id, status, created_at);
        CREATE INDEX IF NOT EXISTS idx_delivery_targets_kind_scope ON delivery_targets(kind, scope, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_session_output_routes_session_enabled ON session_output_routes(session_id, enabled, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_session_output_routes_target_id ON session_output_routes(target_id);
        CREATE INDEX IF NOT EXISTS idx_event_sources_kind_enabled ON event_sources(kind, enabled, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_router_rules_enabled_priority ON router_rules(enabled, priority ASC, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_inbound_events_source_received_at ON inbound_events(source_kind, source_id, received_at DESC);
        CREATE INDEX IF NOT EXISTS idx_inbound_events_status_received_at ON inbound_events(status, received_at);
        CREATE INDEX IF NOT EXISTS idx_routed_events_session_status ON routed_events(session_id, status, routed_at);
        CREATE INDEX IF NOT EXISTS idx_event_outbox_status_next_attempt ON event_outbox(status, next_attempt_at, created_at);
        CREATE INDEX IF NOT EXISTS idx_event_deliveries_target_status ON event_deliveries(target_id, status, updated_at);
        CREATE INDEX IF NOT EXISTS idx_task_registry_status_updated_at ON task_registry(status, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_task_registry_chain_id ON task_registry(chain_id);
        CREATE INDEX IF NOT EXISTS idx_task_followers_task_enabled ON task_followers(task_id, enabled, updated_at DESC);
        CREATE INDEX IF NOT EXISTS idx_task_followers_target_id ON task_followers(target_id);
        ",
    )?;

    Ok(())
}

pub(super) fn validate_schema(client: &mut Client) -> Result<(), StoreError> {
    for (table, column, required_not_null) in REQUIRED_COLUMNS {
        validate_column(client, table, column, *required_not_null)?;
    }
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

const REQUIRED_COLUMNS: &[(&str, &str, bool)] = &[
    ("sessions", "id", true),
    ("sessions", "title", true),
    ("sessions", "settings_json", true),
    ("sessions", "workspace_root", true),
    ("sessions", "agent_profile_id", true),
    ("missions", "id", true),
    ("missions", "session_id", true),
    ("runs", "id", true),
    ("runs", "session_id", true),
    ("jobs", "id", true),
    ("jobs", "session_id", true),
    ("transcripts", "id", true),
    ("transcripts", "session_id", true),
    ("tool_calls", "id", true),
    ("tool_calls", "session_id", true),
    ("tool_calls", "run_id", true),
    ("trace_links", "entity_kind", true),
    ("trace_links", "entity_id", true),
    ("session_inbox_events", "id", true),
    ("session_inbox_events", "session_id", true),
    ("agent_profiles", "id", true),
    ("daemon_state", "key", true),
    ("kv_entries", "scope", true),
    ("kv_entries", "namespace_id", true),
    ("kv_entries", "key", true),
    ("agent_schedules", "id", true),
    ("context_summaries", "session_id", true),
    ("context_offloads", "session_id", true),
    ("session_retention", "session_id", true),
    ("session_search_docs", "doc_id", true),
    ("knowledge_sources", "source_id", true),
    ("knowledge_search_docs", "doc_id", true),
    ("mcp_connectors", "id", true),
    ("telegram_user_pairings", "token", true),
    ("telegram_chat_bindings", "telegram_chat_id", true),
    ("telegram_chat_statuses", "telegram_chat_id", true),
    ("telegram_update_cursors", "consumer", true),
    ("plans", "session_id", true),
    ("artifacts", "id", true),
    ("artifacts", "session_id", true),
    ("file_delivery_requests", "id", true),
    ("file_delivery_requests", "session_id", true),
    ("delivery_targets", "target_id", true),
    ("session_output_routes", "route_id", true),
    ("session_output_routes", "session_id", true),
    ("event_sources", "source_id", true),
    ("router_rules", "rule_id", true),
    ("inbound_events", "event_id", true),
    ("routed_events", "routed_event_id", true),
    ("event_outbox", "outbox_id", true),
    ("event_deliveries", "delivery_event_id", true),
    ("task_registry", "task_id", true),
    ("task_followers", "follower_id", true),
    ("task_followers", "task_id", true),
    ("task_followers", "target_id", true),
];

fn validate_column(
    client: &mut Client,
    table: &'static str,
    column: &'static str,
    required_not_null: bool,
) -> Result<(), StoreError> {
    let row = client.query_opt(
        "
        SELECT is_nullable
        FROM information_schema.columns
        WHERE table_schema = current_schema()
          AND table_name = $1
          AND column_name = $2
        ",
        &[&table, &column],
    )?;

    let Some(row) = row else {
        return Err(StoreError::SchemaMismatch {
            table,
            reason: format!("missing required column {column}"),
        });
    };

    let is_nullable: String = row.get(0);
    if required_not_null && is_nullable == "YES" {
        return Err(StoreError::SchemaMismatch {
            table,
            reason: format!("{column} must be NOT NULL"),
        });
    }

    Ok(())
}
