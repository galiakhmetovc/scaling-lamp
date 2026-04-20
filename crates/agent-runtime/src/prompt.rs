use crate::context::{ContextOffloadSnapshot, ContextSummary};
use crate::plan::PlanSnapshot;
use crate::provider::ProviderMessage;
use crate::session::MessageRole;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHeadFsActivity {
    pub action: String,
    pub target: String,
    pub detail: String,
    pub recorded_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHeadWorkspaceEntry {
    pub path: String,
    pub kind: SessionHeadWorkspaceEntryKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionHeadWorkspaceEntryKind {
    File,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionHead {
    pub session_id: String,
    pub title: String,
    pub message_count: usize,
    pub context_tokens: u32,
    pub compactifications: u32,
    pub summary_covered_message_count: u32,
    pub pending_approval_count: usize,
    pub last_user_preview: Option<String>,
    pub last_assistant_preview: Option<String>,
    pub recent_filesystem_activity: Vec<SessionHeadFsActivity>,
    pub workspace_tree: Vec<SessionHeadWorkspaceEntry>,
    pub workspace_tree_truncated: bool,
}

impl SessionHead {
    pub fn render(&self) -> String {
        let mut lines = vec![
            format!("Session: {}", self.title),
            format!("Session ID: {}", self.session_id),
            format!("Messages: {}", self.message_count),
            format!("Context Tokens: {}", self.context_tokens),
            format!("Compactifications: {}", self.compactifications),
        ];
        if self.summary_covered_message_count > 0 {
            lines.push(format!(
                "Summary Covers: {} messages",
                self.summary_covered_message_count
            ));
        }
        if self.pending_approval_count > 0 {
            lines.push(format!(
                "Pending Approvals: {}",
                self.pending_approval_count
            ));
        }
        if let Some(last_user_preview) = self.last_user_preview.as_deref() {
            lines.push(format!("Last User: {last_user_preview}"));
        }
        if let Some(last_assistant_preview) = self.last_assistant_preview.as_deref() {
            lines.push(format!("Last Assistant: {last_assistant_preview}"));
        }
        if !self.recent_filesystem_activity.is_empty() {
            lines.push("Recent Filesystem Activity:".to_string());
            lines.extend(
                self.recent_filesystem_activity
                    .iter()
                    .map(|activity| format!("- {} {}", activity.action, activity.target)),
            );
        }
        if !self.workspace_tree.is_empty() {
            lines.push("Workspace Tree:".to_string());
            lines.extend(self.workspace_tree.iter().map(|entry| {
                let suffix = match entry.kind {
                    SessionHeadWorkspaceEntryKind::File => "",
                    SessionHeadWorkspaceEntryKind::Directory => "/",
                };
                format!("- {}{}", entry.path, suffix)
            }));
            if self.workspace_tree_truncated {
                lines.push("- ...".to_string());
            }
        }
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptAssemblyInput {
    pub system_prompt: Option<String>,
    pub agents_prompt: Option<String>,
    pub session_head: Option<SessionHead>,
    pub plan_snapshot: Option<PlanSnapshot>,
    pub context_summary: Option<ContextSummary>,
    pub context_offload: Option<ContextOffloadSnapshot>,
    pub transcript_messages: Vec<ProviderMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PromptAssembly;

impl PromptAssembly {
    pub fn build_messages(input: PromptAssemblyInput) -> Vec<ProviderMessage> {
        let mut messages = Vec::with_capacity(input.transcript_messages.len() + 6);

        if let Some(session_head) = input.session_head {
            let rendered = session_head.render();
            if !rendered.trim().is_empty() {
                messages.push(ProviderMessage {
                    role: MessageRole::System,
                    content: rendered,
                });
            }
        }

        if let Some(system_prompt) = input.system_prompt
            && !system_prompt.trim().is_empty()
        {
            messages.push(ProviderMessage {
                role: MessageRole::System,
                content: system_prompt,
            });
        }

        if let Some(agents_prompt) = input.agents_prompt
            && !agents_prompt.trim().is_empty()
        {
            messages.push(ProviderMessage {
                role: MessageRole::System,
                content: agents_prompt,
            });
        }

        if let Some(plan_snapshot) = input.plan_snapshot
            && !plan_snapshot.is_empty()
        {
            messages.push(ProviderMessage {
                role: MessageRole::System,
                content: plan_snapshot.system_message_text(),
            });
        }

        let covered_message_count = input
            .context_summary
            .as_ref()
            .map(|summary| summary.covered_message_count as usize)
            .unwrap_or(0)
            .min(input.transcript_messages.len());

        if let Some(context_summary) = input.context_summary
            && !context_summary.summary_text.trim().is_empty()
        {
            messages.push(ProviderMessage {
                role: MessageRole::System,
                content: context_summary.system_message_text(),
            });
        }

        if let Some(context_offload) = input.context_offload
            && let Some(rendered) = render_context_offload_refs(&context_offload)
        {
            messages.push(ProviderMessage {
                role: MessageRole::System,
                content: rendered,
            });
        }

        messages.extend(
            input
                .transcript_messages
                .into_iter()
                .skip(covered_message_count),
        );
        messages
    }
}

fn render_context_offload_refs(snapshot: &ContextOffloadSnapshot) -> Option<String> {
    if snapshot.refs.is_empty() {
        return None;
    }

    let mut lines = vec!["Offloaded Context References:".to_string()];
    const MAX_REFS: usize = 8;
    for reference in snapshot.refs.iter().take(MAX_REFS) {
        lines.push(format!(
            "- [{}] {} | artifact_id={} | tokens={} | messages={} | summary={}",
            reference.id,
            reference.label,
            reference.artifact_id,
            reference.token_estimate,
            reference.message_count,
            reference.summary
        ));
    }

    if snapshot.refs.len() > MAX_REFS {
        lines.push(format!(
            "- ... ({} more refs)",
            snapshot.refs.len() - MAX_REFS
        ));
    }

    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::{
        PromptAssembly, PromptAssemblyInput, SessionHead, SessionHeadFsActivity,
        SessionHeadWorkspaceEntry, SessionHeadWorkspaceEntryKind,
    };
    use crate::context::{ContextOffloadRef, ContextOffloadSnapshot, ContextSummary};
    use crate::plan::PlanSnapshot;
    use crate::provider::ProviderMessage;
    use crate::session::MessageRole;

    #[test]
    fn session_head_render_emits_stable_compact_lines() {
        let rendered = SessionHead {
            session_id: "session-1".to_string(),
            title: "Compacted Chat".to_string(),
            message_count: 8,
            context_tokens: 42,
            compactifications: 1,
            summary_covered_message_count: 2,
            pending_approval_count: 1,
            last_user_preview: Some("latest question".to_string()),
            last_assistant_preview: Some("recent answer".to_string()),
            recent_filesystem_activity: vec![
                SessionHeadFsActivity {
                    action: "read".to_string(),
                    target: ".env".to_string(),
                    detail: "fs_read path=.env -> fs_read path=.env bytes=42".to_string(),
                    recorded_at: 12,
                },
                SessionHeadFsActivity {
                    action: "list".to_string(),
                    target: ".".to_string(),
                    detail: "fs_list path=. recursive=false -> fs_list entries=6".to_string(),
                    recorded_at: 11,
                },
            ],
            workspace_tree: vec![
                SessionHeadWorkspaceEntry {
                    path: "README.md".to_string(),
                    kind: SessionHeadWorkspaceEntryKind::File,
                },
                SessionHeadWorkspaceEntry {
                    path: "crates".to_string(),
                    kind: SessionHeadWorkspaceEntryKind::Directory,
                },
            ],
            workspace_tree_truncated: false,
        }
        .render();

        assert!(rendered.contains("Session: Compacted Chat"));
        assert!(rendered.contains("Session ID: session-1"));
        assert!(rendered.contains("Messages: 8"));
        assert!(rendered.contains("Context Tokens: 42"));
        assert!(rendered.contains("Compactifications: 1"));
        assert!(rendered.contains("Summary Covers: 2 messages"));
        assert!(rendered.contains("Pending Approvals: 1"));
        assert!(rendered.contains("Last User: latest question"));
        assert!(rendered.contains("Last Assistant: recent answer"));
        assert!(rendered.contains("Recent Filesystem Activity:"));
        assert!(rendered.contains("- read .env"));
        assert!(rendered.contains("- list ."));
        assert!(rendered.contains("Workspace Tree:"));
        assert!(rendered.contains("- README.md"));
        assert!(rendered.contains("- crates/"));
    }

    #[test]
    fn prompt_assembly_orders_session_head_then_compact_summary_then_transcript() {
        let messages = PromptAssembly::build_messages(PromptAssemblyInput {
            system_prompt: None,
            agents_prompt: None,
            session_head: Some(SessionHead {
                session_id: "session-1".to_string(),
                title: "Compacted Chat".to_string(),
                message_count: 8,
                context_tokens: 42,
                compactifications: 1,
                summary_covered_message_count: 2,
                pending_approval_count: 0,
                last_user_preview: Some("latest question".to_string()),
                last_assistant_preview: Some("recent answer".to_string()),
                recent_filesystem_activity: vec![SessionHeadFsActivity {
                    action: "patch".to_string(),
                    target: "src/main.rs".to_string(),
                    detail:
                        "fs_patch path=src/main.rs edits=1 -> fs_patch path=src/main.rs edits=1"
                            .to_string(),
                    recorded_at: 14,
                }],
                workspace_tree: vec![SessionHeadWorkspaceEntry {
                    path: "src".to_string(),
                    kind: SessionHeadWorkspaceEntryKind::Directory,
                }],
                workspace_tree_truncated: false,
            }),
            plan_snapshot: None,
            context_summary: Some(ContextSummary {
                session_id: "session-1".to_string(),
                summary_text: "Compact summary text.".to_string(),
                covered_message_count: 2,
                summary_token_estimate: 5,
                updated_at: 10,
            }),
            context_offload: None,
            transcript_messages: vec![
                ProviderMessage {
                    role: MessageRole::User,
                    content: "covered first".to_string(),
                },
                ProviderMessage {
                    role: MessageRole::Assistant,
                    content: "covered second".to_string(),
                },
                ProviderMessage {
                    role: MessageRole::User,
                    content: "latest question".to_string(),
                },
            ],
        });

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, MessageRole::System);
        assert!(messages[0].content.contains("Session: Compacted Chat"));
        assert_eq!(messages[1].role, MessageRole::System);
        assert!(messages[1].content.contains("Compact summary text."));
        assert_eq!(messages[2].role, MessageRole::User);
        assert_eq!(messages[2].content, "latest question");
    }

    #[test]
    fn prompt_assembly_omits_missing_optional_sections() {
        let messages = PromptAssembly::build_messages(PromptAssemblyInput {
            system_prompt: None,
            agents_prompt: None,
            session_head: None,
            plan_snapshot: None,
            context_summary: None,
            context_offload: None,
            transcript_messages: vec![ProviderMessage {
                role: MessageRole::User,
                content: "hello".to_string(),
            }],
        });

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[0].content, "hello");
    }

    #[test]
    fn prompt_assembly_places_plan_between_session_head_and_compact_summary() {
        let messages = PromptAssembly::build_messages(PromptAssemblyInput {
            system_prompt: None,
            agents_prompt: None,
            session_head: Some(SessionHead {
                session_id: "session-1".to_string(),
                title: "Plan Chat".to_string(),
                message_count: 3,
                context_tokens: 9,
                compactifications: 0,
                summary_covered_message_count: 1,
                pending_approval_count: 0,
                last_user_preview: None,
                last_assistant_preview: None,
                recent_filesystem_activity: Vec::new(),
                workspace_tree: Vec::new(),
                workspace_tree_truncated: false,
            }),
            plan_snapshot: Some(PlanSnapshot {
                session_id: "session-1".to_string(),
                goal: Some("Ship planning tools".to_string()),
                items: vec![crate::plan::PlanItem {
                    id: "wire".to_string(),
                    content: "Wire planning tools".to_string(),
                    status: crate::plan::PlanItemStatus::InProgress,
                    depends_on: Vec::new(),
                    notes: Vec::new(),
                    blocked_reason: None,
                    parent_task_id: None,
                }],
                updated_at: 11,
            }),
            context_summary: Some(ContextSummary {
                session_id: "session-1".to_string(),
                summary_text: "Compact summary text.".to_string(),
                covered_message_count: 1,
                summary_token_estimate: 5,
                updated_at: 10,
            }),
            context_offload: None,
            transcript_messages: vec![
                ProviderMessage {
                    role: MessageRole::User,
                    content: "covered first".to_string(),
                },
                ProviderMessage {
                    role: MessageRole::User,
                    content: "latest question".to_string(),
                },
            ],
        });

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, MessageRole::System);
        assert!(messages[0].content.contains("Session: Plan Chat"));
        assert_eq!(messages[1].role, MessageRole::System);
        assert!(messages[1].content.contains("Plan:"));
        assert!(messages[1].content.contains("Wire planning tools"));
        assert_eq!(messages[2].role, MessageRole::System);
        assert!(messages[2].content.contains("Compact summary text."));
        assert_eq!(messages[3].content, "latest question");
    }

    #[test]
    fn prompt_assembly_places_session_head_before_system_and_agents_prompts() {
        let messages = PromptAssembly::build_messages(PromptAssemblyInput {
            system_prompt: Some("You are a useful AI assistant.".to_string()),
            agents_prompt: Some("Project instructions: keep edits minimal.".to_string()),
            session_head: Some(SessionHead {
                session_id: "session-1".to_string(),
                title: "Prompt Order".to_string(),
                message_count: 2,
                context_tokens: 8,
                compactifications: 0,
                summary_covered_message_count: 0,
                pending_approval_count: 0,
                last_user_preview: Some("hello".to_string()),
                last_assistant_preview: None,
                recent_filesystem_activity: Vec::new(),
                workspace_tree: Vec::new(),
                workspace_tree_truncated: false,
            }),
            plan_snapshot: None,
            context_summary: None,
            context_offload: None,
            transcript_messages: vec![ProviderMessage {
                role: MessageRole::User,
                content: "hello".to_string(),
            }],
        });

        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0].role, MessageRole::System);
        assert!(messages[0].content.contains("Session: Prompt Order"));
        assert_eq!(messages[1].role, MessageRole::System);
        assert_eq!(messages[1].content, "You are a useful AI assistant.");
        assert_eq!(messages[2].role, MessageRole::System);
        assert_eq!(
            messages[2].content,
            "Project instructions: keep edits minimal."
        );
        assert_eq!(messages[3].role, MessageRole::User);
    }

    #[test]
    fn prompt_assembly_places_offload_refs_after_summary_and_before_transcript_tail() {
        let messages = PromptAssembly::build_messages(PromptAssemblyInput {
            system_prompt: None,
            agents_prompt: None,
            session_head: None,
            plan_snapshot: None,
            context_summary: Some(ContextSummary {
                session_id: "session-1".to_string(),
                summary_text: "Compacted summary text.".to_string(),
                covered_message_count: 1,
                summary_token_estimate: 5,
                updated_at: 10,
            }),
            context_offload: Some(ContextOffloadSnapshot {
                session_id: "session-1".to_string(),
                refs: vec![ContextOffloadRef {
                    id: "offload-1".to_string(),
                    label: "Earlier tool dump".to_string(),
                    summary: "Shell output with migration diagnostics".to_string(),
                    artifact_id: "artifact-offload-1".to_string(),
                    token_estimate: 120,
                    message_count: 4,
                    created_at: 11,
                }],
                updated_at: 12,
            }),
            transcript_messages: vec![
                ProviderMessage {
                    role: MessageRole::User,
                    content: "covered first".to_string(),
                },
                ProviderMessage {
                    role: MessageRole::User,
                    content: "latest question".to_string(),
                },
            ],
        });

        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, MessageRole::System);
        assert_eq!(messages[1].role, MessageRole::System);
        assert_eq!(messages[2].role, MessageRole::User);
        assert!(messages[0].content.contains("Compacted summary text."));
        assert!(
            messages[1]
                .content
                .contains("Offloaded Context References:")
        );
        assert!(messages[1].content.contains("artifact-offload-1"));
        assert!(messages[1].content.contains("Earlier tool dump"));
        assert_eq!(messages[2].content, "latest question");
    }
}
