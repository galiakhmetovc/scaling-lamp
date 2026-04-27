use super::*;
use agent_persistence::{
    AgentRepository, ArtifactRecord, ContextSummaryRecord, ContextSummaryRepository,
    KnowledgeRepository, KnowledgeSearchDocRecord, KnowledgeSourceRecord, PlanRepository,
    SessionRepository, SessionRetentionRecord, SessionRetentionRepository, SessionSearchDocRecord,
    SessionSearchRepository, TranscriptRecord, TranscriptRepository,
};
use agent_runtime::archive::{ArchivedArtifactEntry, ArchivedSummary, ArchivedTranscriptEntry};
use agent_runtime::memory::{SessionRetentionState, SessionRetentionTier};
use agent_runtime::plan::PlanSnapshot;
use agent_runtime::tool::{
    KnowledgeReadInput, KnowledgeReadMode, KnowledgeReadOutput, KnowledgeRoot,
    KnowledgeSearchInput, KnowledgeSearchOutput, KnowledgeSearchResultOutput, KnowledgeSourceKind,
    SessionReadArtifactOutput, SessionReadInput, SessionReadMessageOutput, SessionReadMode,
    SessionReadOutput, SessionReadSummaryOutput, SessionSearchInput, SessionSearchMatchSource,
    SessionSearchOutput, SessionSearchResultOutput, ToolError,
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

const CANONICAL_KNOWLEDGE_FILES: &[(&str, KnowledgeRoot, KnowledgeSourceKind)] = &[
    (
        "README.md",
        KnowledgeRoot::RootDocs,
        KnowledgeSourceKind::RootDoc,
    ),
    (
        "SYSTEM.md",
        KnowledgeRoot::RootDocs,
        KnowledgeSourceKind::RootDoc,
    ),
    (
        "AGENTS.md",
        KnowledgeRoot::RootDocs,
        KnowledgeSourceKind::RootDoc,
    ),
];

const CANONICAL_KNOWLEDGE_DIRS: &[(&str, KnowledgeRoot, KnowledgeSourceKind)] = &[
    ("docs", KnowledgeRoot::Docs, KnowledgeSourceKind::ProjectDoc),
    (
        "projects",
        KnowledgeRoot::Projects,
        KnowledgeSourceKind::ProjectDoc,
    ),
    (
        "notes",
        KnowledgeRoot::Notes,
        KnowledgeSourceKind::ProjectNote,
    ),
];

const ALLOWED_KNOWLEDGE_EXTENSIONS: &[&str] = &["md", "txt", "json", "yaml", "yml", "toml"];

#[derive(Debug, Clone, Copy)]
struct SessionReadWindow {
    cursor: Option<usize>,
    max_items: Option<usize>,
    max_bytes: Option<usize>,
}

#[derive(Debug, Clone)]
struct ScannedKnowledgeSource {
    record: KnowledgeSourceRecord,
    doc: KnowledgeSearchDocRecord,
}

impl ExecutionService {
    pub(crate) fn search_sessions(
        &self,
        store: &PersistenceStore,
        input: &SessionSearchInput,
    ) -> Result<SessionSearchOutput, ExecutionError> {
        let query = input.query.trim();
        if query.is_empty() {
            return Err(invalid_memory_tool(
                "session_search query cannot be blank".to_string(),
            ));
        }

        self.refresh_session_search_index(store)?;
        let agent_filter =
            resolve_agent_profile_id_by_identifier(store, input.agent_identifier.as_deref())?;
        let tier_filter = input.tiers.clone();
        let query_lower = query.to_lowercase();
        let sessions = store
            .list_sessions()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(Session::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;
        let mut sessions_by_id = BTreeMap::new();
        for session in sessions {
            let retention = load_session_retention(store, &session)?;
            sessions_by_id.insert(session.id.clone(), (session, retention));
        }

        let mut results_by_session = BTreeMap::new();
        for doc in store
            .list_session_search_docs()
            .map_err(ExecutionError::Store)?
        {
            let Some((session, retention)) = sessions_by_id.get(&doc.session_id) else {
                continue;
            };
            if let Some(expected_agent_id) = agent_filter.as_deref()
                && session.agent_profile_id != expected_agent_id
            {
                continue;
            }
            if let Some(updated_after) = input.updated_after
                && session.updated_at < updated_after
            {
                continue;
            }
            if let Some(updated_before) = input.updated_before
                && session.updated_at > updated_before
            {
                continue;
            }
            if let Some(filter) = tier_filter.as_ref()
                && !filter.contains(&retention.tier)
            {
                continue;
            }
            let Some(match_source) = parse_session_search_match_source(doc.source_kind.as_str())
            else {
                continue;
            };
            let Some(snippet) = excerpt_for_query(doc.body.as_str(), query, &query_lower) else {
                continue;
            };

            let candidate = SessionSearchResultOutput {
                session_id: session.id.clone(),
                title: session.title.clone(),
                agent_profile_id: session.agent_profile_id.clone(),
                tier: retention.tier,
                updated_at: session.updated_at,
                match_source,
                snippet,
            };
            let replace = results_by_session
                .get(&session.id)
                .map(|current: &SessionSearchResultOutput| {
                    session_search_match_source_priority(candidate.match_source)
                        < session_search_match_source_priority(current.match_source)
                })
                .unwrap_or(true);
            if replace {
                results_by_session.insert(session.id.clone(), candidate);
            }
        }
        let mut results = results_by_session.into_values().collect::<Vec<_>>();
        results.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then(left.session_id.cmp(&right.session_id))
        });

        let (offset, limit, next_offset) = normalized_pagination(
            results.len(),
            input.offset,
            input.limit,
            self.config.runtime_limits.session_search_default_limit,
            self.config.runtime_limits.session_search_max_limit,
        );
        let end = offset.saturating_add(limit).min(results.len());
        let page = results[offset..end].to_vec();

        Ok(SessionSearchOutput {
            query: query.to_string(),
            results: page,
            truncated: next_offset.is_some(),
            offset,
            limit,
            total_results: results.len(),
            next_offset,
        })
    }

    pub(crate) fn read_session(
        &self,
        store: &PersistenceStore,
        input: &SessionReadInput,
    ) -> Result<SessionReadOutput, ExecutionError> {
        let session = self.load_session(store, input.session_id.as_str())?;
        let retention = load_session_retention(store, &session)?;
        let mode = input.mode.unwrap_or(SessionReadMode::Summary);
        let include_tools = input.include_tools.unwrap_or(true);
        let window = SessionReadWindow {
            cursor: input.cursor,
            max_items: input.max_items,
            max_bytes: input.max_bytes,
        };

        match mode {
            SessionReadMode::Summary => {
                self.read_session_summary(store, &session, &retention, mode)
            }
            SessionReadMode::Timeline | SessionReadMode::Transcript => {
                self.read_session_messages(store, &session, &retention, mode, window, include_tools)
            }
            SessionReadMode::Artifacts => {
                self.read_session_artifacts(store, &session, &retention, mode, window)
            }
        }
    }

    fn read_session_summary(
        &self,
        store: &PersistenceStore,
        session: &Session,
        retention: &SessionRetentionState,
        mode: SessionReadMode,
    ) -> Result<SessionReadOutput, ExecutionError> {
        let archived_summary = if retention.tier == SessionRetentionTier::Cold {
            store
                .read_session_archive_summary(session.id.as_str())
                .map_err(ExecutionError::Store)?
        } else {
            None
        };
        let (summary, from_archive) = match archived_summary {
            Some(summary) => (Some(summary_output_from_archive(summary)), true),
            None => (
                store
                    .get_context_summary(session.id.as_str())
                    .map_err(ExecutionError::Store)?
                    .map(summary_output_from_record),
                false,
            ),
        };
        let total_items = usize::from(summary.is_some());

        Ok(SessionReadOutput {
            session_id: session.id.clone(),
            title: session.title.clone(),
            agent_profile_id: session.agent_profile_id.clone(),
            mode,
            tier: retention.tier,
            from_archive,
            cursor: 0,
            next_cursor: None,
            truncated: false,
            total_items,
            summary,
            messages: Vec::new(),
            artifacts: Vec::new(),
        })
    }

    fn read_session_messages(
        &self,
        store: &PersistenceStore,
        session: &Session,
        retention: &SessionRetentionState,
        mode: SessionReadMode,
        window: SessionReadWindow,
        include_tools: bool,
    ) -> Result<SessionReadOutput, ExecutionError> {
        let archived_entries = if retention.tier == SessionRetentionTier::Cold {
            store
                .read_session_archive_transcripts(session.id.as_str())
                .map_err(ExecutionError::Store)?
        } else {
            None
        };
        let from_archive = archived_entries.is_some();
        let mut messages = if let Some(entries) = archived_entries {
            entries
                .into_iter()
                .filter(|entry| include_tools || entry.kind != "tool")
                .map(|entry| {
                    message_output_from_archive(
                        entry,
                        mode,
                        self.config.runtime_limits.timeline_preview_chars,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            store
                .list_transcripts_for_session(session.id.as_str())
                .map_err(ExecutionError::Store)?
                .into_iter()
                .filter(|entry| include_tools || entry.kind != "tool")
                .map(|entry| {
                    message_output_from_record(
                        entry,
                        mode,
                        self.config.runtime_limits.timeline_preview_chars,
                    )
                })
                .collect::<Vec<_>>()
        };
        messages.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.id.cmp(&right.id))
        });
        let total_items = messages.len();
        let (messages, cursor, next_cursor, truncated) = paginate_messages(
            messages,
            window.cursor,
            window.max_items,
            window.max_bytes,
            &self.config.runtime_limits,
        );

        Ok(SessionReadOutput {
            session_id: session.id.clone(),
            title: session.title.clone(),
            agent_profile_id: session.agent_profile_id.clone(),
            mode,
            tier: retention.tier,
            from_archive,
            cursor,
            next_cursor,
            truncated,
            total_items,
            summary: None,
            messages,
            artifacts: Vec::new(),
        })
    }

    fn read_session_artifacts(
        &self,
        store: &PersistenceStore,
        session: &Session,
        retention: &SessionRetentionState,
        mode: SessionReadMode,
        window: SessionReadWindow,
    ) -> Result<SessionReadOutput, ExecutionError> {
        let archived_manifest = if retention.tier == SessionRetentionTier::Cold {
            store
                .read_session_archive_manifest(session.id.as_str())
                .map_err(ExecutionError::Store)?
        } else {
            None
        };
        let from_archive = archived_manifest.is_some();
        let mut artifacts = if let Some(manifest) = archived_manifest {
            manifest
                .artifacts
                .into_iter()
                .map(artifact_output_from_archive)
                .collect::<Vec<_>>()
        } else {
            store
                .list_artifacts_for_session(session.id.as_str())
                .map_err(ExecutionError::Store)?
                .into_iter()
                .map(artifact_output_from_record)
                .collect::<Vec<_>>()
        };
        artifacts.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then(left.artifact_id.cmp(&right.artifact_id))
        });
        let total_items = artifacts.len();
        let (artifacts, cursor, next_cursor, truncated) = paginate_artifacts(
            artifacts,
            window.cursor,
            window.max_items,
            window.max_bytes,
            &self.config.runtime_limits,
        );

        Ok(SessionReadOutput {
            session_id: session.id.clone(),
            title: session.title.clone(),
            agent_profile_id: session.agent_profile_id.clone(),
            mode,
            tier: retention.tier,
            from_archive,
            cursor,
            next_cursor,
            truncated,
            total_items,
            summary: None,
            messages: Vec::new(),
            artifacts,
        })
    }

    pub(crate) fn search_knowledge(
        &self,
        store: &PersistenceStore,
        input: &KnowledgeSearchInput,
    ) -> Result<KnowledgeSearchOutput, ExecutionError> {
        let query = input.query.trim();
        if query.is_empty() {
            return Err(invalid_memory_tool(
                "knowledge_search query cannot be blank".to_string(),
            ));
        }

        self.refresh_knowledge_index(store)?;
        let query_lower = query.to_lowercase();
        let kinds = input.kinds.clone();
        let roots = input.roots.clone();
        let sources_by_id = store
            .list_knowledge_sources()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(|source| (source.source_id.clone(), source))
            .collect::<BTreeMap<_, _>>();

        let mut results = store
            .list_knowledge_search_docs()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .filter_map(|doc| {
                let source = sources_by_id.get(&doc.source_id)?;
                let kind = parse_knowledge_source_kind(&source.kind).ok()?;
                let root = classify_knowledge_root(doc.path.as_str()).ok()?;
                if let Some(filter) = kinds.as_ref()
                    && !filter.contains(&kind)
                {
                    return None;
                }
                if let Some(filter) = roots.as_ref()
                    && !filter.contains(&root)
                {
                    return None;
                }
                let snippet = excerpt_for_query(doc.body.as_str(), query, &query_lower)
                    .or_else(|| excerpt_for_query(doc.path.as_str(), query, &query_lower))?;
                Some(KnowledgeSearchResultOutput {
                    path: doc.path,
                    kind,
                    snippet,
                    sha256: source.sha256.clone(),
                    mtime: source.mtime,
                })
            })
            .collect::<Vec<_>>();
        results.sort_by(|left, right| {
            right
                .mtime
                .cmp(&left.mtime)
                .then(left.path.cmp(&right.path))
        });

        let (offset, limit, next_offset) = normalized_pagination(
            results.len(),
            input.offset,
            input.limit,
            self.config.runtime_limits.knowledge_search_default_limit,
            self.config.runtime_limits.knowledge_search_max_limit,
        );
        let end = offset.saturating_add(limit).min(results.len());
        let page = results[offset..end].to_vec();

        Ok(KnowledgeSearchOutput {
            query: query.to_string(),
            results: page,
            truncated: next_offset.is_some(),
            offset,
            limit,
            total_results: results.len(),
            next_offset,
        })
    }

    pub(crate) fn read_knowledge(
        &self,
        store: &PersistenceStore,
        input: &KnowledgeReadInput,
    ) -> Result<KnowledgeReadOutput, ExecutionError> {
        let path = input.path.trim();
        if path.is_empty() {
            return Err(invalid_memory_tool(
                "knowledge_read path cannot be blank".to_string(),
            ));
        }

        self.refresh_knowledge_index(store)?;
        let normalized_path = normalize_memory_path(path);
        let source = store
            .get_knowledge_source_by_path(normalized_path.as_str())
            .map_err(ExecutionError::Store)?
            .ok_or_else(|| {
                invalid_memory_tool(format!("knowledge source {normalized_path} not found"))
            })?;
        let kind = parse_knowledge_source_kind(&source.kind)?;
        ensure_canonical_knowledge_path(normalized_path.as_str())?;

        let content = self
            .workspace
            .read_text(normalized_path.as_str())
            .map_err(ToolError::Workspace)
            .map_err(ExecutionError::Tool)?;
        let mode = input.mode.unwrap_or(KnowledgeReadMode::Excerpt);
        let max_lines = input
            .max_lines
            .unwrap_or(match mode {
                KnowledgeReadMode::Excerpt => {
                    self.config
                        .runtime_limits
                        .knowledge_read_excerpt_default_max_lines
                }
                KnowledgeReadMode::Full => {
                    self.config
                        .runtime_limits
                        .knowledge_read_full_default_max_lines
                }
            })
            .clamp(1, self.config.runtime_limits.knowledge_read_max_lines);
        let max_bytes = input
            .max_bytes
            .unwrap_or(self.config.runtime_limits.knowledge_read_default_max_bytes)
            .clamp(1, self.config.runtime_limits.knowledge_read_max_bytes);

        let lines = content.lines().map(str::to_string).collect::<Vec<_>>();
        let total_lines = lines.len();
        let start = input.cursor.unwrap_or(0).min(total_lines);
        let mut selected = Vec::new();
        let mut consumed_bytes = 0usize;
        let mut next_cursor = None;
        let mut truncated = false;

        for (index, line) in lines.iter().enumerate().skip(start) {
            if selected.len() >= max_lines {
                next_cursor = Some(index);
                truncated = true;
                break;
            }
            let line_bytes = line.len() + usize::from(!selected.is_empty());
            if consumed_bytes + line_bytes > max_bytes {
                if selected.is_empty() {
                    selected.push(truncate_utf8(line.as_str(), max_bytes));
                    next_cursor = Some(index + 1);
                } else {
                    next_cursor = Some(index);
                }
                truncated = true;
                break;
            }
            consumed_bytes += line_bytes;
            selected.push(line.clone());
        }

        if next_cursor.is_none() && start + selected.len() < total_lines {
            next_cursor = Some(start + selected.len());
            truncated = true;
        }

        let start_line = if selected.is_empty() { 0 } else { start + 1 };
        let end_line = if selected.is_empty() {
            0
        } else {
            start + selected.len()
        };

        Ok(KnowledgeReadOutput {
            path: normalized_path,
            kind,
            sha256: source.sha256,
            mtime: source.mtime,
            mode,
            cursor: start,
            next_cursor,
            truncated,
            total_lines,
            start_line,
            end_line,
            text: selected.join("\n"),
        })
    }

    pub(crate) fn maintain_memory(
        &self,
        store: &PersistenceStore,
        now: i64,
    ) -> Result<(), ExecutionError> {
        let sessions = store
            .list_sessions()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(Session::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;

        for session in sessions {
            let mut retention = load_session_retention(store, &session)?;
            if retention.tier == SessionRetentionTier::Cold {
                continue;
            }

            let last_accessed_at = retention.last_accessed_at.max(session.updated_at);
            let has_active_run = self.session_has_active_run(store, &session.id)?;
            let desired_tier = if has_active_run
                || now.saturating_sub(last_accessed_at)
                    < self.config.runtime_limits.session_warm_idle_seconds as i64
            {
                SessionRetentionTier::Active
            } else {
                SessionRetentionTier::Warm
            };

            if retention.tier != desired_tier || retention.last_accessed_at != last_accessed_at {
                retention.tier = desired_tier;
                retention.last_accessed_at = last_accessed_at;
                retention.updated_at = now;
                store
                    .put_session_retention(&SessionRetentionRecord::from(&retention))
                    .map_err(ExecutionError::Store)?;
            }
        }

        self.refresh_session_search_index(store)?;
        self.refresh_knowledge_index(store)?;

        Ok(())
    }

    pub(crate) fn archive_session_to_cold(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        now: i64,
    ) -> Result<SessionRetentionState, ExecutionError> {
        let session = self.load_session(store, session_id)?;
        let mut retention = load_session_retention(store, &session)?;
        let manifest = store
            .archive_session_bundle(session_id, now)
            .map_err(ExecutionError::Store)?;

        retention.tier = SessionRetentionTier::Cold;
        retention.last_accessed_at = retention.last_accessed_at.max(session.updated_at);
        retention.archived_at = Some(now);
        retention.archive_manifest_path =
            Some(format!("archives/sessions/{session_id}/manifest.json"));
        retention.archive_version = Some(manifest.archive_version);
        retention.updated_at = now;
        store
            .put_session_retention(&SessionRetentionRecord::from(&retention))
            .map_err(ExecutionError::Store)?;

        Ok(retention)
    }

    fn refresh_knowledge_index(&self, store: &PersistenceStore) -> Result<(), ExecutionError> {
        let scanned = collect_knowledge_sources(self.workspace.root.as_path())?;
        let scanned_ids = scanned
            .iter()
            .map(|source| source.record.source_id.clone())
            .collect::<BTreeSet<_>>();

        for existing in store
            .list_knowledge_sources()
            .map_err(ExecutionError::Store)?
        {
            if !scanned_ids.contains(&existing.source_id) {
                store
                    .delete_knowledge_source(existing.source_id.as_str())
                    .map_err(ExecutionError::Store)?;
            }
        }

        for source in scanned {
            store
                .put_knowledge_source(&source.record)
                .map_err(ExecutionError::Store)?;
            store
                .replace_knowledge_search_docs(
                    source.record.source_id.as_str(),
                    std::slice::from_ref(&source.doc),
                )
                .map_err(ExecutionError::Store)?;
        }

        Ok(())
    }

    fn refresh_session_search_index(&self, store: &PersistenceStore) -> Result<(), ExecutionError> {
        let sessions = store
            .list_sessions()
            .map_err(ExecutionError::Store)?
            .into_iter()
            .map(Session::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ExecutionError::RecordConversion)?;

        for session in sessions {
            let retention = load_session_retention(store, &session)?;
            let docs = collect_session_search_docs(store, &session, &retention)?;
            store
                .replace_session_search_docs(session.id.as_str(), &docs)
                .map_err(ExecutionError::Store)?;
        }

        Ok(())
    }
}

