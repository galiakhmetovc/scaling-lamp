use super::provider_ids::sanitize_identifier;
use super::provider_text::utf8_byte_page;
use super::*;
use crate::prompting;
use agent_persistence::{ContextOffloadRepository, PersistenceStore};
use agent_runtime::context::{
    ContextOffloadPayload, ContextOffloadRef, ContextOffloadSnapshot, approximate_token_count,
};
use agent_runtime::tool::{
    ArtifactPinOutput, ArtifactReadInput, ArtifactReadOutput, ArtifactSearchOutput,
    ArtifactSearchResult, ToolCall, ToolError, ToolOutput,
};

const MAX_CONTEXT_OFFLOAD_REFS: usize = 16;
const INLINE_TOOL_OUTPUT_TOKEN_LIMIT: u32 = 512;
const INLINE_FIND_IN_FILES_PREVIEW_LIMIT: usize = 6;
const DEFAULT_ARTIFACT_READ_MAX_BYTES: usize = 8 * 1024;
const MAX_ARTIFACT_READ_MAX_BYTES: usize = 32 * 1024;

type OffloadableToolOutput = (String, String, Vec<u8>, String);

impl ExecutionService {
    pub(super) fn is_stale_context_offload_payload_error(
        error: &agent_persistence::StoreError,
    ) -> bool {
        match error {
            agent_persistence::StoreError::MissingPayload { .. }
            | agent_persistence::StoreError::IntegrityMismatch { .. } => true,
            agent_persistence::StoreError::Io { source, .. } => {
                source.kind() == std::io::ErrorKind::NotFound
            }
            _ => false,
        }
    }

    pub(super) fn load_context_offload_payload_for_refresh(
        &self,
        store: &PersistenceStore,
        artifact_id: &str,
    ) -> Result<Option<ContextOffloadPayload>, ExecutionError> {
        match store.get_context_offload_payload(artifact_id) {
            Ok(payload) => Ok(payload),
            Err(source) if Self::is_stale_context_offload_payload_error(&source) => Ok(None),
            Err(source) => Err(ExecutionError::Store(source)),
        }
    }

