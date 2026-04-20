use crate::bootstrap::{App, BootstrapError};
use crate::execution::{
    ApprovalContinuationReport, ChatExecutionEvent, ChatTurnExecutionReport, ExecutionError,
    ToolExecutionStatus,
};
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
    ToolRequested { tool_name: String },
    ToolRunning { tool_name: String },
    ToolCompleted { tool_name: String },
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerOutcome {
    ChatCompleted(ChatTurnExecutionReport),
    ApprovalCompleted(ApprovalContinuationReport),
    ApprovalRequired { approval_id: String, reason: String },
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
    phase: ActiveRunPhase,
    interrupt_after_tool_step: Arc<AtomicBool>,
    receiver: Receiver<WorkerEvent>,
    join_handle: Option<JoinHandle<()>>,
}

impl ActiveRunHandle {
    pub fn spawn_chat(app: App, session_id: String, message: String, started_at: i64) -> Self {
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
            phase: ActiveRunPhase::Sending,
            interrupt_after_tool_step,
            receiver,
            join_handle: Some(join_handle),
        }
    }

    pub fn spawn_approval(
        app: App,
        session_id: String,
        run_id: String,
        approval_id: String,
        started_at: i64,
    ) -> Self {
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
            ChatExecutionEvent::ReasoningDelta(_) | ChatExecutionEvent::AssistantTextDelta(_) => {
                self.phase = ActiveRunPhase::Streaming;
            }
            ChatExecutionEvent::ToolStatus { tool_name, status } => {
                self.phase = match status {
                    ToolExecutionStatus::Requested => ActiveRunPhase::ToolRequested {
                        tool_name: tool_name.clone(),
                    },
                    ToolExecutionStatus::WaitingApproval => ActiveRunPhase::WaitingApproval,
                    ToolExecutionStatus::Approved | ToolExecutionStatus::Running => {
                        ActiveRunPhase::ToolRunning {
                            tool_name: tool_name.clone(),
                        }
                    }
                    ToolExecutionStatus::Completed => ActiveRunPhase::ToolCompleted {
                        tool_name: tool_name.clone(),
                    },
                    ToolExecutionStatus::Failed => ActiveRunPhase::Failed,
                };
            }
        }
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