fn collect_knowledge_sources(
    workspace_root: &Path,
) -> Result<Vec<ScannedKnowledgeSource>, ExecutionError> {
    let mut sources = Vec::new();

    for (path, _root, kind) in CANONICAL_KNOWLEDGE_FILES {
        let absolute = workspace_root.join(path);
        if absolute.is_file() {
            sources.push(scan_knowledge_file(
                workspace_root,
                absolute.as_path(),
                *kind,
            )?);
        }
    }

    for (dir, _root, kind) in CANONICAL_KNOWLEDGE_DIRS {
        let absolute = workspace_root.join(dir);
        if absolute.is_dir() {
            collect_knowledge_dir(workspace_root, absolute.as_path(), *kind, &mut sources)?;
        }
    }

    sources.sort_by(|left, right| left.record.path.cmp(&right.record.path));
    Ok(sources)
}

fn collect_knowledge_dir(
    workspace_root: &Path,
    current: &Path,
    kind: KnowledgeSourceKind,
    output: &mut Vec<ScannedKnowledgeSource>,
) -> Result<(), ExecutionError> {
    let mut entries = fs::read_dir(current)
        .map_err(|source| {
            ExecutionError::Tool(ToolError::Workspace(
                agent_runtime::workspace::WorkspaceError::Io {
                    path: current.to_path_buf(),
                    source,
                },
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|source| {
            ExecutionError::Tool(ToolError::Workspace(
                agent_runtime::workspace::WorkspaceError::Io {
                    path: current.to_path_buf(),
                    source,
                },
            ))
        })?;
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().map_err(|source| {
            ExecutionError::Tool(ToolError::Workspace(
                agent_runtime::workspace::WorkspaceError::Io {
                    path: path.clone(),
                    source,
                },
            ))
        })?;
        if file_type.is_dir() {
            collect_knowledge_dir(workspace_root, path.as_path(), kind, output)?;
        } else if file_type.is_file() && is_allowed_knowledge_extension(path.as_path()) {
            output.push(scan_knowledge_file(workspace_root, path.as_path(), kind)?);
        }
    }

    Ok(())
}

fn scan_knowledge_file(
    workspace_root: &Path,
    path: &Path,
    kind: KnowledgeSourceKind,
) -> Result<ScannedKnowledgeSource, ExecutionError> {
    let content = fs::read_to_string(path).map_err(|source| {
        ExecutionError::Tool(ToolError::Workspace(
            agent_runtime::workspace::WorkspaceError::Io {
                path: path.to_path_buf(),
                source,
            },
        ))
    })?;
    let metadata = fs::metadata(path).map_err(|source| {
        ExecutionError::Tool(ToolError::Workspace(
            agent_runtime::workspace::WorkspaceError::Io {
                path: path.to_path_buf(),
                source,
            },
        ))
    })?;
    let relative = path.strip_prefix(workspace_root).map_err(|_| {
        invalid_memory_tool(format!(
            "knowledge path {} escaped workspace",
            path.display()
        ))
    })?;
    let relative_path = relative.to_string_lossy().replace('\\', "/");
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|duration| i64::try_from(duration.as_secs()).unwrap_or(i64::MAX))
        .unwrap_or(0);
    let digest = Sha256::digest(content.as_bytes());
    let sha256 = format!("{digest:x}");
    let indexed_at = mtime.max(0);
    Ok(ScannedKnowledgeSource {
        record: KnowledgeSourceRecord {
            source_id: relative_path.clone(),
            path: relative_path.clone(),
            kind: kind.as_str().to_string(),
            sha256,
            byte_len: i64::try_from(content.len()).unwrap_or(i64::MAX),
            mtime,
            indexed_at,
        },
        doc: KnowledgeSearchDocRecord {
            doc_id: format!("{relative_path}#0"),
            source_id: relative_path.clone(),
            path: relative_path,
            kind: kind.as_str().to_string(),
            body: content,
            updated_at: indexed_at,
        },
    })
}

fn ensure_canonical_knowledge_path(
    path: &str,
) -> Result<(KnowledgeRoot, KnowledgeSourceKind), ExecutionError> {
    let normalized = normalize_memory_path(path);
    for (candidate, root, kind) in CANONICAL_KNOWLEDGE_FILES {
        if normalized == *candidate {
            return Ok((*root, *kind));
        }
    }
    for (prefix, root, kind) in CANONICAL_KNOWLEDGE_DIRS {
        if normalized.starts_with(&format!("{prefix}/"))
            && is_allowed_knowledge_extension(Path::new(&normalized))
        {
            return Ok((*root, *kind));
        }
    }
    Err(invalid_memory_tool(format!(
        "knowledge path {normalized} is outside canonical knowledge roots"
    )))
}

fn classify_knowledge_root(path: &str) -> Result<KnowledgeRoot, ExecutionError> {
    ensure_canonical_knowledge_path(path).map(|(root, _)| root)
}

fn parse_knowledge_source_kind(value: &str) -> Result<KnowledgeSourceKind, ExecutionError> {
    match value {
        "root_doc" => Ok(KnowledgeSourceKind::RootDoc),
        "project_doc" => Ok(KnowledgeSourceKind::ProjectDoc),
        "project_note" => Ok(KnowledgeSourceKind::ProjectNote),
        "extra_doc" => Ok(KnowledgeSourceKind::ExtraDoc),
        other => Err(invalid_memory_tool(format!(
            "unknown knowledge source kind {other}"
        ))),
    }
}

fn is_allowed_knowledge_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .map(|value| {
            let lowered = value.to_ascii_lowercase();
            ALLOWED_KNOWLEDGE_EXTENSIONS.contains(&lowered.as_str())
        })
        .unwrap_or(false)
}

