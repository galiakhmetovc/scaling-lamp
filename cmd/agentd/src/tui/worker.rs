use crate::bootstrap::BootstrapError;
use crate::execution::{
    ApprovalContinuationReport, ChatExecutionEvent, ChatTurnExecutionReport, ExecutionError,
    ToolExecutionStatus,
};
use crate::tui::backend::TuiBackend;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuedDraftMode {
    Deferred,
    Priority,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedDraft {
    pub content: String,
    pub queued_at: i64,
    pub mode: QueuedDraftMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveRunKind {
    Chat,
    Approval,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActiveRunPhase {
    Sending,
    Streaming,
    WaitingApproval,
    ToolRequested { tool_name: String, summary: String },
    ToolRunning { tool_name: String, summary: String },
    ToolCompleted { tool_name: String, summary: String },
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerOutcome {
    ChatCompleted(ChatTurnExecutionReport),
    ApprovalCompleted(ApprovalContinuationReport),
    ApprovalRequired { approval_id: String, reason: String },
    Cancelled,
    InterruptedByQueuedInput,
    Failed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerEvent {
    Chat(ChatExecutionEvent),
    Finished(WorkerOutcome),
}

pub struct ActiveRunHandle {
    kind: ActiveRunKind,
    session_id: String,
    started_at: i64,
    last_status_notice_at: i64,
    phase: ActiveRunPhase,
    interrupt_after_tool_step: Arc<AtomicBool>,
    receiver: Receiver<WorkerEvent>,
    join_handle: Option<JoinHandle<()>>,
}

impl ActiveRunHandle {
    pub fn spawn_chat<B>(app: B, session_id: String, message: String, started_at: i64) -> Self
    where
        B: TuiBackend,
    {
        let (sender, receiver) = mpsc::channel();
        let interrupt_after_tool_step = Arc::new(AtomicBool::new(false));
        let worker_session_id = session_id.clone();
        let join_handle = {
            let sender = sender.clone();
            let interrupt_after_tool_step = Arc::clone(&interrupt_after_tool_step);
            thread::spawn(move || {
                let mut observer = |event: ChatExecutionEvent| {
                    let _ = sender.send(WorkerEvent::Chat(event));
                };
                let outcome = match app.execute_chat_turn_with_control_and_observer(
                    &worker_session_id,
                    &message,
                    started_at,
                    Some(interrupt_after_tool_step.as_ref()),
                    &mut observer,
                ) {
                    Ok(report) => WorkerOutcome::ChatCompleted(report),
                    Err(BootstrapError::Execution(ExecutionError::ApprovalRequired {
                        approval_id,
                        reason,
                        ..
                    })) => WorkerOutcome::ApprovalRequired {
                        approval_id,
                        reason,
                    },
                    Err(BootstrapError::Execution(ExecutionError::CancelledByOperator)) => {
                        WorkerOutcome::Cancelled
                    }
                    Err(BootstrapError::Execution(ExecutionError::InterruptedByQueuedInput)) => {
                        WorkerOutcome::InterruptedByQueuedInput
                    }
                    Err(error) => WorkerOutcome::Failed(error.to_string()),
                };
                let _ = sender.send(WorkerEvent::Finished(outcome));
                interrupt_after_tool_step.store(false, Ordering::SeqCst);
            })
        };

        Self {
            kind: ActiveRunKind::Chat,
            session_id,
            started_at,
            last_status_notice_at: started_at,
            phase: ActiveRunPhase::Sending,
            interrupt_after_tool_step,
            receiver,
            join_handle: Some(join_handle),
        }
    }

    pub fn spawn_approval<B>(
        app: B,
        session_id: String,
        run_id: String,
        approval_id: String,
        started_at: i64,
    ) -> Self
    where
        B: TuiBackend,
    {
        let (sender, receiver) = mpsc::channel();
        let interrupt_after_tool_step = Arc::new(AtomicBool::new(false));
        let join_handle = {
            let sender = sender.clone();
            let interrupt_after_tool_step = Arc::clone(&interrupt_after_tool_step);
            thread::spawn(move || {
                let mut observer = |event: ChatExecutionEvent| {
                    let _ = sender.send(WorkerEvent::Chat(event));
                };
                let outcome = match app.approve_run_with_control_and_observer(
                    &run_id,
                    &approval_id,
                    started_at,
                    Some(interrupt_after_tool_step.as_ref()),
                    &mut observer,
                ) {
                    Ok(report) => {
                        if let Some(next_approval_id) = report.approval_id.clone() {
                            WorkerOutcome::ApprovalRequired {
                                approval_id: next_approval_id,
                                reason: "model requested another approval".to_string(),
                            }
                        } else {
                            WorkerOutcome::ApprovalCompleted(report)
                        }
                    }
                    Err(BootstrapError::Execution(ExecutionError::CancelledByOperator)) => {
                        WorkerOutcome::Cancelled
                    }
                    Err(BootstrapError::Execution(ExecutionError::InterruptedByQueuedInput)) => {
                        WorkerOutcome::InterruptedByQueuedInput
                    }
                    Err(error) => WorkerOutcome::Failed(error.to_string()),
                };
                let _ = sender.send(WorkerEvent::Finished(outcome));
                interrupt_after_tool_step.store(false, Ordering::SeqCst);
            })
        };

        Self {
            kind: ActiveRunKind::Approval,
            session_id,
            started_at,
            last_status_notice_at: started_at,
            phase: ActiveRunPhase::Sending,
            interrupt_after_tool_step,
            receiver,
            join_handle: Some(join_handle),
        }
    }

    pub fn kind(&self) -> ActiveRunKind {
        self.kind.clone()
    }

    pub fn session_id(&self) -> &str {
        self.session_id.as_str()
    }

    pub fn started_at(&self) -> i64 {
        self.started_at
    }

    pub fn phase(&self) -> &ActiveRunPhase {
        &self.phase
    }

    pub fn queue_interrupt_after_tool_step(&self) {
        self.interrupt_after_tool_step.store(true, Ordering::SeqCst);
    }

    pub fn interrupt_after_tool_step_requested(&self) -> bool {
        self.interrupt_after_tool_step.load(Ordering::SeqCst)
    }

    pub fn current_tool_summary(&self) -> Option<&str> {
        match &self.phase {
            ActiveRunPhase::ToolRequested { summary, .. }
            | ActiveRunPhase::ToolRunning { summary, .. }
            | ActiveRunPhase::ToolCompleted { summary, .. } => Some(summary.as_str()),
            _ => None,
        }
    }

    pub fn heartbeat_notice(&mut self, now: i64, interval_seconds: i64) -> Option<String> {
        if now.saturating_sub(self.last_status_notice_at) < interval_seconds {
            return None;
        }
        let detail = match &self.phase {
            ActiveRunPhase::ToolRunning { tool_name, summary } => Some(format!(
                "ход всё ещё выполняется: {} {} уже {} ({summary}). Команды: \\статус, \\пауза, \\стоп, \\отмена, \\отладка",
                self.kind_label(),
                tool_name,
                format_elapsed(now.saturating_sub(self.started_at))
            )),
            _ => None,
        }?;
        self.last_status_notice_at = now;
        Some(detail)
    }

    pub fn drain_events(&mut self) -> Vec<WorkerEvent> {
        let events = self.receiver.try_iter().collect::<Vec<_>>();
        for event in &events {
            if let WorkerEvent::Chat(chat) = event {
                self.apply_chat_event(chat);
            }
        }
        events
    }

    pub fn join(&mut self) {
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }

    fn apply_chat_event(&mut self, event: &ChatExecutionEvent) {
        match event {
            ChatExecutionEvent::ReasoningDelta(_)
            | ChatExecutionEvent::AssistantTextDelta(_)
            | ChatExecutionEvent::ProviderLoopProgress { .. } => {
                self.phase = ActiveRunPhase::Streaming;
            }
            ChatExecutionEvent::ToolStatus {
                tool_name, status, ..
            } => {
                self.phase = match status {
                    ToolExecutionStatus::Requested => ActiveRunPhase::ToolRequested {
                        tool_name: tool_name.clone(),
                        summary: event_tool_summary(event),
                    },
                    ToolExecutionStatus::WaitingApproval => ActiveRunPhase::WaitingApproval,
                    ToolExecutionStatus::Approved | ToolExecutionStatus::Running => {
                        ActiveRunPhase::ToolRunning {
                            tool_name: tool_name.clone(),
                            summary: event_tool_summary(event),
                        }
                    }
                    ToolExecutionStatus::Completed => ActiveRunPhase::ToolCompleted {
                        tool_name: tool_name.clone(),
                        summary: event_tool_summary(event),
                    },
                    ToolExecutionStatus::Failed => ActiveRunPhase::Failed,
                };
            }
        }
    }

    fn kind_label(&self) -> &'static str {
        match self.kind {
            ActiveRunKind::Chat => "чат",
            ActiveRunKind::Approval => "апрув",
        }
    }
}

fn format_elapsed(total_seconds: i64) -> String {
    let total_seconds = total_seconds.max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours:02}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

fn event_tool_summary(event: &ChatExecutionEvent) -> String {
    match event {
        ChatExecutionEvent::ToolStatus { summary, .. } => summary.clone(),
        _ => String::new(),
    }
}

#[derive(Debug, Default)]
pub struct ComposerQueue {
    deferred: VecDeque<QueuedDraft>,
    priority: VecDeque<QueuedDraft>,
}

impl ComposerQueue {
    pub fn enqueue(&mut self, draft: QueuedDraft) {
        match draft.mode {
            QueuedDraftMode::Deferred => self.deferred.push_back(draft),
            QueuedDraftMode::Priority => self.priority.push_back(draft),
        }
    }

    pub fn pop_priority(&mut self) -> Option<QueuedDraft> {
        self.priority.pop_front()
    }

    pub fn pop_deferred(&mut self) -> Option<QueuedDraft> {
        self.deferred.pop_front()
    }

    pub fn priority_len(&self) -> usize {
        self.priority.len()
    }

    pub fn deferred_len(&self) -> usize {
        self.deferred.len()
    }

    pub fn total_len(&self) -> usize {
        self.priority.len() + self.deferred.len()
    }
}