    pub(super) fn load_context_offload_payload_for_tool(
        &self,
        store: &PersistenceStore,
        artifact_id: &str,
    ) -> Result<ContextOffloadPayload, ExecutionError> {
        match store.get_context_offload_payload(artifact_id) {
            Ok(Some(payload)) => Ok(payload),
            Ok(None)
            | Err(agent_persistence::StoreError::MissingPayload { .. })
            | Err(agent_persistence::StoreError::IntegrityMismatch { .. }) => {
                Err(ExecutionError::Tool(ToolError::InvalidArtifactTool {
                    reason: format!(
                        "artifact {} is missing from context offload storage",
                        artifact_id
                    ),
                }))
            }
            Err(source) => Err(ExecutionError::Store(source)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare_model_tool_output(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        tool_call_id: &str,
        parsed: &ToolCall,
        output: &ToolOutput,
        inline_output: String,
        now: i64,
    ) -> Result<String, ExecutionError> {
        let Some((label, summary, payload_bytes, compact_output)) =
            self.offloadable_tool_output(parsed, output)?
        else {
            return Ok(inline_output);
        };
        let payload_text = String::from_utf8_lossy(&payload_bytes).to_string();
        let token_estimate = approximate_token_count(&payload_text);

        if token_estimate <= INLINE_TOOL_OUTPUT_TOKEN_LIMIT {
            return Ok(inline_output);
        }

        let mut snapshot = store
            .get_context_offload(session_id)
            .map_err(ExecutionError::Store)?
            .map(ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?
            .unwrap_or_else(|| ContextOffloadSnapshot {
                session_id: session_id.to_string(),
                refs: Vec::new(),
                updated_at: 0,
            });

        let normalized_id = sanitize_identifier(tool_call_id);
        let artifact_id = format!("artifact-tool-offload-{session_id}-{normalized_id}");
        let ref_id = format!("tool-offload-{normalized_id}");
        let current_ref = ContextOffloadRef {
            id: ref_id.clone(),
            label,
            summary,
            artifact_id: artifact_id.clone(),
            token_estimate,
            message_count: 1,
            created_at: now,
            pinned: false,
            explicit_read_count: 0,
        };
        snapshot.refs.push(current_ref.clone());
        snapshot.refs.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        snapshot.refs.truncate(MAX_CONTEXT_OFFLOAD_REFS);
        snapshot.updated_at = now;

        let mut retained_refs = Vec::with_capacity(snapshot.refs.len());
        let mut payloads = Vec::with_capacity(snapshot.refs.len());
        for reference in &snapshot.refs {
            if reference.artifact_id == artifact_id {
                retained_refs.push(reference.clone());
                payloads.push(ContextOffloadPayload {
                    artifact_id: artifact_id.clone(),
                    bytes: payload_bytes.clone(),
                });
                continue;
            }

            if let Some(payload) = self
                .load_context_offload_payload_for_refresh(store, reference.artifact_id.as_str())?
            {
                retained_refs.push(reference.clone());
                payloads.push(payload);
            }
        }
        snapshot.refs = retained_refs;

        let snapshot_record = agent_persistence::ContextOffloadRecord::try_from(&snapshot)
            .map_err(ExecutionError::RecordConversion)?;
        match store.put_context_offload(&snapshot_record, &payloads) {
            Ok(()) => {}
            Err(source) if Self::is_stale_context_offload_payload_error(&source) => {
                let fallback_snapshot = ContextOffloadSnapshot {
                    session_id: session_id.to_string(),
                    refs: vec![current_ref],
                    updated_at: now,
                };
                let fallback_payloads = vec![ContextOffloadPayload {
                    artifact_id: artifact_id.clone(),
                    bytes: payload_bytes,
                }];
                store
                    .put_context_offload(
                        &agent_persistence::ContextOffloadRecord::try_from(&fallback_snapshot)
                            .map_err(ExecutionError::RecordConversion)?,
                        &fallback_payloads,
                    )
                    .map_err(ExecutionError::Store)?;
            }
            Err(source) => return Err(ExecutionError::Store(source)),
        }

        Ok(compact_output
            .replace("__ARTIFACT_ID__", artifact_id.as_str())
            .replace("__REF_ID__", ref_id.as_str()))
    }

    fn offloadable_tool_output(
        &self,
        _parsed: &ToolCall,
        output: &ToolOutput,
    ) -> Result<Option<OffloadableToolOutput>, ExecutionError> {
        match output {
            ToolOutput::FsReadText(result) => {
                let payload = output.model_output().into_bytes();
                let preview = prompting::preview_text(result.content.as_str(), 240);
                Ok(Some((
                    format!("fs_read_text {}", result.path),
                    format!("Large file read from {}", result.path),
                    payload,
                    serde_json::json!({
                        "tool": "fs_read_text",
                        "path": result.path,
                        "offloaded": true,
                        "artifact_id": "__ARTIFACT_ID__",
                        "ref_id": "__REF_ID__",
                        "summary": format!("Large file read from {}", result.path),
                        "preview": preview,
                    })
                    .to_string(),
                )))
            }
            ToolOutput::FsReadLines(result) => {
                let payload = output.model_output().into_bytes();
                let preview = prompting::preview_text(result.content.as_str(), 240);
                Ok(Some((
                    format!("fs_read_lines {}", result.path),
                    format!(
                        "Large line-range read from {} ({}-{})",
                        result.path, result.start_line, result.end_line
                    ),
                    payload,
                    serde_json::json!({
                        "tool": "fs_read_lines",
                        "path": result.path,
                        "start_line": result.start_line,
                        "end_line": result.end_line,
                        "total_lines": result.total_lines,
                        "eof": result.eof,
                        "next_start_line": result.next_start_line,
                        "offloaded": true,
                        "artifact_id": "__ARTIFACT_ID__",
                        "ref_id": "__REF_ID__",
                        "summary": format!("Large line-range read from {} ({}-{})", result.path, result.start_line, result.end_line),
                        "preview": preview,
                    })
                    .to_string(),
                )))
            }
            ToolOutput::FsFindInFiles(result) => {
                let payload = output.model_output().into_bytes();
                let preview_matches = result
                    .matches
                    .iter()
                    .take(INLINE_FIND_IN_FILES_PREVIEW_LIMIT)
                    .map(|entry| {
                        serde_json::json!({
                            "path": entry.path,
                            "line_number": entry.line_number,
                            "line": entry.line,
                        })
                    })
                    .collect::<Vec<_>>();
                Ok(Some((
                    "fs_find_in_files workspace search".to_string(),
                    format!("Large multi-file search result with {} matches", result.matches.len()),
                    payload,
                    serde_json::json!({
                        "tool": "fs_find_in_files",
                        "offloaded": true,
                        "artifact_id": "__ARTIFACT_ID__",
                        "ref_id": "__REF_ID__",
                        "summary": format!("Large multi-file search result with {} matches", result.matches.len()),
                        "match_count": result.matches.len(),
                        "preview_matches": preview_matches,
                    })
                    .to_string(),
                )))
            }
            ToolOutput::WebFetch(result) => {
                let payload = output.model_output().into_bytes();
                let preview = prompting::preview_text(result.body.as_str(), 240);
                let summary = if result.extracted_from_html {
                    format!("Large readable web fetch from {}", result.url)
                } else {
                    format!("Large web fetch response from {}", result.url)
                };
                Ok(Some((
                    format!("web_fetch {}", result.url),
                    summary.clone(),
                    payload,
                    serde_json::json!({
                        "tool": "web_fetch",
                        "url": result.url,
                        "status_code": result.status_code,
                        "content_type": result.content_type,
                        "title": result.title,
                        "extracted_from_html": result.extracted_from_html,
                        "offloaded": true,
                        "artifact_id": "__ARTIFACT_ID__",
                        "ref_id": "__REF_ID__",
                        "summary": summary,
                        "preview": preview,
                    })
                    .to_string(),
                )))
            }
            ToolOutput::ProcessResult(result) => {
                let payload = output.model_output().into_bytes();
                let stdout_preview = prompting::preview_text(result.stdout.as_str(), 180);
                let stderr_preview = prompting::preview_text(result.stderr.as_str(), 180);
                Ok(Some((
                    format!("exec_wait {}", result.process_id),
                    format!(
                        "Large process output for {} (exit_code={:?})",
                        result.process_id, result.exit_code
                    ),
                    payload,
                    serde_json::json!({
                        "tool": "process_result",
                        "process_id": result.process_id,
                        "status": format!("{:?}", result.status).to_lowercase(),
                        "exit_code": result.exit_code,
                        "offloaded": true,
                        "artifact_id": "__ARTIFACT_ID__",
                        "ref_id": "__REF_ID__",
                        "summary": format!("Large process output for {} (exit_code={:?})", result.process_id, result.exit_code),
                        "stdout_preview": stdout_preview,
                        "stderr_preview": stderr_preview,
                    })
                    .to_string(),
                )))
            }
            _ => Ok(None),
        }
    }

    pub(super) fn read_context_offload_artifact(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        input: &ArtifactReadInput,
    ) -> Result<ArtifactReadOutput, ExecutionError> {
        let mut snapshot = self.require_context_offload_snapshot(store, session_id)?;
        let reference_index = snapshot
            .refs
            .iter()
            .position(|reference| reference.artifact_id == input.artifact_id)
            .ok_or_else(|| {
                ExecutionError::Tool(ToolError::InvalidArtifactTool {
                    reason: format!(
                        "artifact {} is not referenced by the current session offload snapshot",
                        input.artifact_id
                    ),
                })
            })?;
        let payload = self.load_context_offload_payload_for_tool(store, &input.artifact_id)?;
        snapshot.refs[reference_index].explicit_read_count = snapshot.refs[reference_index]
            .explicit_read_count
            .saturating_add(1);
        let reference = snapshot.refs[reference_index].clone();
        self.persist_context_offload_snapshot_preserving_payloads(
            store,
            &snapshot,
            Some(&input.artifact_id),
        )?;
        let full_content = String::from_utf8_lossy(&payload.bytes).to_string();
        let total_byte_len = full_content.len();
        let (content, offset, next_offset) = utf8_byte_page(
            &full_content,
            input.offset,
            input.max_bytes,
            DEFAULT_ARTIFACT_READ_MAX_BYTES,
            MAX_ARTIFACT_READ_MAX_BYTES,
        );
        let content_byte_len = content.len();

        Ok(ArtifactReadOutput {
            ref_id: reference.id,
            artifact_id: reference.artifact_id,
            label: reference.label,
            summary: reference.summary,
            content,
            offset,
            content_byte_len,
            total_byte_len,
            content_truncated: next_offset.is_some(),
            next_offset,
        })
    }

    pub(super) fn update_context_offload_pin(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        artifact_id: &str,
        pinned: bool,
    ) -> Result<ArtifactPinOutput, ExecutionError> {
        let mut snapshot = self.require_context_offload_snapshot(store, session_id)?;
        let reference_index = snapshot
            .refs
            .iter()
            .position(|reference| reference.artifact_id == artifact_id)
            .ok_or_else(|| {
                ExecutionError::Tool(ToolError::InvalidArtifactTool {
                    reason: format!(
                        "artifact {} is not referenced by the current session offload snapshot",
                        artifact_id
                    ),
                })
            })?;
        snapshot.refs[reference_index].pinned = pinned;
        let reference = snapshot.refs[reference_index].clone();
        self.persist_context_offload_snapshot_preserving_payloads(
            store,
            &snapshot,
            Some(artifact_id),
        )?;
        let pin_status = reference.pin_status().to_string();

        Ok(ArtifactPinOutput {
            ref_id: reference.id,
            artifact_id: reference.artifact_id,
            pinned: reference.pinned,
            explicit_read_count: reference.explicit_read_count,
            pin_status,
        })
    }

    fn persist_context_offload_snapshot_preserving_payloads(
        &self,
        store: &PersistenceStore,
        snapshot: &ContextOffloadSnapshot,
        required_artifact_id: Option<&str>,
    ) -> Result<ContextOffloadSnapshot, ExecutionError> {
        let mut retained_refs = Vec::with_capacity(snapshot.refs.len());
        let mut payloads = Vec::with_capacity(snapshot.refs.len());
        for reference in &snapshot.refs {
            match self
                .load_context_offload_payload_for_refresh(store, reference.artifact_id.as_str())?
            {
                Some(payload) => {
                    retained_refs.push(reference.clone());
                    payloads.push(payload);
                }
                None if required_artifact_id == Some(reference.artifact_id.as_str()) => {
                    return Err(ExecutionError::Tool(ToolError::InvalidArtifactTool {
                        reason: format!(
                            "artifact {} is missing from context offload storage",
                            reference.artifact_id
                        ),
                    }));
                }
                None => {}
            }
        }
        let retained_snapshot = ContextOffloadSnapshot {
            session_id: snapshot.session_id.clone(),
            refs: retained_refs,
            updated_at: snapshot.updated_at,
        };
        store
            .put_context_offload(
                &agent_persistence::ContextOffloadRecord::try_from(&retained_snapshot)
                    .map_err(ExecutionError::RecordConversion)?,
                &payloads,
            )
            .map_err(ExecutionError::Store)?;
        Ok(retained_snapshot)
    }

    pub(super) fn search_context_offload_artifacts(
        &self,
        store: &PersistenceStore,
        session_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<ArtifactSearchOutput, ExecutionError> {
        let snapshot = self.require_context_offload_snapshot(store, session_id)?;
        let query = query.trim();
        if query.is_empty() {
            return Err(ExecutionError::Tool(ToolError::InvalidArtifactTool {
                reason: "artifact_search query must not be empty".to_string(),
            }));
        }
        let normalized_query = query.to_ascii_lowercase();
        let mut results = Vec::new();
        let effective_limit = limit.max(1);

        for reference in snapshot.refs {
            let payload =
                self.load_context_offload_payload_for_tool(store, reference.artifact_id.as_str())?;
            let content = String::from_utf8_lossy(&payload.bytes).to_string();
            let haystack = format!(
                "{}\n{}\n{}\n{}",
                reference.artifact_id, reference.label, reference.summary, content
            )
            .to_ascii_lowercase();
            if !haystack.contains(&normalized_query) {
                continue;
            }

            results.push(ArtifactSearchResult {
                ref_id: reference.id,
                artifact_id: reference.artifact_id,
                label: reference.label,
                summary: reference.summary,
                token_estimate: reference.token_estimate,
                message_count: reference.message_count,
                preview: prompting::preview_text(&content, 240),
            });
            if results.len() >= effective_limit {
                break;
            }
        }

        Ok(ArtifactSearchOutput {
            query: query.to_string(),
            results,
        })
    }

    fn require_context_offload_snapshot(
        &self,
        store: &PersistenceStore,
        session_id: &str,
    ) -> Result<ContextOffloadSnapshot, ExecutionError> {
        store
            .get_context_offload(session_id)
            .map_err(ExecutionError::Store)?
            .map(ContextOffloadSnapshot::try_from)
            .transpose()
            .map_err(ExecutionError::RecordConversion)?
            .filter(|snapshot| !snapshot.is_empty())
            .ok_or_else(|| {
                ExecutionError::Tool(ToolError::InvalidArtifactTool {
                    reason: "the current session has no offloaded context".to_string(),
                })
            })
    }
}