fn normalize_memory_path(path: &str) -> String {
    path.trim_start_matches("./").replace('\\', "/")
}

fn load_session_retention(
    store: &PersistenceStore,
    session: &Session,
) -> Result<SessionRetentionState, ExecutionError> {
    let record = store
        .get_session_retention(session.id.as_str())
        .map_err(ExecutionError::Store)?;
    match record {
        Some(record) => {
            SessionRetentionState::try_from(record).map_err(ExecutionError::RecordConversion)
        }
        None => Ok(SessionRetentionState {
            session_id: session.id.clone(),
            tier: SessionRetentionTier::Active,
            last_accessed_at: session.updated_at,
            archived_at: None,
            archive_manifest_path: None,
            archive_version: None,
            updated_at: session.updated_at,
        }),
    }
}

fn collect_session_search_docs(
    store: &PersistenceStore,
    session: &Session,
    retention: &SessionRetentionState,
) -> Result<Vec<SessionSearchDocRecord>, ExecutionError> {
    let mut docs = Vec::new();
    docs.push(session_search_doc(
        session.id.as_str(),
        SessionSearchMatchSource::Title,
        session.id.as_str(),
        session.title.clone(),
        session.updated_at,
    ));

    if retention.tier == SessionRetentionTier::Cold {
        if let Some(summary) = store
            .read_session_archive_summary(session.id.as_str())
            .map_err(ExecutionError::Store)?
        {
            docs.push(session_search_doc(
                session.id.as_str(),
                SessionSearchMatchSource::ArchiveSummary,
                "archive:summary",
                summary.summary_text,
                summary.updated_at,
            ));
        }
        if let Some(entries) = store
            .read_session_archive_transcripts(session.id.as_str())
            .map_err(ExecutionError::Store)?
        {
            for entry in entries {
                docs.push(session_search_doc(
                    session.id.as_str(),
                    SessionSearchMatchSource::ArchiveTranscript,
                    entry.id,
                    entry.content,
                    entry.created_at,
                ));
            }
        }
    }

    if let Some(summary) = store
        .get_context_summary(session.id.as_str())
        .map_err(ExecutionError::Store)?
    {
        docs.push(session_search_doc(
            session.id.as_str(),
            SessionSearchMatchSource::Summary,
            session.id.as_str(),
            summary.summary_text,
            summary.updated_at,
        ));
    }

    if let Some(plan_record) = store
        .get_plan(session.id.as_str())
        .map_err(ExecutionError::Store)?
    {
        let updated_at = plan_record.updated_at;
        let snapshot =
            PlanSnapshot::try_from(plan_record).map_err(ExecutionError::RecordConversion)?;
        let body = plan_search_text(&snapshot);
        if !body.trim().is_empty() {
            docs.push(session_search_doc(
                session.id.as_str(),
                SessionSearchMatchSource::Plan,
                session.id.as_str(),
                body,
                updated_at,
            ));
        }
    }

    for entry in store
        .list_transcripts_for_session(session.id.as_str())
        .map_err(ExecutionError::Store)?
    {
        let source = if entry.kind == "system" {
            SessionSearchMatchSource::SystemNote
        } else {
            SessionSearchMatchSource::Transcript
        };
        docs.push(session_search_doc(
            session.id.as_str(),
            source,
            entry.id,
            entry.content,
            entry.created_at,
        ));
    }

    for artifact in store
        .list_artifacts_for_session(session.id.as_str())
        .map_err(ExecutionError::Store)?
    {
        let label = artifact_metadata_string(artifact.metadata_json.as_str(), "label");
        let summary = artifact_metadata_string(artifact.metadata_json.as_str(), "summary");
        let haystack = [label.as_deref(), summary.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" — ");
        if !haystack.is_empty() {
            docs.push(session_search_doc(
                session.id.as_str(),
                SessionSearchMatchSource::Artifact,
                artifact.id,
                haystack,
                artifact.created_at,
            ));
        }
    }

    Ok(docs)
}

