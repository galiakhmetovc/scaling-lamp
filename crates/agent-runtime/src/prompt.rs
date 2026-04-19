use crate::context::ContextSummary;
use crate::provider::ProviderMessage;
use crate::session::MessageRole;

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
        lines.join("\n")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptAssemblyInput {
    pub session_head: Option<SessionHead>,
    pub context_summary: Option<ContextSummary>,
    pub transcript_messages: Vec<ProviderMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PromptAssembly;

impl PromptAssembly {
    pub fn build_messages(input: PromptAssemblyInput) -> Vec<ProviderMessage> {
        let mut messages = Vec::with_capacity(input.transcript_messages.len() + 2);

        if let Some(session_head) = input.session_head {
            let rendered = session_head.render();
            if !rendered.trim().is_empty() {
                messages.push(ProviderMessage {
                    role: MessageRole::System,
                    content: rendered,
                });
            }
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

        messages.extend(
            input
                .transcript_messages
                .into_iter()
                .skip(covered_message_count),
        );
        messages
    }
}

#[cfg(test)]
mod tests {
    use super::{PromptAssembly, PromptAssemblyInput, SessionHead};
    use crate::context::ContextSummary;
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
    }

    #[test]
    fn prompt_assembly_orders_session_head_then_compact_summary_then_transcript() {
        let messages = PromptAssembly::build_messages(PromptAssemblyInput {
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
            }),
            context_summary: Some(ContextSummary {
                session_id: "session-1".to_string(),
                summary_text: "Compact summary text.".to_string(),
                covered_message_count: 2,
                summary_token_estimate: 5,
                updated_at: 10,
            }),
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
            session_head: None,
            context_summary: None,
            transcript_messages: vec![ProviderMessage {
                role: MessageRole::User,
                content: "hello".to_string(),
            }],
        });

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].role, MessageRole::User);
        assert_eq!(messages[0].content, "hello");
    }
}
