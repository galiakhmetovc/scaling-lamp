use super::*;
use crate::bootstrap::SessionPreferencesPatch;

pub(super) trait ChatReplBackend {
    fn show_chat(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError>;
    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError>;
    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError>;
    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<(), BootstrapError>;
    fn find_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<ReplPendingApproval>, BootstrapError>;
    fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<crate::execution::ApprovalContinuationReport, BootstrapError>;
    fn send_chat_with_observer(
        &self,
        session_id: &str,
        message: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatSendOutcome, BootstrapError>;
}

impl ChatReplBackend for App {
    fn show_chat(&self, session_id: &str) -> Result<String, BootstrapError> {
        render::show_chat(self, session_id)
    }

    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_context_state(session_id)
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_plan(session_id)
    }

    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_background_jobs(session_id)
    }

    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_skills(session_id)
    }

    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        self.enable_session_skill(session_id, skill_name)?;
        self.render_session_skills(session_id)
    }

    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        self.disable_session_skill(session_id, skill_name)?;
        self.render_session_skills(session_id)
    }

    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<(), BootstrapError> {
        App::update_session_preferences(self, session_id, patch).map(|_| ())
    }

    fn find_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<ReplPendingApproval>, BootstrapError> {
        find_pending_approval(self, session_id, requested_approval_id)
    }

    fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<crate::execution::ApprovalContinuationReport, BootstrapError> {
        self.approve_run_with_observer(run_id, approval_id, unix_timestamp()?, observer)
    }

    fn send_chat_with_observer(
        &self,
        session_id: &str,
        message: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatSendOutcome, BootstrapError> {
        send_chat_outcome_with_observer(self, session_id, message, observer)
    }
}

impl ChatReplBackend for DaemonClient {
    fn show_chat(&self, session_id: &str) -> Result<String, BootstrapError> {
        render::show_chat_via_client(self, session_id)
    }

    fn render_context(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_context_state(session_id)
    }

    fn render_plan(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_plan(session_id)
    }

    fn render_active_jobs(&self, session_id: &str) -> Result<String, BootstrapError> {
        self.render_session_background_jobs(session_id)
    }

    fn render_session_skills(&self, session_id: &str) -> Result<String, BootstrapError> {
        render::render_session_skills_list(self.session_skills(session_id)?)
    }

    fn enable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        let skills = self.enable_session_skill(session_id, skill_name)?;
        render::render_session_skills_list(skills)
    }

    fn disable_session_skill(
        &self,
        session_id: &str,
        skill_name: &str,
    ) -> Result<String, BootstrapError> {
        let skills = self.disable_session_skill(session_id, skill_name)?;
        render::render_session_skills_list(skills)
    }

    fn update_session_preferences(
        &self,
        session_id: &str,
        patch: SessionPreferencesPatch,
    ) -> Result<(), BootstrapError> {
        DaemonClient::update_session_preferences(self, session_id, patch).map(|_| ())
    }

    fn find_pending_approval(
        &self,
        session_id: &str,
        requested_approval_id: Option<&str>,
    ) -> Result<Option<ReplPendingApproval>, BootstrapError> {
        Ok(self
            .latest_pending_approval(session_id, requested_approval_id)?
            .map(|pending| ReplPendingApproval {
                run_id: pending.run_id,
                approval_id: pending.approval_id,
            }))
    }

    fn approve_run_with_observer(
        &self,
        run_id: &str,
        approval_id: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<crate::execution::ApprovalContinuationReport, BootstrapError> {
        self.approve_run_with_control_and_observer(
            run_id,
            approval_id,
            unix_timestamp()?,
            None,
            observer,
        )
    }

    fn send_chat_with_observer(
        &self,
        session_id: &str,
        message: &str,
        observer: &mut dyn FnMut(ChatExecutionEvent),
    ) -> Result<ChatSendOutcome, BootstrapError> {
        send_chat_outcome_via_client_with_observer(self, session_id, message, observer)
    }
}

struct ReplRenderer<'a, W: Write> {
    output: &'a mut W,
    active_tool: Option<String>,
    reasoning_open: bool,
    assistant_open: bool,
    assistant_streamed_this_turn: bool,
}