fn session_search_doc(
    session_id: &str,
    source: SessionSearchMatchSource,
    source_ref: impl Into<String>,
    body: impl Into<String>,
    updated_at: i64,
) -> SessionSearchDocRecord {
    let source_ref = source_ref.into();
    SessionSearchDocRecord {
        doc_id: format!("{session_id}#{}:{source_ref}", source.as_str()),
        session_id: session_id.to_string(),
        source_kind: source.as_str().to_string(),
        source_ref,
        body: body.into(),
        updated_at,
    }
}

fn parse_session_search_match_source(value: &str) -> Option<SessionSearchMatchSource> {
    match value {
        "title" => Some(SessionSearchMatchSource::Title),
        "summary" => Some(SessionSearchMatchSource::Summary),
        "plan" => Some(SessionSearchMatchSource::Plan),
        "system_note" => Some(SessionSearchMatchSource::SystemNote),
        "transcript" => Some(SessionSearchMatchSource::Transcript),
        "artifact" => Some(SessionSearchMatchSource::Artifact),
        "archive_summary" => Some(SessionSearchMatchSource::ArchiveSummary),
        "archive_transcript" => Some(SessionSearchMatchSource::ArchiveTranscript),
        _ => None,
    }
}

fn session_search_match_source_priority(source: SessionSearchMatchSource) -> u8 {
    match source {
        SessionSearchMatchSource::Title => 0,
        SessionSearchMatchSource::ArchiveSummary => 1,
        SessionSearchMatchSource::ArchiveTranscript => 2,
        SessionSearchMatchSource::Summary => 3,
        SessionSearchMatchSource::Plan => 4,
        SessionSearchMatchSource::SystemNote => 5,
        SessionSearchMatchSource::Transcript => 6,
        SessionSearchMatchSource::Artifact => 7,
    }
}

