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
                "reasoning" => timeline.push_reasoning(entry.content.as_str(), entry.created_at),
                "tool" => timeline.push_tool(
                    entry.tool_name.as_deref().unwrap_or("tool"),
                    entry
                        .tool_status
                        .as_deref()
                        .unwrap_or(ToolExecutionStatus::Completed.as_str()),
                    entry.content.as_str(),
                    entry.created_at,
                ),
                "approval" => timeline.push_approval(
                    entry.approval_id.as_deref().unwrap_or("approval"),
                    entry.content.as_str(),
                    entry.created_at,
                ),
                "system" => timeline.push_system(entry.content.as_str(), entry.created_at),
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

    pub fn push_reasoning(&mut self, content: &str, timestamp: i64) {
        self.finish_assistant();
        self.entries.push(TimelineEntry {
            timestamp,
            kind: TimelineEntryKind::Reasoning,
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

    pub fn push_tool(&mut self, tool_name: &str, status: &str, summary: &str, timestamp: i64) {
        self.finish_turn();
        self.entries.push(TimelineEntry {
            timestamp,
            kind: TimelineEntryKind::Tool {
                tool_name: tool_name.to_string(),
                status: status.to_string(),
                summary: summary.to_string(),
            },
            content: summary.to_string(),
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

    pub fn finalize_assistant_output(&mut self, content: &str, timestamp: i64) {
        self.finish_reasoning();
        if let Some(index) = self.active_assistant {
            self.entries[index].timestamp = timestamp;
            self.entries[index].content = content.to_string();
            return;
        }
        self.push_assistant(content, timestamp);
    }

    pub fn sync_pending_approvals(&mut self, pending_approvals: &[SessionPendingApproval]) {
        self.entries
            .retain(|entry| !matches!(entry.kind, TimelineEntryKind::Approval { .. }));
        for approval in pending_approvals {
            self.push_approval(
                approval.approval_id.as_str(),
                approval.reason.as_str(),
                approval.requested_at,
            );
        }
    }

    pub fn merge_ephemeral_from(&mut self, previous: &Timeline) {
        for entry in previous
            .entries
            .iter()
            .filter(|entry| should_preserve_entry(entry))
        {
            if !self.entries.iter().any(|existing| existing == entry) {
                self.entries.push(entry.clone());
            }
        }
        self.entries.sort_by(|left, right| {
            left.timestamp
                .cmp(&right.timestamp)
                .then_with(|| {
                    timeline_entry_sort_weight(left).cmp(&timeline_entry_sort_weight(right))
                })
                .then_with(|| left.content.cmp(&right.content))
        });
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

fn should_preserve_entry(entry: &TimelineEntry) -> bool {
    match &entry.kind {
        TimelineEntryKind::Tool { status, .. } => {
            !matches!(status.as_str(), "completed" | "failed")
        }
        TimelineEntryKind::System => entry.content.starts_with("автоапрув ожидающего запроса:"),
        _ => false,
    }
}

fn timeline_entry_sort_weight(entry: &TimelineEntry) -> u8 {
    match entry.kind {
        TimelineEntryKind::User => 0,
        TimelineEntryKind::Reasoning => 1,
        TimelineEntryKind::Tool { .. } => 2,
        TimelineEntryKind::Approval { .. } => 3,
        TimelineEntryKind::System => 4,
        TimelineEntryKind::Assistant => 5,
    }
}