impl<'a, W: Write> ReplRenderer<'a, W> {
    fn new(output: &'a mut W) -> Self {
        Self {
            output,
            active_tool: None,
            reasoning_open: false,
            assistant_open: false,
            assistant_streamed_this_turn: false,
        }
    }

    fn emit(&mut self, event: ChatExecutionEvent) -> Result<(), BootstrapError> {
        match event {
            ChatExecutionEvent::ReasoningDelta(delta) => self.write_reasoning_delta(&delta),
            ChatExecutionEvent::AssistantTextDelta(delta) => self.write_assistant_delta(&delta),
            ChatExecutionEvent::ProviderLoopProgress { .. } => Ok(()),
            ChatExecutionEvent::ToolStatus {
                tool_name,
                summary,
                status,
            } => self.write_tool_status(&tool_name, &summary, status),
        }
    }

    fn finish_turn(&mut self) -> Result<(), BootstrapError> {
        if self.reasoning_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.reasoning_open = false;
        }
        if self.assistant_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.assistant_open = false;
        }
        Ok(())
    }

    fn begin_turn(&mut self) {
        self.assistant_streamed_this_turn = false;
    }

    fn assistant_streamed_this_turn(&self) -> bool {
        self.assistant_streamed_this_turn
    }

    fn write_reasoning_delta(&mut self, delta: &str) -> Result<(), BootstrapError> {
        if self.assistant_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.assistant_open = false;
        }
        if self.reasoning_open {
            write!(self.output, "{delta}").map_err(BootstrapError::Stream)?;
        } else {
            write!(self.output, "reasoning: {delta}").map_err(BootstrapError::Stream)?;
            self.reasoning_open = true;
        }
        self.output.flush().map_err(BootstrapError::Stream)
    }

    fn write_assistant_delta(&mut self, delta: &str) -> Result<(), BootstrapError> {
        if self.reasoning_open {
            writeln!(self.output).map_err(BootstrapError::Stream)?;
            self.reasoning_open = false;
        }
        if self.assistant_open {
            write!(self.output, "{delta}").map_err(BootstrapError::Stream)?;
        } else {
            write!(self.output, "assistant: {delta}").map_err(BootstrapError::Stream)?;
            self.assistant_open = true;
        }
        self.assistant_streamed_this_turn = true;
        self.output.flush().map_err(BootstrapError::Stream)
    }

    fn write_tool_status(
        &mut self,
        tool_name: &str,
        summary: &str,
        status: ToolExecutionStatus,
    ) -> Result<(), BootstrapError> {
        self.finish_turn()?;
        let line = if summary.is_empty() || summary == tool_name {
            format!("tool: {tool_name} | {}", status.as_str())
        } else {
            format!("tool: {tool_name} | {} | {summary}", status.as_str())
        };
        match &self.active_tool {
            Some(current) if current == tool_name => {
                write!(self.output, "\x1b[1A\r\x1b[2K{line}\n").map_err(BootstrapError::Stream)?;
            }
            _ => {
                writeln!(self.output, "{line}").map_err(BootstrapError::Stream)?;
            }
        }
        if matches!(
            status,
            ToolExecutionStatus::Completed | ToolExecutionStatus::Failed
        ) {
            self.active_tool = None;
        } else {
            self.active_tool = Some(tool_name.to_string());
        }
        self.output.flush().map_err(BootstrapError::Stream)
    }
}

pub(super) fn run_chat_repl<R, W>(
    app: &App,
    session_id: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), BootstrapError>
where
    R: BufRead,
    W: Write,
{
    run_chat_repl_with_backend(app, session_id, input, output)
}