fn plan_search_text(snapshot: &PlanSnapshot) -> String {
    let mut lines = Vec::new();
    if let Some(goal) = snapshot
        .goal
        .as_ref()
        .map(|goal| goal.trim())
        .filter(|goal| !goal.is_empty())
    {
        lines.push(goal.to_string());
    }

    for item in &snapshot.items {
        lines.push(format!("{} {}", item.id, item.content));
        if !item.depends_on.is_empty() {
            lines.push(format!("depends_on: {}", item.depends_on.join(", ")));
        }
        if let Some(blocked_reason) = item
            .blocked_reason
            .as_deref()
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
        {
            lines.push(format!("blocked_reason: {blocked_reason}"));
        }
        for note in item
            .notes
            .iter()
            .map(String::as_str)
            .map(str::trim)
            .filter(|note| !note.is_empty())
        {
            lines.push(note.to_string());
        }
    }

    lines.join("\n")
}

fn resolve_agent_profile_id_by_identifier(
    store: &PersistenceStore,
    identifier: Option<&str>,
) -> Result<Option<String>, ExecutionError> {
    let Some(identifier) = identifier.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if let Some(agent) = store
        .get_agent_profile(identifier)
        .map_err(ExecutionError::Store)?
    {
        return Ok(Some(agent.id));
    }
    let found = store
        .list_agent_profiles()
        .map_err(ExecutionError::Store)?
        .into_iter()
        .find(|record| record.name.eq_ignore_ascii_case(identifier))
        .ok_or_else(|| invalid_memory_tool(format!("agent {identifier} not found")))?;
    Ok(Some(found.id))
}

fn normalized_pagination(
    total: usize,
    offset: Option<usize>,
    limit: Option<usize>,
    default_limit: usize,
    max_limit: usize,
) -> (usize, usize, Option<usize>) {
    let offset = offset.unwrap_or(0).min(total);
    let limit = limit.unwrap_or(default_limit).clamp(1, max_limit);
    let next_offset = if offset.saturating_add(limit) < total {
        Some(offset + limit)
    } else {
        None
    };
    (offset, limit, next_offset)
}

fn paginate_messages(
    messages: Vec<SessionReadMessageOutput>,
    cursor: Option<usize>,
    max_items: Option<usize>,
    max_bytes: Option<usize>,
    runtime_limits: &agent_persistence::RuntimeLimitsConfig,
) -> (Vec<SessionReadMessageOutput>, usize, Option<usize>, bool) {
    let total = messages.len();
    let start = cursor.unwrap_or(0).min(total);
    let item_limit = max_items
        .unwrap_or(runtime_limits.session_read_default_max_items)
        .clamp(1, runtime_limits.session_read_max_items);
    let byte_limit = max_bytes
        .unwrap_or(runtime_limits.session_read_default_max_bytes)
        .clamp(1, runtime_limits.session_read_max_bytes);
    let mut page = Vec::new();
    let mut consumed_bytes = 0usize;
    let mut next_cursor = None;
    let mut truncated = false;

    for (index, message) in messages.into_iter().enumerate().skip(start) {
        if page.len() >= item_limit {
            next_cursor = Some(index);
            truncated = true;
            break;
        }
        let remaining_bytes = byte_limit.saturating_sub(consumed_bytes);
        if remaining_bytes == 0 {
            next_cursor = Some(index);
            truncated = true;
            break;
        }
        let message_bytes = message.content.len();
        if message_bytes > remaining_bytes {
            let mut message = message;
            message.content = truncate_utf8(message.content.as_str(), remaining_bytes);
            page.push(message);
            next_cursor = Some(index + 1);
            truncated = true;
            break;
        }
        consumed_bytes += message_bytes;
        page.push(message);
    }

    if next_cursor.is_none() && start + page.len() < total {
        next_cursor = Some(start + page.len());
        truncated = true;
    }

    (page, start, next_cursor, truncated)
}

fn paginate_artifacts(
    artifacts: Vec<SessionReadArtifactOutput>,
    cursor: Option<usize>,
    max_items: Option<usize>,
    max_bytes: Option<usize>,
    runtime_limits: &agent_persistence::RuntimeLimitsConfig,
) -> (Vec<SessionReadArtifactOutput>, usize, Option<usize>, bool) {
    let total = artifacts.len();
    let start = cursor.unwrap_or(0).min(total);
    let item_limit = max_items
        .unwrap_or(runtime_limits.session_read_default_max_items)
        .clamp(1, runtime_limits.session_read_max_items);
    let byte_limit = max_bytes
        .unwrap_or(runtime_limits.session_read_default_max_bytes)
        .clamp(1, runtime_limits.session_read_max_bytes);
    let mut page = Vec::new();
    let mut consumed_bytes = 0usize;
    let mut next_cursor = None;
    let mut truncated = false;

    for (index, artifact) in artifacts.into_iter().enumerate().skip(start) {
        if page.len() >= item_limit {
            next_cursor = Some(index);
            truncated = true;
            break;
        }
        let preview_bytes = artifact.path.len()
            + artifact.label.as_deref().unwrap_or_default().len()
            + artifact.summary.as_deref().unwrap_or_default().len();
        if consumed_bytes + preview_bytes > byte_limit && !page.is_empty() {
            next_cursor = Some(index);
            truncated = true;
            break;
        }
        consumed_bytes = consumed_bytes.saturating_add(preview_bytes);
        page.push(artifact);
    }

    if next_cursor.is_none() && start + page.len() < total {
        next_cursor = Some(start + page.len());
        truncated = true;
    }

    (page, start, next_cursor, truncated)
}

fn summary_output_from_record(record: ContextSummaryRecord) -> SessionReadSummaryOutput {
    SessionReadSummaryOutput {
        summary_text: record.summary_text,
        covered_message_count: u32::try_from(record.covered_message_count).unwrap_or(0),
        summary_token_estimate: u32::try_from(record.summary_token_estimate).unwrap_or(0),
        updated_at: record.updated_at,
    }
}

fn summary_output_from_archive(summary: ArchivedSummary) -> SessionReadSummaryOutput {
    SessionReadSummaryOutput {
        summary_text: summary.summary_text,
        covered_message_count: summary.covered_message_count,
        summary_token_estimate: summary.summary_token_estimate,
        updated_at: summary.updated_at,
    }
}

