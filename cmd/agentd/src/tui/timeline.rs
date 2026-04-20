use crate::bootstrap::SessionPendingApproval;
use crate::bootstrap::SessionTranscriptView;
use crate::execution::ToolExecutionStatus;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineEntryKind {
    User,
    Assistant,
    Reasoning,
    Tool {
        tool_name: String,
        status: String,
        summary: String,
    },
    Approval {
        approval_id: String,
    },
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    pub timestamp: i64,
    pub kind: TimelineEntryKind,
    pub content: String,
}

#[derive(Debug, Clone, Default)]
pub struct Timeline {
    entries: Vec<TimelineEntry>,
    active_assistant: Option<usize>,
    active_reasoning: Option<usize>,
    active_tool: Option<(String, usize)>,
}

impl Timeline {
    pub fn from_session_view(
        transcript: &SessionTranscriptView,
        pending_approvals: &[SessionPendingApproval],
    ) -> Self {
        let mut timeline = Self::default();
        for entry in &transcript.entries {
            match entry.role.as_str() {
                "user" => timeline.push_user(entry.content.as_str(), entry.created_at),
                "assistant" => timeline.push_assistant(entry.content.as_str(), entry.created_at),
                _ => timeline.push_system(
                    &format!("{}: {}", entry.role, entry.content),
                    entry.created_at,
                ),
            }
        }
        for approval in pending_approvals {
            timeline.push_approval(
                approval.approval_id.as_str(),
                approval.reason.as_str(),
                approval.requested_at,
            );
        }
        timeline
    }

    pub fn entries(&self, reasoning_visible: bool) -> Vec<&TimelineEntry> {
        self.entries
            .iter()
            .filter(|entry| {
                reasoning_visible || !matches!(entry.kind, TimelineEntryKind::Reasoning)
            })
            .collect()
    }

    pub fn message_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| {
                matches!(
                    entry.kind,
                    TimelineEntryKind::User | TimelineEntryKind::Assistant
                )
            })
            .count()
    }

    pub fn push_user(&mut self, content: &str, timestamp: i64) {
        self.finish_turn();
        self.entries.push(TimelineEntry {
            timestamp,
            kind: TimelineEntryKind::User,
            content: content.to_string(),
        });
    }

    pub fn push_assistant(&mut self, content: &str, timestamp: i64) {
        self.finish_turn();
        self.entries.push(TimelineEntry {
            timestamp,
            kind: TimelineEntryKind::Assistant,
            content: content.to_string(),
        });
    }

    pub fn push_system(&mut self, content: &str, timestamp: i64) {
        self.finish_turn();
        self.entries.push(TimelineEntry {
            timestamp,
            kind: TimelineEntryKind::System,
            content: content.to_string(),
        });
    }

    pub fn push_approval(&mut self, approval_id: &str, reason: &str, timestamp: i64) {
        self.finish_turn();
        self.entries.push(TimelineEntry {
            timestamp,
            kind: TimelineEntryKind::Approval {
                approval_id: approval_id.to_string(),
            },
            content: reason.to_string(),
        });
    }

    pub fn push_reasoning_delta(&mut self, delta: &str, timestamp: i64) {
        if let Some(index) = self.active_reasoning {
            self.entries[index].content.push_str(delta);
            return;
        }
        self.finish_assistant();
        let index = self.entries.len();
        self.entries.push(TimelineEntry {
            timestamp,
            kind: TimelineEntryKind::Reasoning,
            content: delta.to_string(),
        });
        self.active_reasoning = Some(index);
    }

    pub fn push_assistant_delta(&mut self, delta: &str, timestamp: i64) {
        self.finish_reasoning();
        if let Some(index) = self.active_assistant {
            self.entries[index].content.push_str(delta);
            return;
        }
        let index = self.entries.len();
        self.entries.push(TimelineEntry {
            timestamp,
            kind: TimelineEntryKind::Assistant,
            content: delta.to_string(),
        });
        self.active_assistant = Some(index);
    }

    pub fn update_tool_status(
        &mut self,
        tool_name: &str,
        summary: &str,
        status: ToolExecutionStatus,
        timestamp: i64,
    ) {
        self.finish_turn();
        let status_text = status.as_str().to_string();
        match self.active_tool.as_ref() {
            Some((current_tool, index)) if current_tool == tool_name => {
                self.entries[*index].kind = TimelineEntryKind::Tool {
                    tool_name: tool_name.to_string(),
                    status: status_text,
                    summary: summary.to_string(),
                };
                self.entries[*index].content = summary.to_string();
                if matches!(
                    status,
                    ToolExecutionStatus::Completed | ToolExecutionStatus::Failed
                ) {
                    self.active_tool = None;
                }
            }
            _ => {
                let index = self.entries.len();
                self.entries.push(TimelineEntry {
                    timestamp,
                    kind: TimelineEntryKind::Tool {
                        tool_name: tool_name.to_string(),
                        status: status_text,
                        summary: summary.to_string(),
                    },
                    content: summary.to_string(),
                });
                if matches!(
                    status,
                    ToolExecutionStatus::Completed | ToolExecutionStatus::Failed
                ) {
                    self.active_tool = None;
                } else {
                    self.active_tool = Some((tool_name.to_string(), index));
                }
            }
        }
    }

    pub fn finish_turn(&mut self) {
        self.finish_reasoning();
        self.finish_assistant();
    }

    pub fn remove_approval(&mut self, approval_id: &str) {
        self.entries.retain(|entry| match &entry.kind {
            TimelineEntryKind::Approval {
                approval_id: current,
            } => current != approval_id,
            _ => true,
        });
    }

    fn finish_reasoning(&mut self) {
        self.active_reasoning = None;
    }

    fn finish_assistant(&mut self) {
        self.active_assistant = None;
    }
}