pub(super) fn run_chat_repl_with_backend<B, R, W>(
    backend: &B,
    session_id: &str,
    input: &mut R,
    output: &mut W,
) -> Result<(), BootstrapError>
where
    B: ChatReplBackend,
    R: BufRead,
    W: Write,
{
    writeln!(output, "chat repl session_id={session_id}").map_err(BootstrapError::Stream)?;
    writeln!(output, "{REPL_HELP}").map_err(BootstrapError::Stream)?;

    let mut line = String::new();
    let mut renderer = ReplRenderer::new(output);

    loop {
        write!(renderer.output, "> ").map_err(BootstrapError::Stream)?;
        renderer.output.flush().map_err(BootstrapError::Stream)?;

        line.clear();
        let bytes = read_repl_line(input, &mut line).map_err(BootstrapError::Stream)?;
        if bytes == 0 {
            renderer.finish_turn()?;
            writeln!(renderer.output, "leaving chat repl session_id={session_id}")
                .map_err(BootstrapError::Stream)?;
            return Ok(());
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match canonical_repl_command(trimmed) {
            Some("/exit") => {
                renderer.finish_turn()?;
                writeln!(renderer.output, "leaving chat repl session_id={session_id}")
                    .map_err(BootstrapError::Stream)?;
                return Ok(());
            }
            Some("/help") => {
                renderer.finish_turn()?;
                writeln!(renderer.output, "{REPL_HELP}").map_err(BootstrapError::Stream)?;
            }
            Some("/show") => {
                renderer.finish_turn()?;
                let transcript = backend.show_chat(session_id)?;
                writeln!(renderer.output, "{transcript}").map_err(BootstrapError::Stream)?;
            }
            Some("/context") => {
                renderer.finish_turn()?;
                let context = backend.render_context(session_id)?;
                writeln!(renderer.output, "{context}").map_err(BootstrapError::Stream)?;
            }
            Some("/plan") => {
                renderer.finish_turn()?;
                let plan = backend.render_plan(session_id)?;
                writeln!(renderer.output, "{plan}").map_err(BootstrapError::Stream)?;
            }
            Some("/jobs") => {
                renderer.finish_turn()?;
                let jobs = backend.render_active_jobs(session_id)?;
                writeln!(renderer.output, "{jobs}").map_err(BootstrapError::Stream)?;
            }
            Some("/skills") => {
                renderer.finish_turn()?;
                let skills = backend.render_session_skills(session_id)?;
                writeln!(renderer.output, "{skills}").map_err(BootstrapError::Stream)?;
            }
            Some("/completion") => {
                renderer.finish_turn()?;
                let value = split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                    reason: "\\доводка requires off|выкл or a non-negative integer".to_string(),
                })?;
                let completion_nudges = parse_completion_nudges(value)?;
                backend.update_session_preferences(
                    session_id,
                    SessionPreferencesPatch {
                        completion_nudges: Some(completion_nudges),
                        ..SessionPreferencesPatch::default()
                    },
                )?;
                writeln!(
                    renderer.output,
                    "completion gate {}",
                    describe_completion_mode(completion_nudges)
                )
                .map_err(BootstrapError::Stream)?;
            }
            Some("/autoapprove") => {
                renderer.finish_turn()?;
                let value = split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                    reason: "\\автоапрув requires on|off".to_string(),
                })?;
                let auto_approve = parse_auto_approve(value)?;
                backend.update_session_preferences(
                    session_id,
                    SessionPreferencesPatch {
                        auto_approve: Some(auto_approve),
                        ..SessionPreferencesPatch::default()
                    },
                )?;
                writeln!(
                    renderer.output,
                    "auto-approval {}",
                    if auto_approve { "enabled" } else { "disabled" }
                )
                .map_err(BootstrapError::Stream)?;
            }
            Some("/enable") => {
                renderer.finish_turn()?;
                let skill_name =
                    split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                        reason: "\\включить requires a skill name".to_string(),
                    })?;
                let skills = backend.enable_session_skill(session_id, skill_name)?;
                writeln!(renderer.output, "{skills}").map_err(BootstrapError::Stream)?;
            }
            Some("/disable") => {
                renderer.finish_turn()?;
                let skill_name =
                    split_command_arg(trimmed).ok_or_else(|| BootstrapError::Usage {
                        reason: "\\выключить requires a skill name".to_string(),
                    })?;
                let skills = backend.disable_session_skill(session_id, skill_name)?;
                writeln!(renderer.output, "{skills}").map_err(BootstrapError::Stream)?;
            }
            Some("/approve") => {
                let requested = trimmed.split_whitespace().nth(1).map(ToString::to_string);
                let Some(current) =
                    backend.find_pending_approval(session_id, requested.as_deref())?
                else {
                    renderer.finish_turn()?;
                    writeln!(
                        renderer.output,
                        "no pending approval for session_id={session_id}"
                    )
                    .map_err(BootstrapError::Stream)?;
                    continue;
                };
                let approval_id = current.approval_id.clone();
                renderer.begin_turn();
                let mut emit_error = None;
                let mut emit = |event| {
                    if emit_error.is_none() {
                        emit_error = renderer.emit(event).err();
                    }
                };
                let report =
                    backend.approve_run_with_observer(&current.run_id, &approval_id, &mut emit)?;
                if let Some(error) = emit_error {
                    return Err(error);
                }
                renderer.finish_turn()?;
                if let Some(text) = report.output_text.as_deref() {
                    if text.is_empty() || renderer.assistant_streamed_this_turn() {
                        continue;
                    }
                    writeln!(renderer.output, "assistant: {text}")
                        .map_err(BootstrapError::Stream)?;
                }
            }
            _ => {
                let message = trimmed;
                if backend.find_pending_approval(session_id, None)?.is_some() {
                    renderer.finish_turn()?;
                    writeln!(
                        renderer.output,
                        "finish the pending approval before sending another message"
                    )
                    .map_err(BootstrapError::Stream)?;
                    continue;
                }

                renderer.begin_turn();
                let mut emit_error = None;
                let mut emit = |event| {
                    if emit_error.is_none() {
                        emit_error = renderer.emit(event).err();
                    }
                };
                match backend.send_chat_with_observer(session_id, message, &mut emit)? {
                    ChatSendOutcome::Completed { output_text, .. } => {
                        if let Some(error) = emit_error {
                            return Err(error);
                        }
                        renderer.finish_turn()?;
                        if output_text.is_empty() || renderer.assistant_streamed_this_turn() {
                            continue;
                        }
                        writeln!(renderer.output, "assistant: {output_text}")
                            .map_err(BootstrapError::Stream)?;
                    }
                    ChatSendOutcome::WaitingApproval { approval_id, .. } => {
                        if let Some(error) = emit_error {
                            return Err(error);
                        }
                        let _ = approval_id;
                        renderer.finish_turn()?;
                    }
                }
            }
        }
    }
}