fn message_output_from_record(
    record: TranscriptRecord,
    mode: SessionReadMode,
    timeline_preview_chars: usize,
) -> SessionReadMessageOutput {
    SessionReadMessageOutput {
        id: record.id,
        run_id: record.run_id,
        role: record.kind,
        created_at: record.created_at,
        content: render_message_content(record.content.as_str(), mode, timeline_preview_chars),
    }
}

fn message_output_from_archive(
    entry: ArchivedTranscriptEntry,
    mode: SessionReadMode,
    timeline_preview_chars: usize,
) -> SessionReadMessageOutput {
    SessionReadMessageOutput {
        id: entry.id,
        run_id: entry.run_id,
        role: entry.kind,
        created_at: entry.created_at,
        content: render_message_content(entry.content.as_str(), mode, timeline_preview_chars),
    }
}

fn artifact_output_from_record(record: ArtifactRecord) -> SessionReadArtifactOutput {
    SessionReadArtifactOutput {
        artifact_id: record.id,
        kind: record.kind,
        path: record.path.display().to_string(),
        byte_len: record.bytes.len() as u64,
        created_at: record.created_at,
        label: artifact_metadata_string(record.metadata_json.as_str(), "label"),
        summary: artifact_metadata_string(record.metadata_json.as_str(), "summary"),
    }
}

fn artifact_output_from_archive(entry: ArchivedArtifactEntry) -> SessionReadArtifactOutput {
    SessionReadArtifactOutput {
        artifact_id: entry.artifact_id,
        kind: entry.kind,
        path: entry.relative_path,
        byte_len: entry.byte_len,
        created_at: entry.created_at,
        label: None,
        summary: None,
    }
}

fn artifact_metadata_string(metadata_json: &str, key: &str) -> Option<String> {
    serde_json::from_str::<Value>(metadata_json)
        .ok()
        .and_then(|value| value.get(key).and_then(Value::as_str).map(str::to_string))
}

fn render_message_content(
    content: &str,
    mode: SessionReadMode,
    timeline_preview_chars: usize,
) -> String {
    match mode {
        SessionReadMode::Timeline => truncate_chars(content.trim(), timeline_preview_chars),
        SessionReadMode::Transcript | SessionReadMode::Summary | SessionReadMode::Artifacts => {
            content.to_string()
        }
    }
}

fn excerpt_for_query(text: &str, query: &str, query_lower: &str) -> Option<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let start = find_query_start(trimmed, query, query_lower)?;
    let query_chars = query.chars().count().max(1);
    let start_chars = byte_to_char_index(trimmed, start);
    let excerpt_start = start_chars.saturating_sub(40);
    let excerpt_end = (start_chars + query_chars + 80).min(trimmed.chars().count());
    Some(extract_char_range(trimmed, excerpt_start, excerpt_end))
}

fn find_query_start(text: &str, query: &str, query_lower: &str) -> Option<usize> {
    if let Some(index) = text.find(query) {
        return Some(index);
    }
    let lowered_text = text.to_lowercase();
    let lowered_index = lowered_text.find(query_lower)?;
    let char_offset = lowered_text[..lowered_index].chars().count();
    Some(char_to_byte_index(text, char_offset))
}

fn byte_to_char_index(text: &str, byte_index: usize) -> usize {
    text[..byte_index.min(text.len())].chars().count()
}

fn char_to_byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn extract_char_range(text: &str, start: usize, end: usize) -> String {
    let start_byte = char_to_byte_index(text, start);
    let end_byte = char_to_byte_index(text, end);
    text[start_byte..end_byte].to_string()
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let end = char_to_byte_index(text, max_chars.saturating_sub(1));
    format!("{}…", &text[..end])
}

fn truncate_utf8(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let mut end = max_bytes.min(text.len());
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

fn invalid_memory_tool(reason: String) -> ExecutionError {
    ExecutionError::Tool(ToolError::InvalidMemoryTool { reason })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bootstrap::build_from_config;
    use agent_persistence::{
        AppConfig, ArtifactRecord, ArtifactRepository, ContextSummaryRecord, PersistenceScaffold,
        PlanRecord, PlanRepository, SessionRecord, SessionRetentionRecord, SessionSearchRepository,
    };
    use agent_runtime::permission::PermissionConfig;
    use agent_runtime::plan::{PlanItem, PlanItemStatus, PlanSnapshot};
    use agent_runtime::session::SessionSettings;
    use agent_runtime::tool::{
        KnowledgeReadInput, KnowledgeReadMode, KnowledgeRoot, KnowledgeSearchInput,
        KnowledgeSourceKind, SharedProcessRegistry,
    };
    use agent_runtime::workspace::WorkspaceRef;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn session_search_finds_summary_and_transcript_matches() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_from_config(AppConfig {
            data_dir: temp.path().join("state-root"),
            ..AppConfig::default()
        })
        .expect("build app");
        let store = PersistenceStore::open(&app.persistence).expect("open store");
        let service = ExecutionService::default();

        store
            .put_session(&SessionRecord {
                id: "session-memory-summary".to_string(),
                title: "PostgreSQL setup".to_string(),
                prompt_override: None,
                settings_json: serde_json::to_string(&SessionSettings::default()).unwrap(),
                workspace_root: app.runtime.workspace.root.display().to_string(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: 10,
                updated_at: 20,
            })
            .expect("put summary session");
        store
            .put_context_summary(&ContextSummaryRecord {
                session_id: "session-memory-summary".to_string(),
                summary_text: "Configured offline ADQM prerequisites.".to_string(),
                covered_message_count: 4,
                summary_token_estimate: 12,
                updated_at: 20,
            })
            .expect("put summary");
        store
            .put_session_retention(&SessionRetentionRecord {
                session_id: "session-memory-summary".to_string(),
                tier: "warm".to_string(),
                last_accessed_at: 20,
                archived_at: None,
                archive_manifest_path: None,
                archive_version: None,
                updated_at: 20,
            })
            .expect("put retention");

        store
            .put_session(&SessionRecord {
                id: "session-memory-transcript".to_string(),
                title: "Install ADET".to_string(),
                prompt_override: None,
                settings_json: serde_json::to_string(&SessionSettings::default()).unwrap(),
                workspace_root: app.runtime.workspace.root.display().to_string(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: 30,
                updated_at: 40,
            })
            .expect("put transcript session");
        store
            .put_transcript(&TranscriptRecord {
                id: "session-memory-transcript-1".to_string(),
                session_id: "session-memory-transcript".to_string(),
                run_id: None,
                kind: "assistant".to_string(),
                content: "Need to unpack adet.txz for the offline installer.".to_string(),
                created_at: 41,
            })
            .expect("put transcript");

        let summary_match = service
            .search_sessions(
                &store,
                &SessionSearchInput {
                    query: "offline ADQM".to_string(),
                    limit: Some(10),
                    offset: Some(0),
                    tiers: None,
                    agent_identifier: None,
                    updated_after: None,
                    updated_before: None,
                },
            )
            .expect("search summary");
        assert_eq!(summary_match.results.len(), 1);
        assert_eq!(
            summary_match.results[0].session_id,
            "session-memory-summary"
        );
        assert_eq!(
            summary_match.results[0].match_source,
            SessionSearchMatchSource::Summary
        );
        assert_eq!(summary_match.results[0].tier, SessionRetentionTier::Warm);

        let transcript_match = service
            .search_sessions(
                &store,
                &SessionSearchInput {
                    query: "adet.txz".to_string(),
                    limit: Some(10),
                    offset: Some(0),
                    tiers: None,
                    agent_identifier: None,
                    updated_after: None,
                    updated_before: None,
                },
            )
            .expect("search transcript");
        assert_eq!(transcript_match.results.len(), 1);
        assert_eq!(
            transcript_match.results[0].session_id,
            "session-memory-transcript"
        );
        assert_eq!(
            transcript_match.results[0].match_source,
            SessionSearchMatchSource::Transcript
        );
    }

    #[test]
    fn session_search_indexes_plan_and_system_note_matches() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_from_config(AppConfig {
            data_dir: temp.path().join("state-root"),
            ..AppConfig::default()
        })
        .expect("build app");
        let store = PersistenceStore::open(&app.persistence).expect("open store");
        let service = ExecutionService::default();

        store
            .put_session(&SessionRecord {
                id: "session-memory-plan".to_string(),
                title: "Resume offline install".to_string(),
                prompt_override: None,
                settings_json: serde_json::to_string(&SessionSettings::default()).unwrap(),
                workspace_root: app.runtime.workspace.root.display().to_string(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: 50,
                updated_at: 60,
            })
            .expect("put session");
        store
            .put_plan(
                &PlanRecord::try_from(&PlanSnapshot {
                    session_id: "session-memory-plan".to_string(),
                    goal: Some("Complete offline ET install".to_string()),
                    items: vec![PlanItem {
                        id: "adet-unpack".to_string(),
                        content: "Unpack adet.txz into /tmp/adet before resuming.".to_string(),
                        status: PlanItemStatus::Pending,
                        depends_on: Vec::new(),
                        notes: vec!["offline payload already downloaded".to_string()],
                        blocked_reason: None,
                        parent_task_id: None,
                    }],
                    updated_at: 61,
                })
                .expect("plan record"),
            )
            .expect("put plan");
        store
            .put_transcript(&TranscriptRecord {
                id: "session-memory-system-1".to_string(),
                session_id: "session-memory-plan".to_string(),
                run_id: None,
                kind: "system".to_string(),
                content: "Verify /tmp/adet exists before the next scheduled resume.".to_string(),
                created_at: 62,
            })
            .expect("put system transcript");

        let plan_match = service
            .search_sessions(
                &store,
                &SessionSearchInput {
                    query: "offline payload already downloaded".to_string(),
                    limit: Some(10),
                    offset: Some(0),
                    tiers: None,
                    agent_identifier: None,
                    updated_after: None,
                    updated_before: None,
                },
            )
            .expect("search plan");
        assert_eq!(plan_match.results.len(), 1);
        assert_eq!(plan_match.results[0].session_id, "session-memory-plan");
        assert_eq!(
            plan_match.results[0].match_source,
            SessionSearchMatchSource::Plan
        );

        let system_match = service
            .search_sessions(
                &store,
                &SessionSearchInput {
                    query: "next scheduled resume".to_string(),
                    limit: Some(10),
                    offset: Some(0),
                    tiers: None,
                    agent_identifier: None,
                    updated_after: None,
                    updated_before: None,
                },
            )
            .expect("search system note");
        assert_eq!(system_match.results.len(), 1);
        assert_eq!(system_match.results[0].session_id, "session-memory-plan");
        assert_eq!(
            system_match.results[0].match_source,
            SessionSearchMatchSource::SystemNote
        );
        let docs = store
            .list_session_search_docs()
            .expect("list session search docs");
        assert!(
            docs.iter()
                .any(|doc| doc.session_id == "session-memory-plan" && doc.source_kind == "plan")
        );
        assert!(
            docs.iter()
                .any(|doc| doc.session_id == "session-memory-plan"
                    && doc.source_kind == "system_note")
        );
    }

    #[test]
    fn session_search_indexes_artifact_metadata_matches() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_from_config(AppConfig {
            data_dir: temp.path().join("state-root"),
            ..AppConfig::default()
        })
        .expect("build app");
        let store = PersistenceStore::open(&app.persistence).expect("open store");
        let service = ExecutionService::default();

        store
            .put_session(&SessionRecord {
                id: "session-memory-artifact".to_string(),
                title: "Artifact session".to_string(),
                prompt_override: None,
                settings_json: serde_json::to_string(&SessionSettings::default()).unwrap(),
                workspace_root: app.runtime.workspace.root.display().to_string(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: 70,
                updated_at: 80,
            })
            .expect("put session");
        store
            .put_artifact(&ArtifactRecord {
                id: "artifact-memory-1".to_string(),
                session_id: "session-memory-artifact".to_string(),
                kind: "report".to_string(),
                metadata_json: serde_json::json!({
                    "label": "ADQM offline report",
                    "summary": "Contains adet.txz unpack checklist"
                })
                .to_string(),
                path: PathBuf::from("artifacts/artifact-memory-1.bin"),
                bytes: b"report bytes".to_vec(),
                created_at: 81,
            })
            .expect("put artifact");

        let artifact_match = service
            .search_sessions(
                &store,
                &SessionSearchInput {
                    query: "unpack checklist".to_string(),
                    limit: Some(10),
                    offset: Some(0),
                    tiers: None,
                    agent_identifier: None,
                    updated_after: None,
                    updated_before: None,
                },
            )
            .expect("search artifact");
        assert_eq!(artifact_match.results.len(), 1);
        assert_eq!(
            artifact_match.results[0].session_id,
            "session-memory-artifact"
        );
        assert_eq!(
            artifact_match.results[0].match_source,
            SessionSearchMatchSource::Artifact
        );
        assert!(
            store
                .list_session_search_docs()
                .expect("list session search docs")
                .iter()
                .any(|doc| doc.session_id == "session-memory-artifact"
                    && doc.source_kind == "artifact")
        );
    }

    #[test]
    fn session_read_prefers_archive_for_cold_transcripts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_from_config(AppConfig {
            data_dir: temp.path().join("state-root"),
            ..AppConfig::default()
        })
        .expect("build app");
        let store = PersistenceStore::open(&app.persistence).expect("open store");
        let service = ExecutionService::default();

        store
            .put_session(&SessionRecord {
                id: "session-cold".to_string(),
                title: "Cold Session".to_string(),
                prompt_override: None,
                settings_json: serde_json::to_string(&SessionSettings::default()).unwrap(),
                workspace_root: app.runtime.workspace.root.display().to_string(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: 50,
                updated_at: 60,
            })
            .expect("put cold session");
        store
            .put_transcript(&TranscriptRecord {
                id: "session-cold-1".to_string(),
                session_id: "session-cold".to_string(),
                run_id: None,
                kind: "assistant".to_string(),
                content: "This content should be served from the archive.".to_string(),
                created_at: 61,
            })
            .expect("put cold transcript");
        store
            .put_context_summary(&ContextSummaryRecord {
                session_id: "session-cold".to_string(),
                summary_text: "Archived summary".to_string(),
                covered_message_count: 1,
                summary_token_estimate: 4,
                updated_at: 62,
            })
            .expect("put cold summary");
        store
            .archive_session_bundle("session-cold", 70)
            .expect("archive bundle");
        store
            .put_session_retention(&SessionRetentionRecord {
                session_id: "session-cold".to_string(),
                tier: "cold".to_string(),
                last_accessed_at: 70,
                archived_at: Some(70),
                archive_manifest_path: Some(
                    "archives/sessions/session-cold/manifest.json".to_string(),
                ),
                archive_version: Some(1),
                updated_at: 70,
            })
            .expect("put cold retention");

        let read = service
            .read_session(
                &store,
                &SessionReadInput {
                    session_id: "session-cold".to_string(),
                    mode: Some(SessionReadMode::Transcript),
                    cursor: None,
                    max_items: Some(10),
                    max_bytes: Some(1024),
                    include_tools: Some(true),
                },
            )
            .expect("read cold session");

        assert!(read.from_archive);
        assert_eq!(read.tier, SessionRetentionTier::Cold);
        assert_eq!(read.messages.len(), 1);
        assert_eq!(
            read.messages[0].content,
            "This content should be served from the archive."
        );
    }

    #[test]
    fn knowledge_search_scans_canonical_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("docs")).expect("create docs");
        fs::create_dir_all(workspace.join("notes")).expect("create notes");
        fs::write(
            workspace.join("docs/architecture.md"),
            "# Architecture\nOffline ADQM memory architecture.\n",
        )
        .expect("write docs");
        fs::write(
            workspace.join("notes/2026-04-22.md"),
            "Reminder: inspect archive retrieval.\n",
        )
        .expect("write note");

        let scaffold = PersistenceScaffold::from_config(AppConfig {
            data_dir: temp.path().join("state-root"),
            ..AppConfig::default()
        });
        let store = PersistenceStore::open(&scaffold).expect("open store");
        let service = ExecutionService::new(
            PermissionConfig::default(),
            WorkspaceRef::new(&workspace),
            SharedProcessRegistry::default(),
            crate::mcp::SharedMcpRegistry::default(),
            ExecutionServiceConfig::default(),
        );

        let search = service
            .search_knowledge(
                &store,
                &KnowledgeSearchInput {
                    query: "Offline ADQM".to_string(),
                    limit: Some(10),
                    offset: Some(0),
                    kinds: Some(vec![KnowledgeSourceKind::ProjectDoc]),
                    roots: Some(vec![KnowledgeRoot::Docs]),
                },
            )
            .expect("knowledge search");

        assert_eq!(search.results.len(), 1);
        assert_eq!(search.results[0].path, "docs/architecture.md");
        assert_eq!(search.results[0].kind, KnowledgeSourceKind::ProjectDoc);
    }

    #[test]
    fn knowledge_read_returns_bounded_excerpt() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("projects/demo")).expect("create projects");
        fs::write(
            workspace.join("projects/demo/README.md"),
            "line one\nline two\nline three\nline four\n",
        )
        .expect("write project doc");

        let scaffold = PersistenceScaffold::from_config(AppConfig {
            data_dir: temp.path().join("state-root"),
            ..AppConfig::default()
        });
        let store = PersistenceStore::open(&scaffold).expect("open store");
        let service = ExecutionService::new(
            PermissionConfig::default(),
            WorkspaceRef::new(&workspace),
            SharedProcessRegistry::default(),
            crate::mcp::SharedMcpRegistry::default(),
            ExecutionServiceConfig::default(),
        );

        let read = service
            .read_knowledge(
                &store,
                &KnowledgeReadInput {
                    path: "projects/demo/README.md".to_string(),
                    mode: Some(KnowledgeReadMode::Excerpt),
                    cursor: Some(1),
                    max_bytes: Some(128),
                    max_lines: Some(2),
                },
            )
            .expect("knowledge read");

        assert_eq!(read.path, "projects/demo/README.md");
        assert_eq!(read.kind, KnowledgeSourceKind::ProjectDoc);
        assert_eq!(read.start_line, 2);
        assert_eq!(read.end_line, 3);
        assert_eq!(read.text, "line two\nline three");
        assert!(read.next_cursor.is_some());
    }

    #[test]
    fn memory_maintenance_warms_idle_active_sessions() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_from_config(AppConfig {
            data_dir: temp.path().join("state-root"),
            ..AppConfig::default()
        })
        .expect("build app");
        let store = PersistenceStore::open(&app.persistence).expect("open store");
        let service = ExecutionService::default();

        store
            .put_session(&SessionRecord {
                id: "session-idle-active".to_string(),
                title: "Idle Active".to_string(),
                prompt_override: None,
                settings_json: serde_json::to_string(&SessionSettings::default()).unwrap(),
                workspace_root: app.runtime.workspace.root.display().to_string(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: 10,
                updated_at: 10,
            })
            .expect("put session");
        store
            .put_session_retention(&SessionRetentionRecord {
                session_id: "session-idle-active".to_string(),
                tier: "active".to_string(),
                last_accessed_at: 10,
                archived_at: None,
                archive_manifest_path: None,
                archive_version: None,
                updated_at: 10,
            })
            .expect("put retention");

        let report = service.maintain_memory(
            &store,
            10 + service.config.runtime_limits.session_warm_idle_seconds as i64 + 1,
        );

        assert!(report.is_ok(), "maintenance should succeed: {report:?}");
        let retention = store
            .get_session_retention("session-idle-active")
            .expect("get retention")
            .expect("retention should exist");
        assert_eq!(retention.tier, "warm");
    }

    #[test]
    fn archive_session_to_cold_writes_bundle_and_updates_retention() {
        let temp = tempfile::tempdir().expect("tempdir");
        let app = build_from_config(AppConfig {
            data_dir: temp.path().join("state-root"),
            ..AppConfig::default()
        })
        .expect("build app");
        let store = PersistenceStore::open(&app.persistence).expect("open store");
        let service = ExecutionService::default();

        store
            .put_session(&SessionRecord {
                id: "session-to-archive".to_string(),
                title: "Archive me".to_string(),
                prompt_override: None,
                settings_json: serde_json::to_string(&SessionSettings::default()).unwrap(),
                workspace_root: app.runtime.workspace.root.display().to_string(),
                agent_profile_id: "default".to_string(),
                active_mission_id: None,
                parent_session_id: None,
                parent_job_id: None,
                delegation_label: None,
                created_at: 50,
                updated_at: 60,
            })
            .expect("put session");
        store
            .put_transcript(&TranscriptRecord {
                id: "archive-transcript-1".to_string(),
                session_id: "session-to-archive".to_string(),
                run_id: None,
                kind: "assistant".to_string(),
                content: "Archive this transcript.".to_string(),
                created_at: 61,
            })
            .expect("put transcript");

        let archived = service.archive_session_to_cold(&store, "session-to-archive", 70);

        assert!(archived.is_ok(), "archive should succeed: {archived:?}");
        let archived = archived.expect("archive result");
        assert_eq!(archived.tier, SessionRetentionTier::Cold);
        assert_eq!(archived.archived_at, Some(70));
        assert_eq!(
            archived.archive_manifest_path.as_deref(),
            Some("archives/sessions/session-to-archive/manifest.json")
        );
        let manifest = store
            .read_session_archive_manifest("session-to-archive")
            .expect("read manifest")
            .expect("manifest should exist");
        assert_eq!(manifest.session_id, "session-to-archive");
    }

    #[test]
    fn knowledge_search_prunes_stale_sources_after_file_removal() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(workspace.join("docs")).expect("create docs");
        let path = workspace.join("docs/stale.md");
        fs::write(&path, "stale indexed content\n").expect("write stale doc");

        let scaffold = PersistenceScaffold::from_config(AppConfig {
            data_dir: temp.path().join("state-root"),
            ..AppConfig::default()
        });
        let store = PersistenceStore::open(&scaffold).expect("open store");
        let service = ExecutionService::new(
            PermissionConfig::default(),
            WorkspaceRef::new(&workspace),
            SharedProcessRegistry::default(),
            crate::mcp::SharedMcpRegistry::default(),
            ExecutionServiceConfig::default(),
        );

        let first = service
            .search_knowledge(
                &store,
                &KnowledgeSearchInput {
                    query: "stale indexed".to_string(),
                    limit: Some(10),
                    offset: Some(0),
                    kinds: None,
                    roots: None,
                },
            )
            .expect("initial search");
        assert_eq!(first.results.len(), 1);

        fs::remove_file(&path).expect("remove stale doc");

        let second = service
            .search_knowledge(
                &store,
                &KnowledgeSearchInput {
                    query: "stale indexed".to_string(),
                    limit: Some(10),
                    offset: Some(0),
                    kinds: None,
                    roots: None,
                },
            )
            .expect("second search");
        assert!(second.results.is_empty());
    }
}