pub(super) fn send_chat_outcome(
    app: &App,
    session_id: &str,
    message: &str,
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = None;
    send_chat_outcome_internal(app, session_id, message, &mut observer)
}

pub(super) fn send_chat_outcome_with_observer(
    app: &App,
    session_id: &str,
    message: &str,
    observer: &mut dyn FnMut(ChatExecutionEvent),
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = Some(observer);
    send_chat_outcome_internal(app, session_id, message, &mut observer)
}

pub(super) fn send_chat_outcome_via_client(
    client: &DaemonClient,
    session_id: &str,
    message: &str,
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = None;
    send_chat_outcome_via_client_internal(client, session_id, message, &mut observer)
}

pub(super) fn send_chat_outcome_via_client_with_observer(
    client: &DaemonClient,
    session_id: &str,
    message: &str,
    observer: &mut dyn FnMut(ChatExecutionEvent),
) -> Result<ChatSendOutcome, BootstrapError> {
    let mut observer = Some(observer);
    send_chat_outcome_via_client_internal(client, session_id, message, &mut observer)
}

fn send_chat_outcome_internal(
    app: &App,
    session_id: &str,
    message: &str,
    observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
) -> Result<ChatSendOutcome, BootstrapError> {
    let now = unix_timestamp()?;
    let run_id = format!("run-chat-{session_id}-{now}");
    let result = match observer.as_deref_mut() {
        Some(observer) => app.execute_chat_turn_with_observer(session_id, message, now, observer),
        None => app.execute_chat_turn(session_id, message, now),
    };
    let report = match result {
        Ok(report) => report,
        Err(BootstrapError::Execution(ExecutionError::ApprovalRequired {
            approval_id, ..
        })) => {
            return Ok(ChatSendOutcome::WaitingApproval {
                session_id: session_id.to_string(),
                run_id: Some(run_id),
                approval_id,
            });
        }
        Err(error) => return Err(error),
    };
    Ok(ChatSendOutcome::Completed {
        session_id: report.session_id,
        run_id: Some(report.run_id),
        response_id: report.response_id,
        output_text: report.output_text,
    })
}

fn send_chat_outcome_via_client_internal(
    client: &DaemonClient,
    session_id: &str,
    message: &str,
    observer: &mut Option<&mut dyn FnMut(ChatExecutionEvent)>,
) -> Result<ChatSendOutcome, BootstrapError> {
    let now = unix_timestamp()?;
    let result = match observer.as_deref_mut() {
        Some(observer) => client
            .execute_chat_turn_with_control_and_observer(session_id, message, now, None, observer),
        None => {
            let mut noop = |_| {};
            client.execute_chat_turn_with_control_and_observer(
                session_id, message, now, None, &mut noop,
            )
        }
    };
    let report = match result {
        Ok(report) => report,
        Err(BootstrapError::Execution(ExecutionError::ApprovalRequired {
            approval_id, ..
        })) => {
            return Ok(ChatSendOutcome::WaitingApproval {
                session_id: session_id.to_string(),
                run_id: None,
                approval_id,
            });
        }
        Err(error) => return Err(error),
    };
    Ok(ChatSendOutcome::Completed {
        session_id: report.session_id,
        run_id: Some(report.run_id),
        response_id: report.response_id,
        output_text: report.output_text,
    })
}

fn canonical_repl_command(raw: &str) -> Option<&'static str> {
    let command = raw
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_end_matches(['\\', '/']);
    match command {
        "/exit" | "\\выход" => Some("/exit"),
        "/help" | "\\помощь" => Some("/help"),
        "/show" | "\\показать" => Some("/show"),
        "/context" | "\\контекст" => Some("/context"),
        "/plan" | "\\план" => Some("/plan"),
        "/jobs" | "\\задачи" => Some("/jobs"),
        "/completion" | "\\доводка" => Some("/completion"),
        "/autoapprove" | "\\автоапрув" => Some("/autoapprove"),
        "/skills" | "\\скиллы" => Some("/skills"),
        "/enable" | "\\включить" => Some("/enable"),
        "/disable" | "\\выключить" => Some("/disable"),
        "/approve" | "\\апрув" => Some("/approve"),
        _ => None,
    }
}

fn split_command_arg(raw: &str) -> Option<&str> {
    raw.split_once(char::is_whitespace)
        .map(|(_, rest)| rest.trim())
        .filter(|value| !value.is_empty())
}

fn parse_completion_nudges(raw: &str) -> Result<Option<u32>, BootstrapError> {
    let trimmed = raw.trim();
    if matches!(trimmed, "off" | "выкл" | "disable") {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| BootstrapError::Usage {
            reason: format!(
                "unsupported completion mode {trimmed}; expected off|выкл or a non-negative integer"
            ),
        })
}

fn describe_completion_mode(completion_nudges: Option<u32>) -> String {
    match completion_nudges {
        None => "disabled".to_string(),
        Some(0) => "enabled with operator approval after the first early stop".to_string(),
        Some(value) => format!("enabled with {value} auto-nudges"),
    }
}

fn parse_auto_approve(raw: &str) -> Result<bool, BootstrapError> {
    match raw.trim() {
        "on" | "1" | "yes" | "да" | "вкл" | "enable" => Ok(true),
        "off" | "0" | "no" | "нет" | "выкл" | "disable" => Ok(false),
        value => Err(BootstrapError::Usage {
            reason: format!("unsupported auto-approve mode {value}; expected on|off"),
        }),
    }
}

fn find_pending_approval(
    app: &App,
    session_id: &str,
    requested_approval_id: Option<&str>,
) -> Result<Option<ReplPendingApproval>, BootstrapError> {
    Ok(app
        .latest_pending_approval(session_id, requested_approval_id)?
        .map(|pending| ReplPendingApproval {
            run_id: pending.run_id,
            approval_id: pending.approval_id,
        }))
}
